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
        if ctypes.CDLL(None).prctl(1, signal.SIGTERM) != 0:
            raise OSError("prctl failed")
        if os.getppid() != parent_pid:
            raise SystemExit
    os.environ["ADBUTILS_ADB_PATH"] = adb_binary
    with open(os.devnull, "w") as sink:
        os.dup2(sink.fileno(), 1)
        os.dup2(sink.fileno(), 2)


def _helper_entry(channel, gate, parent_pid, target, adb_binary):
    device = None
    stage = "isolate_process"
    try:
        # The first handshake is BEFORE setsid/prctl/stdout isolation.
        # Without it, an isolation failure looks like an unexplained EOF.
        channel.send({"stage": stage})
        _isolate(parent_pid, adb_binary)
        stage = "wait_for_gate"
        channel.send({"stage": stage})
        if not gate.wait(5):
            channel.send({"ok": False, "stage": stage, "code": "automation_worker_gate_timeout",
                          "message": "控件进程未收到启动许可，尚未访问手机"})
            return
        stage = "import_sdk"
        channel.send({"stage": stage})
        import uiautomator2 as u2
        stage = "connect_device"
        channel.send({"stage": stage})
        device = u2.connect_usb(f'{target["ip"]}:{target["adb_port"]}')
        # Unlike raw JSON-RPC, these public APIs can initialize the
        # device-side UiAutomator service. They avoid Android 17 deviceInfo.
        stage = "start_uiautomator"
        channel.send({"stage": stage})
        # ADB-backed dimensions and a real UI hierarchy prove readiness without
        # invoking Android 17's broken UiDevice.getDisplaySizeDp / deviceInfo path.
        width, height = device.window_size()
        stage = "verify_hierarchy"
        channel.send({"stage": stage})
        hierarchy = device.dump_hierarchy()
        if not hierarchy or "<hierarchy" not in hierarchy:
            raise RuntimeError("ui hierarchy not available")
        channel.send({"ok": True, "result": {"state": "automation_ready",
                     "message": "控件组件已准备；中文通过控件文本设置，不自动切换输入法",
                     "capabilities": {"automation": True, "input_text": True, "selectors": True},
                     "display": {"width": width, "height": height}}})
        # Keep the device-side server alive until explicit release.
        channel.recv()
    except ModuleNotFoundError:
        channel.send({"ok": False, "stage": stage, "code": "automation_unavailable",
                      "message": "执行器缺少 UiAutomator 依赖"})
    except (EOFError, BrokenPipeError):
        pass
    except BaseException as exc:
        try:
            # Only exception class (not detail or traceback) may go to the journal.
            # Exception detail can contain device addresses, arguments or secrets.
            category = type(exc).__name__ if type(exc).__name__ in (
                "OSError", "PermissionError", "RuntimeError", "SystemExit",
                "ValueError", "AssertionError", "TimeoutError", "RPCUnknownError",
            ) else "UnexpectedError"
            bootstrap = stage in ("isolate_process", "wait_for_gate")
            channel.send({"ok": False, "stage": stage,
                          "code": "automation_worker_bootstrap_failed" if bootstrap else "automation_prepare_failed",
                          "message": f"控件进程失败（阶段：{stage}，异常类型：{category}）"})
        except (OSError, EOFError, BrokenPipeError):
            pass
    finally:
        if device is not None:
            try:
                device.stop_uiautomator(wait=False)
            except Exception:
                pass
        channel.close()


