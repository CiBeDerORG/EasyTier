"""Advanced UiAutomator actions: isolated selection, read-only checks and bounded gestures."""
import sys
import types
import unittest
from unittest.mock import patch
from uuid import uuid4

from automation_executor.steps import execute, StepFailure
from automation_executor.validation import steps
from automation_executor.worker import _step_timeout_seconds


TARGET = {"ip": "10.2.3.4", "adb_port": 33001,
          "device_id": "ci-test-device", "target_fingerprint": "ci-test-target"}
BUTTON = {"resource_id": "app:id/confirm"}
SCROLLER = {"resource_id": "app:id/list"}


def record(kind, **values):
    return {"id": str(uuid4()), "type": kind, **values}


class FakeSelector:
    def __init__(self, **values):
        self.values = values


class FakeDevice:
    def get_state(self):
        return "device"

    def window_size(self):
        return 320, 640


class ActionHarness:
    def __init__(self, count=None, info=None, on_swipe=None, hierarchy=None):
        self.calls = []
        self.hierarchy = hierarchy
        self.count = count or (lambda sel: 1)
        self.info = info or (lambda sel: {})
        self.on_swipe = on_swipe
        self.device = FakeDevice()

    def rpc(self, dev, port, method, params, timeout, auto_restart):
        self.calls.append((method, params))
        if method == "count":
            return self.count(params[0])
        if method == "objInfo":
            return self.info(params[0])
        if method == "dumpWindowHierarchy":
            return self.hierarchy
        if method in ("swipe", "click", "drag"):
            return self.on_swipe() if self.on_swipe else True
        raise AssertionError("Unexpected method: " + method)

    def run(self, step):
        adbutils = types.ModuleType("adbutils")
        adbutils.adb = types.SimpleNamespace(device=lambda serial: self.device)
        core = types.ModuleType("uiautomator2.core")
        core.DEFAULT_SERVER_PORT = 9008
        core._jsonrpc_call = self.rpc
        selector = types.ModuleType("uiautomator2._selector")
        selector.Selector = FakeSelector
        parent = types.ModuleType("uiautomator2")
        parent.__path__ = []
        with patch.dict(sys.modules, {
            "adbutils": adbutils,
            "uiautomator2": parent,
            "uiautomator2.core": core,
            "uiautomator2._selector": selector,
        }):
            return execute(TARGET, step, {})


class AdvancedValidationTests(unittest.TestCase):
    def test_accepts_three_bounded_operations(self):
        value = steps([
            record("wait_gone", selector=BUTTON, timeout_ms=600),
            record("assert_property", selector=BUTTON, property="enabled", expected=False),
            record("scroll_find", selector=BUTTON, scroll_container=SCROLLER,
                   direction="down", max_swipes=3, duration_ms=300, timeout_ms=20000),
        ])
        self.assertEqual(len(value), 3)

    def test_clickable_selector_preserves_false_and_rejects_string(self):
        value = steps([record("wait_for", selector={"description": "领淘金币", "clickable": False},
                              timeout_ms=500)])
        self.assertIs(value[0]["selector"]["clickable"], False)
        self.assertIs(steps([record("assert", selector={"clickable": True}, timeout_ms=500)])[0]
                      ["selector"]["clickable"], True)
        for invalid in ("false", "true", 0, 1, None):
            with self.subTest(value=invalid), self.assertRaises(Exception):
                steps([record("wait_for", selector={"clickable": invalid}, timeout_ms=500)])

    def test_extended_boolean_selectors_accept_only_real_booleans(self):
        for field in ("enabled", "long_clickable", "focusable", "checkable", "scrollable"):
            for state in (True, False):
                item = steps([record("wait_for", selector={"description": "领淘金币", field: state},
                                     timeout_ms=500)])[0]
                self.assertIs(item["selector"][field], state)
            for bad in ("true", "false", 0, 1, None):
                with self.subTest(field=field, value=bad), self.assertRaises(Exception):
                    steps([record("wait_for", selector={field: bad}, timeout_ms=500)])

    def test_rejects_unbounded_and_ambiguous_operations(self):
        valid_scroll = dict(selector=BUTTON, scroll_container=SCROLLER, direction="down",
                            max_swipes=3, duration_ms=300, timeout_ms=20000)
        invalid = [
            record("wait_gone", selector={}, timeout_ms=1000),
            record("assert_property", selector=BUTTON, property="text", expected=True),
            record("assert_property", selector=BUTTON, property="enabled", expected="true"),
            record("assert_property", selector={"xpath": "//*[@text='a']"}, property="enabled", expected=True),
            record("scroll_find", **{**valid_scroll, "max_swipes": 10}),
            record("scroll_find", **{**valid_scroll, "scroll_container": {"xpath": "//*"}}),
            record("scroll_find", **{**valid_scroll, "direction": "left"}),
            record("scroll_find", **{**valid_scroll, "duration_ms": 100}),
        ]
        for item in invalid:
            with self.subTest(kind=item["type"], item=item), self.assertRaises(Exception):
                steps([item])


