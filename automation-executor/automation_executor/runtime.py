import asyncio
from collections import OrderedDict
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from pathlib import Path
from uuid import uuid4

import httpx

from . import PROTOCOL_VERSION
from . import adb, validation
from .errors import ExecutorError, invalid
from .journal import Journal, TERMINAL, now
from .scheduler import FairDeviceSlots
from .worker import PreparedHelper, cleanup_record, execute_step


@dataclass
class Session:
    target: dict
    deadline: float
    timer: asyncio.Task | None = None
    lock: asyncio.Lock = field(default_factory=asyncio.Lock)
    commands: OrderedDict = field(default_factory=OrderedDict)
    last_seq: int = 0
    cancel: asyncio.Event = field(default_factory=asyncio.Event)
    closing: bool = False


class Runtime:
    def __init__(self, settings):
        from .media import MediaManager
        from .auth import authorization
        self.settings = settings
        settings.data_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        self.artifact_dir = settings.data_dir / "artifacts"
        self.artifact_dir.mkdir(mode=0o700, exist_ok=True)
        self.journal = Journal(settings.data_dir / "execution.sqlite3")
        self.journal.recover()
        self.media = MediaManager()
        self.http = httpx.AsyncClient(timeout=8, follow_redirects=False, trust_env=False,
                                     headers={"Authorization": authorization(settings.token)})
        self.sessions: dict[str, Session] = {}
        self.tasks: dict[str, asyncio.Task] = {}
        self.cancellations: dict[str, asyncio.Event] = {}
        self.device_tasks: dict[tuple[str,str], asyncio.Task] = {}
        self.device_cancellations: dict[tuple[str,str], asyncio.Event] = {}
        self.helpers: dict[str, PreparedHelper] = {}
        self.preparations: dict[tuple[str,str], asyncio.Task] = {}
        self.preparation_tasks: dict[str, asyncio.Task] = {}
        self.admission = asyncio.Lock()
        self.capacity = FairDeviceSlots(settings.max_running)
        self.limits_revision = 0
        self.limits = {
            "max_running": settings.max_running,
            "max_pending": settings.max_pending,
            "max_media_sessions": self.media._max_sessions,
            "max_media_starting": self.media._starting.limit,
        }
        row = self.journal.db.execute("SELECT value FROM metadata WHERE key='runtime_limits'").fetchone()
        if row:
            import json
            stored = json.loads(row[0])
            values = stored.get("limits") if isinstance(stored, dict) else None
            if (isinstance(values, dict) and set(values) == set(self.limits)
                    and all(type(values[key]) is int and values[key] >= 1 for key in self.limits)):
                self.limits = dict(values)
                self.limits_revision = stored.get("revision", 0)
                self.capacity.limit = values["max_running"]
                self.media._max_sessions = values["max_media_sessions"]
                self.media._starting.limit = values["max_media_starting"]
        self.artifact_lock = asyncio.Lock()
        self.shutting_down = False

    def health(self):
        return {"protocol_version": PROTOCOL_VERSION, "state": "ready", "executor_id": self.journal.executor_id, "generation": self.journal.generation,
                "capabilities": {"adb": True, "webrtc": True, "scripts": bool(self.settings.callback_url)},
                "limits": {"running_devices": self.limits["max_running"], "pending_runs": self.limits["max_pending"],
                           "media_sessions": self.limits["max_media_sessions"],
                           "media_starting": self.limits["max_media_starting"]}}

    def runtime_limits(self):
        return {
            "executor_id": self.journal.executor_id,
            "revision": self.limits_revision,
            **self.limits,
            "recommended": {"max_running": 5, "max_pending": 30,
                            "max_media_sessions": 10, "max_media_starting": 2},
            "source": "database" if self.limits_revision else "environment",
        }

    async def save_runtime_limits(self, payload):
        keys = {"max_running", "max_pending", "max_media_sessions", "max_media_starting", "revision"}
        if not isinstance(payload, dict) or set(payload) != keys:
            raise ExecutorError("invalid_limits", "执行器容量配置字段无效", 422)
        if any(type(payload[key]) is not int or payload[key] < 1 for key in keys - {"revision"}):
            raise ExecutorError("invalid_limits", "容量必须是正整数", 422)
        if type(payload["revision"]) is not int or payload["revision"] < 0:
            raise ExecutorError("invalid_limits_revision", "配置版本无效", 422)
        async with self.admission:
            if payload["revision"] != self.limits_revision:
                raise ExecutorError("limits_revision_conflict", "执行器容量配置已被修改，请刷新重试", 409)
            values = {key: payload[key] for key in self.limits}
            version = self.limits_revision + 1
            import json
            with self.journal.transaction() as db:
                db.execute(
                    "INSERT INTO metadata(key,value) VALUES('runtime_limits',?) "
                    "ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                    (json.dumps({"revision": version, "limits": values}),),
                )
            self.limits, self.limits_revision = values, version
            await self.capacity.reconfigure(values["max_running"])
            await self.media._starting.reconfigure(values["max_media_starting"])
            async with self.media._lock:
                self.media._max_sessions = values["max_media_sessions"]
            return self.runtime_limits()

    def prepared(self, target):
        helper = self.helpers.get(target["device_id"])
        return bool(helper and helper.ready_flag and helper.alive() and helper.target["target_fingerprint"] == target["target_fingerprint"]
                    and self.journal.prepared(target))

    def session(self, sid):
        current = self.sessions.get(sid)
        if not current or current.closing or asyncio.get_running_loop().time() >= current.deadline:
            raise ExecutorError("session_expired", "会话已关闭或授权到期，请重新打开画面", 410)
        self.journal.ensure_target(current.target)
        return current

    async def start_preparation(self, body):
        validation.object_fields(body, {"target", "request_id"}, {"target", "request_id"})
        target = validation.target(body["target"])
        request_id = validation.identifier(body["request_id"])
        self.journal.ensure_target(target)
        if self.shutting_down:
            raise ExecutorError("executor_stopping", "执行器正在停止", 503)
        async with self.admission:
            existing = self.journal.preparation_by_request(request_id)
            if existing:
                if existing["device_id"] != target["device_id"] or existing["target_fingerprint"] != target["target_fingerprint"]:
                    raise ExecutorError("idempotency_conflict", "请求编号已经绑定其他目标", 409)
                return existing
            active = self.journal.active_preparation(target["device_id"])
            if active:
                if active["target_fingerprint"] != target["target_fingerprint"]:
                    raise ExecutorError("device_busy", "旧设备目标仍在准备，请先确认其结束", 409)
                return active
            previous = self.journal.control(target)
            if (previous and previous["kind"] == "prepare" and previous["state"] == "cleanup_pending"):
                previous_did = previous["device_id"]
                old_helper = self.helpers.pop(previous_did, None)
                if old_helper and not await old_helper.close():
                    self.helpers[previous_did] = old_helper
                    raise ExecutorError("cleanup_pending", "旧控件进程尚未确认停止", 409)
                old_record = self.journal.worker_record("helper:" + previous_did)
                if not await asyncio.to_thread(cleanup_record, old_record):
                    raise ExecutorError("cleanup_pending", "旧控件进程尚未确认清理", 409)
                self.journal.clear_worker("helper:" + previous_did)
                self.journal.release(previous["holder"])
            pid = str(uuid4())
            holder = "prepare:" + pid
            self.journal.claim(target, holder, "prepare")
            try:
                result = self.journal.create_preparation(pid, request_id, target)
                task = asyncio.create_task(self._execute_preparation(pid, target, holder))
                self.preparation_tasks[pid] = task
                self.preparations[(target["device_id"],target["target_fingerprint"])] = task
                task.add_done_callback(lambda _, key=pid: self.preparation_tasks.pop(key, None))
                return result
            except BaseException:
                self.journal.release(holder)
                raise

    async def _execute_preparation(self, pid, target, holder):
        stage = "starting"
        def progress(code, label):
            nonlocal stage
            stage = code
            self.journal.preparation_update(pid, "running", code)
            self.journal.preparation_event(pid, "info", code, label)
        try:
            self.journal.preparation_update(pid, "running", stage)
            self.journal.preparation_event(pid, "info", stage, "开始检查 ADB 和手机控件组件")
            result = await self._prepare_automation_helper(target, on_stage=progress)
            if not self.prepared(target):
                raise ExecutorError("automation_prepare_invalid", "组件未能确认就绪", 502)
            self.journal.preparation_update(pid, "succeeded", "ready", message=result.get("message"))
            self.journal.preparation_event(pid, "success", "ready", "控件服务已就绪")
        except asyncio.CancelledError:
            self.journal.preparation_update(pid, "cancelled", "cancelled", "preparation_cancelled", "初始化任务已停止")
            self.journal.preparation_event(pid, "warning", stage, "任务被取消或执行器正在停止")
        except ExecutorError as exc:
            self.journal.preparation_update(pid, "failed", stage, exc.code, exc.message)
            self.journal.preparation_event(pid, "error", stage, f"{exc.code}：{exc.message}")
        except Exception:
            self.journal.preparation_update(pid, "failed", stage, "automation_prepare_failed", "初始化异常，已停止准备")
            self.journal.preparation_event(pid, "error", stage, "初始化出现未分类异常")
        finally:
            self.preparations.pop((target["device_id"],target["target_fingerprint"]), None)
            helper = self.helpers.get(target["device_id"])
            clean = not helper or helper.ready_flag or not helper.alive()
            self.journal.release(holder, clean=clean)
            if not clean:
                self.journal.preparation_update(pid, "cleanup_pending", stage, "cleanup_pending", "旧控件进程尚未确认清理")
                self.journal.preparation_event(pid, "warning", stage, "等待确认旧控件进程清理")

    def preparation(self, pid):
        return self.journal.preparation(validation.identifier(pid))

    def preparation_logs(self, pid, after_seq=0):
        return {"items": self.journal.preparation_logs(validation.identifier(pid), after_seq)}

    def preparation_history(self, device_id, owner):
        return {"items": self.journal.preparation_history(validation.identifier(device_id), owner)}

    async def cancel_preparation(self, pid):
        pid = validation.identifier(pid)
        record = self.journal.preparation(pid)
        task = self.preparation_tasks.get(pid)
        if task and not task.done():
            self.journal.preparation_update(pid, "cancel_requested", record["stage"], message="已申请停止")
            task.cancel()
        return self.journal.preparation(pid)

    async def adb_status(self, body):
        validation.object_fields(body, {"target"}, {"target"})
        target = validation.target(body["target"])
        self.journal.ensure_target(target)
        return await adb.connection_status(self.settings.adb_binary, target)

    async def hierarchy(self, body):
        """Capture a single bounded UI tree without pairing or restarting UiAutomator."""
        validation.object_fields(body, {"target"}, {"target"})
        target = validation.target(body["target"])
        self.journal.ensure_target(target)
        if self.shutting_down:
            raise ExecutorError("executor_stopping", "执行器正在停止", 503)
        if not self.prepared(target):
            raise ExecutorError("automation_not_prepared", "请先初始化该手机的 UiAutomator2 控件组件", 409)
        await adb.require_connected(self.settings.adb_binary, target)
        # Reuse the supervised short-lived worker; a hung RPC is terminated.
        return await execute_step(
            self.journal, "hierarchy:" + str(uuid4()), target,
            {"id": str(uuid4()), "type": "hierarchy", "timeout_ms": 20000},
            {}, asyncio.Event(), self.settings.adb_binary,
        )

    async def automation_status(self, body):
        validation.object_fields(body, {"target"}, {"target"})
        target = validation.target(body["target"])
        self.journal.ensure_target(target)
        device_id = target["device_id"]
        pending = self.journal.active_preparation(device_id)
        if pending:
            return {"device_id": device_id, "state": "preparing", "preparation_id": pending["id"],
                    "stage": pending["stage"], "executor_id": self.journal.executor_id}
        try:
            await adb.require_connected(self.settings.adb_binary, target)
        except ExecutorError:
            return {"device_id": device_id, "state": "adb_unavailable", "executor_id": self.journal.executor_id}
        if not self.prepared(target):
            return {"device_id": device_id, "state": "not_prepared", "executor_id": self.journal.executor_id}
        # Probe the running server through raw read-only RPC. This must NEVER
        # install, start or reconnect UiAutomator on a status query.
        def check():
            import adbutils
            from uiautomator2.core import _jsonrpc_call, DEFAULT_SERVER_PORT
            from uiautomator2._selector import Selector
            dev = adbutils.adb.device(serial=f'{target["ip"]}:{target["adb_port"]}')
            return _jsonrpc_call(dev, DEFAULT_SERVER_PORT, "count",
                                 (Selector(className="android.view.View"),), 3, False)
        try:
            observed = await asyncio.wait_for(asyncio.to_thread(check), timeout=5)
            state = "ready" if isinstance(observed, int) and not isinstance(observed, bool) else "unavailable"
        except Exception:
            state = "unavailable"
        return {"device_id": device_id, "state": state, "executor_id": self.journal.executor_id}

    async def prepare(self, body):
        if self.shutting_down:
            raise ExecutorError("executor_stopping", "执行器正在停止", 503)
        validation.object_fields(body, {"target", "action", "pairing_port", "code"}, {"target", "action"})
        target = validation.target(body["target"])
        self.journal.ensure_target(target)
        action = body["action"]
        if action not in ("pair", "connect", "inspect", "automation"):
            raise invalid("不支持该准备操作")
        if action == "inspect":
            result = await adb.prepare(self.settings.adb_binary, target, action, body.get("pairing_port"), body.get("code"))
            result["capabilities"]["automation"] = self.prepared(target)
            return result
        async with self.admission:
            previous = self.journal.control(target)
            if (previous and previous["kind"] == "prepare" and previous["state"] == "cleanup_pending"
                    and previous["device_id"] not in self.helpers):
                # Only explicit preparation can reclaim a preparation interrupted by
                # restart. Never steal a current active preparation or a run/session.
                helper_holder = "helper:" + previous["device_id"]
                record = self.journal.worker_record(helper_holder)
                if not await asyncio.to_thread(cleanup_record, record):
                    raise ExecutorError("cleanup_pending", "旧控件准备进程尚未确认停止，请稍后手动重试")
                self.journal.clear_worker(helper_holder)
                self.journal.release(previous["holder"])
            holder = "prepare:" + str(uuid4())
            self.journal.claim(target, holder, "prepare")
            self.preparations[(target["device_id"],target["target_fingerprint"])] = asyncio.current_task()
        try:
            if action == "automation":
                if body.get("code") is not None or body.get("pairing_port") is not None:
                    raise invalid()
                return await self._prepare_automation_helper(target)
            return await adb.prepare(self.settings.adb_binary, target, action, body.get("pairing_port"), body.get("code"))
        finally:
            self.preparations.pop((target["device_id"],target["target_fingerprint"]), None)
            self.journal.release(holder, clean=self.journal.worker_record(holder) is None)

    async def _prepare_automation_helper(self, target, on_stage=None):
        """Share the exact same uiautomator2 readiness and cleanup process
        between explicit preparation and a newly accepted script run.

        The caller must already hold a journal control claim for this target
        (prepare or script). A script never pairs or reconnects ADB implicitly.
        """
        self.journal.ensure_target(target)
        await adb.require_connected(self.settings.adb_binary, target)
        if any(did != target["device_id"] and validation.endpoint(helper.target) == validation.endpoint(target)
               for did, helper in self.helpers.items()):
            raise ExecutorError("target_alias_conflict", "同一 ADB 地址已由另一设备资料准备，请先修正重复目标")
        old = self.helpers.pop(target["device_id"], None)
        if old and not await old.close():
            self.helpers[target["device_id"]] = old
            raise ExecutorError("cleanup_pending", "之前的控件组件尚未完成清理")
        if len(self.helpers) >= 30:
            raise ExecutorError("executor_capacity", "控件准备容量已满", 429)
        old_record = self.journal.worker_record("helper:" + target["device_id"])
        if not await asyncio.to_thread(cleanup_record, old_record):
            raise ExecutorError("cleanup_pending", "旧控件进程尚未完成清理")
        helper = PreparedHelper(self.journal, target, self.settings.adb_binary)
        try:
            result = await helper.ready(on_stage=on_stage)
            self.journal.ensure_target(target)
        except BaseException:
            if not await helper.close():
                self.helpers[target["device_id"]] = helper
            raise
        self.helpers[target["device_id"]] = helper
        self.journal.mark_prepared(target)
        return result

    async def create_session(self, body):
        validation.object_fields(body, {"id", "target", "quality", "media", "unlock_password", "lease_seconds", "expected_executor_id"}, {"id", "target", "quality", "lease_seconds", "expected_executor_id"})
        self.require_identity(body["expected_executor_id"])
        sid = validation.identifier(body["id"])
        target = validation.target(body["target"])
        validation.integer(body["lease_seconds"], 1, 60)
        if body["quality"] not in ("wall", "focus"):
            raise invalid("画质档位无效")
        secret=body.get("unlock_password")
        if secret is not None and (not isinstance(secret,str) or not 4<=len(secret)<=64
                                  or not all(33<=ord(ch)<=126 for ch in secret)):
            raise invalid("开屏密码格式无效")
        async with self.admission:
            if self.shutting_down:
                raise ExecutorError("executor_stopping", "执行器正在停止", 503)
            self.journal.create_session(sid, target)
            try:
                details = await self.media.create(sid, target, body["quality"], body.get("media"), secret)
                if self.journal.db.execute("SELECT 1 FROM closed_sessions WHERE id=?", (sid,)).fetchone():
                    await self.media.close(sid)
                    raise ExecutorError("session_closed", "创建期间会话已被关闭")
            except Exception:
                self.journal.session_state(sid, "failed")
                raise
            current = Session(target, asyncio.get_running_loop().time() + body["lease_seconds"])
            self.sessions[sid] = current
            current.timer = asyncio.create_task(self._expire_session(sid, current))
        return {"id": sid, "executor_id": self.journal.executor_id, "device_id": target["device_id"], "state": "opening", **details,
                "expires_at": (datetime.now(timezone.utc) + timedelta(seconds=body["lease_seconds"])).isoformat()}

    async def _expire_session(self, sid, session):
        try:
            while not session.closing:
                remaining = session.deadline - asyncio.get_running_loop().time()
                if remaining <= 0:
                    await self.close_session(sid)
                    return
                await asyncio.sleep(remaining)
        except asyncio.CancelledError:
            pass

    async def offer(self, sid, body):
        validation.object_fields(body, {"sdp", "type"}, {"sdp", "type"})
        if body["type"] != "offer":
            raise invalid("SDP 类型必须为 offer")
        sdp = validation.text(body["sdp"], 65536)
        self.session(sid)
        result = await self.media.offer(sid, sdp)
        self.session(sid)  # It may have expired while ADB/ICE was being prepared.
        self.journal.session_state(sid, "active")
        return result

    async def unlock(self, sid, body):
        validation.object_fields(body, {"expected_executor_id", "unlock_password"}, {"expected_executor_id"})
        self.require_identity(body["expected_executor_id"])
        secret = body.get("unlock_password")
        if secret is not None and (not isinstance(secret, str) or not 4 <= len(secret) <= 64
                                  or not all(33 <= ord(ch) <= 126 for ch in secret)):
            raise invalid("开屏密码格式无效")
        current = self.session(sid)
        async with current.lock:
            self.session(sid)
            # Same ownership/ADB endpoint arbitration as other manual commands.
            self.journal.claim(current.target, "session:" + sid, "manual")
            result = await self.media.unlock(sid, secret)
            return {**result, "executor_id": self.journal.executor_id}

    async def renew(self, sid, body):
        validation.object_fields(body, {"target_fingerprint", "lease_seconds"}, {"target_fingerprint", "lease_seconds"})
        seconds = validation.integer(body["lease_seconds"], 1, 60)
        current = self.session(sid)
        if body["target_fingerprint"] != current.target["target_fingerprint"]:
            await self.close_session(sid)
            raise ExecutorError("target_changed", "设备目标已经改变")
        try:
            status = self.media.status(sid)
        except Exception:
            await self.close_session(sid)
            raise ExecutorError("media_closed", "媒体会话已结束，请手动重新连接") from None
        if status.get("state") in ("closed", "failed", "cleanup_pending"):
            await self.close_session(sid)
            raise ExecutorError("media_closed", "媒体会话已结束，请手动重新连接")
        current.deadline = asyncio.get_running_loop().time() + seconds
        return {"id": sid, "state": status.get("state", "active"), "expires_at": (datetime.now(timezone.utc) + timedelta(seconds=seconds)).isoformat()}

    async def command(self, sid, body):
        current = self.session(sid)
        allowed = {"type", "command_id", "seq", "action", "x", "y", "width", "height", "key", "text"}
        validation.object_fields(body, allowed, {"type", "command_id", "seq"})
        command_id = validation.identifier(body["command_id"])
        sequence = validation.integer(body["seq"], 1, 2 ** 53 - 1)
        kind = body["type"]
        if kind not in ("touch", "touch.release", "key", "input_text", "stop"):
            raise invalid("不支持该控制动作")
        # Let cancellation interrupt a text worker before waiting for its command lock.
        if kind in ("stop", "touch.release") and sequence == current.last_seq + 1:
            current.cancel.set()
        request_digest = validation.digest(body)
        async with current.lock:
            self.session(sid)
            old = current.commands.get(command_id)
            if old:
                if old[0] != request_digest:
                    raise ExecutorError("idempotency_conflict", "控制编号与原命令不一致")
                if old[1]["type"] == "error":
                    raise ExecutorError(old[1]["code"], old[1]["message"], old[1].get("status", 409))
                return old[1]
            if sequence != current.last_seq + 1:
                raise ExecutorError("command_order_invalid", "控制顺序不一致，请重新打开控制通道")
            current.last_seq = sequence
            pending = {"type": "error", "command_id": command_id, "state": "outcome_unknown", "code": "outcome_unknown", "message": "该命令结果未确认，不会重复发送"}
            current.commands[command_id] = (request_digest, pending)
            while len(current.commands) > 256:
                current.commands.popitem(last=False)
            holder = "session:" + sid
            try:
                if kind in ("stop", "touch.release"):
                    # A viewer is not an action holder. An unclaimed STOP or
                    # RELEASE must NEVER send scrcpy control bytes while a
                    # script/prepare/other session owns the physical device.
                    control = self.journal.control(current.target)
                    if control and (control["holder"] != holder or control["kind"] != "manual"
                                    or control["state"] != "active"):
                        raise ExecutorError(
                            "cleanup_pending" if control["state"] == "cleanup_pending" else "device_busy",
                            "设备由脚本/准备任务占用或仍待清理，已拒绝当前会话的手控释放", 409)
                    current.cancel.set()
                    if control is None:
                        # No session action claim means this viewer owns no
                        # touch to release. Ack the request as a no-op.
                        outcome = {"state": "action_sent"}
                    else:
                        # Under the session command lock, release the actual
                        # touch before relinquishing the journal write lease.
                        outcome = await self.media.command(sid, body)
                        if kind == "stop":
                            self.journal.release(holder, clean=self.journal.worker_record(holder) is None)
                else:
                    if kind == "input_text":
                        value = validation.text(body.get("text"), 2000, empty=True)
                        if not self.prepared(current.target):
                            raise ExecutorError("capability_missing", "请先在设备配置中点击准备脚本控制，文字输入需要控件组件")
                    self.journal.claim(current.target, holder, "manual")
                    if kind == "input_text":
                        current.cancel = asyncio.Event()
                        outcome = await execute_step(self.journal, holder, current.target,
                                                     {"id": command_id, "type": "input_text", "text": value}, {},
                                                     current.cancel, self.settings.adb_binary)
                    else:
                        outcome = await self.media.command(sid, body)
            except Exception as exc:
                if all(hasattr(exc, key) for key in ("code", "message", "status")):
                    current.commands[command_id] = (request_digest, {"type": "error", "code": exc.code,
                                                                    "message": exc.message, "status": exc.status})
                raise
            result = {"type": "ack", "command_id": command_id, "state": outcome.get("state", "action_sent")}
            current.commands[command_id] = (request_digest, result)
            return result

    def require_identity(self, expected):
        if validation.identifier(expected) != self.journal.executor_id:
            raise ExecutorError("executor_identity_changed", "执行器身份已改变，无法确认旧资源清理")

    async def close_session_request(self, sid, body):
        validation.object_fields(body, {"expected_executor_id"}, {"expected_executor_id"})
        self.require_identity(body["expected_executor_id"])
        return await self.close_session(sid)

    async def close_session(self, sid):
        sid = validation.identifier(sid)
        # Persist before any await: a late create/offer cannot revive this id.
        self.journal.refuse_session(sid)
        current = self.sessions.get(sid)
        if current:
            current.closing = True
            current.cancel.set()
            if current.timer and current.timer is not asyncio.current_task():
                current.timer.cancel()
        holder = "session:" + sid
        result = await self.media.close(sid)
        worker = self.journal.worker_record(holder)
        if worker:
            clean = await asyncio.to_thread(cleanup_record, worker)
            if clean:
                self.journal.clear_worker(holder)
        else:
            clean = True
        clean = clean and not result.get("cleanup_pending", False)
        self.journal.release(holder, clean=clean)
        self.journal.session_state(sid, "closed" if clean else "cleanup_pending")
        if clean:
            self.sessions.pop(sid, None)
        return {"id": sid, "executor_id": self.journal.executor_id, "state": "closed" if clean else "cleanup_pending", "cleanup_pending": not clean}

    async def _authorize(self, rid, target):
        if not self.settings.callback_url:
            raise ExecutorError("callback_unconfigured", "执行器未配置主服务授权地址", 503)
        try:
            response = await self.http.post(self.settings.callback_url + "/api/v1/automation/internal/authorize",
                                            json={"run_id": rid, "executor_id": self.journal.executor_id, "device_id": target["device_id"], "target_fingerprint": target["target_fingerprint"]})
            if response.status_code != 200 or response.json().get("authorized") is not True:
                raise ExecutorError("grant_revoked", "任务权限已撤销或设备目标已改变")
        except (httpx.HTTPError, ValueError):
            raise ExecutorError("authorization_unavailable", "无法确认任务权限，已停止发送新动作", 503) from None

    async def _publish(self, event):
        if not self.settings.callback_url:
            return
        try:
            response = await self.http.post(self.settings.callback_url + "/api/v1/automation/internal/events", json=event)
            if 200 <= response.status_code < 300:
                self.journal.delivered(event["event_id"])
        except httpx.HTTPError:
            pass  # Persisted evidence remains available for explicit refresh.

    async def _update(self, rid, did, values):
        event = self.journal.update_device(rid, did, values)
        await self._publish(event)

    async def create_run(self, body):
        request = validation.run_request(body)
        if request["expected_executor_id"] != self.journal.executor_id:
            raise ExecutorError("executor_identity_changed", "执行器身份已改变，禁止继续旧派发")
        if not self.settings.callback_url:
            raise ExecutorError("callback_unconfigured", "请先配置执行器的主服务授权地址", 503)
        async with self.admission:
            result, created = self.journal.accept_run(request, allow_new=not self.shutting_down and len(self.tasks) < self.limits["max_pending"])
            if created:
                cancel = asyncio.Event()
                self.cancellations[request["id"]] = cancel
                task = asyncio.create_task(self._run(request, cancel))
                self.tasks[request["id"]] = task
                task.add_done_callback(lambda _task, rid=request["id"]: self._task_done(rid))
        return result

    def _task_done(self, rid):
        task = self.tasks.pop(rid, None)
        self.cancellations.pop(rid, None)
        if task and not task.cancelled():
            # Retrieve exceptions without logging potentially sensitive third-party details.
            task.exception()

    async def _run(self, body, cancel):
        limit = asyncio.Semaphore(body["concurrency"])
        children = []
        try:
            for target in body["targets"]:
                key = (body["id"],target["device_id"])
                device_cancel = asyncio.Event()
                if cancel.is_set() or self.journal.withdrawn(*key):
                    device_cancel.set()
                self.device_cancellations[key] = device_cancel
                task = asyncio.create_task(self._run_device(body,target,device_cancel,limit))
                self.device_tasks[key] = task
                children.append(task)
            await asyncio.gather(*children, return_exceptions=True)
        finally:
            for target in body["targets"]:
                key=(body["id"],target["device_id"])
                self.device_tasks.pop(key,None)
                self.device_cancellations.pop(key,None)
            body["parameters"].clear()

    async def _run_device(self, body, target, cancel, limit):
        rid, did = body["id"], target["device_id"]
        holder = "run:" + rid + ":" + did
        results = [{"step_id": step["id"], "state": "pending"} for step in body["steps"]]
        acquired = False
        current_step = None
        in_flight = False
        try:
            async with asyncio.timeout(300):
                await limit.acquire()
                try:
                    await self.capacity.acquire(rid)
                except BaseException:
                    limit.release()
                    raise
            acquired = True
            if cancel.is_set() or (self.journal.cancelled(rid) or self.journal.withdrawn(rid, did)):
                await self._update(rid, did, {"state": "cancelled", "steps": results})
                return
            self.journal.claim(target, holder, "script")
            # Authorize before any helper-side effects. If a valid helper is
            # not running, prepare it once under this run's exclusive claim.
            await self._authorize(rid, target)
            await adb.require_connected(self.settings.adb_binary, target)
            # HOME/BACK/RECENT/WAKE and launching an app are native ADB
            # operations. Don't start/install the heavy UiAutomator helper
            # merely to execute these steps. Selector/touch/text/screenshot
            # scripts still require the authenticated helper.
            needs_ui = any(step["type"] not in ("key", "launch_app") for step in body["steps"])
            if needs_ui and not self.prepared(target):
                await self._update(rid, did, {"state": "starting", "steps": results})
                await self._prepare_automation_helper(target)
                if not self.prepared(target):
                    raise ExecutorError("automation_prepare_failed", "手机脚本控制组件未能确认就绪")
            if cancel.is_set() or self.journal.cancelled(rid) or self.journal.withdrawn(rid, did):
                await self._update(rid, did, {"state": "cancelled", "steps": results})
                return
            await self._authorize(rid, target)
            await self._update(rid, did, {"state": "running", "steps": results})
            # One skip map per phone. A skipped outer branch also skips nested
            # conditions without ever evaluating their predicates.
            branch_skips = {}
            for index, step in enumerate(body["steps"]):
                if cancel.is_set() or (self.journal.cancelled(rid) or self.journal.withdrawn(rid, did)):
                    await self._update(rid, did, {"state": "cancelled", "steps": results})
                    return
                await self._authorize(rid, target)
                current_step = step["id"]
                if index in branch_skips:
                    reason, origin = branch_skips.pop(index)
                    results[index].update(state="skipped", message=reason,
                                          skipped_by_step_id=origin, finished_at=now())
                    await self._update(rid, did, {"state": "running", "current_step": current_step, "steps": results})
                    continue
                results[index].update(state="running", started_at=now())
                await self._update(rid, did, {"state": "running", "current_step": current_step, "steps": results})
                if cancel.is_set() or self.journal.withdrawn(rid,did):
                    raise ExecutorError("operation_cancelled", "该设备已退出当前脚本")
                if step["type"] == "screenshot":
                    async with self.artifact_lock:
                        # Count actual local files too: a crash between file write and
                        # journal registration must not make orphan bytes invisible.
                        stored_bytes = sum(path.stat().st_size for path in self.artifact_dir.iterdir() if path.is_file())
                        if stored_bytes + 8 * 1024 * 1024 > self.settings.max_artifact_bytes:
                            raise ExecutorError("artifact_capacity", "截图存储容量不足，请手动清理后重新运行")
                        aid = str(uuid4())
                        path = self.artifact_dir / aid
                        registered = False
                        try:
                            in_flight = True
                            result = await execute_step(self.journal, holder, target, step, body["parameters"], cancel, self.settings.adb_binary, path)
                            in_flight = False
                            size = path.stat().st_size
                            self.journal.artifact(aid, rid, did, current_step, path, size)
                            registered = True
                        finally:
                            if not registered:
                                path.unlink(missing_ok=True)
                        result["artifact_id"] = aid
                        result["size"] = size
                else:
                    in_flight = True
                    result = await execute_step(self.journal, holder, target, step, body["parameters"], cancel, self.settings.adb_binary)
                    in_flight = False
                if step["type"] == "if_gone":
                    count = result.get("skip_steps")
                    expected = step["skip_count"]
                    if type(count) is not int or count not in (0, expected) or count > len(body["steps"]) - index - 1:
                        raise ExecutorError("condition_result_invalid", "条件判定与脚本结构不一致，停止执行")
                    if result.get("state") != "condition_passed":
                        raise ExecutorError("condition_result_invalid", "If 条件结果无效，停止执行")
                    if count:
                        for skipped_index in range(index + 1, index + count + 1):
                            if skipped_index in branch_skips:
                                raise ExecutorError("condition_overlap", "条件跳过范围发生冲突")
                            branch_skips[skipped_index] = ("控件仍存在，If 条件区域未执行", step["id"])
                if step["type"] == "if_else":
                    branch = result.get("branch_taken")
                    then_count, else_count = step["then_count"], step["else_count"]
                    if result.get("state") != "condition_passed" or branch not in ("then", "else") or (
                        index + then_count + else_count >= len(body["steps"])
                    ):
                        raise ExecutorError("condition_result_invalid", "if-else 条件结果或分支范围无效，停止执行")
                    if branch == "then":
                        ignored = range(index + then_count + 1, index + then_count + else_count + 1)
                        reason = "Then 分支执行，Else 分支未执行"
                    else:
                        ignored = range(index + 1, index + then_count + 1)
                        reason = "Else 分支执行，Then 分支未执行"
                    for skipped_index in ignored:
                        if skipped_index in branch_skips:
                            raise ExecutorError("condition_overlap", "条件分支范围发生冲突")
                        branch_skips[skipped_index] = (reason, step["id"])
                results[index].update(result)
                results[index]["finished_at"] = now()
                await self._update(rid, did, {"state": "running", "steps": results})
            await self._update(rid, did, {"state": "completed", "current_step": current_step, "steps": results})
        except BaseException as exc:
            cancelled = cancel.is_set() or (self.journal.cancelled(rid) or self.journal.withdrawn(rid, did)) or isinstance(exc, asyncio.CancelledError)
            code = getattr(exc, "code", "executor_interrupted")
            unknown = (getattr(exc, "outcome_unknown", False)
                       or (isinstance(exc, asyncio.CancelledError) and in_flight)
                       or code in ("operation_cancelled", "step_timeout", "worker_interrupted", "device_action_unconfirmed"))
            state = "outcome_unknown" if unknown else ("cancelled" if cancelled else "failed")
            if code in ("grant_revoked", "authorization_unavailable", "executor_interrupted") and not cancelled:
                state = "interrupted"
            message = getattr(exc, "message", "执行已中断，未自动重放动作")
            for step_result in results:
                if step_result["state"] == "running":
                    step_result.update(state="outcome_unknown" if unknown else "failed", message=message, finished_at=now())
                elif step_result["state"] == "pending":
                    step_result["state"] = "cancelled" if cancelled else "skipped"
            await self._update(rid, did, {"state": state, "current_step": current_step, "steps": results,
                                         "error": {"code": code, "message": message}})
        finally:
            clean = self.journal.worker_record(holder) is None
            self.journal.release(holder, clean=clean)
            if acquired:
                await self.capacity.release()
                limit.release()
            if clean and (self.journal.cancelled(rid) or self.journal.withdrawn(rid, did)):
                # This device's execution path is finished and cannot send another action.
                await self._update(rid, did, {"cleanup_confirmed": True})

    async def cancel_run(self, rid, body):
        rid = validation.identifier(rid)
        validation.object_fields(body, {"expected_executor_id"}, {"expected_executor_id"})
        if validation.identifier(body["expected_executor_id"]) != self.journal.executor_id:
            raise ExecutorError("executor_identity_changed", "执行器身份已改变，无法确认旧任务停止")
        result = self.journal.cancel(rid)
        cancel = self.cancellations.get(rid)
        if cancel:
            cancel.set()
        task = self.tasks.get(rid)
        if task and not task.done():
            if not task.cancelling():
                task.cancel()
            await asyncio.wait({task}, timeout=5)
        # On an explicit cancellation after restart, clean only recorded owned processes.
        if (not task or task.done()) and result.get("devices"):
            for device in self.journal.run(rid)["devices"]:
                holder = "run:" + rid + ":" + device["device_id"]
                record = self.journal.worker_record(holder)
                clean = await asyncio.to_thread(cleanup_record, record)
                self.journal.release(holder, clean=clean)
                if clean:
                    values = {"cleanup_confirmed": True}
                    if device["state"] not in TERMINAL:
                        values["state"] = "cancelled"
                        values["steps"] = [{**step, "state": "outcome_unknown" if step["state"] == "running" else
                                            ("cancelled" if step["state"] in ("pending", "queued") else step["state"])}
                                           for step in device["steps"]]
                        if any(step["state"] == "outcome_unknown" for step in values["steps"]):
                            values["state"] = "outcome_unknown"
                    # Response is immediate durable proof; caller refreshes without a timer.
                    self.journal.update_device(rid, device["device_id"], values)
        return self.journal.run(rid) if result.get("devices") else result

    async def withdraw_device(self, rid, did, body):
        rid,did=validation.identifier(rid),validation.identifier(did)
        validation.object_fields(body,{"event_id","expected_executor_id","old_target_fingerprint"},{"event_id","expected_executor_id","old_target_fingerprint"})
        if validation.identifier(body["expected_executor_id"]) != self.journal.executor_id:
            raise ExecutorError("executor_identity_changed", "执行器身份已改变，不能确认旧设备退出")
        event_id=validation.identifier(body["event_id"])
        fingerprint=validation.text(body["old_target_fingerprint"],256)
        never_started=self.journal.withdraw(rid,did,fingerprint,event_id)
        if never_started:
            return {"id":rid,"device_id":did,"executor_id":self.journal.executor_id,"event_id":event_id,
                    "old_target_fingerprint":fingerprint,"fenced":True,"never_started":True,"cleanup_confirmed":True}
        cancel=self.device_cancellations.get((rid,did))
        if cancel: cancel.set()
        task=self.device_tasks.get((rid,did))
        if task and not task.done():
            if not task.cancelling(): task.cancel()
            await asyncio.wait({task},timeout=6)
        holder="run:"+rid+":"+did
        clean=False
        if not task or task.done():
            clean=await asyncio.to_thread(cleanup_record,self.journal.worker_record(holder))
            self.journal.release(holder,clean=clean)
        device=next(d for d in self.journal.run(rid)["devices"] if d["device_id"]==did)
        values={"cleanup_confirmed":clean}
        if device["state"] not in TERMINAL:
            values["steps"]=[{**step,"state":"outcome_unknown" if step["state"]=="running" else ("cancelled" if step["state"] in ("pending","queued") else step["state"])} for step in device["steps"]]
            values["state"]="outcome_unknown" if any(step["state"]=="outcome_unknown" for step in values["steps"]) else ("cancelled" if clean else "cleanup_pending")
        self.journal.update_device(rid,did,values)
        return self.journal.run(rid)

    async def verify_unused_target(self, did, body):
        """Read-only proof for a phantom registration before first ADB pairing.

        Never clear retired-target refusals or declare an unknown worker idle.
        """
        did = validation.identifier(did)
        validation.object_fields(body, {"event_id", "expected_executor_id", "target"},
                                 {"event_id", "expected_executor_id", "target"})
        target = validation.target(body["target"])
        event_id = validation.identifier(body["event_id"])
        if target["device_id"] != did or body["expected_executor_id"] != self.journal.executor_id:
            raise ExecutorError("target_mismatch", "清理目标或执行器身份不一致")
        import json
        async with self.admission:
            idle = not self.journal.target_refused(target) and self.journal.control(target) is None
            idle = idle and not any(k[0] == did and not task.done() for k,task in self.preparations.items())
            idle = idle and not any(s.target["device_id"] == did or validation.endpoint(s.target) == validation.endpoint(target) for s in self.sessions.values())
            idle = idle and not any(h.target["device_id"] == did or validation.endpoint(h.target) == validation.endpoint(target) for h in self.helpers.values())
            idle = idle and self.journal.worker_record("helper:"+did) is None and not self.journal.prepared(target)
            if idle:
                for row in self.journal.db.execute("SELECT target FROM sessions WHERE state NOT IN ('closed','failed')"):
                    saved = json.loads(row["target"])
                    if saved["device_id"] == did or validation.endpoint(saved) == validation.endpoint(target):
                        idle = False
                        break
            if idle:
                for row in self.journal.db.execute("SELECT target FROM run_targets WHERE device_id=?", (did,)):
                    saved = json.loads(row["target"])
                    if saved["target_fingerprint"] == target["target_fingerprint"]:
                        idle = False
                        break
            if idle:
                status, listing, _ = await adb.command(self.settings.adb_binary, "devices", timeout=8)
                idle = status == 0 and all(line.split()[0] != adb.serial(target) for line in listing.splitlines()[1:] if line.split())
            return {"device_id": did, "event_id": event_id, "executor_id": self.journal.executor_id,
                    "target_fingerprint": target["target_fingerprint"], "unused_confirmed": bool(idle)}

    async def retire_target(self, did, body):
        did=validation.identifier(did)
        validation.object_fields(body,{"event_id","expected_executor_id","target"},{"event_id","expected_executor_id","target"})
        target=validation.target(body["target"])
        if target["device_id"] != did or validation.identifier(body["expected_executor_id"]) != self.journal.executor_id:
            raise ExecutorError("target_mismatch", "清理目标或执行器身份不一致")
        event_id=validation.identifier(body["event_id"])
        async with self.admission:
            if self.journal.retire(target,event_id):
                return {"device_id":did,"event_id":event_id,"executor_id":self.journal.executor_id,"cleanup_confirmed":True}
            clean=True
            preparation=self.preparations.get((did,target["target_fingerprint"]))
            if preparation and not preparation.done():
                preparation.cancel()
                await asyncio.wait({preparation},timeout=10)
                clean=preparation.done()
            # Helpers can survive an executor restart; their process identity is journaled.
            record=self.journal.worker_record("helper:"+did)
            if did not in self.helpers and record:
                prepared=self.journal.db.execute("SELECT fingerprint FROM prepared WHERE device_id=?",(did,)).fetchone()
                control=self.journal.control(target)
                import json
                owned=self.journal.prepared(target) or bool(control and control["target"] and json.loads(control["target"])["target_fingerprint"]==target["target_fingerprint"])
                if owned:
                    stopped=await asyncio.to_thread(cleanup_record,record)
                    clean=clean and stopped
                    if stopped: self.journal.clear_worker("helper:"+did)
                elif not prepared:
                    clean=False  # Unknown process ownership is not proof of release.

            for row in list(self.journal.db.execute("SELECT id,target FROM sessions WHERE state NOT IN ('closed','failed')")):
                import json
                old=json.loads(row["target"])
                if old["device_id"]==did and old["target_fingerprint"]==target["target_fingerprint"]:
                    result=await self.close_session(row["id"])
                    clean=clean and not result["cleanup_pending"]
            helper=self.helpers.get(did)
            if helper and helper.target["target_fingerprint"]==target["target_fingerprint"]:
                stopped=await helper.close()
                clean=clean and stopped
                if stopped: self.helpers.pop(did,None)
            control=self.journal.control(target)
            if control and control["target"]:
                import json
                old=json.loads(control["target"])
                if old["target_fingerprint"]==target["target_fingerprint"]:
                    if control["kind"]=="script": clean=False
                    else:
                        stopped=await asyncio.to_thread(cleanup_record,self.journal.worker_record(control["holder"]))
                        self.journal.release(control["holder"],clean=stopped)
                        clean=clean and stopped
            # A newer target can legitimately reuse an address. Never disconnect it.
            if clean and not self.journal.control(target) and not any(validation.endpoint(s.target)==validation.endpoint(target) for s in self.sessions.values()) and not any(validation.endpoint(h.target)==validation.endpoint(target) for h in self.helpers.values()):
                await adb.disconnect(self.settings.adb_binary,target)
            if clean: self.journal.retired(target,event_id)
            return {"device_id":did,"event_id":event_id,"executor_id":self.journal.executor_id,"cleanup_confirmed":clean}

    def artifact(self, rid, aid):
        row = self.journal.find_artifact(validation.identifier(rid), validation.identifier(aid))
        if not row:
            raise ExecutorError("artifact_not_found", "截图不存在", 404)
        path = Path(row["path"])
        if path.parent.resolve() != self.artifact_dir.resolve() or not path.is_file():
            raise ExecutorError("artifact_not_found", "截图文件不可用", 404)
        return path

    async def shutdown(self):
        self.shutting_down = True
        for task in list(self.preparation_tasks.values()):
            if not task.done():
                task.cancel()
        if self.preparation_tasks:
            await asyncio.gather(*list(self.preparation_tasks.values()), return_exceptions=True)
        for sid in list(self.sessions):
            try:
                await self.close_session(sid)
            except Exception:
                pass
        for rid, cancel in list(self.cancellations.items()):
            self.journal.cancel(rid)
            cancel.set()
        tasks = list(self.tasks.values())
        for task in tasks:
            if not task.done() and not task.cancelling():
                task.cancel()
        if tasks:
            await asyncio.gather(*tasks, return_exceptions=True)
        for helper in self.helpers.values():
            await helper.close()
        self.helpers.clear()
        await self.media.close_all()
        await self.http.aclose()
        self.journal.close()
