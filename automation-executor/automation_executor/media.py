"""WebRTC adapter. Authentication, leases and write ownership live in runtime.

Only user-created sessions allocate devices. No discovery, route fallback,
runtime download or automatic reconnect is performed here.
"""
from __future__ import annotations

import asyncio
import base64
import copy
import hashlib
import hmac
import ipaddress
import json
import os
import time
import uuid
from dataclasses import dataclass, field
from typing import Any

from .vendor.scrcpy_source import ScrcpySource, SourceError, PROFILES


class MediaError(Exception):
    def __init__(self, code: str, message: str, status: int = 409):
        super().__init__(message)
        self.code, self.message, self.status = code, message, status


def _bounded_env(name: str, default: int, maximum: int) -> int:
    try:
        value = int(os.environ.get(name, str(default)))
    except ValueError as exc:
        raise MediaError("media_config_invalid", f"{name} 必须为整数", 503) from exc
    if not 1 <= value <= maximum:
        raise MediaError("media_config_invalid", f"{name} 超出支持范围", 503)
    return value


def _ice_config() -> list[dict]:
    try:
        entries = json.loads(os.environ.get("AUTOMATION_ICE_SERVERS", "[]"))
    except (ValueError, TypeError) as exc:
        raise MediaError("ice_config_invalid", "执行器 ICE 配置不是有效 JSON", 503) from exc
    if not isinstance(entries, list) or len(entries) > 8:
        raise MediaError("ice_config_invalid", "执行器 ICE 配置格式无效", 503)
    result = []
    for entry in entries:
        if not isinstance(entry, dict) or set(entry) - {"urls", "username", "credential"}:
            raise MediaError("ice_config_invalid", "ICE 配置包含不支持的字段", 503)
        urls = entry.get("urls")
        values = [urls] if isinstance(urls, str) else urls
        if not isinstance(values, list) or not 1 <= len(values) <= 8 or any(
            not isinstance(url, str) or len(url) > 1024
            or not url.startswith(("stun:", "stuns:", "turn:", "turns:")) for url in values
        ):
            raise MediaError("ice_config_invalid", "ICE 地址配置无效", 503)
        if any(key in entry and (not isinstance(entry[key], str) or len(entry[key]) > 1024)
               for key in ("username", "credential")):
            raise MediaError("ice_config_invalid", "ICE 凭据配置无效", 503)
        result.append(copy.deepcopy(entry))
    return result


@dataclass
class _Session:
    id: str
    target: dict
    serial: str
    quality: str
    options: dict = field(default_factory=dict)
    ice_servers: list[dict] = field(default_factory=list)
    state: str = "created"
    source: ScrcpySource | None = None
    track: Any = None
    peer: Any = None
    answer: dict | None = None
    offer_sdp: str | None = None
    error: MediaError | None = None
    closing: bool = False
    operation_lock: asyncio.Lock = field(default_factory=asyncio.Lock)
    close_lock: asyncio.Lock = field(default_factory=asyncio.Lock)
    close_task: asyncio.Task | None = None


