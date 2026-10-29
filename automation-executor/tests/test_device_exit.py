"""Simulated device isolation using the real durable journal and run scheduler."""
import asyncio
import json
import struct
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace, ModuleType
import unittest
from unittest.mock import AsyncMock, patch
from uuid import uuid4

from automation_executor.errors import ExecutorError
from automation_executor.journal import Journal
from automation_executor.runtime import Runtime
from automation_executor.scheduler import FairDeviceSlots


def target(ip='10.60.1.20'):
    return dict(device_id=str(uuid4()), name='Phone', owner_user_id=1, machine_id=str(uuid4()),
                ip=ip, adb_port=37001, connection_mode='wireless', target_fingerprint=str(uuid4()),
                route_context='default', adb_service_id=str(uuid4()), endpoint_revision=1)


def _spawned_prepared_helper_probe(channel, gate, parent_pid, target, adb_binary):
    # Match PreparedHelper's five-argument entrypoint without touching Android.
    from automation_executor.worker import _isolation_probe
    _isolation_probe(channel, gate, parent_pid, adb_binary)


def run_body(journal, targets):
    return dict(id=str(uuid4()), name='two phones', owner_user_id=1, actor_user_id=1,
                expected_executor_id=journal.executor_id, targets=targets, concurrency=2, parameters={},
                steps=[dict(id=str(uuid4()), type='key', key='HOME'), dict(id=str(uuid4()), type='key', key='BACK')])


class FairSchedulingTests(unittest.IsolatedAsyncioTestCase):
    async def test_round_robin_across_runs_and_cancelled_waiter_does_not_leak(self):
        slots = FairDeviceSlots(1)
        await slots.acquire("hold")
        completed = []
        async def wait(run_id, token):
            await slots.acquire(run_id)
            try:
                completed.append(token)
            finally:
                await slots.release()
        a1 = asyncio.create_task(wait("A", "A1"))
        a2 = asyncio.create_task(wait("A", "A2"))
        b1 = asyncio.create_task(wait("B", "B1"))
        b2 = asyncio.create_task(wait("B", "B2"))
        await asyncio.sleep(0)
        await slots.release()
        await asyncio.wait_for(asyncio.gather(a1,a2,b1,b2), 2)
        self.assertEqual(completed, ["A1", "B1", "A2", "B2"])
        self.assertEqual(slots.running, 0)
        await slots.acquire("hold")
        cancelled = asyncio.create_task(slots.acquire("C"))
        await asyncio.sleep(0)
        cancelled.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await cancelled
        await slots.release()
        self.assertEqual(slots.running, 0)
        self.assertEqual(len(slots._waiting), 0)


