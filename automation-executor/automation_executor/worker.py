"""Process isolation and bounded cancellation for blocking Android SDK calls."""
import asyncio
import ctypes
import multiprocessing
import os
from pathlib import Path
import signal

from .errors import ExecutorError


def start_tick(pid):
    try:
        # comm can contain spaces or ')': split after its final closing bracket.
        fields = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()
        return fields[19]
    except (OSError, IndexError):
        return None


def _isolate(parent_pid, adb_binary):
    os.setsid()
    if os.name == "posix" and Path("/proc").exists():
        ctypes.CDLL(None).prctl(1, signal.SIGTERM)
        if os.getppid() != parent_pid:
            raise SystemExit
    os.environ["ADBUTILS_ADB_PATH"] = adb_binary
    with open(os.devnull, "w") as sink:
        os.dup2(sink.fileno(), 1)
        os.dup2(sink.fileno(), 2)


def _helper_entry(channel, gate, parent_pid, target, adb_binary):
    device = None
    try:
        _isolate(parent_pid, adb_binary)
        if not gate.wait(5):
            return
        import uiautomator2 as u2
        from uiautomator2.core import DEFAULT_SERVER_PORT, _jsonrpc_call
        device = u2.connect_usb(f'{target["ip"]}:{target["adb_port"]}')
        info = _jsonrpc_call(device.adb_device, DEFAULT_SERVER_PORT, "deviceInfo", (), 8, False)
        channel.send({"ok": True, "result": {"state": "automation_ready",
                     "message": "控件组件已准备；中文通过控件文本设置，不自动切换输入法",
                     "capabilities": {"automation": True, "input_text": True, "selectors": True},
                     "display": {"width": info.get("displayWidth"), "height": info.get("displayHeight")}}})
        # Keep the uiautomator2 server's stream alive. No device polling or input.
        channel.recv()
    except ModuleNotFoundError:
        channel.send({"ok": False, "code": "automation_unavailable", "message": "执行器缺少控件执行依赖"})
    except (EOFError, BrokenPipeError):
        pass
    except BaseException:
        try:
            channel.send({"ok": False, "code": "automation_prepare_failed", "message": "手机控件组件未准备完成，请检查调试授权或系统兼容性"})
        except (OSError, EOFError):
            pass
    finally:
        if device is not None:
            try:
                device.stop_uiautomator(wait=False)
            except Exception:
                pass
        channel.close()


class PreparedHelper:
    """Long-lived owner of the device-side helper started by explicit preparation."""
    def __init__(self, journal, target, adb_binary):
        context = multiprocessing.get_context("spawn")
        self.channel, child = context.Pipe()
        self.target = target
        self.holder = "helper:" + target["device_id"]
        self.journal = journal
        self.ready_flag = False
        self.closed = False
        self.close_lock = asyncio.Lock()
        gate = context.Event()
        self.process = context.Process(target=_helper_entry, args=(child, gate, os.getpid(), target, adb_binary), daemon=True)
        self.process.start()
        child.close()
        try:
            self.journal.worker(self.holder, self.process.pid, start_tick(self.process.pid))
        except BaseException:
            self.process.terminate()
            self.process.join(2)
            self.channel.close()
            raise
        gate.set()  # No device-side preparation can happen before durable ownership.

    def alive(self):
        return not self.closed and self.process.is_alive()

    async def ready(self):
        try:
            response = await asyncio.wait_for(asyncio.to_thread(self.channel.recv), 45)
            if not response.get("ok"):
                raise ExecutorError(response.get("code", "automation_prepare_failed"), response.get("message", "控件组件未准备完成"), 502)
            self.ready_flag = True
            return response["result"]
        except (asyncio.TimeoutError, EOFError, OSError):
            raise ExecutorError("automation_prepare_failed", "手机控件组件准备超时或中断", 502) from None

    async def close(self):
        async with self.close_lock:
            if self.closed:
                return True
            self.ready_flag = False
            try:
                self.channel.send("stop")
            except (BrokenPipeError, EOFError, OSError):
                pass
            await asyncio.to_thread(self.process.join, 2)
            if self.process.is_alive():
                cleanup_record(self.journal.worker_record(self.holder))
                await asyncio.to_thread(self.process.join, 2)
            clean = not self.process.is_alive()
            self.channel.close()
            if clean:
                self.journal.clear_worker(self.holder)
                self.process.close()
                self.closed = True
            return clean


