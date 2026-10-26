"""Closed wire schema; no caller-controlled shell strings or network fallbacks."""
import hashlib
import ipaddress
import json
import math
import re
from uuid import UUID

from .errors import invalid


def object_fields(value, allowed, required=()):
    if not isinstance(value, dict) or set(value) - set(allowed) or any(k not in value for k in required):
        raise invalid("请求缺少字段或包含不支持的字段")
    return value


def identifier(value):
    if not isinstance(value, str):
        raise invalid("标识必须为 UUID")
    try:
        return str(UUID(value))
    except (ValueError, AttributeError):
        raise invalid("标识必须为 UUID") from None


def integer(value, low, high):
    if type(value) is not int or not low <= value <= high:
        raise invalid("数值超出允许范围")
    return value


def text(value, maximum=512, empty=False):
    if not isinstance(value, str) or len(value.encode("utf-8")) > maximum or (not empty and not value.strip()):
        raise invalid("文本为空或超过长度限制")
    if "\x00" in value:
        raise invalid("文本包含不支持的字符")
    return value


def target(value):
    object_fields(value, {"device_id", "name", "owner_user_id", "machine_id", "ip", "adb_port",
                          "connection_mode", "target_fingerprint", "route_context", "adb_service_id", "endpoint_revision"},
                  {"device_id", "name", "owner_user_id", "machine_id", "ip", "adb_port",
                   "connection_mode", "target_fingerprint", "route_context"})
    result = dict(value)
    if "adb_service_id" in value:
        result["adb_service_id"] = identifier(value["adb_service_id"])
        integer(value.get("endpoint_revision"), 0, 2 ** 63 - 1)
    result["device_id"] = identifier(value["device_id"])
    result["machine_id"] = identifier(value["machine_id"])
    result["name"] = text(value["name"], 512)
    integer(value["owner_user_id"], 1, 2 ** 31 - 1)
    integer(value["adb_port"], 1, 65535)
    try:
        ip = ipaddress.IPv4Address(value["ip"])
    except (ipaddress.AddressValueError, TypeError):
        raise invalid("设备地址必须为 IPv4") from None
    if (ip.is_unspecified or ip.is_loopback or ip.is_multicast or ip.is_link_local
            or ip.is_reserved or str(ip) == "255.255.255.255"):
        raise invalid("不允许使用该设备地址")
    result["ip"] = str(ip)
    if value["route_context"] != "default" or value["connection_mode"] not in ("tcp", "wireless"):
        raise invalid("执行器不支持该路由域或连接模式")
    result["target_fingerprint"] = text(value["target_fingerprint"], 256)
    return result


def endpoint(value):
    return f'{value["route_context"]}|{value["ip"]}:{value["adb_port"]}'


def serial(value):
    return f'{value["ip"]}:{value["adb_port"]}'