class JournalTests(unittest.TestCase):
    def test_early_withdraw_survives_restart_and_does_not_cancel_other_phone(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'journal.sqlite'; journal=Journal(path)
            a,b=target(),target('10.60.1.21'); body=run_body(journal,[a,b])
            self.assertTrue(journal.withdraw(body['id'],a['device_id'],a['target_fingerprint'],str(uuid4())))
            journal.close(); journal=Journal(path); journal.recover()
            result,created=journal.accept_run(body)
            self.assertTrue(created); self.assertFalse(journal.cancelled(body['id']))
            self.assertTrue(journal.withdrawn(body['id'],a['device_id']))
            with self.assertRaises(ExecutorError): journal.claim(a,'a','script')
            journal.claim(b,'b','script'); journal.release('b'); journal.close()

    def test_wrong_fingerprint_cannot_fence_a_known_run(self):
        with tempfile.TemporaryDirectory() as directory:
            journal=Journal(Path(directory)/'journal.sqlite'); a=target();body=run_body(journal,[a]);journal.accept_run(body)
            with self.assertRaises(ExecutorError): journal.withdraw(body['id'],a['device_id'],'wrong',str(uuid4()))
            self.assertFalse(journal.withdrawn(body['id'],a['device_id']));journal.close()

    def test_force_cleanup_fences_only_old_run_not_future_phone_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            journal=Journal(Path(directory)/'execution.sqlite3')
            phone=target(); other=target('10.60.1.23')
            body=run_body(journal,[phone,other])
            journal.accept_run(body)
            journal.fence_device_run_only(body['id'],phone['device_id'],
                                          phone['target_fingerprint'],str(uuid4()))
            self.assertTrue(journal.withdrawn(body['id'],phone['device_id']))
            self.assertFalse(journal.withdrawn(body['id'],other['device_id']))
            self.assertFalse(journal.target_refused(phone))
            journal.claim(phone,'new:user-click','manual')
            journal.release('new:user-click')
            journal.close()


class RuntimeTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.directory=tempfile.TemporaryDirectory()
        media=ModuleType('automation_executor.media')
        media.MediaManager=lambda:SimpleNamespace(
            close_all=AsyncMock(), _max_sessions=10,
            _starting=FairDeviceSlots(2), _lock=asyncio.Lock())
        self.media_patch=patch.dict(sys.modules,{'automation_executor.media':media});self.media_patch.start()
        settings=SimpleNamespace(data_dir=Path(self.directory.name),token='x'*32,max_running=3,max_pending=10,
                                 callback_url='http://test.invalid',adb_binary='adb',max_artifact_bytes=10000000)
        self.runtime=Runtime(settings)
        self.runtime._authorize=AsyncMock();self.runtime._publish=AsyncMock();self.runtime.prepared=lambda _:True
        # These tests mock the Android boundary. Dedicated tests cover the live
        # RPC health probe; scheduler tests must never reach a real ADB server.
        self.runtime._automation_rpc_ready=AsyncMock(return_value=True)
        self.adb_patch=patch('automation_executor.runtime.adb.require_connected',new=AsyncMock());self.adb_patch.start()

    async def asyncTearDown(self):
        await self.runtime.shutdown();self.media_patch.stop();self.adb_patch.stop();self.directory.cleanup()

    async def test_capacity_settings_have_no_arbitrary_cap_and_survive_restart(self):
        first = self.runtime.runtime_limits()
        self.assertEqual(first["max_running"], 3)
        request = {
            "revision": first["revision"], "max_running": 25,
            "max_pending": 200, "max_media_sessions": 50, "max_media_starting": 8,
        }
        saved = await self.runtime.save_runtime_limits(request)
        self.assertEqual(saved["max_running"], 25)
        self.assertEqual(self.runtime.capacity.limit, 25)
        self.assertEqual(self.runtime.media._max_sessions, 50)
        self.assertEqual(self.runtime.media._starting.limit, 8)
        with self.assertRaises(ExecutorError) as conflict:
            await self.runtime.save_runtime_limits(request)
        self.assertEqual(conflict.exception.code, "limits_revision_conflict")
        # Re-open the SQLite journal using a new runtime, without replaying
        # previous actions and without changing the persistent executor id.
        other = Runtime(self.runtime.settings)
        try:
            self.assertEqual(other.runtime_limits()["max_pending"], 200)
            self.assertEqual(other.runtime_limits()["max_running"], 25)
            self.assertEqual(other.runtime_limits()["executor_id"], saved["executor_id"])
        finally:
            await other.shutdown()

    async def test_reducing_capacity_does_not_abort_active_device(self):
        slots = self.runtime.capacity
        await slots.acquire("A")
        await slots.acquire("B")
        await self.runtime.save_runtime_limits({
            "revision": 0, "max_running": 1, "max_pending": 10,
            "max_media_sessions": 10, "max_media_starting": 2,
        })
        self.assertEqual(slots.running, 2)
        blocked = asyncio.create_task(slots.acquire("C"))
        await asyncio.sleep(0)
        self.assertFalse(blocked.done())
        await slots.release()
        self.assertFalse(blocked.done())
        await slots.release()
        await asyncio.wait_for(blocked, 1)
        await slots.release()

    async def test_session_identity_rejects_replacement_executor_before_side_effects(self):
        runtime=self.runtime; sid=str(uuid4()); a=target()
        runtime.media.create=AsyncMock(return_value={'ice_servers':[]})
        runtime.media.close=AsyncMock(return_value={})
        body=dict(id=sid,target=a,quality='wall',lease_seconds=60,expected_executor_id=str(uuid4()))
        with self.assertRaises(ExecutorError): await runtime.create_session(body)
        runtime.media.create.assert_not_called()
        with self.assertRaises(ExecutorError): await runtime.close_session_request(sid,{'expected_executor_id':str(uuid4())})
        runtime.media.close.assert_not_called()
        body['expected_executor_id']=runtime.journal.executor_id
        opened=await runtime.create_session(body)
        self.assertEqual(opened['executor_id'],runtime.journal.executor_id)
        closed=await runtime.close_session_request(sid,{'expected_executor_id':runtime.journal.executor_id})
        self.assertEqual(closed['executor_id'],runtime.journal.executor_id)
        self.assertFalse(closed['cleanup_pending'])

    async def test_three_phones_start_concurrently_and_finish_independently(self):
        devices = [target(f'10.60.2.{number}') for number in range(21, 24)]
        body = run_body(self.runtime.journal, devices)
        body['concurrency'] = 3
        started = {device['device_id']: asyncio.Event() for device in devices}
        release = asyncio.Event()
        async def execute(journal, holder, target_item, step, *args):
            started[target_item['device_id']].set()
            await release.wait()
            return {'state': 'succeeded'}
        with patch('automation_executor.runtime.execute_step', new=execute):
            accepted = await self.runtime.create_run(body)
            self.assertEqual(accepted['state'], 'queued')
            try:
                # All three must enter the first device action before any finishes.
                await asyncio.wait_for(asyncio.gather(*(flag.wait() for flag in started.values())), 3)
                mid = self.runtime.journal.run(body['id'])
                self.assertEqual(sum(device['state'] == 'running' for device in mid['devices']), 3)
            finally:
                release.set()
            await asyncio.wait_for(self.runtime.tasks[body['id']], 3)
        final = self.runtime.journal.run(body['id'])
        self.assertEqual(final['state'], 'completed')
        self.assertTrue(all(device['state'] == 'completed' for device in final['devices']))

    async def test_aut08_two_phone_success_and_terminal_assert_failure(self):
        """Full durable journal + scheduler; only the phone with a failed
        final assertion fails, while the other phone completes independently.
        No Android hardware required (worker RPC deliberately mocked).
        """
        good, bad = target("10.60.12.21"), target("10.60.12.22")
        body = run_body(self.runtime.journal, [good, bad])
        body["concurrency"] = 2
        body["steps"] = [
            {"id": str(uuid4()), "type": "launch_app", "package": "com.example.demo"},
            {"id": str(uuid4()), "type": "if_else", "selector": {"text": "Confirm?"},
             "then_count": 1, "else_count": 1},
            {"id": str(uuid4()), "type": "tap", "selector": {"resource_id": "demo:id/yes"}},
            {"id": str(uuid4()), "type": "tap", "selector": {"resource_id": "demo:id/no"}},
            {"id": str(uuid4()), "type": "long_press", "x": 0.5, "y": 0.5,
             "width": 1080, "height": 2400, "duration_ms": 900},
            {"id": str(uuid4()), "type": "assert", "selector": {"text": "Completed"},
             "timeout_ms": 10000},
        ]
        seen = []
        async def execute(journal, holder, phone, step, *_args):
            seen.append((phone["device_id"], step["id"]))
            if step["type"] == "if_else":
                return {"state": "condition_passed",
                        "branch_taken": "then" if phone["device_id"] == good["device_id"] else "else"}
            if step["type"] == "assert":
                if phone["device_id"] == bad["device_id"]:
                    raise ExecutorError("selector_not_found", "Success indicator absent")
                return {"state": "assertion_passed"}
            return {"state": "action_sent"}

        with patch("automation_executor.runtime.execute_step", new=execute):
            await self.runtime.create_run(body)
            await asyncio.wait_for(self.runtime.tasks[body["id"]], 3)
        stored = self.runtime.journal.run(body["id"])
        results = {device["device_id"]: device for device in stored["devices"]}
        self.assertEqual(results[good["device_id"]]["state"], "completed")
        self.assertEqual(results[bad["device_id"]]["state"], "failed")
        self.assertEqual([entry["state"] for entry in results[good["device_id"]]["steps"]],
                         ["action_sent", "condition_passed", "action_sent", "skipped",
                          "action_sent", "assertion_passed"])
        self.assertEqual([entry["state"] for entry in results[bad["device_id"]]["steps"]],
                         ["action_sent", "condition_passed", "skipped", "action_sent",
                          "action_sent", "failed"])
        self.assertNotIn((good["device_id"], body["steps"][3]["id"]), seen)
        self.assertNotIn((bad["device_id"], body["steps"][2]["id"]), seen)
        self.assertEqual(results[bad["device_id"]]["error"]["code"], "selector_not_found")
        self.assertEqual(sum(did == good["device_id"] and sid == body["steps"][-1]["id"]
                             for did, sid in seen), 1)
        self.assertEqual(sum(did == bad["device_id"] and sid == body["steps"][-1]["id"]
                             for did, sid in seen), 1)
        # Running an assertion is not proof of business success by itself;
        # that semantic conclusion is made by the Rust server from the
        # immutable, explicitly marked snapshot and this persisted result.

    async def test_nested_if_else_never_evaluates_skipped_child_condition(self):
        first, second = target("10.60.9.21"), target("10.60.9.22")
        body = run_body(self.runtime.journal, [first, second])
        body["steps"] = [
            {"id": str(uuid4()), "type": "if_else", "selector": {"text": "Outer"},
             "then_count": 3, "else_count": 1},
            {"id": str(uuid4()), "type": "if_else", "selector": {"text": "Inner"},
             "then_count": 1, "else_count": 1},
            {"id": str(uuid4()), "type": "key", "key": "HOME"},
            {"id": str(uuid4()), "type": "key", "key": "BACK"},
            {"id": str(uuid4()), "type": "key", "key": "RECENT"},
            {"id": str(uuid4()), "type": "key", "key": "WAKE"},
        ]
        calls = []
        async def execute(journal, holder, phone, step, *args):
            calls.append((phone["device_id"], step["id"]))
            if step["type"] == "if_else":
                if step["id"] == body["steps"][0]["id"]:
                    return {"state": "condition_passed",
                            "branch_taken": "then" if phone["device_id"] == first["device_id"] else "else"}
                if phone["device_id"] == second["device_id"]:
                    raise AssertionError("Nested predicate must not run in skipped outer branch")
                return {"state": "condition_passed", "branch_taken": "else"}
            return {"state": "action_sent"}
        with patch("automation_executor.runtime.execute_step", new=execute):
            await self.runtime.create_run(body)
            await asyncio.wait_for(self.runtime.tasks[body["id"]], 3)
        devices = {device["device_id"]: device for device in self.runtime.journal.run(body["id"])["devices"]}
        self.assertEqual([entry["state"] for entry in devices[first["device_id"]]["steps"]],
                         ["condition_passed", "condition_passed", "skipped", "action_sent", "skipped", "action_sent"])
        self.assertEqual([entry["state"] for entry in devices[second["device_id"]]["steps"]],
                         ["condition_passed", "skipped", "skipped", "skipped", "action_sent", "action_sent"])
        self.assertNotIn((second["device_id"], body["steps"][1]["id"]), calls)
        self.assertEqual(devices[second["device_id"]]["steps"][1]["skipped_by_step_id"], body["steps"][0]["id"])
        self.assertEqual(devices[first["device_id"]]["steps"][2]["skipped_by_step_id"], body["steps"][1]["id"])
        self.assertTrue(all(device["state"] == "completed" for device in devices.values()))

    async def test_if_else_executes_different_branches_per_phone(self):
        first, second = target("10.60.8.21"), target("10.60.8.22")
        body = run_body(self.runtime.journal, [first, second])
        body["steps"] = [
            {"id": str(uuid4()), "type": "if_else", "selector": {"text": "Dialog"},
             "then_count": 2, "else_count": 1},
            {"id": str(uuid4()), "type": "key", "key": "HOME"},
            {"id": str(uuid4()), "type": "key", "key": "BACK"},
            {"id": str(uuid4()), "type": "key", "key": "RECENT"},
            {"id": str(uuid4()), "type": "key", "key": "WAKE"},
        ]
        calls = []
        async def execute(journal, holder, phone, step, *args):
            calls.append((phone["device_id"], step["id"]))
            if step["type"] == "if_else":
                branch = "then" if phone["device_id"] == first["device_id"] else "else"
                return {"state": "condition_passed", "branch_taken": branch}
            return {"state": "action_sent"}
        with patch("automation_executor.runtime.execute_step", new=execute):
            await self.runtime.create_run(body)
            await asyncio.wait_for(self.runtime.tasks[body["id"]], 3)
        devices = {item["device_id"]: item for item in self.runtime.journal.run(body["id"])["devices"]}
        self.assertEqual(devices[first["device_id"]]["state"], "completed")
        self.assertEqual(devices[second["device_id"]]["state"], "completed")
        self.assertEqual([x["state"] for x in devices[first["device_id"]]["steps"]],
                         ["condition_passed", "action_sent", "action_sent", "skipped", "action_sent"])
        self.assertEqual([x["state"] for x in devices[second["device_id"]]["steps"]],
                         ["condition_passed", "skipped", "skipped", "action_sent", "action_sent"])
        self.assertEqual(devices[first["device_id"]]["steps"][0]["branch_taken"], "then")
        self.assertEqual(devices[second["device_id"]]["steps"][0]["branch_taken"], "else")
        self.assertEqual(devices[first["device_id"]]["steps"][3]["skipped_by_step_id"], body["steps"][0]["id"])
        self.assertNotIn((second["device_id"], body["steps"][1]["id"]), calls)
        self.assertNotIn((first["device_id"], body["steps"][3]["id"]), calls)
        self.assertIn((first["device_id"], body["steps"][4]["id"]), calls)
        self.assertIn((second["device_id"], body["steps"][4]["id"]), calls)

    async def test_if_else_invalid_worker_branch_blocks_all_actions(self):
        phone = target("10.60.8.23")
        body = run_body(self.runtime.journal, [phone])
        body["steps"] = [
            {"id": str(uuid4()), "type": "if_else", "selector": {"text": "Dialog"},
             "then_count": 1, "else_count": 1},
            {"id": str(uuid4()), "type": "key", "key": "HOME"},
            {"id": str(uuid4()), "type": "key", "key": "BACK"},
        ]
        calls = []
        async def execute(journal, holder, target_item, step, *args):
            calls.append(step["id"])
            return {"state": "condition_passed", "branch_taken": "maybe"}
        with patch("automation_executor.runtime.execute_step", new=execute):
            await self.runtime.create_run(body)
            await asyncio.wait_for(self.runtime.tasks[body["id"]], 3)
        device = self.runtime.journal.run(body["id"])["devices"][0]
        self.assertEqual(device["state"], "failed")
        self.assertEqual(device["error"]["code"], "condition_result_invalid")
        self.assertEqual(calls, [body["steps"][0]["id"]])
        self.assertTrue(all(x["state"] == "skipped" for x in device["steps"][1:]))

    async def test_if_gone_skips_only_its_own_phone_actions_and_records_skipped(self):
        first, second = target("10.60.7.21"), target("10.60.7.22")
        body = run_body(self.runtime.journal, [first, second])
        body["steps"] = [
            {"id": str(uuid4()), "type": "if_gone", "selector": {"text": "Loading"}, "skip_count": 2},
            {"id": str(uuid4()), "type": "key", "key": "HOME"},
            {"id": str(uuid4()), "type": "key", "key": "BACK"},
            {"id": str(uuid4()), "type": "key", "key": "RECENT"},
        ]
        calls = []
        async def execute(journal, holder, phone, step, *args):
            calls.append((phone["device_id"], step["id"]))
            if step["type"] == "if_gone":
                return {"state": "condition_passed", "skip_steps": 2 if phone["device_id"] == first["device_id"] else 0}
            return {"state": "action_sent"}
        with patch("automation_executor.runtime.execute_step", new=execute):
            await self.runtime.create_run(body)
            await asyncio.wait_for(self.runtime.tasks[body["id"]], 3)
        devices = {item["device_id"]: item for item in self.runtime.journal.run(body["id"])["devices"]}
        self.assertEqual(devices[first["device_id"]]["state"], "completed")
        self.assertEqual(devices[second["device_id"]]["state"], "completed")
        self.assertEqual([x["state"] for x in devices[first["device_id"]]["steps"]],
                         ["condition_passed", "skipped", "skipped", "action_sent"])
        self.assertEqual([x["state"] for x in devices[second["device_id"]]["steps"]],
                         ["condition_passed", "action_sent", "action_sent", "action_sent"])
        self.assertNotIn((first["device_id"], body["steps"][1]["id"]), calls)
        self.assertNotIn((first["device_id"], body["steps"][2]["id"]), calls)
        self.assertIn((first["device_id"], body["steps"][3]["id"]), calls)

    async def test_invalid_if_gone_worker_result_fails_closed(self):
        phone = target("10.60.7.23")
        body = run_body(self.runtime.journal, [phone])
        body["steps"] = [
            {"id": str(uuid4()), "type": "if_gone", "selector": {"text": "Loading"}, "skip_count": 1},
            {"id": str(uuid4()), "type": "key", "key": "HOME"},
        ]
        calls = []
        async def execute(journal, holder, target_item, step, *args):
            calls.append(step["id"])
            return {"state": "condition_passed", "skip_steps": 99}
        with patch("automation_executor.runtime.execute_step", new=execute):
            await self.runtime.create_run(body)
            await asyncio.wait_for(self.runtime.tasks[body["id"]], 3)
        device = self.runtime.journal.run(body["id"])["devices"][0]
        self.assertEqual(device["state"], "failed")
        self.assertEqual(device["error"]["code"], "condition_result_invalid")
        self.assertEqual(calls, [body["steps"][0]["id"]])
        self.assertEqual(device["steps"][1]["state"], "skipped")

    async def test_two_distinct_runs_progress_in_parallel(self):
        first, second = target('10.60.3.31'), target('10.60.3.32')
        runs = [run_body(self.runtime.journal, [first]), run_body(self.runtime.journal, [second])]
        started = {d['device_id']: asyncio.Event() for d in (first,second)}
        release = asyncio.Event()
        async def execute(journal, holder, t, step, *args):
            started[t['device_id']].set()
            await release.wait()
            return {'state': 'succeeded'}
        with patch('automation_executor.runtime.execute_step', new=execute):
            for body in runs:
                await self.runtime.create_run(body)
            try:
                await asyncio.wait_for(asyncio.gather(*(flag.wait() for flag in started.values())), 3)
            finally:
                release.set()
            await asyncio.wait_for(asyncio.gather(*(self.runtime.tasks[body['id']] for body in runs)), 3)
        self.assertTrue(all(self.runtime.journal.run(body['id'])['state'] == 'completed' for body in runs))

    async def test_withdraw_running_phone_preserves_other_phone_and_its_second_step(self):
        a,b=target(),target('10.60.1.21'); body=run_body(self.runtime.journal,[a,b])
        started={a['device_id']:asyncio.Event(),b['device_id']:asyncio.Event()};release_b=asyncio.Event();calls=[]
        async def execute(journal,holder,t,step,*args):
            calls.append((t['device_id'],step['id']));started[t['device_id']].set()
            if t['device_id']==a['device_id']: await asyncio.Event().wait()
            elif step['id']==body['steps'][0]['id']: await release_b.wait()
            return {'state':'succeeded'}
        with patch('automation_executor.runtime.execute_step',new=execute):
            await self.runtime.create_run(body)
            await asyncio.wait_for(asyncio.gather(*(e.wait() for e in started.values())),2)
            result=await self.runtime.withdraw_device(body['id'],a['device_id'],dict(event_id=str(uuid4()),expected_executor_id=self.runtime.journal.executor_id,old_target_fingerprint=a['target_fingerprint']))
            rows={d['device_id']:d for d in result['devices']}
            self.assertTrue(rows[a['device_id']]['cleanup_confirmed']);self.assertEqual(rows[b['device_id']]['state'],'running')
            self.assertFalse(self.runtime.journal.cancelled(body['id']))
            release_b.set();await asyncio.wait_for(self.runtime.tasks[body['id']],2)
        final={d['device_id']:d for d in self.runtime.journal.run(body['id'])['devices']}
        self.assertEqual(final[b['device_id']]['state'],'completed')
        self.assertNotIn((a['device_id'],body['steps'][1]['id']),calls)
        self.assertIn((b['device_id'],body['steps'][1]['id']),calls)
        self.assertEqual(final[a['device_id']]['steps'][0]['state'],'outcome_unknown')

    async def test_preparation_is_async_idempotent_and_has_persisted_history(self):
        a = target()
        self.runtime.prepared = lambda t: Runtime.prepared(self.runtime, t)
        helper = SimpleNamespace(target=a, ready_flag=True, alive=lambda: True,
                                 ready=AsyncMock(return_value={'state':'automation_ready','message':'ready'}),
                                 close=AsyncMock(return_value=True))
        with patch('automation_executor.runtime.PreparedHelper',return_value=helper) as factory:
            request = {'target':a, 'request_id':str(uuid4())}
            accepted = await self.runtime.start_preparation(request)
            repeated = await self.runtime.start_preparation(request)
            self.assertEqual(accepted['id'], repeated['id'])
            await asyncio.wait_for(self.runtime.preparation_tasks[accepted['id']], 2)
            outcome = self.runtime.preparation(accepted['id'])
            self.assertEqual(outcome['state'],'succeeded')
            self.assertEqual(factory.call_count,1)
            self.assertTrue(self.runtime.preparation_logs(accepted['id'])['items'])
            history = self.runtime.preparation_history(a['device_id'],a['owner_user_id'])
            self.assertEqual(history['items'][0]['id'],accepted['id'])
        self.runtime.helpers.clear()
        # Read-only queries never bootstrap an absent helper or contact a real phone.
        self.assertEqual((await self.runtime.automation_status({'target':a}))['state'],'not_prepared')

    async def test_native_only_script_never_bootstraps_uiautomator(self):
        a=target()
        self.runtime.prepared=lambda t: Runtime.prepared(self.runtime,t)
        body=run_body(self.runtime.journal,[a])
        execute=AsyncMock(return_value={'state':'action_sent'})
        with patch('automation_executor.runtime.PreparedHelper') as helper, \
             patch('automation_executor.runtime.execute_step',new=execute):
            await self.runtime.create_run(body)
            await asyncio.wait_for(self.runtime.tasks[body['id']],3)
            self.assertEqual(self.runtime.journal.run(body['id'])['devices'][0]['state'],'completed')
            helper.assert_not_called()
            self.assertEqual(execute.await_count,2)

    async def test_script_auto_prepares_helper_and_reuses_it_for_next_run(self):
        # Previously, the first script failed capability_missing even though
        # ADB connected successfully. The executor now prepares the existing
        # helper using the same script's control claim before running steps.
        a=target()
        self.runtime.prepared=lambda t: Runtime.prepared(self.runtime,t)
        helper=SimpleNamespace(target=a, ready_flag=True, alive=lambda:True,
                               ready=AsyncMock(return_value={'state':'automation_ready'}),
                               close=AsyncMock(return_value=True))
        execute=AsyncMock(return_value={'state':'succeeded'})
        with patch('automation_executor.runtime.PreparedHelper',return_value=helper) as create_helper, \
             patch('automation_executor.runtime.execute_step',new=execute):
            body=run_body(self.runtime.journal,[a])
            body['steps'] = [
                {'id':str(uuid4()), 'type':'wait_for', 'selector':{'text':'Home'}, 'timeout_ms':200},
                {'id':str(uuid4()), 'type':'assert', 'selector':{'description':'Ready'}, 'timeout_ms':200},
            ]
            await self.runtime.create_run(body)
            await asyncio.wait_for(self.runtime.tasks[body['id']],3)
            self.assertEqual(self.runtime.journal.run(body['id'])['devices'][0]['state'],'completed')
            self.assertTrue(self.runtime.prepared(a))
            self.assertEqual(create_helper.call_count,1)
            self.assertEqual(execute.await_count,2)

            second=run_body(self.runtime.journal,[a])
            second['steps'] = [
                {'id':str(uuid4()), 'type':'wait_for', 'selector':{'text':'Home'}, 'timeout_ms':200},
                {'id':str(uuid4()), 'type':'assert', 'selector':{'description':'Ready'}, 'timeout_ms':200},
            ]
            await self.runtime.create_run(second)
            await asyncio.wait_for(self.runtime.tasks[second['id']],3)
            self.assertEqual(self.runtime.journal.run(second['id'])['devices'][0]['state'],'completed')
            self.assertEqual(create_helper.call_count,1,'healthy helper is reused rather than restarted')
            self.assertEqual(execute.await_count,4)
        self.runtime.helpers.clear()  # mocked worker needs no real shutdown.

    async def test_script_preparation_failure_is_recorded_without_executing_steps(self):
        a=target()
        self.runtime.prepared=lambda t: Runtime.prepared(self.runtime,t)
        helper=SimpleNamespace(target=a,ready_flag=False,alive=lambda:False,
                               ready=AsyncMock(side_effect=ExecutorError('automation_prepare_failed','准备控件失败')),
                               close=AsyncMock(return_value=True))
        execute=AsyncMock()
        body=run_body(self.runtime.journal,[a])
        body['steps'] = [{'id':str(uuid4()), 'type':'wait_for', 'selector':{'text':'Home'}, 'timeout_ms':200}]
        with patch('automation_executor.runtime.PreparedHelper',return_value=helper), \
             patch('automation_executor.runtime.execute_step',new=execute):
            await self.runtime.create_run(body)
            await asyncio.wait_for(self.runtime.tasks[body['id']],3)
        device=self.runtime.journal.run(body['id'])['devices'][0]
        self.assertEqual(device['state'],'failed')
        self.assertEqual(device['error']['code'],'automation_prepare_failed')
        self.assertTrue(all(s['state']=='skipped' for s in device['steps']))
        execute.assert_not_awaited()
        helper.close.assert_awaited_once()

    async def test_script_never_prepares_helper_before_authorization(self):
        a=target()
        self.runtime.prepared=lambda t: Runtime.prepared(self.runtime,t)
        self.runtime._authorize=AsyncMock(side_effect=ExecutorError('grant_revoked','权限已撤销'))
        body=run_body(self.runtime.journal,[a])
        with patch('automation_executor.runtime.PreparedHelper') as create_helper:
            await self.runtime.create_run(body)
            await asyncio.wait_for(self.runtime.tasks[body['id']],3)
            create_helper.assert_not_called()
        self.assertEqual(self.runtime.journal.run(body['id'])['devices'][0]['state'],'interrupted')

    async def test_retire_cancels_inflight_preparation_before_target_disconnect(self):
        a=target();started=asyncio.Event();stopped=asyncio.Event();order=[]
        async def prepare(*args):
            started.set()
            try: await asyncio.Event().wait()
            finally: order.append('prepare_stopped');stopped.set()
        async def disconnect(*args):
            self.assertTrue(stopped.is_set());order.append('disconnected')
        with patch('automation_executor.runtime.adb.prepare',new=prepare),patch('automation_executor.runtime.adb.disconnect',new=disconnect):
            task=asyncio.create_task(self.runtime.prepare({'target':a,'action':'connect'}));await asyncio.wait_for(started.wait(),2)
            event=str(uuid4());body=dict(event_id=event,expected_executor_id=self.runtime.journal.executor_id,target=a)
            result=await self.runtime.retire_target(a['device_id'],body)
            self.assertTrue(result['cleanup_confirmed']);self.assertEqual(result['executor_id'],self.runtime.journal.executor_id)
            await asyncio.gather(task,return_exceptions=True)
            self.assertEqual(order,['prepare_stopped','disconnected'])
            await self.runtime.retire_target(a['device_id'],body)
            self.assertEqual(len(order),2,'completed cleanup must not disconnect again')
            with self.assertRaises(ExecutorError): await self.runtime.prepare({'target':a,'action':'connect'})

    async def test_disconnect_is_target_specific_and_does_not_disconnect_new_owner(self):
        old=target();new={**old,'target_fingerprint':str(uuid4()),'endpoint_revision':2}
        self.runtime.journal.claim(new,'new-session','session')
        with patch('automation_executor.runtime.adb.disconnect',new=AsyncMock()) as disconnect:
            result=await self.runtime.retire_target(old['device_id'],dict(event_id=str(uuid4()),expected_executor_id=self.runtime.journal.executor_id,target=old))
            self.assertTrue(result['cleanup_confirmed']);disconnect.assert_not_called()
        self.assertIsNotNone(self.runtime.journal.control(new));self.runtime.journal.release('new-session')

    async def test_restart_helper_cleanup_requires_owned_process_exit_proof(self):
        a=target();runtime=self.runtime
        runtime.journal.worker('helper:'+a['device_id'],99999999,'unknown')
        body=dict(event_id=str(uuid4()),expected_executor_id=runtime.journal.executor_id,target=a)
        with patch('automation_executor.runtime.cleanup_record',return_value=False) as cleanup,patch('automation_executor.runtime.adb.disconnect',new=AsyncMock()) as disconnect:
            unknown=await runtime.retire_target(a['device_id'],body)
            self.assertFalse(unknown['cleanup_confirmed']);cleanup.assert_not_called();disconnect.assert_not_called()
            runtime.journal.mark_prepared(a)
            pending=await runtime.retire_target(a['device_id'],body)
            self.assertFalse(pending['cleanup_confirmed']);disconnect.assert_not_called()
        with patch('automation_executor.runtime.cleanup_record',return_value=True),patch('automation_executor.runtime.adb.disconnect',new=AsyncMock()):
            done=await runtime.retire_target(a['device_id'],body)
            self.assertTrue(done['cleanup_confirmed'])
        self.assertIsNone(runtime.journal.worker_record('helper:'+a['device_id']))

    async def test_adb_disconnect_uses_serial_not_global_disconnect(self):
        from automation_executor import adb
        a=target()
        with patch('automation_executor.adb.command',new=AsyncMock(return_value=(0,'',''))) as command:
            await adb.disconnect('adb',a)
            command.assert_awaited_once_with('adb','disconnect','10.60.1.20:37001',timeout=8)

if __name__=='__main__': unittest.main()


class PhantomRegistrationTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.directory = tempfile.TemporaryDirectory()
        media = ModuleType('automation_executor.media')
        media.MediaManager = lambda: SimpleNamespace(
            close_all=AsyncMock(), _max_sessions=10,
            _starting=FairDeviceSlots(2), _lock=asyncio.Lock())
        self.media_patch = patch.dict(sys.modules, {'automation_executor.media': media})
        self.media_patch.start()
        self.settings = SimpleNamespace(data_dir=Path(self.directory.name), token='x'*32,
            max_running=2, max_pending=10, callback_url='', adb_binary='adb',
            max_artifact_bytes=10000000)
        self.runtime = Runtime(self.settings)

    async def asyncTearDown(self):
        await self.runtime.shutdown()
        self.media_patch.stop()
        self.directory.cleanup()

    async def test_unconnected_pre_pairing_target_can_be_verified_without_retiring_it(self):
        a=target()
        body={'event_id':str(uuid4()),'expected_executor_id':self.runtime.journal.executor_id,'target':a}
        with patch('automation_executor.runtime.adb.command',new=AsyncMock(return_value=(0,'List of devices attached\n\n',''))):
            result=await self.runtime.verify_unused_target(a['device_id'],body)
        self.assertTrue(result['unused_confirmed'])
        self.assertFalse(self.runtime.journal.target_refused(a))
        with patch('automation_executor.runtime.adb.command',new=AsyncMock(return_value=(0,'List of devices attached\n'+a['ip']+':37001\tdevice\n',''))):
            result=await self.runtime.verify_unused_target(a['device_id'],body)
        self.assertFalse(result['unused_confirmed'])

    async def test_offline_serial_is_not_proof_of_unused_target(self):
        a=target()
        body={'event_id':str(uuid4()),'expected_executor_id':self.runtime.journal.executor_id,'target':a}
        with patch('automation_executor.runtime.adb.command',new=AsyncMock(return_value=(0,'List of devices attached\n'+a['ip']+':37001\toffline\n',''))):
            result=await self.runtime.verify_unused_target(a['device_id'],body)
        self.assertFalse(result['unused_confirmed'])

    async def test_disconnect_unknown_serial_needs_fresh_absence_proof(self):
        from automation_executor import adb
        a=target()
        with patch('automation_executor.adb.command',new=AsyncMock(side_effect=[
            (1,'','not connected'),(0,'List of devices attached\n\n','')
        ])):
            await adb.disconnect('adb',a)
        with patch('automation_executor.adb.command',new=AsyncMock(side_effect=[
            (1,'','not connected'),(0,'List of devices attached\n'+a['ip']+':37001\toffline\n','')
        ])):
            with self.assertRaises(ExecutorError): await adb.disconnect('adb',a)


class AndroidKeyguardTests(unittest.TestCase):
    def make_source(self):
        from automation_executor.vendor.scrcpy_source import ScrcpySource
        source=ScrcpySource('10.60.1.20:37001','wall',None,lambda:None)
        packets=[]
        source.size=(1080,2400)
        source.control=SimpleNamespace(sendall=packets.append)
        return source, packets

    def test_wake_without_password_sends_only_wakeup(self):
        src,packets=self.make_source()
        self.assertEqual(src.wake_and_unlock(None),'wake_sent')
        # Explicit physical screen-on packet must precede the WAKE key.
        self.assertEqual(len(packets),2)
        self.assertEqual(packets[0],struct.pack('>BB',10,1))
        self.assertEqual(packets[1],struct.pack('>BBIII',0,0,224,0,0)
                         +struct.pack('>BBIII',0,1,224,0,0))
        self.assertNotIn(b'1234',b''.join(packets))

    def test_unlocked_device_never_receives_secret(self):
        src,packets=self.make_source()
        src._adb=lambda *args,**kwargs: 'mShowingLockscreen=false'
        with patch('automation_executor.vendor.scrcpy_source.time.sleep',return_value=None):
            self.assertEqual(src.wake_and_unlock('123456'),'already_unlocked')
        self.assertNotIn(b'123456',b''.join(packets))
        self.assertEqual(len(packets),2)
        self.assertEqual(packets[0],struct.pack('>BB',10,1))

    def test_password_injected_only_after_two_positive_keyguard_checks(self):
        src,packets=self.make_source()
        calls=[]
        def fake_adb(*args, **kwargs):
            calls.append(args)
            return 'mShowingLockscreen=true'
        src._adb=fake_adb
        with patch('automation_executor.vendor.scrcpy_source.time.sleep',return_value=None):
            self.assertEqual(src.wake_and_unlock('123456'),'unlock_sent')
        self.assertFalse(any(args[:3] in (('shell','wm','size'),('shell','input','swipe')) for args in calls))
        self.assertGreaterEqual(sum(args[:2] == ('shell','dumpsys') for args in calls), 4)
        # Numeric PIN uses the same full scrcpy text frame as other passwords.
        self.assertEqual(len(packets),4)
        self.assertEqual(packets[0],struct.pack('>BB',10,1))
        self.assertEqual(packets[2],bytes([1])+struct.pack('>I',6)+b'123456')
        self.assertEqual(packets[3],struct.pack('>BBIII',0,0,66,0,0)
                         +struct.pack('>BBIII',0,1,66,0,0))
        self.assertFalse(any(packet[:2] == bytes([0,0]) and
                             struct.unpack('>BBIII',packet[:14])[2] in range(7,17)
                             for packet in packets if len(packet)>=14))

    def test_numeric_pin_preserves_leading_zero_and_checks_result_after_enter(self):
        src,packets=self.make_source()
        src._adb=lambda *args,**kwargs: 'Physical size: 1080x2400'
        with patch.object(src,'_stable_keyguard',side_effect=[True,True]), patch.object(src,'_keyguard',return_value=False):
            self.assertEqual(src.wake_and_unlock('0123'),'unlock_confirmed')
        self.assertEqual(packets[0],struct.pack('>BB',10,1))
        self.assertEqual(packets[2],bytes([1])+struct.pack('>I',4)+b'0123')
        self.assertEqual(packets[3],struct.pack('>BBIII',0,0,66,0,0)
                         +struct.pack('>BBIII',0,1,66,0,0))

    def test_mixed_password_keeps_private_text_input(self):
        src,packets=self.make_source()
        src._adb=lambda *args,**kwargs: 'Physical size: 1080x2400'
        with patch.object(src,'_stable_keyguard',return_value=True), patch.object(src,'_keyguard',return_value=True):
            self.assertEqual(src.wake_and_unlock('a123'),'unlock_sent')
        self.assertEqual(packets[-2],bytes([1,0,0,0,4])+b'a123')
        self.assertEqual(len(packets),4)
        self.assertEqual(packets[0],struct.pack('>BB',10,1))

    def test_wake_waits_1_5_seconds_without_swipe_before_text(self):
        src,packets=self.make_source()
        waits=[]
        stages=[]
        src.stop_event.wait=lambda seconds: (waits.append(seconds), False)[1]
        src._adb=lambda *args,**kwargs: stages.append(args) or 'mShowingLockscreen=true'
        with patch.object(src,'_stable_keyguard',return_value=True):
            self.assertEqual(src.wake_and_unlock('1234'),'unlock_sent')
        self.assertEqual(packets[0],struct.pack('>BB',10,1))
        self.assertEqual(waits[0],1.0)
        self.assertIn(1.5,waits)
        self.assertLess(waits.index(1.0),waits.index(1.5))
        self.assertFalse(any(args[:3] in (('shell','wm','size'),('shell','input','swipe')) for args in stages))
        self.assertEqual(packets[2],bytes([1])+struct.pack('>I',4)+b'1234')
        self.assertEqual(len([p for p in packets if p[0] == 0]),2) # WAKE, ENTER
        self.assertIn(0.18,waits)
        self.assertLess(waits.index(1.5),waits.index(0.18))

    def test_lock_disappearing_during_wait_never_receives_password(self):
        src,packets=self.make_source()
        src._adb=lambda *args,**kwargs: 'mShowingLockscreen=true'
        waits=[]
        src.stop_event.wait=lambda seconds: (waits.append(seconds), False)[1]
        with patch.object(src,'_stable_keyguard',side_effect=[True,False]):
            self.assertEqual(src.wake_and_unlock('1234'),'already_unlocked')
        self.assertEqual(waits,[1.0,1.5])
        self.assertEqual(len(packets),2)
        self.assertFalse(any(packet[0]==1 for packet in packets))

    def test_lock_state_becomes_unknown_never_types_password(self):
        src,packets=self.make_source()
        src._adb=lambda *args,**kwargs: 'unknown Android policy output'
        with patch('automation_executor.vendor.scrcpy_source.time.sleep',return_value=None):
            self.assertEqual(src.wake_and_unlock('123456'),'lock_state_unknown')
        self.assertNotIn(b'123456',b''.join(packets))



class HelperStartupTests(unittest.IsolatedAsyncioTestCase):
    async def test_real_spawn_isolation_probe_without_android(self):
        import multiprocessing
        import os
        from automation_executor.worker import _isolation_probe
        context = multiprocessing.get_context("spawn")
        parent, child = context.Pipe()
        gate = context.Event()
        process = context.Process(target=_isolation_probe,
                                  args=(child, gate, os.getpid(), "adb"), daemon=True)
        process.start()
        child.close()
        try:
            gate.set()
            messages = []
            for _ in range(3):
                self.assertTrue(await asyncio.to_thread(parent.poll, 5), "spawned worker did not answer")
                messages.append(parent.recv())
            self.assertEqual([item.get("stage") for item in messages[:2]],
                             ["isolate_process", "wait_for_gate"])
            self.assertEqual(messages[2], {"ok": True})
        finally:
            parent.close()
            await asyncio.to_thread(process.join, 2)
            if process.is_alive():
                process.kill()
                await asyncio.to_thread(process.join, 2)
            process.close()

    async def test_prepared_helper_retains_spawn_semaphore_until_worker_exit(self):
        # Reproduces the production ownership pattern, with a real spawned
        # process and no ADB or phone access.
        import gc
        from automation_executor import worker
        journal = SimpleNamespace(
            worker=lambda *args: None,
            clear_worker=lambda *args: None,
            worker_record=lambda *args: None,
        )
        with patch.object(worker, "_helper_entry", _spawned_prepared_helper_probe):
            helper = worker.PreparedHelper(journal, target(), "adb")
        try:
            gc.collect()  # Trigger the premature SemLock unlink from the old code.
            self.assertIsNotNone(helper.start_gate)
            self.assertTrue(helper.start_gate.is_set())
            messages = []
            for _ in range(3):
                self.assertTrue(await asyncio.to_thread(helper.channel.poll, 5))
                messages.append(helper.channel.recv())
            self.assertEqual(
                [event.get("stage") for event in messages[:2]],
                ["isolate_process", "wait_for_gate"],
            )
            self.assertEqual(messages[2], {"ok": True})
        finally:
            clean = await helper.close()
        self.assertTrue(clean)
        self.assertIsNone(helper.start_gate)

    async def test_pipe_eof_reports_process_signal_not_android_timeout(self):
        from automation_executor.worker import PreparedHelper
        helper = object.__new__(PreparedHelper)
        helper.channel = SimpleNamespace(poll=lambda timeout: True,
                                         recv=lambda: (_ for _ in ()).throw(EOFError()))
        helper.process = SimpleNamespace(is_alive=lambda: False, join=lambda timeout: None, exitcode=-9)
        helper.ready_flag = False
        with self.assertRaises(ExecutorError) as failure:
            await helper.ready(timeout=2)
        self.assertEqual(failure.exception.code, "automation_worker_signalled")
        self.assertIn("信号 9", failure.exception.message)

    async def test_readiness_progress_and_success_without_hanging_a_thread(self):
        from automation_executor.worker import PreparedHelper
        helper=object.__new__(PreparedHelper)
        events=[
            {'stage':'import_sdk'}, {'stage':'connect_device'},
            {'stage':'start_uiautomator'},
            {'ok':True,'result':{'state':'automation_ready'}},
        ]
        helper.channel=SimpleNamespace(poll=lambda timeout: bool(events),recv=lambda: events.pop(0))
        helper.process=SimpleNamespace(is_alive=lambda: True)
        helper.ready_flag=False
        result=await helper.ready(timeout=2)
        self.assertEqual(result['state'],'automation_ready')
        self.assertTrue(helper.ready_flag)

    async def test_dead_worker_reports_phase_without_a_full_timeout(self):
        from automation_executor.worker import PreparedHelper
        helper=object.__new__(PreparedHelper)
        events=[{'stage':'connect_device'}]
        helper.channel=SimpleNamespace(poll=lambda timeout: bool(events),recv=lambda: events.pop(0))
        helper.process=SimpleNamespace(is_alive=lambda: False, join=lambda timeout: None, exitcode=None)
        helper.ready_flag=False
        with self.assertRaises(ExecutorError) as exc:
            await helper.ready(timeout=1)
        self.assertEqual(exc.exception.code,'automation_worker_interrupted')
        self.assertIn('绑定 ADB 设备',exc.exception.message)

    async def test_no_worker_reply_reports_timeout_stage(self):
        from automation_executor.worker import PreparedHelper
        import time
        helper=object.__new__(PreparedHelper)
        helper.channel=SimpleNamespace(poll=lambda timeout: (time.sleep(timeout),False)[-1])
        helper.process=SimpleNamespace(is_alive=lambda: True)
        helper.ready_flag=False
        with self.assertRaises(ExecutorError) as exc:
            await helper.ready(timeout=.08)
        self.assertEqual(exc.exception.code,'automation_prepare_timeout')
        self.assertIn('启动执行进程',exc.exception.message)


class HelperBootstrapTests(unittest.TestCase):
    def test_initialization_verifies_ui_hierarchy_without_device_info_rpc(self):
        from automation_executor import worker
        messages=[]
        stopped=[]
        class Device:
            @property
            def info(self):
                raise AssertionError("Android 17 deviceInfo must not be required")
            def window_size(self):
                return (1080,2400)
            def dump_hierarchy(self):
                return '<hierarchy rotation="0"></hierarchy>'
            def stop_uiautomator(self,wait=False):
                stopped.append(wait)
        module=ModuleType('uiautomator2')
        module.connect_usb=lambda address: Device()
        channel=SimpleNamespace(send=messages.append,recv=lambda:'stop',close=lambda:None)
        gate=SimpleNamespace(wait=lambda seconds:True)
        with patch('automation_executor.worker._isolate'), patch.dict(sys.modules,{'uiautomator2':module}):
            worker._helper_entry(channel,gate,1234,target(),'adb')
        self.assertEqual([m.get('stage') for m in messages[:5]],
                         ['isolate_process','wait_for_gate','import_sdk','connect_device','start_uiautomator'])
        self.assertEqual(messages[5]['stage'],'verify_hierarchy')
        self.assertEqual(messages[6]['result']['state'],'automation_ready')
        self.assertEqual(messages[6]['result']['display'],{'width':1080,'height':2400})
        self.assertEqual(stopped,[False])


class NativeActionTests(unittest.TestCase):
    def test_key_and_launch_app_do_not_require_uiautomator(self):
        from automation_executor import steps
        import adbutils
        commands=[]
        class Device:
            def get_state(self):return 'device'
            def shell2(self,args,timeout=None):
                commands.append((args,timeout))
                return SimpleNamespace(returncode=0,output='Starting: Intent')
        fake=Device()
        with patch.object(adbutils.adb,'device',return_value=fake):
            for step in ({'type':'key','key':'HOME'},
                         {'type':'launch_app','package':'com.example.launcher'}):
                result=steps.execute(target(),step,{},None)
                self.assertEqual(result['state'],'action_sent')
        self.assertEqual(commands[0],(['input','keyevent','3'],8))
        self.assertEqual(commands[1][0][-1],'com.example.launcher')
