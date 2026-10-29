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
    # Read-only mode is monotonic for the lifetime of this viewer. A separate
    # server-authorized resume protocol is required before enabling writes.
    view_only: bool = False
    # Only confirmed post-release viewers may return an idempotent success.
    view_only_confirmed: bool = False


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

    def _automation_helper_diagnostics(self, target):
        """Expose the independent helper/lease facts needed to explain status."""
        helper = self.helpers.get(target["device_id"])
        helper_target = getattr(helper, "target", None) if helper else None
        target_matches = bool(
            helper_target and helper_target.get("target_fingerprint") == target["target_fingerprint"]
        )
        try:
            helper_alive = bool(helper and helper.alive())
        except Exception:
            helper_alive = False
        try:
            journal_prepared = bool(self.journal.prepared(target))
        except Exception:
            journal_prepared = False
        worker_record = self.journal.worker_record("helper:" + target["device_id"])
        process = getattr(helper, "process", None) if helper else None
        return {
            "helper_present": helper is not None,
            "helper_alive": helper_alive,
            "helper_ready_flag": bool(getattr(helper, "ready_flag", False)) if helper else False,
            "helper_target_matches": target_matches,
            "journal_prepared": journal_prepared,
            "worker_record_present": worker_record is not None,
            "worker_pid": getattr(process, "pid", None) if process else (worker_record.get("pid") if worker_record else None),
        }

    @staticmethod
    def _automation_probe_exception_message(exc, target):
        """Keep useful SDK detail while redacting the selected phone endpoint."""
        try:
            message = " ".join(str(exc).split())[:400]
        except Exception:
            message = ""
        endpoint = f'{target["ip"]}:{target["adb_port"]}'
        if endpoint:
            message = message.replace(endpoint, "<device-endpoint>")
        if target["ip"]:
            message = message.replace(target["ip"], "<device-ip>")
        return message or "异常没有提供可读详情"

    async def _automation_rpc_probe(self, target):
        """Return a bounded, read-only RPC probe plus actionable failure evidence."""
        helper = self._automation_helper_diagnostics(target)
        if not self.prepared(target):
            if helper["helper_present"] and not helper["helper_target_matches"]:
                code, message = "automation_target_changed", "现存控件进程绑定的设备指纹与当前设备不一致"
            elif not helper["helper_present"]:
                code, message = "automation_helper_missing", "执行器中没有该设备的控件进程，请重新初始化控件"
            elif not helper["helper_alive"]:
                code, message = "automation_helper_process_exited", "控件进程已退出，手机端服务状态尚未验证"
            elif not helper["helper_ready_flag"]:
                code, message = "automation_helper_not_ready", "控件进程尚未完成初始化握手"
            else:
                code, message = "automation_preparation_not_recorded", "控件进程存在，但执行器没有当前设备的就绪记录"
            return {
                "ready": False, "code": code, "message": message, "stage": "helper_state",
                "diagnostics": {"helper": helper},
            }

        class ProbeFailure(Exception):
            def __init__(self, stage, original):
                super().__init__(str(original))
                self.stage = stage
                self.original = original

        probe_stage = {"value": "load_rpc_sdk"}

        def check():
            try:
                probe_stage["value"] = "load_rpc_sdk"
                import adbutils
                from uiautomator2.core import _jsonrpc_call, DEFAULT_SERVER_PORT
                probe_stage["value"] = "adb_device_lookup"
                # The global adbutils client has no socket timeout. A blocked
                # ADB transport can outlive asyncio.wait_for and leak a thread.
                # Give this read-only probe its own bounded client.
                client = adbutils.AdbClient(socket_timeout=3)
                dev = client.device(serial=f'{target["ip"]}:{target["adb_port"]}')
                probe_stage["value"] = "uiautomator_rpc"
                # Initialization verifies dumpWindowHierarchy successfully.
                # Probe that same lightweight RPC at shallow depth instead of
                # using a selector/count call that can stall on some devices.
                observed = _jsonrpc_call(
                    dev, DEFAULT_SERVER_PORT, "dumpWindowHierarchy",
                    (True, 2), 3, False,
                )
                probe_stage["value"] = "rpc_response_validation"
                return observed
            except Exception as exc:
                raise ProbeFailure(probe_stage["value"], exc) from exc

        try:
            observed = await asyncio.wait_for(asyncio.to_thread(check), timeout=5)
        except asyncio.TimeoutError:
            stage = probe_stage["value"]
            return {
                "ready": False, "code": "automation_rpc_timeout",
                "message": "UiAutomator2 控件树 RPC 在 5 秒内未返回；执行器进程存活不代表手机端 RPC 已响应",
                "stage": stage,
                "diagnostics": {
                    "helper": helper,
                    "probe": {"method": "dumpWindowHierarchy", "max_depth": 2,
                              "timeout_seconds": 5, "exception_type": "TimeoutError",
                              "exception_message": "控件树 RPC 未在期限内返回；已为 ADB 传输设置 3 秒 socket timeout"},
                },
            }
        except ProbeFailure as failure:
            stage = failure.stage
            exc = failure.original
            exception_type = type(exc).__name__
            if stage == "load_rpc_sdk":
                code, message = "uiautomator_sdk_unavailable", "执行器无法加载 UiAutomator2 RPC 依赖"
            elif stage == "adb_device_lookup":
                code, message = "adb_device_lookup_failed", "ADB 已通过前置检查，但 RPC 探测无法重新绑定设备"
            elif exception_type == "UiAutomationNotConnectedError":
                code, message = "uiautomator_service_disconnected", "手机端 UiAutomator2 服务未连接或无障碍服务已断开"
            elif exception_type in ("AdbTimeout", "TimeoutError", "ReadTimeout", "ConnectTimeout"):
                code, message = "uiautomator_rpc_transport_timeout", "建立 ADB 转发连接或等待手机端控件 RPC 响应超时"
            elif exception_type in ("HTTPError", "ConnectionError"):
                code, message = "uiautomator_rpc_transport_error", "访问手机端 UiAutomator2 RPC 时发生连接或 HTTP 错误"
            else:
                code, message = "uiautomator_rpc_failed", "手机端 UiAutomator2 RPC 探测抛出异常"
            return {
                "ready": False, "code": code, "message": message, "stage": stage,
                "diagnostics": {
                    "helper": helper,
                    "probe": {
                        "method": "dumpWindowHierarchy", "max_depth": 2,
                        "exception_type": exception_type,
                        "exception_message": self._automation_probe_exception_message(exc, target),
                    },
                },
            }

        if not isinstance(observed, str) or "<hierarchy" not in observed:
            return {
                "ready": False, "code": "uiautomator_rpc_invalid_response",
                "message": "UiAutomator2 RPC 已响应，但未返回有效的控件树",
                "stage": "rpc_response_validation",
                "diagnostics": {
                    "helper": helper,
                    "probe": {"method": "dumpWindowHierarchy", "max_depth": 2,
                              "result_type": type(observed).__name__,
                              "hierarchy_marker_present": isinstance(observed, str) and "<hierarchy" in observed},
                },
            }
        return {
            "ready": True, "code": None, "message": "UiAutomator2 控件树 RPC 探测成功",
            "stage": "complete",
            "diagnostics": {
                "helper": helper,
                "probe": {"method": "dumpWindowHierarchy", "max_depth": 2,
                          "result_type": type(observed).__name__, "hierarchy_length": len(observed)},
            },
        }

    async def _automation_rpc_ready(self, target):
        """Boolean compatibility wrapper for script admission and readiness checks."""
        result = await self._automation_rpc_probe(target)
        return result["ready"]

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
            await self._claim_preparation_after_manual(target, holder)
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

    async def verify_locator(self, body):
        """Read-only XPath verification for the last manually selected node."""
        validation.object_fields(body, {"target", "xpath", "expected_bounds", "width", "height", "preview_only", "diagnose"},
                                 {"target", "xpath", "width", "height"})
        preview = body.get("preview_only", False)
        diagnose = body.get("diagnose", False)
        if type(diagnose) is not bool or (diagnose and not preview):
            raise ExecutorError("selector_invalid", "只能在只读预览中获取控件对比列表", 422)
        if type(preview) is not bool or (preview and "expected_bounds" in body) or (
                not preview and "expected_bounds" not in body):
            raise ExecutorError("selector_invalid", "预览模式与目标区域参数不匹配", 422)
        target = validation.target(body["target"])
        self.journal.ensure_target(target)
        expression = body["xpath"]
        if not isinstance(expression, str) or not expression.startswith(("/", "(")) or len(expression.encode("utf-8")) > 512:
            raise ExecutorError("xpath_invalid", "关联定位 XPath 不合法", 422)
        expected = body.get("expected_bounds")
        if not preview and (not isinstance(expected, dict) or set(expected) != {"left", "top", "right", "bottom"} or any(
                type(expected.get(k)) is not int or expected[k] < 0 or expected[k] > 16384
                for k in expected)):
            raise ExecutorError("selector_geometry_invalid", "目标控件区域无效", 422)
        width, height = body["width"], body["height"]
        if type(width) is not int or type(height) is not int or not 1 <= width <= 16384 or not 1 <= height <= 16384:
            raise ExecutorError("selector_geometry_invalid", "设备画面尺寸无效", 422)
        if self.shutting_down:
            raise ExecutorError("executor_stopping", "执行器正在停止", 503)
        if not self.prepared(target):
            raise ExecutorError("automation_not_prepared", "请先初始化手机的 UiAutomator2", 409)
        await adb.require_connected(self.settings.adb_binary, target)
        return await execute_step(
            self.journal, "locator-verify:" + str(uuid4()), target,
            {"id": str(uuid4()), "type": "locator_verify", "xpath": expression,
             "expected_bounds": expected, "width": width, "height": height,
             "preview_only": preview, "diagnose": diagnose, "timeout_ms": 20000}, {}, asyncio.Event(), self.settings.adb_binary)

    async def automation_status(self, body):
        validation.object_fields(body, {"target"}, {"target"})
        target = validation.target(body["target"])
        self.journal.ensure_target(target)
        device_id = target["device_id"]
        pending = self.journal.active_preparation(device_id)
        if pending:
            return {
                "device_id": device_id, "state": "preparing",
                "code": pending.get("code") or "automation_preparing",
                "message": pending.get("message") or "控件初始化任务正在执行",
                "preparation_id": pending["id"], "stage": pending["stage"],
                "diagnostics": {
                    "preparation_state": pending.get("state"),
                    "preparation_stage": pending.get("stage"),
                },
                "executor_id": self.journal.executor_id,
            }
        try:
            await adb.require_connected(self.settings.adb_binary, target)
        except ExecutorError as exc:
            return {
                "device_id": device_id, "state": "adb_unavailable",
                "code": exc.code, "message": exc.message, "stage": "adb_connection",
                "diagnostics": {"exception_type": type(exc).__name__},
                "executor_id": self.journal.executor_id,
            }
        except Exception as exc:
            return {
                "device_id": device_id, "state": "adb_unavailable",
                "code": "adb_status_probe_failed", "message": "检查 ADB 连接时发生未分类异常",
                "stage": "adb_connection",
                "diagnostics": {"exception_type": type(exc).__name__,
                                "exception_message": self._automation_probe_exception_message(exc, target)},
                "executor_id": self.journal.executor_id,
            }

        # This endpoint is strictly read-only. Return the actual reason and
        # probe stage; never silently collapse an RPC exception into unavailable.
        probe = await self._automation_rpc_probe(target)
        state = "ready" if probe["ready"] else (
            "not_prepared" if probe["stage"] == "helper_state" else "unavailable"
        )
        return {
            "device_id": device_id, "state": state,
            "code": probe["code"], "message": probe["message"],
            "stage": probe["stage"], "diagnostics": probe["diagnostics"],
            "executor_id": self.journal.executor_id,
        }

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
            if current.view_only:
                raise ExecutorError("viewer_only", "当前会话已移交为只读观看，禁止解锁手机", 409)
            # Same ownership/ADB endpoint arbitration as other manual commands.
            self.journal.claim(current.target, "session:" + sid, "manual")
            result = await self.media.unlock(sid, secret)
            return {**result, "executor_id": self.journal.executor_id}

    async def view_only(self, sid, body):
        """Downgrade a single known media session without closing its WebRTC.

        The main service must fence new commands in its database *before*
        calling this method. This request alone never admits a script run.
        """
        validation.object_fields(body, {"expected_executor_id"}, {"expected_executor_id"})
        self.require_identity(body["expected_executor_id"])
        current = self.session(sid)
        # Signal a possibly running text worker before waiting on the
        # session's command lock; never overlap a new device action.
        current.cancel.set()
        async with current.lock:
            self.session(sid)
            if current.view_only_confirmed:
                # Only a previously confirmed manual-claim release can be
                # acknowledged idempotently; a failed first cleanup must
                # never be upgraded to successful viewer admission.
                return {"id": sid, "executor_id": self.journal.executor_id,
                        "device_id": current.target["device_id"], "state": "view_only"}
            # Fence all further commands, even when cleanup fails/gets an
            # unknown response. Retry does not grant a new write privilege.
            current.view_only = True
            holder = "session:" + sid
            control = self.journal.control(current.target)
            if control and (control["holder"] != holder or control["kind"] != "manual"
                            or control["state"] != "active"):
                raise ExecutorError(
                    "cleanup_pending" if control["state"] == "cleanup_pending" else "device_busy",
                    "设备动作权当前由其它任务占用或仍待清理，拒绝迁移", 409)
            if control:
                # The last owned finger must be released before releasing the
                # journal's write lease. Media/source itself remains alive.
                await self.media.command(sid, {"type": "touch.release"})
                record = self.journal.worker_record(holder)
                cleaned = await asyncio.to_thread(cleanup_record, record) if record else True
                self.journal.release(holder, clean=cleaned)
                if not cleaned:
                    raise ExecutorError("cleanup_pending", "手动文字输入工作进程清理未确认，禁止接管动作权", 409)
            current.view_only_confirmed = True
            return {"id": sid, "executor_id": self.journal.executor_id,
                    "device_id": current.target["device_id"], "state": "view_only"}

    async def resume_manual(self, sid, body):
        """Resume manual eligibility only after Rust fenced new script runs.

        This does NOT seize a device action claim. The next actual command
        must still compete for Journal.claim under current fingerprint.
        """
        validation.object_fields(body, {"expected_executor_id"}, {"expected_executor_id"})
        self.require_identity(body["expected_executor_id"])
        current = self.session(sid)
        async with current.lock:
            self.session(sid)
            status = self.media.status(sid)
            if status.get("state") in ("closed", "failed", "cleanup_pending") or status.get("cleanup_pending"):
                raise ExecutorError("media_closed", "原实时画面已经中断或待清理，不能恢复手控", 409)
            control = self.journal.control(current.target)
            if control and (control["holder"] != "session:" + sid or control["kind"] != "manual"
                            or control["state"] != "active"):
                raise ExecutorError("cleanup_pending" if control["state"] == "cleanup_pending" else "device_busy",
                                    "脚本或其它会话仍在控制手机，不能恢复手控", 409)
            # A normal viewer has no write claim. If this is an idempotent
            # repeat of a completed restore, its own manual claim is valid.
            current.view_only = False
            current.view_only_confirmed = False
            return {"id": sid, "executor_id": self.journal.executor_id,
                    "device_id": current.target["device_id"], "state": "active"}

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
            if current.view_only:
                raise ExecutorError("viewer_only", "当前投屏已降为只读，不能执行任何手动动作", 409)
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

    async def _claim_preparation_after_manual(self, target, preparation_holder):
        """Take the prepare lease after releasing an existing manual gesture.

        Keep the live preview and floating manual switch unchanged. Only the
        physical write lease is handed over, using the same confirmation rules
        as a script run.
        """
        control = self.journal.control(target)
        if not control or control["kind"] != "manual":
            self.journal.claim(target, preparation_holder, "prepare")
            return

        manual_holder = control["holder"]
        if not manual_holder.startswith("session:") or control["state"] != "active":
            code = "cleanup_pending" if control["state"] == "cleanup_pending" else "device_busy"
            raise ExecutorError(code, "手机仍有未确认的动作或清理任务，不能开始控件准备")
        sid = manual_holder[len("session:"):]
        viewer = self.sessions.get(sid)
        if not viewer or viewer.closing or any(
            viewer.target.get(key) != target.get(key)
            for key in ("owner_user_id", "device_id", "target_fingerprint", "ip", "adb_port")
        ):
            raise ExecutorError("device_busy", "当前手控会话与准备目标不一致，不能接管动作权")

        async with viewer.lock:
            self.session(sid)
            current = self.journal.control(target)
            if current is None:
                self.journal.claim(target, preparation_holder, "prepare")
                return
            if (current["holder"] != manual_holder or current["kind"] != "manual"
                    or current["state"] != "active" or viewer.closing):
                code = "cleanup_pending" if current["state"] == "cleanup_pending" else "device_busy"
                raise ExecutorError(code, "手机动作权正在变化，不能开始控件准备")
            record = self.journal.worker_record(manual_holder)
            if record:
                cleaned = await asyncio.to_thread(cleanup_record, record)
                if not cleaned:
                    self.journal.release(manual_holder, clean=False)
                    raise ExecutorError("cleanup_pending", "手控输入进程清理未确认，控件准备未启动")
            await self.media.command(sid, {"type": "touch.release"})
            self.journal.release(manual_holder, clean=True)
            self.journal.claim(target, preparation_holder, "prepare")

    async def _claim_run_after_manual(self, target, script_holder):
        """Release the prior manual gesture lease, not the user's manual switch.

        Run admission has already fenced further manual WS commands in the
        main DB. Under the *same* session command lock used for scrcpy,
        release any old finger/worker before claiming this run. If we cannot
        prove cleanup, do not start a script action or close its video.
        """
        control = self.journal.control(target)
        if not control or control["kind"] != "manual":
            self.journal.claim(target, script_holder, "script")
            return
        manual_holder = control["holder"]
        if not manual_holder.startswith("session:") or control["state"] != "active":
            raise ExecutorError("device_busy", "旧控制权不属于有效的手机手控会话，禁止抢占", 409)
        sid = manual_holder[len("session:"):]
        viewer = self.sessions.get(sid)
        if not viewer or viewer.closing or any(
            viewer.target.get(key) != target.get(key)
            for key in ("owner_user_id", "device_id", "target_fingerprint", "ip", "adb_port")
        ):
            raise ExecutorError("device_busy", "旧手控会话与当前任务目标不符，禁止抢占", 409)
        async with viewer.lock:
            current = self.journal.control(target)
            if current is None:
                self.journal.claim(target, script_holder, "script")
                return
            if (current["holder"] != manual_holder or current["kind"] != "manual"
                    or current["state"] != "active" or viewer.closing):
                raise ExecutorError("device_busy", "手控动作租约正在变化，不能交给脚本", 409)
            record = self.journal.worker_record(manual_holder)
            if record:
                cleaned = await asyncio.to_thread(cleanup_record, record)
                if not cleaned:
                    self.journal.release(manual_holder, clean=False)
                    raise ExecutorError("cleanup_pending", "手控输入工作进程清理未确认，脚本未接管", 409)
            # Scrcpy touch-up is safe only while we still hold its exact
            # manual lease. A failed release MUST keep the claim as occupied.
            await self.media.command(sid, {"type": "touch.release"})
            self.journal.release(manual_holder, clean=True)
            self.journal.claim(target, script_holder, "script")

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
            # First authenticate this exact run and target BEFORE touching
            # the physical device (even before releasing a former gesture).
            await self._authorize(rid, target)
            # Stable manual-enabled video can coexist with this run. Only
            # its old physical gesture lease is released; viewerOnly stays
            # exactly as the floating icon selected.
            await self._claim_run_after_manual(target, holder)
            # If a valid helper is not running, initialize it only under
            # this run's exclusive journal action claim.
            await adb.require_connected(self.settings.adb_binary, target)
            # HOME/BACK/RECENT/WAKE and launching an app are native ADB
            # operations. Don't start/install the heavy UiAutomator helper
            # merely to execute these steps. Selector/touch/text/screenshot
            # scripts still require the authenticated helper.
            needs_ui = any(step["type"] not in ("key", "launch_app") for step in body["steps"])
            if needs_ui and not await self._automation_rpc_ready(target):
                # A helper process can remain alive while the phone-side RPC
                # server has died. Rebuild it before the first action, never
                # discover this by sending a click to a broken service.
                await self._update(rid, did, {"state": "starting", "steps": results})
                await self._prepare_automation_helper(target)
                if not self.prepared(target) or not await self._automation_rpc_ready(target):
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

    async def device_task_status(self, did, body):
        """Read-only inventory of unresolved executor work for one physical phone."""
        did = validation.identifier(did)
        validation.object_fields(body, {"target", "expected_executor_id"},
                                  {"target", "expected_executor_id"})
        self.require_identity(body["expected_executor_id"])
        target = validation.target(body["target"])
        if target["device_id"] != did:
            raise ExecutorError("target_mismatch", "任务查询目标与指定手机不一致", 409)
        endpoint = validation.endpoint(target)

        def same(saved):
            return saved.get("device_id") == did or validation.endpoint(saved) == endpoint

        tasks = []
        # outcome_unknown is terminal for scheduling but remains a cleanup
        # blocker until cleanup_confirmed is durably recorded.
        pending_states = {
            "queued", "accepted", "starting", "running", "cancel_requested",
            "cleanup_pending", "outcome_unknown", "interrupted",
        }
        for row in list(self.journal.db.execute("SELECT run_id,device_id,target FROM run_targets")):
            try:
                saved_target = json.loads(row["target"])
                if not same(saved_target):
                    continue
            except (ValueError, TypeError, KeyError, ExecutorError):
                continue
            run_id, saved_device_id = row["run_id"], row["device_id"]
            run_row = self.journal.db.execute(
                "SELECT result FROM runs WHERE id=?", (run_id,)).fetchone()
            if not run_row:
                continue
            try:
                result = json.loads(run_row["result"])
                device_result = next(
                    item for item in result.get("devices", [])
                    if item.get("device_id") == saved_device_id
                )
            except (ValueError, TypeError, StopIteration):
                continue
            state = device_result.get("state", result.get("state", "unknown"))
            cleanup_confirmed = device_result.get("cleanup_confirmed") is True
            holder = "run:" + run_id + ":" + saved_device_id
            worker_present = self.journal.worker_record(holder) is not None
            running_task = self.device_tasks.get((run_id, saved_device_id))
            task_running = bool(running_task and not running_task.done())
            if worker_present or task_running or (state in pending_states and not cleanup_confirmed):
                tasks.append({
                    "kind": "script",
                    "run_id": run_id,
                    "device_id": saved_device_id,
                    "state": state,
                    "cleanup_confirmed": cleanup_confirmed,
                    "worker_present": worker_present,
                    "task_running": task_running,
                    "target_fingerprint_matches": saved_target.get("target_fingerprint") == target.get("target_fingerprint"),
                })

        # Initialization/preparation jobs remain visible even if their original
        # process no longer exists after an executor restart.
        for row in self.journal.db.execute(
                "SELECT id,target,state,stage FROM preparation_jobs "
                "WHERE state IN ('queued','running','cancel_requested')"):
            try:
                saved_target = json.loads(row["target"])
                if not same(saved_target):
                    continue
            except (ValueError, TypeError, KeyError, ExecutorError):
                continue
            tasks.append({
                "kind": "preparation",
                "task_id": row["id"],
                "device_id": saved_target["device_id"],
                "state": row["state"],
                "stage": row["stage"],
            })

        return {
            "device_id": did,
            "executor_id": self.journal.executor_id,
            "state": "ready",
            "has_tasks": bool(tasks),
            "task_count": len(tasks),
            "tasks": tasks,
            "checked_at": now(),
        }

    async def force_cleanup_device(self, did, body):
        """Explicit, device-wide stop: every owner/session/script/helper for this phone.

        Never just delete leases. Persist fences before cancellation, terminate
        only recorded process identities, and report unresolved cleanup honestly.
        """
        did = validation.identifier(did)
        validation.object_fields(body, {"target", "expected_executor_id", "event_id"},
                                 {"target", "expected_executor_id", "event_id"})
        self.require_identity(body["expected_executor_id"])
        validation.identifier(body["event_id"])
        target = validation.target(body["target"])
        if target["device_id"] != did:
            raise ExecutorError("target_mismatch", "强制清理目标与指定手机不一致", 409)
        endpoint = validation.endpoint(target)
        # The button is explicitly device-wide: device ID or physical ADB
        # endpoint wins over account ownership, matching the server admission
        # fence. A reused endpoint cannot be safely treated as a separate phone.
        same = lambda value: (
            value["device_id"] == did or validation.endpoint(value) == endpoint
        )
        import json
        async with self.admission:
            related_targets = {(did, target["target_fingerprint"]): target}
            # Fence every historical run on this physical phone, regardless
            # of which account originally created the run.
            old_runs = []
            for row in list(self.journal.db.execute("SELECT run_id,device_id,target FROM run_targets")):
                old = json.loads(row["target"])
                if same(old):
                    related_targets[(old["device_id"], old["target_fingerprint"])] = old
                    self.journal.fence_device_run_only(
                        row["run_id"], row["device_id"], old["target_fingerprint"], body["event_id"])
                    old_runs.append((row["run_id"], row["device_id"]))
            pending = []
            for run_id, device_id in old_runs:
                flag = self.device_cancellations.get((run_id,device_id))
                if flag: flag.set()
                task = self.device_tasks.get((run_id,device_id))
                if task and not task.done():
                    task.cancel()
                    pending.append(task)
            for key, task in list(self.preparations.items()):
                if key[0] == did and not task.done():
                    task.cancel()
                    pending.append(task)
            for pid, task in list(self.preparation_tasks.items()):
                row = self.journal.db.execute(
                    "SELECT target FROM preparation_jobs WHERE id=?", (pid,)).fetchone()
                if row:
                    old_target = json.loads(row["target"])
                    if same(old_target):
                        related_targets[(old_target["device_id"], old_target["target_fingerprint"])] = old_target
                        if not task.done():
                            task.cancel()
                            pending.append(task)
            if pending:
                await asyncio.wait(pending,timeout=9)
            active = any(not task.done() for task in pending)
            # Close any viewing session for this physical endpoint, even if
            # an older browser opened it. This intentionally ends its video.
            sessions = []
            for sid, item in list(self.sessions.items()):
                if same(item.target):
                    related_targets[(item.target["device_id"], item.target["target_fingerprint"])] = item.target
                    sessions.append(sid)
                    try:
                        reply = await asyncio.wait_for(self.close_session(sid), timeout=9)
                        active |= bool(reply.get("cleanup_pending"))
                    except Exception:
                        active = True
            # A process restart can leave journal rows without an in-memory
            # WebRTC object. Include them in the cleanup receipt and worker sweep.
            for row in self.journal.db.execute("SELECT id,target FROM sessions WHERE state NOT IN ('closed','failed')"):
                try:
                    old_target = json.loads(row["target"])
                except (ValueError, TypeError):
                    active = True
                    continue
                if same(old_target):
                    related_targets[(old_target["device_id"], old_target["target_fingerprint"])] = old_target
                    if row["id"] not in sessions:
                        sessions.append(row["id"])
            # Stop remembered UIA helpers regardless of which script started them.
            # If graceful close fails, the recorded process identity is checked
            # again below. A later confirmed reap must clear this transient failure.
            helpers_needing_reap = {}
            for helper_id, helper in list(self.helpers.items()):
                if same(helper.target):
                    related_targets[(helper.target["device_id"], helper.target["target_fingerprint"])] = helper.target
                    try:
                        stopped = await asyncio.wait_for(helper.close(), timeout=8)
                    except Exception:
                        stopped = False
                    if stopped:
                        self.helpers.pop(helper_id,None)
                    else:
                        helpers_needing_reap[helper_id] = helper
            # Match journal claims by original saved target; a reused IP on
            # another phone must never be stopped by device-id-only inference.
            controls = []
            for row in list(self.journal.db.execute("SELECT * FROM controls")):
                try:
                    old = json.loads(row["target"])
                except (ValueError, TypeError):
                    active = True
                    continue
                if same(old):
                    related_targets[(old["device_id"], old["target_fingerprint"])] = old
                    controls.append(dict(row))
            holders = {c["holder"] for c in controls}
            for run_id, device_id in old_runs:
                holders.add("run:" + run_id + ":" + device_id)
            # Helpers can outlive the current device ID when the same physical
            # ADB endpoint was previously registered under another account.
            holders.update("helper:" + device_id for device_id, _ in related_targets)
            holders.update("session:" + sid for sid in sessions)
            # Include stranded workers even where a prior crash lost the control.
            for row in self.journal.db.execute("SELECT holder FROM workers"):
                holder = row["holder"]
                if holder in holders: continue
                if holder.startswith("run:") and any(holder=="run:"+rid+":"+d for rid,d in old_runs):
                    holders.add(holder)
            cleaned = 0
            for holder in holders:
                record = self.journal.worker_record(holder)
                stopped = False
                for _ in range(4):
                    if await asyncio.to_thread(cleanup_record, record):
                        stopped=True
                        break
                    await asyncio.sleep(.15)
                if stopped:
                    self.journal.release(holder,clean=True)
                    cleaned+=1
                else:
                    active=True
            # Reconcile helper objects after final process-identity cleanup.
            for helper_id, helper in helpers_needing_reap.items():
                try:
                    still_alive = helper.alive()
                except Exception:
                    still_alive = True
                if still_alive:
                    active = True
                else:
                    self.helpers.pop(helper_id, None)
            # A preparation's control record may be present even if the task
            # has already disappeared from memory.
            for record in controls:
                if record["holder"] not in holders:
                    active=True
            if not active:
                # Record that every affected run-device lease was physically
                # reaped. Preserve unknown action outcomes instead of claiming
                # that an interrupted action succeeded.
                for run_id, related_device in old_runs:
                    row = self.journal.db.execute(
                        "SELECT result FROM runs WHERE id=?", (run_id,)).fetchone()
                    if not row:
                        continue
                    try:
                        saved_result = json.loads(row["result"])
                        saved_device = next(
                            item for item in saved_result.get("devices", [])
                            if item.get("device_id") == related_device
                        )
                        steps = []
                        has_unknown = False
                        for step in saved_device.get("steps", []):
                            state = step.get("state")
                            if state == "running":
                                state = "outcome_unknown"
                                has_unknown = True
                            elif state in ("pending", "queued"):
                                state = "cancelled"
                            elif state == "outcome_unknown":
                                has_unknown = True
                            steps.append({**step, "state": state})
                        values = {"cleanup_confirmed": True}
                        if saved_device.get("state") not in TERMINAL:
                            values.update({
                                "state": "outcome_unknown" if has_unknown else "cancelled",
                                "steps": steps,
                                "error": {
                                    "code": "forced_cleanup",
                                    "message": "设备任务已被强制清理；未完成动作结果不再自动重放",
                                },
                            })
                        elif steps != saved_device.get("steps", []):
                            values["steps"] = steps
                        await self._update(run_id, related_device, values)
                    except (ValueError, TypeError, StopIteration, ExecutorError):
                        active = True
                        break
                    except Exception:
                        active = True
                        break
            if not active:
                with self.journal.transaction() as db:
                    for related_device, fingerprint in related_targets:
                        db.execute("DELETE FROM prepared WHERE device_id=? AND fingerprint=?",
                                   (related_device, fingerprint))
                        # Clear only non-retirement refusals for each exact
                        # target proven clean; never revive a retired target.
                        db.execute(
                            "DELETE FROM target_refusals WHERE device_id=? AND fingerprint=? "
                            "AND NOT EXISTS(SELECT 1 FROM retirements r WHERE "
                            "r.device_id=? AND r.fingerprint=?)",
                            (related_device, fingerprint, related_device, fingerprint))
                    db.execute("UPDATE sessions SET state='closed',updated_at=? "
                               "WHERE id IN (" + ",".join("?" for _ in sessions) + ")",
                               (now(), *sessions)) if sessions else None
                    for related_device, _ in related_targets:
                        db.execute("UPDATE preparation_jobs SET state='interrupted',"
                                   "stage='interrupted',code='forced_cleanup',"
                                   "message='用户强制清理本机占用',updated_at=? "
                                   "WHERE device_id=? AND state IN ('queued','running','cancel_requested')",
                                   (now(), related_device))
            return {"device_id":did,"executor_id":self.journal.executor_id,
                    "event_id":body["event_id"],"cleanup_confirmed":not active,
                    "stopped_sessions":len(sessions),"fenced_run_devices":len(old_runs),
                    "cleaned_workers":cleaned,
                    "message":"本机全部自动化占用已清理，请重新打开实时画面" if not active
                              else "仍有工作进程或设备会话未确认结束；已发停止请求，请检查执行器"}

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
