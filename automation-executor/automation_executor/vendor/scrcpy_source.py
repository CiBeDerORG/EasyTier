"""Pinned main-display scrcpy 5.0.1 source, one explicit already-connected ADB target.

No discovery, reconnect, alternate route, clipboard sync, or shared ADB
server shutdown. The reader owns only its randomly named forward and process.
"""
from __future__ import annotations

import hashlib
import os
import re
import secrets
import signal
import socket
import struct
import subprocess
import tempfile
import threading
import time
from pathlib import Path
from typing import Any, Callable

VERSION = "5.0.1"
SERVER_SHA256 = "764eb6f79811d5211fe9df341120882ba9994c7a61b897d7bf3fb662e53bc536"
PROFILES = {
    "wall": {"max_size": 640, "video_bit_rate": 1_000_000, "max_fps": 15},
    "focus": {"max_size": 1280, "video_bit_rate": 4_000_000, "max_fps": 30},
}
MAX_FRAME_BYTES = 8 * 1024 * 1024


class SourceError(Exception):
    def __init__(self, code: str, message: str, status: int = 502):
        super().__init__(message)
        self.code, self.message, self.status = code, message, status


class SourceStopped(Exception):
    pass


# Follow EasyADB's conservative keyguard detection: Android/EMUI can
# report true/false or 1/0. A locked work profile is not the active user.
TRUST_LOCK = re.compile(r"(?<![\w])(?:deviceLocked|mDeviceLocked|isDeviceLocked)\s*[:=]\s*(true|false|1|0)\b", re.I)
WINDOW_LOCK = re.compile(
    r"(?<![\w])(?:mShowingLockscreen|isStatusBarKeyguard|mKeyguardShowing|"
    r"isKeyguardShowing|KeyguardShowing|mKeyguardLocked)\s*[:=]\s*(true|false|1|0)\b", re.I
)
KEYGUARD_SECTION = re.compile(
    r"(?im)^\s*(?:KeyguardServiceDelegate|KeyguardStateMonitor)\s*\r?\n"
    r"\s*(?:showing|mIsShowing)\s*[:=]\s*(true|false|1|0)\b"
)
DREAM_LOCK = re.compile(r"(?<![\w])mDreamingLockscreen\s*[:=]\s*(true|1)\b", re.I)
SIZE_PATTERN = re.compile(r"(?:Physical size|Override size):\s*(\d+)x(\d+)")


def _lock_consensus(values: list[str]) -> bool | None:
    states = {value.lower() in ("true", "1") for value in values}
    return next(iter(states)) if len(states) == 1 else None


def _trust_lock(output: str) -> bool | None:
    users = [line for line in output.splitlines()
             if re.match(r"^\s*User\s+.*\(id=\d+\b", line)]
    current = [line for line in users if re.search(r"\(current\)", line, re.I)]
    if current:
        return _lock_consensus(TRUST_LOCK.findall("\n".join(current)))
    if len(users) > 1:
        return None
    return _lock_consensus(TRUST_LOCK.findall("\n".join(users) if users else output))


def _window_lock(output: str) -> bool | None:
    states = WINDOW_LOCK.findall(output) + KEYGUARD_SECTION.findall(output)
    if DREAM_LOCK.search(output):
        states.append("true")
    return _lock_consensus(states)


