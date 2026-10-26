"""Bounded, read-only conversion of an Android UiAutomator hierarchy snapshot."""
import re
from xml.etree import ElementTree

from .errors import ExecutorError

BOUNDS = re.compile(r"^\[(-?\d+),(-?\d+)\]\[(-?\d+),(-?\d+)\]$")
MAX_XML_BYTES = 512 * 1024
MAX_ELEMENTS = 1500


def selector_text(value, maximum=512):
    """Only emit complete, usable selector strings; never truncate an exact match."""
    if not value or not value.strip() or len(value.encode("utf-8")) > maximum:
        return ""
    return value


def parse_hierarchy(xml, width, height):
    if not isinstance(xml, str) or not isinstance(width, int) or not isinstance(height, int):
        raise ExecutorError("hierarchy_invalid", "手机返回的控件树格式无效", 502)
    if width <= 0 or height <= 0 or len(xml.encode("utf-8")) > MAX_XML_BYTES:
        raise ExecutorError("hierarchy_too_large", "控件树过大或屏幕尺寸无效，请缩小页面内容", 502)
    if "<!DOCTYPE" in xml.upper() or "<!ENTITY" in xml.upper():
        raise ExecutorError("hierarchy_invalid", "手机返回的控件树包含不支持的声明", 502)
    try:
        root = ElementTree.fromstring(xml)
    except ElementTree.ParseError:
        raise ExecutorError("hierarchy_invalid", "无法解析手机控件树", 502) from None
    if root.tag != "hierarchy":
        raise ExecutorError("hierarchy_invalid", "手机控件树根节点无效", 502)

    nodes = []
    truncated = False

    def walk(element, parent_id, depth):
        nonlocal truncated
        if len(nodes) >= MAX_ELEMENTS:
            truncated = True
            return
        parent = parent_id
        if element.tag == "node":
            raw = element.attrib
            match = BOUNDS.fullmatch(raw.get("bounds", ""))
            if match:
                left, top, right, bottom = map(int, match.groups())
                left, right = max(0, left), min(width, right)
                top, bottom = max(0, top), min(height, bottom)
                if left < right and top < bottom:
                    node_id = len(nodes)
                    nodes.append({
                        "id": node_id, "parent_id": parent_id, "depth": depth,
                        "class_name": raw.get("class", "")[:256],
                        "package": raw.get("package", "")[:256],
                        "resource_id": selector_text(raw.get("resource-id", "")),
                        "text": "" if raw.get("password") == "true" else selector_text(raw.get("text", "")),
                        "description": "" if raw.get("password") == "true" else selector_text(raw.get("content-desc", "")),
                        "clickable": raw.get("clickable") == "true",
                        "enabled": raw.get("enabled") == "true",
                        "scrollable": raw.get("scrollable") == "true",
                        "selected": raw.get("selected") == "true",
                        "checked": raw.get("checked") == "true",
                        "password": raw.get("password") == "true",
                        "bounds": {"left": left, "top": top, "right": right, "bottom": bottom},
                    })
                    parent = node_id
        for child in element:
            if truncated:
                break
            walk(child, parent, depth + 1)

    for child in root:
        walk(child, None, 0)
        if truncated:
            break
    return {"width": width, "height": height, "nodes": nodes, "truncated": truncated}
