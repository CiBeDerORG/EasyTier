"""Finite device actions executed in a separate, bounded worker process.

The pinned uiautomator2 low-level RPC is intentionally used without its
restart-and-retry wrapper: a failed response must never repeat a click/input.
"""
import base64
import os
from pathlib import Path
from .validation import adb_shell_tokens
import time


class StepFailure(Exception):
    def __init__(self, code, message, outcome_unknown=False):
        self.code, self.message, self.outcome_unknown = code, message, outcome_unknown
        super().__init__(message)


def execute(target, step, parameters, artifact_path=None):
    import adbutils

    address = f'{target["ip"]}:{target["adb_port"]}'
    dev = adbutils.adb.device(serial=address)
    if dev.get_state() != "device":
        raise StepFailure("adb_unauthorized", "手机未连接或未授权")
    kind = step["type"]
    # Native-only actions must not call UiAutomator's RPC or attempt to
    # bootstrap its service. Every shell command targets the exact serial.
    if kind in ("key", "launch_app", "adb_command"):
        try:
            if kind == "key":
                code = {"HOME": 3, "BACK": 4, "RECENT": 187, "WAKE": 224}[step["key"]]
                response = dev.shell2(["input", "keyevent", str(code)], timeout=8)
                if response.returncode != 0:
                    raise StepFailure("device_action_unconfirmed", "按键是否执行无法确认，请查看手机",
                                      outcome_unknown=True)
            elif kind == "adb_command":
                argv = adb_shell_tokens(step["command"])
                response = dev.shell2(argv, timeout=step["timeout_ms"] / 1000)
                if response.returncode != 0:
                    raise StepFailure("adb_command_failed", "ADB 命令执行失败（退出码 " + str(response.returncode) + "）")
                output = response.output.strip().replace("\x00", "")
                return {"state": "action_sent",
                        "message": ("ADB 命令已执行" + ("；输出：" + output[:300] if output else ""))}
            else:
                response = dev.shell2(["am", "start", "-a", "android.intent.action.MAIN",
                                       "-c", "android.intent.category.LAUNCHER", "-p", step["package"]], timeout=10)
                if response.returncode != 0 or "Error:" in response.output:
                    raise StepFailure("app_start_failed", "无法打开指定应用，请检查是否已安装且提供启动入口")
        except StepFailure:
            raise
        except Exception:
            raise StepFailure("device_action_unconfirmed",
                              "ADB 动作是否执行无法确认，请检查手机后处理",
                              outcome_unknown=True) from None
        return {"state": "action_sent", "message": "动作已交给设备；应用结果需通过后续断言确认"}

    from uiautomator2.core import DEFAULT_SERVER_PORT, _jsonrpc_call
    from uiautomator2._selector import Selector
    def rpc(method, *params, timeout=8):
        # No exception text/params escape into logs or user-facing errors.
        readonly = method in ("count", "objInfo", "deviceInfo", "takeScreenshot", "dumpWindowHierarchy")
        try:
            result = _jsonrpc_call(dev, DEFAULT_SERVER_PORT, method, params, timeout, False)
        except Exception:
            raise StepFailure("capability_unavailable" if readonly else "device_action_unconfirmed",
                              "控件组件不可用，请检查手机后手动重新准备" if readonly else "设备动作结果未确认，未重复发送",
                              outcome_unknown=not readonly) from None
        if result is False and not readonly:
            raise StepFailure("action_rejected", "设备拒绝执行该动作")
        return result

    # Do not run an unrelated generic count probe before every action. Native
    # selectors already count in find(); XPath selectors need a hierarchy read.
    # The extra RPC used to consume up to five seconds before the XPath RPC and
    # made the supervisor kill valid XPath actions before their own timeout.
    def find(raw, timeout_ms=0):
        translated = {"resource_id": "resourceId", "description": "description", "text": "text",
                      "class_name": "className", "package": "packageName", "clickable": "clickable",
                      "enabled": "enabled", "long_clickable": "longClickable",
                      "focusable": "focusable", "checkable": "checkable", "scrollable": "scrollable"}
        selected = None if "xpath" in raw else Selector(**{translated[key]: value for key, value in raw.items()})
        deadline = time.monotonic() + timeout_ms / 1000
        while True:
            if selected is None:
                from .hierarchy import xpath_match
                from .errors import ExecutorError
                width, height = dev.window_size()
                xml = rpc("dumpWindowHierarchy", False, 50, timeout=12)
                try:
                    bounds = xpath_match(xml, raw["xpath"], width, height)
                except ExecutorError as exc:
                    raise StepFailure(exc.code, exc.message) from None
                if bounds is not None:
                    return {"xpath_bounds": bounds}
            else:
                count = rpc("count", selected)
                if type(count) is not int or count < 0:
                    raise StepFailure("selector_count_invalid", "控件组件返回了无法确认的匹配数量")
                if count == 1:
                    return selected
                if isinstance(count, int) and count > 1:
                    raise StepFailure("selector_ambiguous", "控件条件匹配多项，请缩小条件")
            if time.monotonic() >= deadline:
                raise StepFailure("selector_not_found", "在规定时间内未找到控件")
            time.sleep(min(.2, max(0, deadline - time.monotonic())))

    if kind == "locator_verify":
        # Strictly read-only: recapture the hierarchy and match exactly one
        # element against the user's ORIGINAL selected screen bounds.
        from .hierarchy import xpath_match
        from .errors import ExecutorError
        width, height = dev.window_size()
        xml = rpc("dumpWindowHierarchy", False, 50, timeout=12)
        if step.get("diagnose") is True:
            from .hierarchy import xpath_diagnose
            report=xpath_diagnose(xml,step["xpath"],width,height)
            bounds=xpath_match(xml,step["xpath"],width,height) if report["count"]==1 else None
            return {"device_id":target["device_id"],"target_fingerprint":target["target_fingerprint"],
                    "verified":report["count"]==1 and bounds is not None,
                    "reason":"unique_preview" if report["count"]==1 else
                             "ambiguous" if report["count"]>1 else "selector_not_found",
                    "bounds":bounds,**report}
        try:
            bounds = xpath_match(xml, step["xpath"], width, height)
        except ExecutorError as exc:
            if exc.code == "selector_ambiguous":
                return {"device_id": target["device_id"], "target_fingerprint": target["target_fingerprint"],
                        "verified": False, "reason": "ambiguous", "bounds": None}
            raise StepFailure(exc.code, exc.message) from None
        same_size = width == step["width"] and height == step["height"]
        if step.get("preview_only") is True:
            # A saved XPath may be previewed without knowing its old bounds;
            # this proves CURRENT uniqueness only, not business identity.
            # XPath is selector-based, NOT a recording of the source-phone
            # viewport. The hierarchy parser already validates returned bounds
            # against the live device size; don't require another snapshot to
            # have identical dimensions for read-only uniqueness checking.
            unique = bounds is not None
            return {"device_id": target["device_id"], "target_fingerprint": target["target_fingerprint"],
                    "verified": bool(unique), "reason": "unique_preview" if unique else "selector_not_found",
                    "bounds": bounds if unique else None}
        expected = step["expected_bounds"]
        # A different display or moved target requires a new user pick.
        same = bounds is not None and same_size and all(
            abs(bounds[k] - expected[k]) <= 3 for k in ("left", "top", "right", "bottom"))
        return {"device_id": target["device_id"], "target_fingerprint": target["target_fingerprint"],
                "verified": bool(same), "reason": "verified" if same else "snapshot_changed",
                "bounds": bounds if same else None}

    if kind == "hierarchy":
        # Internal read-only snapshot: rejected by public script validation.
        from .hierarchy import parse_hierarchy
        width, height = dev.window_size()
        xml = rpc("dumpWindowHierarchy", False, 50, timeout=12)
        from datetime import datetime, timezone
        return {
            "device_id": target["device_id"],
            "target_fingerprint": target["target_fingerprint"],
            "captured_at": datetime.now(timezone.utc).isoformat(),
            "snapshot": parse_hierarchy(xml, width, height),
        }
    if kind in ("wait_for", "assert"):
        find(step["selector"], step["timeout_ms"])
        return {"state": "assertion_passed", "message": "指定控件条件检查通过"}

    def probe(raw):
        """A single, non-mutating presence check; never suppress transport errors."""
        try:
            return find(raw) is not None
        except StepFailure as exc:
            if exc.code == "selector_not_found":
                return False
            if exc.code == "selector_ambiguous":
                return True  # More than one element still means it is present.
            raise

    if kind == "if_else":
        # One read-only observation. Multiple matches mean present, never absent.
        present = probe(step["selector"])
        return {
            "state": "condition_passed",
            "branch_taken": "else" if present else "then",
            "message": "控件仍存在，执行 Else 分支" if present else "控件已消失，执行 Then 分支",
        }

    if kind == "if_gone":
        # A one-arm conditional: execute following N steps when gone;
        # if still present, the supervisor marks them skipped explicitly.
        present = probe(step["selector"])
        return {
            "state": "condition_passed",
            "message": "控件仍存在，跳过条件区域" if present else "控件已消失，进入条件区域",
            "skip_steps": step["skip_count"] if present else 0,
            "branch_taken": "skip" if present else "then",
        }

    if kind == "wait_gone":
        deadline = time.monotonic() + step["timeout_ms"] / 1000
        while True:
            if not probe(step["selector"]):
                return {"state": "assertion_passed", "message": "控件已消失"}
            if time.monotonic() >= deadline:
                raise StepFailure("wait_gone_timeout", "等待控件消失超时")
            time.sleep(min(.25, max(0, deadline - time.monotonic())))

    if kind == "assert_property":
        selected = find(step["selector"])
        details = rpc("objInfo", selected)
        name = step["property"]
        # Reject missing/nullable fields rather than coercing them to False.
        actual = details.get(name) if isinstance(details, dict) else None
        if type(actual) is not bool:
            raise StepFailure("property_unavailable", "手机未返回可确认的控件布尔属性")
        if actual is not step["expected"]:
            raise StepFailure("property_mismatch", "控件属性与预期值不一致")
        return {"state": "assertion_passed", "message": "控件布尔属性检查通过"}

    if kind == "scroll_find":
        deadline = time.monotonic() + step["timeout_ms"] / 1000
        swipes = 0
        while True:
            # A target is tested before any swipe, preventing needless movements.
            try:
                found = find(step["selector"])
            except StepFailure as exc:
                if exc.code != "selector_not_found":
                    raise
                found = None
            if found is not None:
                return {"state": "assertion_passed", "message": "滚动查找已找到唯一控件", "swipes": swipes}
            if swipes >= step["max_swipes"] or time.monotonic() >= deadline:
                raise StepFailure("scroll_target_not_found", "已达到滚动次数或等待上限，仍未找到目标控件")
            container = find(step["scroll_container"])
            details = rpc("objInfo", container)
            if not isinstance(details, dict) or details.get("scrollable") is not True:
                raise StepFailure("scroll_container_invalid", "指定控件不是可滚动容器")
            bounds = details.get("visibleBounds") or details.get("bounds")
            if not isinstance(bounds, dict) or not all(type(bounds.get(k)) is int
                                                     for k in ("left", "top", "right", "bottom")):
                raise StepFailure("scroll_container_invalid", "滚动容器范围无效")
            width, height = dev.window_size()
            left, top = max(0, bounds["left"]), max(0, bounds["top"])
            right, bottom = min(width, bounds["right"]), min(height, bounds["bottom"])
            if right - left < 24 or bottom - top < 100:
                raise StepFailure("scroll_container_invalid", "滚动容器尺寸过小")
            if time.monotonic() + step["duration_ms"] / 1000 + .2 >= deadline:
                raise StepFailure("scroll_target_not_found", "滚动剩余等待时间不足")
            x = (left + right) // 2
            start = top + (bottom - top) * (80 if step["direction"] == "down" else 20) // 100
            end = top + (bottom - top) * (20 if step["direction"] == "down" else 80) // 100
            # One explicit gesture per iteration. If its result is uncertain,
            # do not repeat it or silently advance the list.
            rpc("swipe", x, start, x, end, max(1, step["duration_ms"] // 5))
            swipes += 1
            time.sleep(min(.3, max(0, deadline - time.monotonic())))

    if kind in ("tap", "long_press") and "selector" in step:
        selected = find(step["selector"])
        if isinstance(selected, dict) and "xpath_bounds" in selected:
            bounds = selected["xpath_bounds"]
        else:
            info = rpc("objInfo", selected)
            bounds = info.get("visibleBounds") or info.get("bounds")
        if not isinstance(bounds, dict) or not all(isinstance(bounds.get(k), int) for k in ("left", "top", "right", "bottom")):
            raise StepFailure("selector_geometry_invalid", "无法确定控件的有效点击区域")
        if bounds["right"] <= bounds["left"] or bounds["bottom"] <= bounds["top"]:
            raise StepFailure("selector_geometry_invalid", "控件没有有效点击区域")
        center_x = (bounds["left"] + bounds["right"]) // 2
        center_y = (bounds["top"] + bounds["bottom"]) // 2
        if kind == "long_press":
            # uiautomator2 JSON-RPC click(x, y, hold_ms).
            rpc("click", center_x, center_y, step["duration_ms"])
        else:
            rpc("click", center_x, center_y)
    elif kind in ("tap", "swipe", "long_press", "drag"):
        width, height = dev.window_size()
        # Cross-device safety must be enforced in the executor as well as
        # the web preflight: identical aspect ratios are NOT equivalent
        # reference frames. Never silently rescale a recorded coordinate.
        if (width != step["width"] or height != step["height"]):
            raise StepFailure("geometry_changed",
                              "设备实际屏幕宽高与步骤参考画面不一致，禁止缩放坐标；请重新拾取或录制")
        x, y = min(width - 1, round(step["x"] * width)), min(height - 1, round(step["y"] * height))
        if kind == "tap":
            rpc("click", x, y)
        elif kind == "long_press":
            rpc("click", x, y, step["duration_ms"])
        else:
            tx, ty = min(width - 1, round(step["to_x"] * width)), min(height - 1, round(step["to_y"] * height))
            if kind == "drag":
                if x == tx and y == ty:
                    raise StepFailure("drag_zero_distance", "拖拽起点与终点不能相同")
                rpc("drag", x, y, tx, ty, max(2, step["duration_ms"] // 5))
            else:
                rpc("swipe", x, y, tx, ty, max(2, step["duration_ms"] // 5))
    elif kind == "input_text":
        value = step["text"] if "text" in step else parameters[step["parameter"]]
        selected = find(step["selector"]) if "selector" in step else Selector(focused=True)
        if "selector" not in step and rpc("count", selected) != 1:
            raise StepFailure("input_target_missing", "请先选择唯一的文字输入框")
        # Setting text is Unicode-capable; no clipboard, IME fallback or retries.
        if value:
            rpc("setText", selected, value)
        else:
            rpc("clearTextField", selected)
    elif kind == "screenshot":
        encoded = rpc("takeScreenshot", 1, 80)
        if not isinstance(encoded, str) or not encoded or len(encoded) > 12 * 1024 * 1024:
            raise StepFailure("screenshot_failed", "未取得有效截图或截图超过容量限制")
        data = base64.b64decode(encoded, validate=True)
        if len(data) > 8 * 1024 * 1024:
            raise StepFailure("screenshot_failed", "截图超过 8MiB 上限")
        if not (data.startswith(b"\xff\xd8") or data.startswith(b"\x89PNG\r\n\x1a\n")):
            raise StepFailure("screenshot_failed", "截图格式不受支持")
        path = Path(artifact_path)
        with path.open("xb") as handle:
            handle.write(data)
        os.chmod(path, 0o600)
        return {"state": "completed", "message": "截图已保存，仅作为该时刻的画面证据",
                "artifact_size": len(data), "mime_type": "image/png" if data.startswith(b"\x89PNG") else "image/jpeg"}
    else:
        raise StepFailure("unsupported_step", "不支持该脚本动作")
    return {"state": "action_sent", "message": "动作已交给设备；应用结果需通过后续断言确认"}