class ScrcpySource:
    def __init__(self, serial: str, quality: str, track: Any, on_end: Callable[[], None], options: dict | None = None):
        self.serial = serial
        self.settings = {**PROFILES[quality], **(options or {})}
        self._screen_power_applied = False
        self.track = track
        self.on_end = on_end
        self.binary = os.environ.get("AUTOMATION_ADB_BINARY", "adb")
        self.server = Path(os.environ.get("AUTOMATION_SCRCPY_SERVER", "/opt/automation/scrcpy-server-v5.0.1"))
        self.stop_event = threading.Event()
        self.ready = threading.Event()
        self.finished = threading.Event()
        self.error: SourceError | None = None
        self.cleanup_complete = False
        self.thread: threading.Thread | None = None
        self.child: subprocess.Popen | None = None
        self.video: socket.socket | None = None
        self.control: socket.socket | None = None
        self.size: tuple[int, int] | None = None
        self.pressed = False
        self.point: tuple[int, int] | None = None
        self.lock = threading.RLock()
        self.resources_lock = threading.Lock()
        self.port: int | None = None
        self.remote: str | None = None
        self.abstract: str | None = None
        self.remote_pushed = False
        self.forward_unconfirmed = False
        self._release_failed = False
        self._first_frame_deadline = 0.0
        self._has_config = False

    def _check(self) -> None:
        if self.stop_event.is_set():
            raise SourceStopped()

    def _adb(self, *args: str, timeout: float = 8, cleanup: bool = False) -> str:
        if not cleanup:
            self._check()
        try:
            result = subprocess.run(
                [self.binary, "-s", self.serial, *args], stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                timeout=timeout, check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as exc:
            raise SourceError("adb_failed", "ADB 命令未能完成，请检查执行器与当前手机连接") from exc
        if result.returncode != 0:
            raise SourceError("adb_failed", "ADB 未完成当前操作，请检查手机授权及连接")
        return result.stdout.strip()

    def start(self) -> None:
        if self.thread is not None:
            raise SourceError("media_already_started", "本次媒体会话已经启动", 409)
        self.thread = threading.Thread(target=self._run, name="automation-scrcpy", daemon=True)
        self.thread.start()

    def _read(self, sock: socket.socket, count: int) -> bytes:
        data = bytearray()
        while len(data) < count:
            self._check()
            if self.child is not None and self.child.poll() is not None:
                raise SourceError("media_process_ended", "手机端视频进程已经结束")
            if not self.ready.is_set() and time.monotonic() > self._first_frame_deadline:
                raise SourceError("media_start_timeout", "未在限时内收到手机首个可用画面")
            waiting_since = getattr(self.track, "waiting_since", None)
            if waiting_since is not None and time.monotonic() - waiting_since > 15:
                raise SourceError("media_keyframe_timeout", "等待关键帧恢复超时，请手动重新打开画面")
            sock.settimeout(1.0)
            try:
                chunk = sock.recv(count - len(data))
            except socket.timeout:
                continue
            except OSError as exc:
                self._check()
                raise SourceError("media_disconnected", "手机视频连接已中断") from exc
            if not chunk:
                raise SourceError("media_disconnected", "手机视频连接已关闭")
            data.extend(chunk)
        return bytes(data)

    def _connect_video(self) -> socket.socket:
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            self._check()
            if self.child is not None and self.child.poll() is not None:
                raise SourceError("media_process_ended", "手机端视频进程在握手前结束")
            candidate = None
            success = False
            try:
                candidate = socket.create_connection(("127.0.0.1", self.port), timeout=0.4)
                candidate.settimeout(0.5)
                marker = candidate.recv(1)
                if marker == b"\x00":
                    success = True
                    return candidate
                if marker:
                    raise SourceError("media_protocol_mismatch", "scrcpy 握手不匹配，请使用固定版本服务端")
            except OSError:
                pass
            finally:
                if candidate is not None and not success:
                    candidate.close()
            # Bounded startup of this explicitly requested forward, not an ADB reconnect.
            self.stop_event.wait(0.1)
        raise SourceError("media_start_timeout", "手机端视频通道未在限时内就绪")

    def _run(self) -> None:
        scid = f"{secrets.randbelow(0x7FFFFFFE) + 1:08x}"
        self.remote = f"/data/local/tmp/easytier-automation-{scid}.jar"
        self.abstract = f"localabstract:scrcpy_{scid}"
        try:
            self._check()
            if not self.server.is_file() or self.server.stat().st_size > 16 * 1024 * 1024:
                raise SourceError("scrcpy_unavailable", "执行器缺少固定版本 scrcpy-server", 503)
            with self.server.open("rb") as source:
                digest = hashlib.file_digest(source, "sha256").hexdigest()
            if digest != SERVER_SHA256:
                raise SourceError("scrcpy_version_mismatch", "scrcpy-server 摘要不匹配，未启动手机端组件", 503)
            if self._adb("get-state", timeout=5) != "device":
                raise SourceError("adb_not_connected", "手机尚未连接并授权，请先完成接入准备", 409)
            self.remote_pushed = True
            self._adb("push", str(self.server), self.remote, timeout=12)
            self.forward_unconfirmed = True
            port = self._adb("forward", "tcp:0", self.abstract, timeout=5)
            if not port.isdigit() or not 1 <= int(port) <= 65535:
                raise SourceError("media_forward_failed", "ADB 未返回有效的视频转发端口")
            self.port = int(port)
            self.forward_unconfirmed = False
            self._check()
            args = [
                self.binary, "-s", self.serial, "shell", f"CLASSPATH={self.remote}",
                "app_process", "/", "com.genymobile.scrcpy.Server", VERSION,
                f"scid={scid}", "video=true", "audio=false", "control=true",
                "tunnel_forward=true", "send_device_meta=true", "send_frame_meta=true",
                "send_stream_meta=true", "send_dummy_byte=true", "cleanup=true",
                "log_level=warn",
                "power_on=true", "clipboard_autosync=false", "downsize_on_error=false",
                "video_codec=h264", f"video_codec_options=i-frame-interval={1 if self.settings.get('low_latency') else 2}",
                f"max_size={self.settings['max_size']}",
                f"video_bit_rate={self.settings['video_bit_rate']}", f"max_fps={self.settings['max_fps']}",
            ]
            # No raw Android logs are exposed to callers or persisted as task results.
            with tempfile.TemporaryFile() as log:
                child = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=log,
                                         stderr=subprocess.STDOUT, start_new_session=True)
                with self.resources_lock:
                    self.child = child
                self._check()
                video = self._connect_video()
                with self.resources_lock:
                    self.video = video
                self._check()
                control = socket.create_connection(("127.0.0.1", self.port), timeout=3)
                control.settimeout(1.0)
                with self.lock:
                    self.control = control
                self._first_frame_deadline = time.monotonic() + 15
                self._read(video, 64)  # device name; asset data remains authoritative.
                if self._read(video, 4) != b"h264":
                    raise SourceError("media_codec_mismatch", "手机未输出约定的 H.264 视频")
                while not self.stop_event.is_set():
                    header = self._read(video, 12)
                    flags, length = struct.unpack(">QI", header)
                    if flags & (1 << 63):
                        _, width, height = struct.unpack(">III", header)
                        if not 0 < width <= 8192 or not 0 < height <= 8192:
                            raise SourceError("media_geometry_invalid", "手机返回的视频尺寸无效")
                        with self.lock:
                            self._release_locked()
                            self.size = (width, height)
                            # Physical screen power is applied after optional
                            # unlock, not here before keyguard credentials.
                        self.track.reset_stream()
                        self._has_config = False
                        continue
                    if self.size is None or not 0 < length <= MAX_FRAME_BYTES:
                        raise SourceError("media_protocol_mismatch", "手机视频包或尺寸头无效")
                    payload = self._read(video, length)
                    config = bool(flags & (1 << 62))
                    keyframe = bool(flags & (1 << 61))
                    if config:
                        self._has_config = 0 < len(payload) <= 256 * 1024
                    self.track.submit(flags & ((1 << 61) - 1), payload, config=config, keyframe=keyframe)
                    if keyframe and not config and self._has_config:
                        self.ready.set()
        except SourceStopped:
            pass
        except SourceError as exc:
            self.error = exc
        except (OSError, ValueError, struct.error, subprocess.SubprocessError):
            self.error = SourceError("media_failed", "手机视频启动或传输失败，请检查设备及执行器配置")
        finally:
            self.stop()
            self._cleanup()
            self.finished.set()
            self.ready.set()  # Wake a startup waiter; it must inspect error/finished.
            self.track.stop()
            self.on_end()

    def _send_locked(self, data: bytes) -> None:
        if self.stop_event.is_set() or self.control is None or self.size is None:
            raise SourceError("media_not_ready", "手机控制通道尚未就绪", 409)
        try:
            self.control.sendall(data)
        except OSError as exc:
            self.stop_event.set()
            raise SourceError("control_disconnected", "手机控制通道中断，动作结果未确认") from exc

    def touch(self, message: dict) -> None:
        with self.lock:
            if self.size is None:
                raise SourceError("media_not_ready", "手机画面尚未就绪", 409)
            width, height = self.size
            if type(message.get("width")) is not int or type(message.get("height")) is not int or (message["width"], message["height"]) != self.size:
                self._release_locked()
                raise SourceError("geometry_changed", "画面尺寸已经变化，请重新开始手势", 409)
            x, y = message.get("x"), message.get("y")
            if type(x) is not int or type(y) is not int or not 0 <= x < width or not 0 <= y < height:
                raise SourceError("invalid_touch", "触摸坐标必须位于当前画面内", 422)
            action = message.get("action")
            if action not in ("down", "move", "up") or (action == "down") == self.pressed:
                raise SourceError("invalid_touch", "手势顺序无效，请先释放当前触点", 409)
            code = {"down": 0, "up": 1, "move": 2}[action]
            packet = struct.pack(">BBQiiHHHII", 2, code, 0xFFFFFFFFFFFFFFFE,
                                 x, y, width, height, 0 if action == "up" else 0xFFFF, 0, 0)
            self._send_locked(packet)
            self.pressed = action != "up"
            self.point = (x, y) if self.pressed else None

    def key(self, key: str) -> None:
        keycode = {"HOME": 3, "BACK": 4, "RECENT": 187, "WAKE": 224}.get(key)
        if keycode is None:
            raise SourceError("invalid_key", "不支持此导航键", 422)
        with self.lock:
            self._send_locked(struct.pack(">BBIII", 0, 0, keycode, 0, 0)
                              + struct.pack(">BBIII", 0, 1, keycode, 0, 0))

    def set_screen_power(self, on: bool) -> None:
        with self.lock:
            self._send_locked(struct.pack(">BB", 10, 1 if on else 0))
            self._screen_power_applied = not on

    def _keycode(self, code: int) -> None:
        with self.lock:
            self._send_locked(struct.pack(">BBIII",0,0,code,0,0)+struct.pack(">BBIII",0,1,code,0,0))

    def _keyguard(self) -> bool | None:
        """Only positive evidence of current-user lock permits PIN injection."""
        def read(*args: str) -> str:
            try:
                return self._adb("shell", "dumpsys", *args, timeout=6)
            except SourceError:
                return ""

        trust = _trust_lock(read("trust"))
        if trust is False:
            return False
        window = _window_lock(read("window", "policy"))
        if window is None:
            window = _window_lock(read("window"))
        if trust is True:
            return None if window is False else True
        return window

    def _stable_keyguard(self) -> bool | None:
        """Recheck an indeterminate keyguard once after OEM animations settle.

        An explicit unlocked result is never overridden to allow PIN injection.
        """
        state = self._keyguard()
        if state is not None or self.stop_event.wait(0.65):
            return state
        return self._keyguard()

    def wake_and_unlock(self, password: str | None) -> str:
        """One bounded, opt-in attempt; the credential never enters ADB argv."""
        # A manual unlock must first light the physical screen, including for a
        # stream started with screen_off. Never toggle POWER (which can turn it off).
        self.set_screen_power(True)
        self.key("WAKE")
        # Some OEM firmware ignores scrcpy's wake key while the display is
        # dozing. KEYCODE_WAKEUP is idempotent (unlike POWER) and is safe to
        # send using ADB as a best-effort second wake mechanism.
        try:
            self._adb("shell", "input", "keyevent", "224", timeout=5)
        except SourceError:
            pass
        if not password:
            return "wake_sent"
        # Waking the display and revealing the keyguard are asynchronous.
        if self.stop_event.wait(1.0):
            return "unlock_skipped"
        lock_state = self._stable_keyguard()
        if lock_state is not True:
            return "already_unlocked" if lock_state is False else "lock_state_unknown"

        # Use physical display coordinates and an actual timed swipe, like
        # EasyADB. Three immediate scrcpy touch packets are not a swipe gesture
        # on several Android/EMUI keyguards.
        try:
            size = self._adb("shell", "wm", "size", timeout=6)
        except SourceError:
            # A locked or OEM-restricted shell must not break video viewing.
            return "unlock_size_unknown"
        sizes = SIZE_PATTERN.findall(size)
        if not sizes:
            return "unlock_size_unknown"
        width, height = map(int, sizes[-1])
        if not (240 <= width <= 10000 and 400 <= height <= 20000):
            return "unlock_size_unknown"
        # wm size reports the physical portrait dimensions on some phones,
        # even if the active scrcpy stream is currently rotated.
        if self.size and self.size[0] > self.size[1] and width < height:
            width, height = height, width
        try:
            self._adb("shell", "input", "swipe", str(width // 2), str(height * 8 // 10),
                      str(width // 2), str(height * 3 // 10), "240", timeout=7)
        except SourceError:
            # Skip password injection if the swipe could not be completed.
            return "unlock_swipe_failed"
        # PIN/password UI can appear well after the swipe finishes on MIUI/EMUI.
        # Delay is cancellable; re-check keyguard before injecting any secret.
        if self.stop_event.wait(0.8):
            return "unlock_skipped"
        # The user may have unlocked with biometrics while the swipe ran.
        after_swipe = self._stable_keyguard()
        if after_swipe is not True:
            return "already_unlocked" if after_swipe is False else "lock_state_changed"

        # Match EasyADB: transmit the entire saved PIN/password in one scrcpy
        # INJECT_TEXT packet. Never put the credential in an ADB shell command.
        encoded = password.encode("ascii")
        with self.lock:
            self._send_locked(struct.pack(">BI", 1, len(encoded)) + encoded)
        # Give Android's password field time to receive the text before Enter.
        if self.stop_event.wait(0.18):
            return "unlock_skipped"
        self._keycode(66)  # KEYCODE_ENTER
        if self.stop_event.wait(0.4):
            return "unlock_sent"
        return "unlock_confirmed" if self._keyguard() is False else "unlock_sent"

    def _release_locked(self) -> None:
        if not self.pressed:
            return
        try:
            if self.control is not None and self.size and self.point:
                width, height = self.size
                x, y = self.point
                self.control.sendall(struct.pack(">BBQiiHHHII", 2, 3, 0xFFFFFFFFFFFFFFFE,
                                                x, y, width, height, 0, 0, 0))
        except OSError:
            self._release_failed = True
        finally:
            self.pressed = False
            self.point = None

    def release(self) -> None:
        with self.lock:
            self._release_locked()
        if self._release_failed:
            raise SourceError("control_release_unknown", "触点释放结果未确认，请检查手机状态")

    def stop(self) -> None:
        with self.lock:
            if self._screen_power_applied and self.control is not None:
                try:
                    self.control.sendall(struct.pack(">BB", 10, 1))
                except OSError:
                    pass  # Best effort: a dead transport cannot confirm display state.
                self._screen_power_applied = False
            self.stop_event.set()
            self._release_locked()
            control, self.control = self.control, None
        with self.resources_lock:
            video, self.video = self.video, None
            child = self.child
        for channel in (control, video):
            if channel is not None:
                try:
                    channel.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                try:
                    channel.close()
                except OSError:
                    pass
        if child is not None and child.poll() is None:
            try:
                os.killpg(child.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            except OSError:
                self._release_failed = True

    def _cleanup(self) -> None:
        # A missing touch acknowledgement is reported separately. Resource
        # ownership can end once the source and its own channels are gone.
        clean = not self.forward_unconfirmed
        child = self.child
        if child is not None:
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(child.pid, signal.SIGKILL)
                    child.wait(timeout=2)
                except (OSError, subprocess.TimeoutExpired):
                    clean = False
            except OSError:
                clean = False
        if self.port is not None:
            try:
                self._adb("forward", "--remove", f"tcp:{self.port}", timeout=4, cleanup=True)
                self.port = None
            except SourceError:
                clean = False
        if self.remote_pushed and self.remote:
            try:
                self._adb("shell", "rm", "-f", self.remote, timeout=4, cleanup=True)
                self.remote_pushed = False
            except SourceError:
                clean = False
        self.cleanup_complete = clean

    def join(self, timeout: float = 15) -> None:
        if self.thread is not None:
            self.thread.join(timeout)

    def retry_cleanup(self) -> None:
        if not self.finished.is_set():
            return
        if self.forward_unconfirmed and self.abstract:
            # A timed-out tcp:0 response can leave the assigned port unknown.
            # Inspect only this source's random socket name; never remove all
            # forwards or release the slot just because a list was empty.
            try:
                listing = self._adb("forward", "--list", timeout=4, cleanup=True)
                if len(listing) <= 65536:
                    for line in listing.splitlines():
                        parts = line.split()
                        if (len(parts) == 3 and parts[0] == self.serial
                                and parts[2] == self.abstract and parts[1].startswith("tcp:")):
                            local = parts[1][4:]
                            if local.isdigit() and 1 <= int(local) <= 65535:
                                self.port = int(local)
                                self.forward_unconfirmed = False
                                break
            except SourceError:
                pass
        self._cleanup()