class MediaManager:
    def __init__(self) -> None:
        self._sessions: dict[str, _Session] = {}
        self._lock = asyncio.Lock()
        self._max_sessions = _bounded_env("AUTOMATION_MAX_MEDIA_SESSIONS", 10, 32)
        self._starting = asyncio.Semaphore(_bounded_env("AUTOMATION_MAX_MEDIA_STARTING", 2, 4))
        self._ice = _ice_config()
        self._turn_secret = os.environ.get("AUTOMATION_TURN_SECRET", "")
        self._turn_ttl = _bounded_env("AUTOMATION_TURN_TTL_SECONDS", 3600, 86400)
        try:
            self._turn_urls = json.loads(os.environ.get("AUTOMATION_TURN_URLS", "[]"))
        except ValueError as exc:
            raise MediaError("ice_config_invalid", "TURN 地址应为 JSON 数组", 503) from exc
        if not isinstance(self._turn_urls, list) or len(self._turn_urls) > 8 or any(
            not isinstance(url, str) or not url.startswith(("turn:", "turns:")) or len(url) > 1024
            for url in self._turn_urls
        ) or bool(self._turn_secret) != bool(self._turn_urls):
            raise MediaError("ice_config_invalid", "TURN 地址和凭据签发密钥必须同时正确配置", 503)

    def _session_ice(self, session_id: str) -> list[dict]:
        entries = copy.deepcopy(self._ice)
        if self._turn_urls:
            username = f"{int(time.time()) + self._turn_ttl}:{session_id}"
            credential = base64.b64encode(hmac.new(self._turn_secret.encode(), username.encode(), hashlib.sha1).digest()).decode()
            entries.append({"urls": list(self._turn_urls), "username": username, "credential": credential})
        return entries

    async def create(self, session_id: str, target: dict, quality: str, options: dict | None = None) -> dict:
        try:
            uuid.UUID(session_id)
            address = ipaddress.IPv4Address(target["ip"])
            port, device_id = target["adb_port"], target["device_id"]
        except (ValueError, TypeError, KeyError, AttributeError) as exc:
            raise MediaError("invalid_target", "媒体会话目标无效", 422) from exc
        if (address.is_loopback or address.is_multicast or address.is_unspecified
                or address.is_link_local or address.is_reserved
                or type(port) is not int or not 1 <= port <= 65535
                or not isinstance(device_id, str) or not 1 <= len(device_id) <= 128
                or not isinstance(quality, str) or quality not in PROFILES
                or target.get("route_context") != "default"):
            raise MediaError("invalid_target", "媒体目标或画质配置无效", 422)
        settings = copy.deepcopy(PROFILES[quality])
        settings.update({"low_latency": True, "screen_off": False})
        if options is not None:
            if not isinstance(options, dict) or set(options) != {"max_size", "video_bit_rate", "max_fps", "low_latency", "screen_off"}:
                raise MediaError("invalid_media", "投屏参数不完整或包含未知字段", 422)
            for key, lower, upper in (("max_size", 256, 2560), ("video_bit_rate", 250000, 12000000), ("max_fps", 5, 60)):
                value = options[key]
                if type(value) is not int or not lower <= value <= upper:
                    raise MediaError("invalid_media", "分辨率、码率或 FPS 超出允许范围", 422)
                settings[key] = value
            for key in ("low_latency", "screen_off"):
                if type(options[key]) is not bool:
                    raise MediaError("invalid_media", "低延时和息屏必须是布尔值", 422)
                settings[key] = options[key]
        serial = f"{address}:{port}"
        async with self._lock:
            current = self._sessions.get(session_id)
            if current is not None:
                if current.target != target or current.quality != quality or current.options != settings or current.closing:
                    raise MediaError("session_conflict", "此会话编号已被另一请求或已结束会话使用")
                return self._created(current)
            if len(self._sessions) >= self._max_sessions:
                raise MediaError("media_capacity", "实时画面已达到执行器容量，请先关闭其他画面", 429)
            if any(item.serial == serial or item.target["device_id"] == device_id
                   for item in self._sessions.values()):
                raise MediaError("device_busy", "该手机已有观看会话或资源尚未清理")
            session = _Session(session_id, copy.deepcopy(target), serial, quality, settings, self._session_ice(session_id))
            self._sessions[session_id] = session
            return self._created(session)

    def _created(self, session: _Session) -> dict:
        return {"id": session.id, "device_id": session.target["device_id"],
                "state": session.state, "ice_servers": copy.deepcopy(session.ice_servers)}

    def _get(self, session_id: str) -> _Session:
        session = self._sessions.get(session_id)
        if session is None:
            raise MediaError("session_not_found", "媒体会话不存在或已经结束", 404)
        return session

    def status(self, session_id: str) -> dict:
        session = self._get(session_id)
        result = self._created(session)
        if session.source and session.source.size:
            result.update(width=session.source.size[0], height=session.source.size[1])
        error = session.error or (session.source.error if session.source else None)
        if error:
            result.update(state="failed", code=error.code, message=error.message)
        result["cleanup_pending"] = session.closing
        return result

    def _schedule_close(self, session: _Session) -> None:
        if not session.closing and (session.close_task is None or session.close_task.done()):
            session.close_task = asyncio.create_task(self.close(session.id))

    async def offer(self, session_id: str, sdp: str) -> dict:
        session = self._get(session_id)
        if not isinstance(sdp, str) or not 100 <= len(sdp) <= 64_000 or "m=video " not in sdp or "a=recvonly" not in sdp:
            raise MediaError("invalid_offer", "需要有效的只接收视频 SDP", 422)
        async with session.operation_lock:
            if session.closing:
                raise MediaError("session_closed", "媒体会话已结束")
            if session.answer is not None:
                if session.offer_sdp != sdp:
                    raise MediaError("offer_conflict", "此会话已经使用另一份视频协商请求")
                return dict(session.answer)
            if session.peer is not None:
                raise MediaError("offer_conflict", "此会话已经开始视频协商")
            try:
                from aiortc import RTCConfiguration, RTCIceServer, RTCPeerConnection, RTCSessionDescription, RTCRtpSender
                from .vendor.h264_track import ScrcpyH264Track
            except ImportError as exc:
                raise MediaError("media_dependencies_missing", "执行器缺少 aiortc/PyAV 视频依赖", 503) from exc
            session.offer_sdp, session.state = sdp, "starting"
            try:
                await asyncio.wait_for(self._starting.acquire(), timeout=15)
            except asyncio.TimeoutError as exc:
                session.state, session.offer_sdp = "created", None
                raise MediaError("media_start_busy", "其他画面正在启动，请稍后手动重试", 429) from exc
            try:
                if session.closing:
                    raise MediaError("session_closed", "媒体会话已结束")
                loop = asyncio.get_running_loop()
                session.track = ScrcpyH264Track(loop)
                pc = RTCPeerConnection(RTCConfiguration(iceServers=[RTCIceServer(**entry) for entry in session.ice_servers]))
                session.peer = pc
                sender = pc.addTrack(session.track)
                transceiver = next(item for item in pc.getTransceivers() if item.sender is sender)
                codecs = [codec for codec in RTCRtpSender.getCapabilities("video").codecs
                          if codec.mimeType.lower() == "video/h264"]
                if not codecs:
                    raise MediaError("h264_unavailable", "执行器不支持 H.264 WebRTC", 503)
                transceiver.setCodecPreferences(codecs)

                @pc.on("connectionstatechange")
                async def connection_changed() -> None:
                    if not session.closing and pc.connectionState in ("failed", "closed"):
                        session.error = MediaError("media_disconnected", "浏览器视频连接已结束，请手动重开", 502)
                        self._schedule_close(session)

                def source_ended() -> None:
                    try:
                        loop.call_soon_threadsafe(self._schedule_close, session)
                    except RuntimeError:
                        pass

                session.source = ScrcpySource(session.serial, session.quality, session.track, source_ended, session.options)
                await asyncio.wait_for(pc.setRemoteDescription(RTCSessionDescription(sdp=sdp, type="offer")), 12)
                answer = await asyncio.wait_for(pc.createAnswer(), 12)
                await asyncio.wait_for(pc.setLocalDescription(answer), 20)
                if session.closing:
                    raise MediaError("session_closed", "媒体会话已结束")
                session.source.start()
                ready = await asyncio.to_thread(session.source.ready.wait, 30)
                if session.source.error:
                    error = session.source.error
                    raise MediaError(error.code, error.message, error.status)
                if not ready or session.source.finished.is_set() or session.closing or not session.source.size:
                    raise MediaError("media_start_timeout", "未取得手机的可用画面，请检查设备后手动重开", 502)
                width, height = session.source.size
                session.answer = {"sdp": pc.localDescription.sdp, "type": "answer", "width": width, "height": height}
                session.state = "streaming"
                return dict(session.answer)
            except asyncio.CancelledError:
                self._schedule_close(session)
                raise
            except MediaError as exc:
                session.error = exc
                await self.close(session_id)
                raise
            except Exception as exc:
                session.error = MediaError("media_negotiation_failed", "视频协商失败，请检查 H.264 和 ICE/TURN 可达性", 502)
                await self.close(session_id)
                raise session.error from exc
            finally:
                self._starting.release()

    async def command(self, session_id: str, message: dict) -> dict:
        session = self._get(session_id)
        source = session.source
        if session.closing or source is None or source.finished.is_set() or not source.ready.is_set():
            raise MediaError("media_not_ready", "手机画面和控制通道尚未就绪")
        kind = message.get("type") if isinstance(message, dict) else None
        try:
            if kind == "touch":
                await asyncio.to_thread(source.touch, message)
            elif kind in ("touch.release", "stop"):
                await asyncio.to_thread(source.release)
            elif kind == "key":
                if not isinstance(message.get("key"), str):
                    raise MediaError("invalid_key", "导航键无效", 422)
                await asyncio.to_thread(source.key, message["key"])
            elif kind == "input_text":
                raise MediaError("text_adapter_required", "文字输入需要执行器控件适配器", 422)
            else:
                raise MediaError("invalid_command", "不支持此控制命令", 422)
        except SourceError as exc:
            raise MediaError(exc.code, exc.message, exc.status) from exc
        return {"type": "ack", "command_id": message.get("command_id"), "state": "action_sent"}

    async def close(self, session_id: str) -> dict:
        session = self._sessions.get(session_id)
        if session is None:
            return {"id": session_id, "state": "closed", "cleanup_pending": False}
        async with session.close_lock:
            session.closing, session.state = True, "closing"
            source = session.source
            if source is not None:
                await asyncio.to_thread(source.stop)
                await asyncio.to_thread(source.join, 15)
                if source.finished.is_set() and not source.cleanup_complete:
                    await asyncio.to_thread(source.retry_cleanup)
            if session.track is not None:
                session.track.stop()
            peer_clean = True
            if session.peer is not None:
                try:
                    await asyncio.wait_for(session.peer.close(), 8)
                except Exception:
                    peer_clean = False
            source_clean = source is None or source.thread is None or (source.finished.is_set() and source.cleanup_complete)
            clean = peer_clean and source_clean
            if clean:
                async with self._lock:
                    if self._sessions.get(session_id) is session:
                        del self._sessions[session_id]
            session.state = "closed" if clean else "cleanup_pending"
            result = {"id": session_id, "state": session.state, "cleanup_pending": not clean}
            if source is not None and source._release_failed:
                result["input_release_unconfirmed"] = True
            return result

    async def close_all(self) -> None:
        await asyncio.gather(*(self.close(session_id) for session_id in list(self._sessions)), return_exceptions=True)
