"""Read-only per-phone ADB probes. Never pair or connect during status lookup."""
import unittest
from unittest.mock import AsyncMock, patch

from automation_executor import adb
from automation_executor.errors import ExecutorError


class AdbConnectionStatusTests(unittest.IsolatedAsyncioTestCase):
    async def probe(self, output, stderr="", code=0):
        target = {"device_id": "phone-A", "ip": "10.10.30.206", "adb_port": 38805}
        with patch.object(adb, "command", new_callable=AsyncMock, return_value=(code, output, stderr)) as cmd:
            result = await adb.connection_status("adb", target)
            cmd.assert_awaited_once_with("adb", "-s", "10.10.30.206:38805", "get-state", timeout=5)
            return result

    async def test_connected(self):
        self.assertEqual((await self.probe("device\n"))["state"], "connected")

    async def test_offline(self):
        self.assertEqual((await self.probe("", "error: device offline", 1))["state"], "offline")

    async def test_unauthorized(self):
        self.assertEqual((await self.probe("", "error: device unauthorized", 1))["state"], "unauthorized")

    async def test_unknown_device(self):
        self.assertEqual((await self.probe("", "error: device not found", 1))["state"], "disconnected")

    async def test_timeout_is_independent_status(self):
        target = {"device_id": "phone-A", "ip": "10.10.30.206", "adb_port": 38805}
        with patch.object(adb, "command", new_callable=AsyncMock,
                          side_effect=ExecutorError("adb_timeout", "request timeout", 504)):
            result = await adb.connection_status("adb", target)
        self.assertEqual(result, {"device_id": "phone-A", "state": "unavailable", "code": "adb_timeout"})


if __name__ == "__main__":
    unittest.main()