def _isolation_probe(channel, gate, parent_pid, adb_binary):
    """CI/local diagnostic: exercise the real spawned child isolation, no ADB."""
    try:
        channel.send({"stage": "isolate_process"})
        _isolate(parent_pid, adb_binary)
        channel.send({"stage": "wait_for_gate"})
        if not gate.wait(5):
            channel.send({"ok": False, "code": "automation_worker_gate_timeout"})
        else:
            channel.send({"ok": True})
    except BaseException as exc:
        # Fixed, non-sensitive category; do not reveal raw environment or stderr.
        channel.send({"ok": False, "code": "automation_worker_bootstrap_failed",
                      "error_type": type(exc).__name__})
    finally:
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
        # A spawned child unpickles the named SemLock asynchronously. Keep the
        # Event alive on the long-lived helper until the child has exited:
        # releasing a local gate at __init__ return can unlink its semaphore
        # before multiprocessing.spawn has rebuilt it in the child.
        self.start_gate = context.Event()
        self.process = context.Process(
            target=_helper_entry,
            args=(child, self.start_gate, os.getpid(), target, adb_binary),
            daemon=True,
        )
        self.process.start()
        child.close()
        try:
            self.journal.worker(self.holder, self.process.pid, start_tick(self.process.pid))
        except BaseException:
            self.process.terminate()
            self.process.join(2)
            self.channel.close()
            raise
        self.start_gate.set()  # Never release this Event until the child is reaped.

    def alive(self):
        return not self.closed and self.process.is_alive()

    async def ready(self, timeout=90, on_stage=None):
        # A first install may require device-side service initialization.
        # Poll the pipe in bounded increments instead of leaving a recv()
        # blocked forever in the default thread pool after a timeout.
        loop = asyncio.get_running_loop()
        deadline = loop.time() + timeout
        stages = {
            "launch_worker": "启动执行进程",
            "isolate_process": "设置执行进程隔离",
            "wait_for_gate": "等待进程启动许可",
            "import_sdk": "加载控件 SDK",
            "connect_device": "绑定 ADB 设备",
            "start_uiautomator": "启动手机控件服务",
            "verify_hierarchy": "校验手机控件树",
        }
        stage = "launch_worker"
        while loop.time() < deadline:
            try:
                has_message = await asyncio.to_thread(self.channel.poll, min(.5, max(0, deadline - loop.time())))
                if has_message:
                    response = self.channel.recv()
                    if not isinstance(response, dict):
                        raise ExecutorError("automation_prepare_invalid", "控件进程返回格式无效", 502)
                    if "stage" in response and response["stage"] in stages:
                        stage = response["stage"]
                        if on_stage:
                            on_stage(stage, stages[stage])
                    if "ok" not in response:
                        continue
                    if not response["ok"]:
                        raise ExecutorError(response.get("code", "automation_prepare_failed"),
                                            response.get("message", "控件组件未准备完成"), 502)
                    if response.get("result", {}).get("state") != "automation_ready":
                        raise ExecutorError("automation_prepare_invalid", "控件组件未确认就绪", 502)
                    self.ready_flag = True
                    return response["result"]
                if not self.process.is_alive():
                    await asyncio.to_thread(self.process.join, .1)
                    code, reason = self.exit_reason()
                    raise ExecutorError(code, f"控件子进程提前退出（阶段：{stages[stage]}；{reason}）", 502)
            except (EOFError, OSError):
                # A closed pipe means the worker stopped before returning a final
                # reply. Preserve its exit status; never report a phone timeout.
                await asyncio.to_thread(self.process.join, .2)
                code, reason = self.exit_reason()
                raise ExecutorError(code, f"控件进程通信已断开（阶段：{stages[stage]}；{reason}）", 502) from None
        raise ExecutorError("automation_prepare_timeout",
                            f"手机控件服务初始化超时（阶段：{stages[stage]}）；ADB 可连接，但请检查手机控件服务启动限制", 504)

    def exit_reason(self):
        exitcode = getattr(self.process, "exitcode", None)
        if exitcode is None:
            return "automation_worker_interrupted", "进程状态尚未确认，请查看执行器容器日志"
        if exitcode < 0:
            signum = -exitcode
            return "automation_worker_signalled", f"执行进程被信号 {signum} 终止，请检查执行器资源或外部终止操作"
        if exitcode != 0:
            return "automation_worker_bootstrap_failed", f"子进程退出码 {exitcode}，请检查 Python 子进程启动日志"
        return "automation_worker_exited", "子进程退出码 0，但未返回就绪结果"

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
                self.start_gate = None  # Safe to unlink the semaphore after child exit.
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
