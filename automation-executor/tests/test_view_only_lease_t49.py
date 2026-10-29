"""T49: actual Runtime/Journal viewer-vs-action lease contracts.

Mock only the external media/ADB boundary. These tests never launch scrcpy,
UiAutomator2, ADB subprocesses or a phone. Not executed in debug mode.
"""
import asyncio
from pathlib import Path
import sys
from tempfile import TemporaryDirectory
from types import ModuleType, SimpleNamespace
import unittest
from unittest.mock import AsyncMock, patch
from uuid import uuid4

from automation_executor.errors import ExecutorError
from automation_executor.runtime import Runtime
from automation_executor.scheduler import FairDeviceSlots


def target():
    return dict(
        device_id=str(uuid4()), name="T49 phone", owner_user_id=42,
        machine_id=str(uuid4()), ip="10.81.42.11", adb_port=37123,
        connection_mode="wireless", target_fingerprint=str(uuid4()),
        route_context="default", adb_service_id=str(uuid4()),
        endpoint_revision=1,
    )


class ViewOnlyLeaseTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.directory=TemporaryDirectory()
        media=ModuleType("automation_executor.media")
        media.MediaManager=lambda:SimpleNamespace(
            close_all=AsyncMock(), _max_sessions=6,
            _starting=FairDeviceSlots(2), _lock=asyncio.Lock())
        self.media_patch=patch.dict(sys.modules,{"automation_executor.media":media})
        self.media_patch.start()
        settings=SimpleNamespace(
            data_dir=Path(self.directory.name),token="t"*32,
            max_running=2,max_pending=10,
            callback_url="http://test.invalid",adb_binary="adb",
            max_artifact_bytes=10_000_000)
        self.runtime=Runtime(settings)
        self.runtime.media.create=AsyncMock(return_value={"ice_servers":[]})
        self.runtime.media.close=AsyncMock(return_value={})
        self.runtime.media.status=lambda sid:{"state":"active"}
        self.runtime.media.command=AsyncMock(return_value={"state":"action_sent"})
        self.phone=target()
        self.sid=str(uuid4())
        self.body={
            "id":self.sid,"target":self.phone,"quality":"wall",
            "lease_seconds":60,
            "expected_executor_id":self.runtime.journal.executor_id,
        }
        await self.runtime.create_session(self.body)

    async def asyncTearDown(self):
        await self.runtime.shutdown()
        self.media_patch.stop()
        self.directory.cleanup()

    def identity(self):
        return {"expected_executor_id":self.runtime.journal.executor_id}

    async def test_force_cleanup_ends_all_phone_sessions_and_frees_script_lease(self):
        holder="run:"+str(uuid4())+":"+self.phone["device_id"]
        self.runtime.journal.claim(self.phone,holder,"script")
        reply=await self.runtime.force_cleanup_device(self.phone["device_id"],{
            "target":self.phone,"expected_executor_id":self.runtime.journal.executor_id,
            "event_id":str(uuid4()),
        })
        self.assertTrue(reply["cleanup_confirmed"])
        self.assertEqual(reply["stopped_sessions"],1)
        self.assertIsNone(self.runtime.journal.control(self.phone))
        self.assertNotIn(self.sid,self.runtime.sessions)
        self.runtime.media.close.assert_awaited()
        self.assertFalse(self.runtime.journal.target_refused(self.phone))
        # After proof, ordinary preview claims can be created again.
        self.runtime.journal.claim(self.phone,"session:fresh","manual")
        self.runtime.journal.release("session:fresh")

    async def test_viewer_releases_only_its_manual_write_lease_without_closing_media(self):
        holder="session:"+self.sid
        self.runtime.journal.claim(self.phone,holder,"manual")
        result=await self.runtime.view_only(self.sid,self.identity())
        self.assertEqual(result["state"],"view_only")
        self.assertIsNone(self.runtime.journal.control(self.phone))
        self.runtime.media.close.assert_not_awaited()
        self.runtime.media.command.assert_awaited_once_with(
            self.sid,{"type":"touch.release"})
        with self.assertRaises(ExecutorError) as error:
            await self.runtime.command(self.sid,{
                "type":"key","key":"HOME","seq":1,"command_id":str(uuid4())})
        self.assertEqual(error.exception.code,"viewer_only")
        self.assertEqual(self.runtime.media.command.await_count,1,
                         "blocked viewer must send no new Android action")

    async def test_viewer_can_coexist_with_script_action_holder_but_cannot_resume_manual(self):
        result=await self.runtime.view_only(self.sid,self.identity())
        self.assertEqual(result["state"],"view_only")
        self.runtime.media.command.assert_not_awaited()
        run_holder="run:"+str(uuid4())
        self.runtime.journal.claim(self.phone,run_holder,"script")
        self.assertEqual(self.runtime.journal.control(self.phone)["holder"],run_holder)
        result=await self.runtime.view_only(self.sid,self.identity())
        self.assertEqual(result["state"],"view_only")
        with self.assertRaises(ExecutorError) as busy:
            await self.runtime.resume_manual(self.sid,self.identity())
        self.assertEqual(busy.exception.code,"device_busy")
        self.assertTrue(self.runtime.sessions[self.sid].view_only)
        self.runtime.media.close.assert_not_awaited()
        self.runtime.journal.release(run_holder,clean=True)
        restored=await self.runtime.resume_manual(self.sid,self.identity())
        self.assertEqual(restored["state"],"active")
        self.assertIsNone(self.runtime.journal.control(self.phone),
                          "resume only grants eligibility, not a write lease")
        self.runtime.media.command.assert_not_awaited()

    async def test_unknown_manual_cleanup_never_becomes_confirmed_viewer(self):
        holder="session:"+self.sid
        self.runtime.journal.claim(self.phone,holder,"manual")
        with patch.object(self.runtime.journal,"worker_record",
                          return_value={"pid":12345}), \
             patch("automation_executor.runtime.cleanup_record",
                   return_value=False):
            for _ in range(2):
                with self.assertRaises(ExecutorError) as pending:
                    await self.runtime.view_only(self.sid,self.identity())
                self.assertEqual(pending.exception.code,"cleanup_pending")
        self.assertFalse(self.runtime.sessions[self.sid].view_only_confirmed)
        self.assertTrue(self.runtime.sessions[self.sid].view_only)
        self.assertEqual(self.runtime.journal.control(self.phone)["state"],
                         "cleanup_pending")
        with self.assertRaises(ExecutorError) as blocked:
            self.runtime.journal.claim(self.phone,"run:new","script")
        self.assertEqual(blocked.exception.code,"cleanup_pending")
        self.runtime.media.close.assert_not_awaited()

    async def test_viewer_close_does_not_release_a_different_script_holder(self):
        await self.runtime.view_only(self.sid,self.identity())
        script_holder="run:"+str(uuid4())
        self.runtime.journal.claim(self.phone,script_holder,"script")
        closed=await self.runtime.close_session(self.sid)
        self.assertEqual(closed["state"],"closed")
        self.assertEqual(self.runtime.journal.control(self.phone)["holder"],
                         script_holder,
                         "a viewer teardown must not release the script action lease")
        with self.assertRaises(ExecutorError) as busy:
            self.runtime.journal.claim(self.phone,"session:new","manual")
        self.assertEqual(busy.exception.code,"device_busy")
        self.runtime.journal.release(script_holder,clean=True)

    async def test_foreign_action_holder_never_allows_a_viewer_takeover(self):
        foreign_holder="run:"+str(uuid4())
        self.runtime.journal.claim(self.phone,foreign_holder,"script")
        with self.assertRaises(ExecutorError) as busy:
            await self.runtime.view_only(self.sid,self.identity())
        self.assertEqual(busy.exception.code,"device_busy")
        self.assertEqual(self.runtime.journal.control(self.phone)["holder"],
                         foreign_holder)
        self.assertTrue(self.runtime.sessions[self.sid].view_only,
                        "a failed migration must not regain manual permission")
        self.assertFalse(self.runtime.sessions[self.sid].view_only_confirmed)
        with self.assertRaises(ExecutorError) as rejected:
            await self.runtime.command(self.sid,{
                "type":"stop","seq":1,"command_id":str(uuid4())})
        self.assertEqual(rejected.exception.code,"viewer_only")
        self.runtime.media.command.assert_not_awaited()
        self.runtime.journal.release(foreign_holder,clean=True)

    async def test_preparation_handoff_releases_manual_gesture_without_closing_preview(self):
        manual_holder = "session:" + self.sid
        preparation_holder = "prepare:" + str(uuid4())
        self.runtime.journal.claim(self.phone, manual_holder, "manual")

        await self.runtime._claim_preparation_after_manual(self.phone, preparation_holder)

        control = self.runtime.journal.control(self.phone)
        self.assertEqual(control["holder"], preparation_holder)
        self.assertEqual(control["kind"], "prepare")
        self.assertIn(self.sid, self.runtime.sessions, "the original live preview must stay open")
        self.assertFalse(self.runtime.sessions[self.sid].view_only,
                         "the floating manual switch must not be changed")
        self.runtime.media.command.assert_awaited_once_with(
            self.sid, {"type": "touch.release"})
        self.runtime.journal.release(preparation_holder, clean=True)

        # Manual permission remains enabled; its next command can claim the lease again.
        self.runtime.journal.claim(self.phone, manual_holder, "manual")
        self.runtime.journal.release(manual_holder, clean=True)

    async def test_preparation_handoff_does_not_steal_lease_when_touch_release_fails(self):
        manual_holder = "session:" + self.sid
        self.runtime.journal.claim(self.phone, manual_holder, "manual")
        self.runtime.media.command.side_effect = ExecutorError(
            "touch_release_failed", "touch release was not confirmed", 409)

        with self.assertRaises(ExecutorError):
            await self.runtime._claim_preparation_after_manual(
                self.phone, "prepare:" + str(uuid4()))

        self.assertEqual(self.runtime.journal.control(self.phone)["holder"], manual_holder)
        self.assertFalse(self.runtime.sessions[self.sid].view_only)
        self.runtime.media.command.assert_awaited_once_with(
            self.sid, {"type": "touch.release"})

    async def test_automation_rpc_health_probe_marks_dead_server_unavailable(self):
        with patch.object(self.runtime, "prepared", return_value=True), \
             patch("automation_executor.runtime.asyncio.to_thread",
                   new=AsyncMock(side_effect=TimeoutError())):
            self.assertFalse(await self.runtime._automation_rpc_ready(self.phone))

    async def test_automation_rpc_health_probe_accepts_zero_matching_views(self):
        with patch.object(self.runtime, "prepared", return_value=True), \
             patch("automation_executor.runtime.asyncio.to_thread", new=AsyncMock(return_value=0)):
            self.assertTrue(await self.runtime._automation_rpc_ready(self.phone))

    async def test_run_releases_prior_manual_gesture_without_changing_viewer_switch(self):
        manual_holder="session:"+self.sid
        script_holder="run:"+str(uuid4())
        self.runtime.journal.claim(self.phone,manual_holder,"manual")
        await self.runtime._claim_run_after_manual(self.phone,script_holder)
        self.assertEqual(self.runtime.journal.control(self.phone)["holder"],script_holder)
        self.assertFalse(self.runtime.sessions[self.sid].view_only,
                         "the floating manual switch must stay ON")
        self.runtime.media.command.assert_awaited_once_with(
            self.sid,{"type":"touch.release"})
        self.runtime.media.close.assert_not_awaited()
        with self.assertRaises(ExecutorError) as busy:
            self.runtime.journal.claim(self.phone,"session:new","manual")
        self.assertEqual(busy.exception.code,"device_busy")
        self.runtime.journal.release(script_holder,clean=True)
        self.assertFalse(self.runtime.sessions[self.sid].view_only)

    async def test_run_rejects_failed_manual_release_and_retains_original_lease(self):
        manual_holder="session:"+self.sid
        self.runtime.journal.claim(self.phone,manual_holder,"manual")
        self.runtime.media.command.side_effect=ExecutorError(
            "release_unknown","原触点释放回执未知",502)
        with self.assertRaises(ExecutorError):
            await self.runtime._claim_run_after_manual(self.phone,"run:"+str(uuid4()))
        self.assertEqual(self.runtime.journal.control(self.phone)["holder"],manual_holder)
        self.assertFalse(self.runtime.sessions[self.sid].view_only)
        self.runtime.media.close.assert_not_awaited()

    async def test_other_executor_generation_cannot_downgrade_or_restore_viewer(self):
        wrong={"expected_executor_id":str(uuid4())}
        with self.assertRaises(ExecutorError) as changed:
            await self.runtime.view_only(self.sid,wrong)
        self.assertEqual(changed.exception.code,"executor_identity_changed")
        self.assertFalse(self.runtime.sessions[self.sid].view_only)
        with self.assertRaises(ExecutorError) as changed:
            await self.runtime.resume_manual(self.sid,wrong)
        self.assertEqual(changed.exception.code,"executor_identity_changed")
        self.runtime.media.command.assert_not_awaited()


if __name__=="__main__":
    unittest.main()
