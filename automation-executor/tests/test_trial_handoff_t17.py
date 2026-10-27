"""T17: simulated durable executor handoff, no ADB or media subprocesses.

Server-side Rust create_run/create_session admission is a separate layer; these
checks prove the executor's secondary SQLite control fence and crash cleanup.
"""
from pathlib import Path
from tempfile import TemporaryDirectory
import unittest
from uuid import uuid4

from automation_executor.errors import ExecutorError
from automation_executor.journal import Journal


def target(ip="10.70.0.20", did=None):
    return dict(
        device_id=did or str(uuid4()), name="phone", owner_user_id=15,
        machine_id=str(uuid4()), ip=ip, adb_port=37001,
        connection_mode="wireless", target_fingerprint=str(uuid4()),
        route_context="default", adb_service_id=str(uuid4()),
        endpoint_revision=1,
    )


class TrialHandoffJournalTests(unittest.TestCase):
    def setUp(self):
        self.directory = TemporaryDirectory()
        self.path = Path(self.directory.name) / "executor-t17.sqlite"
        self.journal = Journal(self.path)

    def tearDown(self):
        self.journal.close()
        self.directory.cleanup()

    def test_manual_preview_must_release_before_script_claim(self):
        phone = target()
        self.journal.claim(phone, "session:one", "manual")
        with self.assertRaises(ExecutorError) as blocked:
            self.journal.claim(phone, "run:two", "script")
        self.assertEqual(blocked.exception.code, "device_busy")
        self.assertEqual(self.journal.control(phone)["holder"], "session:one")
        self.journal.release("session:one", clean=True)
        self.journal.claim(phone, "run:two", "script")
        with self.assertRaises(ExecutorError) as rejected:
            self.journal.claim(phone, "session:three", "manual")
        self.assertEqual(rejected.exception.code, "device_busy")
        self.journal.release("run:two", clean=True)
        self.assertIsNone(self.journal.control(phone))

    def test_same_adb_endpoint_with_another_device_id_is_exclusive(self):
        first = target()
        other = target(ip=first["ip"])  # different asset, physical endpoint reused
        self.assertNotEqual(first["device_id"], other["device_id"])
        self.journal.claim(first, "session:first", "manual")
        with self.assertRaises(ExecutorError) as busy:
            self.journal.claim(other, "run:second", "script")
        self.assertEqual(busy.exception.code, "device_busy")
        self.journal.release("session:first", clean=True)
        self.journal.claim(other, "run:second", "script")
        self.journal.release("run:second", clean=True)

    def test_unconfirmed_cleanup_survives_executor_restart_and_never_replays(self):
        phone = target()
        self.journal.claim(phone, "run:unknown", "script")
        self.journal.release("run:unknown", clean=False)
        self.assertEqual(self.journal.control(phone)["state"], "cleanup_pending")
        with self.assertRaises(ExecutorError) as blocked:
            self.journal.claim(phone, "session:new", "manual")
        self.assertEqual(blocked.exception.code, "cleanup_pending")
        self.journal.close()
        self.journal = Journal(self.path)
        self.journal.recover()
        self.assertEqual(self.journal.control(phone)["state"], "cleanup_pending")
        with self.assertRaises(ExecutorError) as blocked:
            self.journal.claim(phone, "run:new", "script")
        self.assertEqual(blocked.exception.code, "cleanup_pending")
        # Only a separately confirmed clean release permits a fresh action.
        self.journal.release("run:unknown", clean=True)
        self.journal.claim(phone, "session:new", "manual")
        self.journal.release("session:new", clean=True)

    def test_another_phone_still_runs_if_first_device_cleanup_is_unknown(self):
        broken, good = target("10.70.0.21"), target("10.70.0.22")
        self.journal.claim(broken, "run:broken", "script")
        self.journal.release("run:broken", clean=False)
        self.journal.claim(good, "run:good", "script")
        self.assertEqual(self.journal.control(good)["holder"], "run:good")
        self.assertEqual(self.journal.control(broken)["state"], "cleanup_pending")
        self.journal.release("run:good", clean=True)
        self.assertIsNone(self.journal.control(good))
        self.assertIsNotNone(self.journal.control(broken))

    def test_idempotent_claim_for_same_active_holder_does_not_grant_second_owner(self):
        phone = target()
        self.journal.claim(phone, "run:one", "script")
        self.journal.claim(phone, "run:one", "script")
        self.assertEqual(self.journal.control(phone)["holder"], "run:one")
        with self.assertRaises(ExecutorError):
            self.journal.claim(phone, "run:two", "script")
        self.journal.release("run:one", clean=True)


if __name__ == "__main__":
    unittest.main()
