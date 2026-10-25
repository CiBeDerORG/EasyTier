"""User-triggered ADB only. No discovery, password handling or device reconnect loop."""
import asyncio
import re

from .errors import ExecutorError, invalid
from .validation import integer, serial


PROPERTIES = {"manufacturer": "ro.product.manufacturer", "brand": "ro.product.brand", "model": "ro.product.model",
              "android_version": "ro.build.version.release", "sdk": "ro.build.version.sdk", "abi": "ro.product.cpu.abi"}


async def command(binary, *args, timeout=8, stdin=None):
    try:
        process = await asyncio.create_subprocess_exec(binary, *args, stdin=asyncio.subprocess.PIPE if stdin is not None else asyncio.subprocess.DEVNULL,
                                                       stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
    except FileNotFoundError:
        raise ExecutorError("adb_unavailable", "执行器未安装 ADB", 503) from None
    try:
        out, err = await asyncio.wait_for(process.communicate(stdin.encode() if stdin is not None else None), timeout)
    except (asyncio.TimeoutError, asyncio.CancelledError):
        if process.returncode is None:
            process.kill()
        await process.wait()
        raise ExecutorError("adb_timeout", "ADB 操作超时，请检查手机授权和网络", 504) from None
    # Diagnostics from ADB may contain pairing codes, input or addresses: never echo stderr.
    return process.returncode, out[:65536].decode("utf-8", "replace"), err[:65536].decode("utf-8", "replace")


async def require_connected(binary, target):
    code, output, _ = await command(binary, "-s", serial(target), "get-state", timeout=5)
    if code != 0 or output.strip() != "device":
        raise ExecutorError("adb_unauthorized", "手机尚未连接或未授权，请先完成手机授权并手动连接", 409)


async def connection_status(binary, target):
    """A bounded, read-only ADB transport probe; never pairs or connects."""
    try:
        code, output, _ = await command(binary, "-s", serial(target), "get-state", timeout=5)
    except ExecutorError as exc:
        if exc.code == "adb_timeout":
            return {"device_id": target["device_id"], "state": "unavailable", "code": exc.code}
        raise
    state = output.strip() if code == 0 else ""
    if state == "device":
        return {"device_id": target["device_id"], "state": "connected"}
    if state in ("offline", "unauthorized"):
        return {"device_id": target["device_id"], "state": state}
    return {"device_id": target["device_id"], "state": "disconnected"}


async def prepare(binary, target, action, pairing_port=None, code=None):
    if action == "pair":
        integer(pairing_port, 1, 65535)
        if not isinstance(code, str) or re.fullmatch(r"[0-9]{6}", code) is None:
            raise invalid("配对码必须为手机显示的六位数字")
        # Use stdin so a pairing code is not exposed in the process argument list.
        status, output, _ = await command(binary, "pair", f'{target["ip"]}:{pairing_port}', timeout=20, stdin=code + "\n")
        if status != 0 or "successfully paired" not in output.lower():
            raise ExecutorError("pair_failed", "配对未完成，请核对手机显示的配对端口和配对码", 502)
        return {"state": "paired", "message": "配对成功，请使用手机显示的连接端口手动连接"}
    if code is not None or pairing_port is not None:
        raise invalid("只有配对操作可以携带配对参数")
    if action == "connect":
        status, output, _ = await command(binary, "connect", serial(target), timeout=12)
        if status != 0 or not any(s in output.lower() for s in ("connected to", "already connected")):
            raise ExecutorError("connect_failed", "ADB 连接未完成，请核对连接端口和手机授权", 502)
    elif action != "inspect":
        raise invalid("不支持该准备操作")
    await require_connected(binary, target)
    metadata = {}
    for key, prop in PROPERTIES.items():
        status, output, _ = await command(binary, "-s", serial(target), "shell", "getprop", prop, timeout=3)
        value = output.strip()
        metadata[key] = value if status == 0 and len(value) <= 256 and value.isprintable() else None
    return {"state": "connected", "message": "ADB 已连接；实时画面和脚本能力需分别准备", "metadata": metadata,
            "capabilities": {"adb": True, "video": True, "automation": False}}


async def disconnect(binary, target):
    status, _, _ = await command(binary,"disconnect",serial(target),timeout=8)
    if status == 0:
        return
    # A never-paired target may fail "adb disconnect" even though no
    # transport exists. A new ADB listing must prove its exact serial absent.
    check, listing, _ = await command(binary, "devices", timeout=8)
    if check == 0 and all(line.split()[0] != serial(target) for line in listing.splitlines()[1:] if line.split()):
        return
    raise ExecutorError("cleanup_pending", "旧设备 ADB 连接尚未确认清理")
