"""Small durable execution journal; never a second editable asset database."""
from contextlib import contextmanager
from datetime import datetime, timezone
import json
import sqlite3
import threading
from uuid import uuid4

from .errors import ExecutorError
from .validation import digest, endpoint


def now():
    return datetime.now(timezone.utc).isoformat()


def encode(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


TERMINAL = {"completed", "succeeded", "failed", "partial_failed", "cancelled", "interrupted", "outcome_unknown"}


class Journal:
    def __init__(self, path):
        self._lock = threading.RLock()
        self.db = sqlite3.connect(str(path), check_same_thread=False, isolation_level=None, timeout=5)
        self.db.row_factory = sqlite3.Row
        self.db.execute("PRAGMA journal_mode=WAL")
        self.db.execute("PRAGMA synchronous=FULL")
        self.db.executescript("""
            CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY, request_digest TEXT NOT NULL, result TEXT NOT NULL,
                cancelled INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS refusals (id TEXT PRIMARY KEY, created_at TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY, target TEXT NOT NULL, state TEXT NOT NULL,
                generation TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS closed_sessions (id TEXT PRIMARY KEY, created_at TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS controls (
                endpoint TEXT PRIMARY KEY, device_id TEXT NOT NULL UNIQUE, holder TEXT NOT NULL,
                kind TEXT NOT NULL, state TEXT NOT NULL, target TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS prepared (
                device_id TEXT PRIMARY KEY, fingerprint TEXT NOT NULL, prepared_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS events (
                event_id TEXT PRIMARY KEY, run_id TEXT NOT NULL, result TEXT NOT NULL,
                delivered INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY, run_id TEXT NOT NULL, device_id TEXT NOT NULL,
                step_id TEXT NOT NULL, path TEXT NOT NULL, size INTEGER NOT NULL, created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS device_refusals (
                run_id TEXT NOT NULL, device_id TEXT NOT NULL, fingerprint TEXT NOT NULL,
                event_id TEXT NOT NULL, PRIMARY KEY(run_id,device_id)
            );
            CREATE TABLE IF NOT EXISTS target_refusals (
                device_id TEXT NOT NULL, fingerprint TEXT NOT NULL, event_id TEXT NOT NULL,
                PRIMARY KEY(device_id,fingerprint)
            );
            CREATE TABLE IF NOT EXISTS run_targets (
                run_id TEXT NOT NULL, device_id TEXT NOT NULL, target TEXT NOT NULL,
                PRIMARY KEY(run_id,device_id)
            );
            CREATE TABLE IF NOT EXISTS retirements (
                event_id TEXT NOT NULL, device_id TEXT NOT NULL, fingerprint TEXT NOT NULL,
                cleanup_confirmed INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY(event_id,device_id,fingerprint)
            );
            CREATE TABLE IF NOT EXISTS workers (
                holder TEXT PRIMARY KEY, pid INTEGER NOT NULL, start_tick TEXT,
                state TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS preparation_jobs (
                id TEXT PRIMARY KEY, request_id TEXT NOT NULL UNIQUE, device_id TEXT NOT NULL,
                owner_user_id INTEGER NOT NULL, target_fingerprint TEXT NOT NULL,
                target TEXT NOT NULL, state TEXT NOT NULL, stage TEXT NOT NULL,
                code TEXT, message TEXT, executor_generation TEXT NOT NULL,
                created_at TEXT NOT NULL, updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS preparation_by_device ON preparation_jobs(device_id,created_at DESC);
            CREATE TABLE IF NOT EXISTS preparation_events (
                preparation_id TEXT NOT NULL, seq INTEGER NOT NULL, at TEXT NOT NULL,
                level TEXT NOT NULL, stage TEXT NOT NULL, message TEXT NOT NULL,
                PRIMARY KEY(preparation_id,seq)
            );
        """)
        self.db.execute("INSERT OR IGNORE INTO metadata VALUES ('executor_id',?)", (str(uuid4()),))
        self.executor_id = self.db.execute("SELECT value FROM metadata WHERE key='executor_id'").fetchone()[0]
        self.generation = str(uuid4())

    @contextmanager
    def transaction(self):
        with self._lock:
            self.db.execute("BEGIN IMMEDIATE")
            try:
                yield self.db
                self.db.execute("COMMIT")
            except BaseException:
                self.db.execute("ROLLBACK")
                raise

    def recover(self):
        """Record uncertainty on startup, never dispatch or reconnect old work."""
        with self.transaction() as db:
            for row in db.execute("SELECT id,result FROM runs"):
                result = json.loads(row["result"])
                if result["state"] in TERMINAL:
                    continue
                result["state"] = "interrupted"
                result["updated_at"] = now()
                result["revision"] = result.get("revision", 0) + 1
                for device in result["devices"]:
                    if device["state"] not in TERMINAL:
                        device["state"] = "outcome_unknown" if device["state"] in ("running", "starting") else "interrupted"
                        device["error"] = {"code": "executor_restarted", "message": "执行器已重启，未自动恢复动作"}
                        for step in device["steps"]:
                            if step["state"] == "running":
                                step["state"] = "outcome_unknown"
                db.execute("UPDATE runs SET result=?,cancelled=1,updated_at=? WHERE id=?", (encode(result), now(), row["id"]))
                db.execute("INSERT OR IGNORE INTO refusals VALUES (?,?)", (row["id"], now()))
            db.execute("UPDATE sessions SET state='interrupted',updated_at=? WHERE state NOT IN ('closed','failed')", (now(),))
            db.execute("UPDATE controls SET state='cleanup_pending'")
            db.execute("UPDATE workers SET state='cleanup_pending' WHERE state='active'")
            for row in db.execute("SELECT id FROM preparation_jobs WHERE state IN ('queued','running','cancel_requested')").fetchall():
                stamp = now()
                db.execute("UPDATE preparation_jobs SET state='interrupted',stage='interrupted',code='executor_restarted',message='执行器重启，准备结果未确认',updated_at=? WHERE id=?", (stamp,row["id"]))
                seq = db.execute("SELECT COALESCE(MAX(seq),0)+1 FROM preparation_events WHERE preparation_id=?", (row["id"],)).fetchone()[0]
                db.execute("INSERT INTO preparation_events VALUES (?,?,?,?,?,?)",
                           (row["id"],seq,stamp,"warning","interrupted","执行器重启，任务未自动恢复"))

    def close(self):
        with self._lock:
            self.db.close()

    def session(self, sid):
        row = self.db.execute("SELECT * FROM sessions WHERE id=?", (sid,)).fetchone()
        return dict(row) if row else None

    def create_session(self, sid, target):
        self.ensure_target(target)
        with self.transaction() as db:
            if db.execute("SELECT 1 FROM closed_sessions WHERE id=?", (sid,)).fetchone():
                raise ExecutorError("session_closed", "该会话已关闭，不接受迟到的创建请求")
            if db.execute("SELECT 1 FROM sessions WHERE id=?", (sid,)).fetchone():
                raise ExecutorError("session_already_used", "会话标识已使用，请重新打开画面")
            db.execute("INSERT INTO sessions VALUES (?,?,?,?,?,?)", (sid, encode(target), "opening", self.generation, now(), now()))

    def session_state(self, sid, state):
        with self.transaction() as db:
            db.execute("UPDATE sessions SET state=?,updated_at=? WHERE id=?", (state, now(), sid))

    def refuse_session(self, sid):
        with self.transaction() as db:
            db.execute("INSERT OR IGNORE INTO closed_sessions VALUES (?,?)", (sid, now()))

    def claim(self, target, holder, kind):
        self.ensure_target(target)
        with self.transaction() as db:
            row = db.execute("SELECT * FROM controls WHERE endpoint=? OR device_id=?", (endpoint(target), target["device_id"])).fetchone()
            if row:
                if row["holder"] == holder and row["state"] == "active":
                    return
                raise ExecutorError("cleanup_pending" if row["state"] == "cleanup_pending" else "device_busy",
                                    "设备仍待清理" if row["state"] == "cleanup_pending" else "设备正由其他会话或任务控制")
            db.execute("INSERT INTO controls VALUES (?,?,?,?,?,?)", (endpoint(target), target["device_id"], holder, kind, "active", encode(target)))

    def control(self, target):
        row = self.db.execute("SELECT * FROM controls WHERE endpoint=? OR device_id=?", (endpoint(target), target["device_id"])).fetchone()
        return dict(row) if row else None

    def release(self, holder, clean=True):
        with self.transaction() as db:
            if clean:
                db.execute("DELETE FROM controls WHERE holder=?", (holder,))
                db.execute("DELETE FROM workers WHERE holder=?", (holder,))
            else:
                db.execute("UPDATE controls SET state='cleanup_pending' WHERE holder=?", (holder,))

    def prepared(self, target):
        row = self.db.execute("SELECT fingerprint FROM prepared WHERE device_id=?", (target["device_id"],)).fetchone()
        return bool(row and row["fingerprint"] == target["target_fingerprint"])

    def mark_prepared(self, target):
        with self.transaction() as db:
            db.execute("INSERT OR REPLACE INTO prepared VALUES (?,?,?)", (target["device_id"], target["target_fingerprint"], now()))

    def preparation(self, pid):
        row = self.db.execute("SELECT * FROM preparation_jobs WHERE id=?", (pid,)).fetchone()
        if not row:
            raise ExecutorError("preparation_not_found", "初始化记录不存在", 404)
        value = dict(row)
        value.pop("target", None)
        return value

    def preparation_by_request(self, request_id):
        row = self.db.execute("SELECT id FROM preparation_jobs WHERE request_id=?", (request_id,)).fetchone()
        return self.preparation(row[0]) if row else None

    def active_preparation(self, did):
        row = self.db.execute("SELECT id FROM preparation_jobs WHERE device_id=? AND state IN ('queued','running','cancel_requested') ORDER BY created_at DESC LIMIT 1", (did,)).fetchone()
        return self.preparation(row[0]) if row else None

    def create_preparation(self, pid, request_id, target):
        stamp = now()
        with self.transaction() as db:
            db.execute("INSERT INTO preparation_jobs VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
                       (pid,request_id,target["device_id"],target["owner_user_id"],target["target_fingerprint"],
                        encode(target),"queued","queued",None,None,self.generation,stamp,stamp))
            db.execute("INSERT INTO preparation_events VALUES (?,?,?,?,?,?)",
                       (pid,1,stamp,"info","queued","预处理任务已接收"))
        return self.preparation(pid)

    def preparation_update(self, pid, state, stage, code=None, message=None):
        with self.transaction() as db:
            db.execute("UPDATE preparation_jobs SET state=?,stage=?,code=?,message=?,updated_at=? WHERE id=?",
                       (state,stage,code,message,now(),pid))

    def preparation_event(self, pid, level, stage, message):
        with self.transaction() as db:
            seq = db.execute("SELECT COALESCE(MAX(seq),0)+1 FROM preparation_events WHERE preparation_id=?", (pid,)).fetchone()[0]
            db.execute("INSERT INTO preparation_events VALUES (?,?,?,?,?,?)",
                       (pid,seq,now(),level,stage,message))

    def preparation_history(self, did, owner):
        rows = self.db.execute("SELECT id FROM preparation_jobs WHERE device_id=? AND owner_user_id=? ORDER BY created_at DESC LIMIT 20", (did,owner)).fetchall()
        return [self.preparation(row[0]) for row in rows]

    def preparation_logs(self, pid, after_seq=0):
        self.preparation(pid)
        return [dict(row) for row in self.db.execute(
            "SELECT seq,at,level,stage,message FROM preparation_events WHERE preparation_id=? AND seq>? ORDER BY seq ASC LIMIT 200",
            (pid,after_seq)).fetchall()]

    def accept_run(self, body, *, allow_new=True):
        request_digest = digest(body)
        with self.transaction() as db:
            old = db.execute("SELECT request_digest,result FROM runs WHERE id=?", (body["id"],)).fetchone()
            if old:
                if old["request_digest"] != request_digest:
                    raise ExecutorError("idempotency_conflict", "任务标识与原请求不一致")
                return json.loads(old["result"]), False
            if db.execute("SELECT 1 FROM refusals WHERE id=?", (body["id"],)).fetchone():
                raise ExecutorError("run_cancelled", "该任务已取消，不接受迟到请求")
            if not allow_new:
                raise ExecutorError("executor_capacity", "执行器任务容量已满，请稍后手动提交", 429)
            stamp = now()
            # No execution parameters/text are copied to this journal or emitted in callbacks.
            result = {"id": body["id"], "executor_id": self.executor_id, "revision": 0, "name": body["name"], "state": "queued", "created_at": stamp,
                      "updated_at": stamp, "script_snapshot": {"schema_version": 1, "step_ids": [s["id"] for s in body["steps"]]},
                      "devices": [{"device_id": t["device_id"], "name": t["name"], "state": "queued", "current_step": None,
                                   "steps": [{"step_id": s["id"], "state": "pending"} for s in body["steps"]]} for t in body["targets"]]}
            db.execute("INSERT INTO runs VALUES (?,?,?,0,?,?)", (body["id"], request_digest, encode(result), stamp, stamp))
            for target in body["targets"]:
                db.execute("INSERT INTO run_targets VALUES (?,?,?)", (body["id"], target["device_id"], encode(target)))
            return result, True

    def run(self, rid):
        row = self.db.execute("SELECT result FROM runs WHERE id=?", (rid,)).fetchone()
        if not row:
            raise ExecutorError("run_not_found", "任务记录不存在；这不代表迟到请求已被取消", 404)
        return json.loads(row["result"])

    def cancelled(self, rid):
        return bool(self.db.execute("SELECT 1 FROM refusals WHERE id=?", (rid,)).fetchone())

    def cancel(self, rid):
        with self.transaction() as db:
            db.execute("INSERT OR IGNORE INTO refusals VALUES (?,?)", (rid, now()))
            row = db.execute("SELECT result FROM runs WHERE id=?", (rid,)).fetchone()
            if not row:
                return {"id": rid, "executor_id": self.executor_id, "state": "cancelled", "devices": [],
                        "refusal_recorded": True, "fenced": True, "never_started": True}
            result = json.loads(row["result"])
            if result["state"] not in TERMINAL:
                result["state"] = "cancel_requested"
            result["updated_at"] = now()
            result["revision"] = result.get("revision", 0) + 1
            db.execute("UPDATE runs SET cancelled=1,result=?,updated_at=? WHERE id=?", (encode(result), now(), rid))
            return result

    def update_device(self, rid, did, values):
        with self.transaction() as db:
            row = db.execute("SELECT result FROM runs WHERE id=?", (rid,)).fetchone()
            if not row:
                raise ExecutorError("run_not_found", "任务不存在", 404)
            result = json.loads(row["result"])
            device = next(d for d in result["devices"] if d["device_id"] == did)
            device.update(values)
            result["updated_at"] = now()
            result["revision"] = result.get("revision", 0) + 1
            states = [d["state"] for d in result["devices"]]
            if any(s not in TERMINAL for s in states):
                result["state"] = "cancel_requested" if self.cancelled(rid) else "running"
            elif all(s in ("completed", "succeeded") for s in states):
                result["state"] = "completed"
            elif any(s == "outcome_unknown" for s in states):
                result["state"] = "outcome_unknown"
            elif all(s == "cancelled" for s in states):
                result["state"] = "cancelled"
            elif any(s in ("completed", "succeeded") for s in states):
                result["state"] = "partial_failed"
            elif all(s == "interrupted" for s in states):
                result["state"] = "interrupted"
            else:
                result["state"] = "failed"
            db.execute("UPDATE runs SET result=?,updated_at=? WHERE id=?", (encode(result), now(), rid))
            event_id = str(uuid4())
            db.execute("INSERT INTO events VALUES (?,?,?,0,?)", (event_id, rid, encode(result), now()))
            return {"event_id": event_id, "executor_id": self.executor_id, "run_id": rid, "result": result}

    def delivered(self, event_id):
        with self.transaction() as db:
            db.execute("UPDATE events SET delivered=1 WHERE event_id=?", (event_id,))

    def worker(self, holder, pid, start_tick):
        with self.transaction() as db:
            db.execute("INSERT OR REPLACE INTO workers VALUES (?,?,?,?,?)", (holder, pid, start_tick, "active", now()))

    def worker_record(self, holder):
        row = self.db.execute("SELECT * FROM workers WHERE holder=?", (holder,)).fetchone()
        return dict(row) if row else None

    def clear_worker(self, holder):
        with self.transaction() as db:
            db.execute("DELETE FROM workers WHERE holder=?", (holder,))

    def artifact(self, aid, rid, did, step_id, path, size):
        with self.transaction() as db:
            db.execute("INSERT INTO artifacts VALUES (?,?,?,?,?,?,?)", (aid, rid, did, step_id, str(path), size, now()))

    def find_artifact(self, rid, aid):
        row = self.db.execute("SELECT * FROM artifacts WHERE run_id=? AND id=?", (rid, aid)).fetchone()
        return dict(row) if row else None

    def artifact_size(self):
        return self.db.execute("SELECT COALESCE(SUM(size),0) FROM artifacts").fetchone()[0]

    def ensure_target(self, target):
        if self.target_refused(target):
            raise ExecutorError("target_retired", "旧目标已失效，禁止迟到命令重新占用")

    def target_refused(self, target):
        return self.db.execute("SELECT 1 FROM target_refusals WHERE device_id=? AND fingerprint=?",
                               (target["device_id"], target["target_fingerprint"])).fetchone() is not None

    def withdrawn(self, rid, did):
        return self.db.execute("SELECT 1 FROM device_refusals WHERE run_id=? AND device_id=?", (rid,did)).fetchone() is not None

    def withdraw(self, rid, did, fingerprint, event_id):
        with self.transaction() as db:
            row = db.execute("SELECT target FROM run_targets WHERE run_id=? AND device_id=?", (rid,did)).fetchone()
            run = db.execute("SELECT 1 FROM runs WHERE id=?", (rid,)).fetchone()
            if run and (not row or json.loads(row["target"])["target_fingerprint"] != fingerprint):
                raise ExecutorError("target_mismatch", "退出目标与任务保存的设备不一致")
            previous = db.execute("SELECT fingerprint FROM device_refusals WHERE run_id=? AND device_id=?", (rid,did)).fetchone()
            if previous and previous["fingerprint"] != fingerprint:
                raise ExecutorError("target_mismatch", "该设备退出记录已绑定另一目标")
            db.execute("INSERT OR IGNORE INTO device_refusals VALUES (?,?,?,?)", (rid,did,fingerprint,event_id))
            db.execute("INSERT OR IGNORE INTO target_refusals VALUES (?,?,?)", (did,fingerprint,event_id))
            return not bool(run)

    def retire(self, target, event_id):
        with self.transaction() as db:
            db.execute("INSERT OR IGNORE INTO target_refusals VALUES (?,?,?)", (target["device_id"],target["target_fingerprint"],event_id))
            db.execute("INSERT OR IGNORE INTO retirements VALUES (?,?,?,0)", (event_id,target["device_id"],target["target_fingerprint"]))
            row=db.execute("SELECT cleanup_confirmed FROM retirements WHERE event_id=? AND device_id=? AND fingerprint=?", (event_id,target["device_id"],target["target_fingerprint"])).fetchone()
            return bool(row[0])

    def retired(self, target, event_id):
        with self.transaction() as db:
            db.execute("UPDATE retirements SET cleanup_confirmed=1 WHERE event_id=? AND device_id=? AND fingerprint=?", (event_id,target["device_id"],target["target_fingerprint"]))
            db.execute("DELETE FROM prepared WHERE device_id=? AND fingerprint=?", (target["device_id"],target["target_fingerprint"]))
