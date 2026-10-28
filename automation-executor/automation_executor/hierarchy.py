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
                        # Missing XML attributes are unknown, not implicitly False.
                        "long_clickable": raw.get("long-clickable") == "true" if "long-clickable" in raw else None,
                        "focusable": raw.get("focusable") == "true" if "focusable" in raw else None,
                        "checkable": raw.get("checkable") == "true" if "checkable" in raw else None,
                        "selected": raw.get("selected") == "true",
                        "checked": raw.get("checked") == "true",
                        "password": raw.get("password") == "true",
                        "bounds": {"left": left, "top": top, "right": right, "bottom": bottom},
                        # XPath uses ORIGINAL bounds, not clamped visible geometry.
                        "raw_bounds": raw.get("bounds", ""),
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


def xpath_match(xml, expression, width, height):
    """Evaluate an XPath 1.0 element selector against one bounded UI snapshot.

    This is read-only; returns a unique on-screen bounds dict or raises a
    controlled exception. The caller sends a click only after this completes.
    """
    from lxml import etree

    if not isinstance(expression, str) or not 1 <= len(expression.encode("utf-8")) <= 512 or not expression.startswith(("/", "(")):
        raise ExecutorError("xpath_invalid", "XPath 必须以 / 或 ( 开头，且不超过 512 字节", 422)
    if not isinstance(xml, str) or len(xml.encode("utf-8")) > MAX_XML_BYTES or width <= 0 or height <= 0:
        raise ExecutorError("hierarchy_too_large", "控件树或屏幕尺寸无效", 502)
    if "<!DOCTYPE" in xml.upper() or "<!ENTITY" in xml.upper():
        raise ExecutorError("hierarchy_invalid", "控件树包含不支持的声明", 502)
    try:
        parser = etree.XMLParser(resolve_entities=False, no_network=True, load_dtd=False, huge_tree=False)
        root = etree.fromstring(xml.encode("utf-8"), parser=parser)
        if root.tag != "hierarchy":
            raise ExecutorError("hierarchy_invalid", "控件树根节点无效", 502)
        count = 0
        for element in root.iter("node"):
            count += 1
            if count > MAX_ELEMENTS:
                raise ExecutorError("hierarchy_too_large", "控件数量超出 XPath 安全上限", 502)
            # Match UiAutomator2's conventional //android.widget.Button paths.
            tag = element.get("class", "")
            if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_.-]{0,255}", tag):
                element.tag = tag
        result = etree.XPath(expression, smart_strings=False)(root)
    except (etree.XMLSyntaxError, etree.XPathError, ValueError, TypeError):
        raise ExecutorError("xpath_invalid", "控件树或 XPath 表达式无效", 422) from None

    if not isinstance(result, list) or any(not isinstance(node, etree._Element) for node in result):
        raise ExecutorError("xpath_invalid", "XPath 必须返回控件元素", 422)
    if len(result) > 1:
        raise ExecutorError("selector_ambiguous", "XPath 匹配多个控件，请缩小条件", 409)
    if not result:
        return None
    match = BOUNDS.fullmatch(result[0].get("bounds", ""))
    if not match:
        raise ExecutorError("selector_geometry_invalid", "XPath 控件缺少有效区域", 422)
    left, top, right, bottom = map(int, match.groups())
    left, top = max(0, left), max(0, top)
    right, bottom = min(width, right), min(height, bottom)
    if left >= right or top >= bottom:
        raise ExecutorError("selector_geometry_invalid", "XPath 控件不在有效屏幕内", 422)
    return {"left": left, "top": top, "right": right, "bottom": bottom}