def digest(value):
    return hashlib.sha256(json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def selector(value):
    object_fields(value, {"resource_id", "description", "text", "class_name", "package", "xpath"})
    if not value or ("xpath" in value and len(value) != 1):
        raise invalid("XPath 与普通控件条件不能混用")
    if "xpath" in value:
        expression = text(value["xpath"], 512)
        if not expression.startswith(("/", "(")):
            raise invalid("XPath 必须以 / 或 ( 开头")
        return {"xpath": expression}
    return {key: text(item, 512) for key, item in value.items()}


def coordinate(value):
    if isinstance(value, bool) or not isinstance(value, (float, int)) or not math.isfinite(value) or not 0 <= value <= 1:
        raise invalid("坐标必须为 0 到 1 之间的有限数值")


def steps(value):
    if not isinstance(value, list) or not 1 <= len(value) <= 100:
        raise invalid("脚本须包含 1 到 100 个步骤")
    result, ids = [], set()
    allowed = {
        "key": {"key"}, "tap": {"selector", "x", "y", "width", "height"},
        "long_press": {"selector", "x", "y", "width", "height", "duration_ms"},
        "drag": {"x", "y", "to_x", "to_y", "width", "height", "duration_ms"},
        "swipe": {"x", "y", "to_x", "to_y", "duration_ms", "width", "height"},
        "input_text": {"text", "parameter", "selector"}, "launch_app": {"package"},
        "wait_for": {"selector", "timeout_ms"}, "assert": {"selector", "timeout_ms"},
        "wait_gone": {"selector", "timeout_ms"},
        "if_gone": {"selector", "skip_count"},
        "assert_property": {"selector", "property", "expected"},
        "scroll_find": {"selector", "scroll_container", "direction", "max_swipes", "duration_ms", "timeout_ms"},
        "screenshot": set(),
    }
    for item in value:
        if not isinstance(item, dict) or item.get("type") not in allowed:
            raise invalid("脚本包含不支持的动作")
        kind = item["type"]
        object_fields(item, allowed[kind] | {"id", "type"}, {"id", "type"})
        sid = identifier(item["id"])
        if sid in ids:
            raise invalid("步骤标识重复")
        ids.add(sid)
        if kind == "key" and item.get("key") not in ("HOME", "BACK", "RECENT", "WAKE"):
            raise invalid("不支持该按键")
        if kind in ("tap", "long_press") and "selector" in item:
            selector(item["selector"])
            if any(key in item for key in ("x", "y", "width", "height")):
                raise invalid("控件点击与坐标点击不能混用")
        elif kind in ("tap", "long_press", "swipe", "drag"):
            for key in (("x", "y") if kind in ("tap", "long_press") else ("x", "y", "to_x", "to_y")):
                coordinate(item.get(key))
            integer(item.get("width"), 1, 16384)
            integer(item.get("height"), 1, 16384)
            if kind == "swipe":
                integer(item.get("duration_ms"), 100, 3000)
        if kind == "long_press":
            integer(item.get("duration_ms"), 400, 3000)
        if kind == "drag":
            integer(item.get("duration_ms"), 300, 3000)
        if kind == "input_text" and isinstance(item.get("selector"), dict) and "xpath" in item["selector"]:
            raise invalid("文字输入暂不支持 XPath，请使用唯一控件条件")
        if kind == "input_text":
            if ("text" in item) == ("parameter" in item):
                raise invalid("输入步骤须选择文字或运行参数")
            if "text" in item:
                text(item["text"], 2000, empty=True)
            else:
                key = text(item["parameter"], 64)
                if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]{0,63}", key):
                    raise invalid("参数名称无效")
            if "selector" in item:
                selector(item["selector"])
        if kind == "launch_app" and not re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z][A-Za-z0-9_]*)+", text(item.get("package"), 256)):
            raise invalid("Android 包名无效")
        if kind in ("wait_for", "assert", "wait_gone", "scroll_find"):
            selector(item.get("selector"))
            integer(item.get("timeout_ms"), 100, 30000)
        if kind == "if_gone":
            selector(item.get("selector"))
            integer(item.get("skip_count"), 1, 20)
            if item["skip_count"] > len(value) - len(result) - 1:
                raise invalid("条件跳过的步骤数量超过脚本剩余步骤")
        if kind == "assert_property":
            chosen = selector(item.get("selector"))
            if "xpath" in chosen:
                raise invalid("属性断言目前只支持原生控件选择器")
            if item.get("property") not in ("enabled", "checked", "selected", "clickable", "scrollable"):
                raise invalid("不支持该布尔控件属性")
            if type(item.get("expected")) is not bool:
                raise invalid("属性断言预期值必须为布尔值")
        if kind == "scroll_find":
            container = selector(item.get("scroll_container"))
            if "xpath" in container:
                raise invalid("滚动容器不能使用 XPath")
            if item.get("direction") not in ("down", "up"):
                raise invalid("滚动方向仅支持向上或向下")
            integer(item.get("max_swipes"), 1, 5)
            integer(item.get("duration_ms"), 200, 1000)
        result.append({**item, "id": sid})
    return result


def run_request(body):
    object_fields(body, {"id", "owner_user_id", "actor_user_id", "name", "steps", "parameters", "concurrency", "targets", "expected_executor_id"},
                  {"id", "owner_user_id", "actor_user_id", "name", "steps", "parameters", "concurrency", "targets", "expected_executor_id"})
    result = dict(body)
    result["id"] = identifier(body["id"])
    result["expected_executor_id"] = identifier(body["expected_executor_id"])
    result["name"] = text(body["name"], 512)
    integer(body["owner_user_id"], 1, 2 ** 31 - 1)
    integer(body["actor_user_id"], 1, 2 ** 31 - 1)
    result["steps"] = steps(body["steps"])
    integer(body["concurrency"], 1, 2 ** 63 - 1)
    if not isinstance(body["parameters"], dict) or len(body["parameters"]) > 30:
        raise invalid("运行参数无效")
    for key, value in body["parameters"].items():
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]{0,63}", key):
            raise invalid("参数名称无效")
        text(value, 2000, empty=True)
    if not isinstance(body["targets"], list) or not 1 <= len(body["targets"]) <= 30:
        raise invalid("每批任务须包含 1 到 30 台设备")
    result["targets"] = [target(item) for item in body["targets"]]
    if any(item["owner_user_id"] != body["owner_user_id"] for item in result["targets"]):
        raise invalid("同一次任务不能混用不同账号")
    if len({item["device_id"] for item in result["targets"]}) != len(result["targets"]) or len({endpoint(item) for item in result["targets"]}) != len(result["targets"]):
        raise invalid("设备或实际连接目标重复")
    for step in result["steps"]:
        if "parameter" in step and step["parameter"] not in body["parameters"]:
            raise invalid("缺少脚本所需运行参数")
    return result