def _entry(output, gate, parent_pid, target, step, parameters, artifact_path, adb_binary):
    try:
        _isolate(parent_pid, adb_binary)
        if not gate.wait(5):
            return
        from .steps import execute, StepFailure
        try:
            output.send({"ok": True, "result": execute(target, step, parameters, artifact_path)})
        except StepFailure as exc:
            output.send({"ok": False, "code": exc.code, "message": exc.message, "outcome_unknown": exc.outcome_unknown})
        except ModuleNotFoundError:
            output.send({"ok": False, "code": "automation_unavailable", "message": "执行器缺少控件执行依赖", "outcome_unknown": False})
        except Exception:
            # Do not repeat actions after transport or library errors.
            output.send({"ok": False, "code": "device_action_unconfirmed", "message": "设备动作结果未确认，请检查手机后处理", "outcome_unknown": True})
    except BaseException:
        pass
    finally:
        output.close()


def cleanup_record(record):
    """Only signal a process whose Linux start identity still matches our record."""
    if not record:
        return True
    pid, expected = record["pid"], record.get("start_tick")
    current = start_tick(pid)
    if current is None:
        return True
    if not expected:
        # An extant PID without a recorded start identity is not cleanup proof.
        return False
    if current != expected:
        # PID was recycled; it is not ours and must never be signalled.
        return True
    try:
        if os.getpgid(pid) == pid:
            os.killpg(pid, signal.SIGKILL)
        else:
            os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        return True
    except (OSError, PermissionError):
        return False
    return start_tick(pid) is None


async def execute_step(journal, holder, target, step, parameters, cancel, adb_binary, artifact_path=None):
    if cancel.is_set():
        raise ExecutorError("operation_cancelled", "该设备已退出当前脚本")
    journal.ensure_target(target)
    context = multiprocessing.get_context("spawn")
    read, write = context.Pipe(duplex=False)
    gate = context.Event()
    process = context.Process(target=_entry, args=(write, gate, os.getpid(), target, step, parameters, str(artifact_path) if artifact_path else None, adb_binary), daemon=True)
    process.start()
    write.close()
    try:
        journal.worker(holder, process.pid, start_tick(process.pid))
    except BaseException:
        process.terminate()
        await asyncio.to_thread(process.join, 2)
        read.close()
        raise
    gate.set()  # Child is forbidden to touch the phone until its PID is journaled.
    # This future only waits for the current bounded child, never queries a device.
    receive = asyncio.create_task(asyncio.to_thread(read.recv))
    cancellation = asyncio.create_task(cancel.wait())
    timeout = min(40, max(15, step.get("timeout_ms", 0) / 1000 + 8))
    outcome = None
    clean = False
    try:
        done, _ = await asyncio.wait({receive, cancellation}, timeout=timeout, return_when=asyncio.FIRST_COMPLETED)
        if cancellation in done and cancellation.result():
            raise ExecutorError("operation_cancelled", "已要求停止后续操作，当前动作是否发生需核对", 409)
        if receive not in done:
            raise ExecutorError("step_timeout", "步骤超时，当前动作结果未确认", 504)
        try:
            outcome = receive.result()
        except (EOFError, OSError):
            raise ExecutorError("worker_interrupted", "执行进程中断，当前动作结果未确认", 502) from None
        if not outcome.get("ok"):
            error = ExecutorError(outcome.get("code", "step_failed"), outcome.get("message", "步骤未完成"), 502)
            error.outcome_unknown = outcome.get("outcome_unknown", False)
            raise error
        return outcome["result"]
    finally:
        cancellation.cancel()
        if process.is_alive():
            try:
                if os.getpgid(process.pid) == process.pid:
                    os.killpg(process.pid, signal.SIGTERM)
                else:
                    process.terminate()
            except ProcessLookupError:
                pass
        await asyncio.to_thread(process.join, 2)
        if process.is_alive():
            try:
                if os.getpgid(process.pid) == process.pid:
                    os.killpg(process.pid, signal.SIGKILL)
                else:
                    process.kill()
            except ProcessLookupError:
                pass
            await asyncio.to_thread(process.join, 2)
        clean = not process.is_alive()
        read.close()
        if not receive.done():
            receive.cancel()
        if clean:
            journal.clear_worker(holder)
            process.close()
        else:
            journal.release(holder, clean=False)