class AdvancedExecutionTests(unittest.TestCase):
    def test_native_clickable_selector_is_forwarded_as_boolean(self):
        for state in (False, True):
            runner = ActionHarness()
            runner.run(record("wait_for", selector={"description": "领淘金币", "clickable": state}, timeout_ms=500))
            selectors = [params[0].values for method, params in runner.calls if method == "count"]
            self.assertIn({"description": "领淘金币", "clickable": state}, selectors)

    def test_extended_boolean_selectors_are_forwarded_to_native_uiautomator(self):
        names = {"enabled": "enabled", "long_clickable": "longClickable",
                 "focusable": "focusable", "checkable": "checkable", "scrollable": "scrollable"}
        for field, native in names.items():
            for state in (True, False):
                runner = ActionHarness()
                runner.run(record("wait_for", selector={"description": "领淘金币", field: state},
                                  timeout_ms=500))
                selectors = [params[0].values for method, params in runner.calls if method == "count"]
                self.assertIn({"description": "领淘金币", native: state}, selectors)


    def test_relational_verification_is_read_only_and_checks_original_bounds(self):
        xml = '''<hierarchy>
          <node class="android.widget.FrameLayout" bounds="[0,0][320,640]">
           <node class="android.widget.FrameLayout" bounds="[0,100][320,250]">
            <node class="android.widget.TextView" text="每日签到" bounds="[0,110][100,140]"/>
            <node class="android.widget.FrameLayout" content-desc="领淘金币" bounds="[180,160][300,230]"/>
           </node>
           <node class="android.widget.FrameLayout" bounds="[0,260][320,420]">
            <node class="android.widget.TextView" text="浏览商品" bounds="[0,270][100,310]"/>
            <node class="android.widget.FrameLayout" content-desc="领淘金币" bounds="[180,330][300,400]"/>
           </node>
          </node></hierarchy>'''
        xpath = "//*[@text='浏览商品']/ancestor::*[1]//*[@content-desc='领淘金币']"
        expected = {"left": 180, "top": 330, "right": 300, "bottom": 400}
        args = dict(xpath=xpath, expected_bounds=expected, width=320, height=640, timeout_ms=20000)
        runner = ActionHarness(hierarchy=xml)
        result = runner.run(record("locator_verify", **args))
        self.assertTrue(result["verified"])
        self.assertEqual(result["bounds"], expected)
        self.assertNotIn("click", [m for m,_ in runner.calls])
        self.assertNotIn("setText", [m for m,_ in runner.calls])
        changed = ActionHarness(hierarchy=xml).run(record("locator_verify",
            **{**args, "expected_bounds": {**expected, "top": 140}}))
        self.assertFalse(changed["verified"])
        ambiguous = ActionHarness(hierarchy=xml).run(record("locator_verify",
            **{**args, "xpath": "//*[@content-desc='领淘金币']"}))
        self.assertFalse(ambiguous["verified"])
        self.assertEqual(ambiguous["reason"], "ambiguous")

    def test_saved_xpath_preview_is_read_only_and_uses_current_geometry(self):
        xml = '''<hierarchy><node class="android.widget.FrameLayout" bounds="[0,0][320,640]">
          <node class="android.widget.TextView" text="唯一锚点" bounds="[0,0][80,40]"/>
          <node class="android.widget.FrameLayout" content-desc="领淘金币" bounds="[40,80][160,140]"/>
        </node></hierarchy>'''
        xpath = "//*[@content-desc='领淘金币']"
        runner = ActionHarness(hierarchy=xml)
        preview = runner.run(record("locator_verify", xpath=xpath, width=320, height=640,
                                    preview_only=True, timeout_ms=20000))
        self.assertTrue(preview["verified"])
        self.assertEqual(preview["reason"], "unique_preview")
        self.assertEqual(preview["bounds"], {"left":40,"top":80,"right":160,"bottom":140})
        self.assertTrue(all(m in ("count", "dumpWindowHierarchy") for m,_ in runner.calls))

    def test_xpath_tap_skips_unrelated_count_probe_and_clicks_matched_bounds(self):
        xml = '''<hierarchy>
          <node class="android.widget.FrameLayout" bounds="[0,0][320,640]">
            <node class="android.widget.Button" text="保存" bounds="[20,50][140,120]"/>
          </node>
        </hierarchy>'''
        runner = ActionHarness(hierarchy=xml)
        result = runner.run(record("tap", selector={"xpath": "//*[@text='保存']"}))
        self.assertEqual(result["state"], "action_sent")
        self.assertEqual([method for method, _ in runner.calls],
                         ["dumpWindowHierarchy", "click"])
        self.assertEqual(runner.calls[-1], ("click", (80, 85)))

    def test_step_timeout_allows_complete_rpc_chain_and_wait_grace(self):
        xpath = {"xpath": "//*[@text='保存']"}
        self.assertEqual(_step_timeout_seconds(record("tap", selector=xpath)), 30)
        self.assertEqual(_step_timeout_seconds(record("wait_for", selector=xpath, timeout_ms=10000)), 28)
        self.assertEqual(_step_timeout_seconds(record("wait_for", selector=xpath, timeout_ms=30000)), 48)
        self.assertEqual(_step_timeout_seconds(record("wait_gone", selector=xpath, timeout_ms=30000)), 48)


    def test_xpath_preview_uniqueness_ignores_stale_recorded_dimensions(self):
        # Read-only XPath preview uses LIVE XML geometry, not a source-phone
        # coordinate recording. The selector is reusable on different screens.
        xml = '<hierarchy><node class="android.widget.FrameLayout" bounds="[0,0][320,640]"><node class="android.widget.FrameLayout" content-desc="领淘金币" bounds="[40,80][160,140]"/></node></hierarchy>'
        runner = ActionHarness(hierarchy=xml)
        preview = runner.run(record("locator_verify", xpath="//*[@content-desc='领淘金币']",
                                    width=1080, height=2340, preview_only=True,
                                    timeout_ms=20000))
        self.assertTrue(preview["verified"])
        self.assertEqual(preview["reason"], "unique_preview")
        self.assertEqual(preview["bounds"]["left"], 40)

    def test_wait_gone_never_sends_an_action(self):
        checks = iter([2, 2, 0])
        runner = ActionHarness(count=lambda s: 1 if s.values.get("className") else next(checks))
        value = runner.run(record("wait_gone", selector=BUTTON, timeout_ms=1000))
        self.assertEqual(value["state"], "assertion_passed")
        self.assertTrue(all(method == "count" for method, _ in runner.calls))

    def test_wait_gone_rejects_invalid_rpc_instead_of_treating_as_absence(self):
        runner = ActionHarness(count=lambda s: 1 if s.values.get("className") else None)
        with self.assertRaises(StepFailure) as raised:
            runner.run(record("wait_gone", selector=BUTTON, timeout_ms=100))
        # None is an unexpected result, not verified absence.
        self.assertNotEqual(raised.exception.code, "selector_not_found")

    def test_assert_property_requires_real_boolean_not_truthiness(self):
        for actual, expect_pass in [(False, True), (True, False), (0, False), (None, False)]:
            runner = ActionHarness(info=lambda s: {"enabled": actual})
            step = record("assert_property", selector=BUTTON, property="enabled", expected=False)
            with self.subTest(actual=actual):
                if expect_pass:
                    self.assertEqual(runner.run(step)["state"], "assertion_passed")
                else:
                    with self.assertRaises(StepFailure):
                        runner.run(step)
                self.assertEqual([m for m, _ in runner.calls].count("objInfo"), 1)
                self.assertNotIn("swipe", [m for m, _ in runner.calls])

    def test_scroll_find_probes_before_swiping_and_stops_after_match(self):
        state = {"swipes": 0}
        def count(sel):
            if sel.values.get("className") == "android.view.View":
                return 1
            if sel.values.get("resourceId") == "app:id/list":
                return 1
            return 1 if state["swipes"] >= 1 else 0
        def info(sel):
            return {"scrollable": True, "bounds": {"left": 20, "top": 50, "right": 300, "bottom": 600}}
        def swipe():
            state["swipes"] += 1
            return True
        runner = ActionHarness(count=count, info=info, on_swipe=swipe)
        step = record("scroll_find", selector=BUTTON, scroll_container=SCROLLER, direction="down",
                      max_swipes=3, duration_ms=300, timeout_ms=20000)
        result = runner.run(step)
        self.assertEqual(result["swipes"], 1)
        self.assertEqual(state["swipes"], 1)
        self.assertEqual([method for method, _ in runner.calls].count("swipe"), 1)

    def test_scroll_find_rejects_container_without_touch(self):
        runner = ActionHarness(
            count=lambda sel: 1 if sel.values.get("className") or sel.values.get("resourceId") == "app:id/list" else 0,
            info=lambda sel: {"scrollable": False, "bounds": {"left": 0, "top": 0, "right": 300, "bottom": 600}},
        )
        with self.assertRaises(StepFailure) as raised:
            runner.run(record("scroll_find", selector=BUTTON, scroll_container=SCROLLER,
                              direction="down", max_swipes=2, duration_ms=300, timeout_ms=10000))
        self.assertEqual(raised.exception.code, "scroll_container_invalid")
        self.assertNotIn("swipe", [method for method, _ in runner.calls])

    def test_scroll_transport_failure_is_not_retried(self):
        runner = ActionHarness(
            count=lambda sel: 1 if sel.values.get("className") or sel.values.get("resourceId") == "app:id/list" else 0,
            info=lambda sel: {"scrollable": True, "bounds": {"left": 0, "top": 0, "right": 300, "bottom": 600}},
            on_swipe=lambda: (_ for _ in ()).throw(ConnectionError("uncertain gesture")),
        )
        with self.assertRaises(StepFailure) as raised:
            runner.run(record("scroll_find", selector=BUTTON, scroll_container=SCROLLER,
                              direction="down", max_swipes=3, duration_ms=300, timeout_ms=10000))
        self.assertEqual(raised.exception.code, "device_action_unconfirmed")
        self.assertTrue(raised.exception.outcome_unknown)
        self.assertEqual([method for method, _ in runner.calls].count("swipe"), 1)


if __name__ == "__main__":
    unittest.main()
