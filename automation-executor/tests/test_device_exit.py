"""Simulated device isolation using the real durable journal and run scheduler."""
import asyncio
import json
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


def target(ip='10.60.1.20'):
    return dict(device_id=str(uuid4()), name='Phone', owner_user_id=1, machine_id=str(uuid4()),
                ip=ip, adb_port=37001, connection_mode='wireless', target_fingerprint=str(uuid4()),
                route_context='default', adb_service_id=str(uuid4()), endpoint_revision=1)


def run_body(journal, targets):
    return dict(id=str(uuid4()), name='two phones', owner_user_id=1, actor_user_id=1,
                expected_executor_id=journal.executor_id, targets=targets, concurrency=2, parameters={},
                steps=[dict(id=str(uuid4()), type='key', key='HOME'), dict(id=str(uuid4()), type='key', key='BACK')])


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


class RuntimeTests(unittest.IsolatedAsyncioTestCase):
    async def asyncSetUp(self):
        self.directory=tempfile.TemporaryDirectory()
        media=ModuleType('automation_executor.media');media.MediaManager=lambda:SimpleNamespace(close_all=AsyncMock())
        self.media_patch=patch.dict(sys.modules,{'automation_executor.media':media});self.media_patch.start()
        settings=SimpleNamespace(data_dir=Path(self.directory.name),token='x'*32,max_running=3,max_pending=10,
                                 callback_url='http://test.invalid',adb_binary='adb',max_artifact_bytes=10000000)
        self.runtime=Runtime(settings)
        self.runtime._authorize=AsyncMock();self.runtime._publish=AsyncMock();self.runtime.prepared=lambda _:True
        self.adb_patch=patch('automation_executor.runtime.adb.require_connected',new=AsyncMock());self.adb_patch.start()

    async def asyncTearDown(self):
        await self.runtime.shutdown();self.media_patch.stop();self.adb_patch.stop();self.directory.cleanup()

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
        media.MediaManager = lambda: SimpleNamespace(close_all=AsyncMock())
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
        self.assertEqual(len(packets),1)
        self.assertNotIn(b'1234',b''.join(packets))

    def test_unlocked_device_never_receives_secret(self):
        src,packets=self.make_source()
        src._adb=lambda *args,**kwargs: 'mShowingLockscreen=false'
        with patch('automation_executor.vendor.scrcpy_source.time.sleep',return_value=None):
            self.assertEqual(src.wake_and_unlock('123456'),'already_unlocked')
        self.assertNotIn(b'123456',b''.join(packets))
        self.assertEqual(len(packets),1)

    def test_password_injected_only_after_two_positive_keyguard_checks(self):
        src,packets=self.make_source()
        src._adb=lambda *args,**kwargs: 'mShowingLockscreen=true'
        with patch('automation_executor.vendor.scrcpy_source.time.sleep',return_value=None):
            self.assertEqual(src.wake_and_unlock('123456'),'unlock_sent')
        self.assertIn(b'123456',b''.join(packets))
        self.assertEqual(packets[-2],bytes([1,0,0,0,6])+b'123456')
        self.assertGreaterEqual(len(packets),7)

    def test_lock_state_becomes_unknown_never_types_password(self):
        src,packets=self.make_source()
        src._adb=lambda *args,**kwargs: 'unknown Android policy output'
        with patch('automation_executor.vendor.scrcpy_source.time.sleep',return_value=None):
            self.assertEqual(src.wake_and_unlock('123456'),'lock_state_unknown')
        self.assertNotIn(b'123456',b''.join(packets))



class HelperStartupTests(unittest.IsolatedAsyncioTestCase):
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
        helper.process=SimpleNamespace(is_alive=lambda: False)
        helper.ready_flag=False
        with self.assertRaises(ExecutorError) as exc:
            await helper.ready(timeout=1)
        self.assertEqual(exc.exception.code,'automation_worker_exited')
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
    def test_initialization_uses_public_device_info_not_private_jsonrpc(self):
        from automation_executor import worker
        messages=[]
        stopped=[]
        class Device:
            @property
            def info(self):
                return {'displayWidth':1080,'displayHeight':2400}
            def stop_uiautomator(self,wait=False):
                stopped.append(wait)
        module=ModuleType('uiautomator2')
        module.connect_usb=lambda address: Device()
        channel=SimpleNamespace(send=messages.append,recv=lambda:'stop',close=lambda:None)
        gate=SimpleNamespace(wait=lambda seconds:True)
        with patch('automation_executor.worker._isolate'), patch.dict(sys.modules,{'uiautomator2':module}):
            worker._helper_entry(channel,gate,1234,target(),'adb')
        self.assertEqual([m.get('stage') for m in messages[:3]],
                         ['import_sdk','connect_device','start_uiautomator'])
        self.assertEqual(messages[3]['result']['state'],'automation_ready')
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
