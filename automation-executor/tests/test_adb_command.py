"""End-to-end ADB step validation and an isolated fake-device execution."""
import sys
import types
import unittest
from unittest.mock import patch
from uuid import uuid4

from automation_executor.steps import StepFailure, execute
from automation_executor.validation import adb_shell_tokens, steps
from automation_executor.worker import _step_timeout_seconds

TARGET = {"device_id": str(uuid4()), "ip": "192.0.2.33", "adb_port": 37555}


def action(command="adb shell wm size", **extra):
    return {"id": str(uuid4()), "type": "adb_command", "title": "读取屏幕信息",
            "command": command, "timeout_ms": 8000, **extra}


class FakePhone:
    def __init__(self, code=0):
        self.calls = []
        self.code = code

    def get_state(self):
        return "device"

    def shell2(self, args, timeout):
        self.calls.append((args, timeout))
        return types.SimpleNamespace(returncode=self.code, output="Physical size: 1080x2400")


class AdbCommandTests(unittest.TestCase):
    def test_valid_command_and_title_survive_validation(self):
        saved = steps([action()])
        self.assertEqual(saved[0]["title"], "读取屏幕信息")
        self.assertEqual(saved[0]["command"], "adb shell wm size")
        self.assertEqual(adb_shell_tokens("adb shell input keyevent 3"),
                         ["input", "keyevent", "3"])
        self.assertEqual(_step_timeout_seconds(saved[0]), 16)

    def test_invalid_commands_cannot_escape_device_scoped_shell(self):
        unsafe = [
            "adb devices", "adb -s other shell input keyevent 3",
            "adb push /etc/passwd /sdcard/", "adb shell sh -c id",
            "adb shell su -c id", "adb shell pm list packages; rm -rf /sdcard",
            "adb shell getprop | cat", "adb shell echo $(id)",
            "adb shell echo `id`", "adb shell echo x > /sdcard/a",
            "adb shell ", "",
        ]
        for value in unsafe:
            with self.subTest(command=value), self.assertRaises(Exception):
                steps([action(value)])
        with self.assertRaises(Exception):
            steps([action(title="x" * 81)])
        with self.assertRaises(Exception):
            steps([action(timeout_ms=60000)])
        with self.assertRaises(Exception):
            steps([action() | {"other": "invalid"}])

    def test_executes_on_selected_phone_only_without_host_shell(self):
        phone = FakePhone()
        adbutils = types.ModuleType("adbutils")
        called = []
        def device(serial):
            called.append(serial)
            return phone
        adbutils.adb = types.SimpleNamespace(device=device)
        with patch.dict(sys.modules, {"adbutils": adbutils}):
            result = execute(TARGET, steps([action()])[0], {})
        self.assertEqual(called, ["192.0.2.33:37555"])
        self.assertEqual(phone.calls, [(["wm", "size"], 8.0)])
        self.assertEqual(result["state"], "action_sent")
        self.assertIn("1080x2400", result["message"])

    def test_nonzero_code_is_error_not_success(self):
        phone = FakePhone(1)
        adbutils = types.ModuleType("adbutils")
        adbutils.adb = types.SimpleNamespace(device=lambda serial: phone)
        with patch.dict(sys.modules, {"adbutils": adbutils}):
            with self.assertRaises(StepFailure) as error:
                execute(TARGET, steps([action()])[0], {})
        self.assertEqual(error.exception.code, "adb_command_failed")


if __name__ == "__main__":
    unittest.main()
