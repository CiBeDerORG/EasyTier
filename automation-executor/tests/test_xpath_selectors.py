"""XPath matching must never silently choose a random element or repeat a tap."""
import unittest
from uuid import uuid4

from automation_executor.errors import ExecutorError
from automation_executor.hierarchy import xpath_match
from automation_executor.validation import steps


XML = """<hierarchy>
  <node class="android.widget.FrameLayout" bounds="[0,0][320,640]">
    <node class="android.widget.Button" text="保存" resource-id="app:id/save"
          package="app.test" bounds="[20,50][140,120]" />
    <node class="android.widget.Button" text="取消" resource-id="app:id/cancel"
          package="app.test" bounds="[150,50][270,120]" />
  </node>
</hierarchy>"""


def step(kind, selector):
    item = {"id": str(uuid4()), "type": kind, "selector": selector}
    if kind in ("assert", "wait_for"):
        item["timeout_ms"] = 1000
    if kind == "input_text":
        item["parameter"] = "input"
    return item


class XPathSelectorTests(unittest.TestCase):
    def test_xpath_matching_a_unique_button(self):
        self.assertEqual(
            xpath_match(XML, "//android.widget.Button[@text='保存']", 320, 640),
            {"left": 20, "top": 50, "right": 140, "bottom": 120},
        )

    def test_xpath_can_match_combined_attributes(self):
        matched = xpath_match(XML, "//*[@resource-id='app:id/save' and @package='app.test']", 320, 640)
        self.assertEqual(matched["left"], 20)
        self.assertIsNone(xpath_match(XML, "//*[@text='不存在']", 320, 640))

    def test_ambiguous_xpath_must_fail(self):
        with self.assertRaises(ExecutorError) as raised:
            xpath_match(XML, "//android.widget.Button", 320, 640)
        self.assertEqual(raised.exception.code, "selector_ambiguous")

    def test_invalid_xpath_and_unsafe_xml_are_rejected(self):
        for expression in ("", "just text", "count(//*)", "//android.widget.Button["):
            with self.subTest(expression=expression), self.assertRaises(ExecutorError):
                xpath_match(XML, expression, 320, 640)
        with self.assertRaises(ExecutorError):
            xpath_match('<!DOCTYPE hierarchy [<!ENTITY x "hello">]><hierarchy/>', "//*", 320, 640)

    def test_xpath_is_exclusive_and_cannot_target_input_text(self):
        self.assertEqual(len(steps([step("tap", {"xpath": "//*[@text='保存']"})])), 1)
        self.assertEqual(len(steps([step("assert", {"xpath": "//*[@text='保存']"})])), 1)
        self.assertEqual(len(steps([step("tap", {"resource_id": "app:id/save", "class_name": "android.widget.Button"})])), 1)
        for invalid_selector in (
            {"xpath": "//*[@text='保存']", "text": "保存"},
            {"xpath": "not XPath"},
        ):
            with self.assertRaises(Exception):
                steps([step("tap", invalid_selector)])
        with self.assertRaises(Exception):
            steps([step("input_text", {"xpath": "//*[@text='保存']"})])


if __name__ == "__main__":
    unittest.main()
