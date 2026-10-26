import assert from 'node:assert/strict';
import test from 'node:test';
import {
  initPreflightRow, probePreflightRow, probePreflightDevices,
  requiresAutomationHelper, selectedReadyIds,
} from '../src/modules/automation-preflight.ts';

function device(id, overrides = {}) {
  return {
    id, name: 'phone-' + id, owner_user_id: 1,
    target_fingerprint: 'fingerprint-' + id, availability_reason: null, active_session: null,
    connection: { enabled: true, adb_port: 37421, connection_mode: 'wireless' },
    ...overrides,
  };
}
function probes({ adb = 'connected', component = 'ready' } = {}) {
  const calls = [];
  return {
    calls,
    async adbStatus(_owner, id) { calls.push('adb:' + id); return { device_id: id, state: adb }; },
    async componentStatus(_owner, id) { calls.push('ui:' + id); return { device_id: id, executor_id: 'runner', state: component }; },
  };
}

test('only native keys and launch-app steps run without UiAutomator helper', () => {
  assert.equal(requiresAutomationHelper([{id:'a',type:'key',key:'BACK'},{id:'b',type:'launch_app',package:'com.app'}]),false);
  for (const type of ['tap','swipe','drag','input_text','if_else','if_gone','assert','screenshot'])
    assert.equal(requiresAutomationHelper([{id:'a',type}]),true,type);
});

test('available native-only phone is ready on ADB check alone', async () => {
  const client = probes({ component: 'not_prepared' });
  const result = await probePreflightRow(initPreflightRow(device('a'),'a'),1,false,client);
  assert.equal(result.state,'ready');
  assert.equal(result.selected,true);
  assert.deepEqual(client.calls,['adb:a']);
});

test('UI steps require ready component; missing helper needs explicit preparation', async () => {
  for (const [component, expected] of [['ready','ready'],['not_prepared','needs_preparation'],['preparing','needs_preparation'],['unavailable','unavailable']]) {
    const client = probes({ component });
    const result = await probePreflightRow(initPreflightRow(device('a'),'a'),1,true,client);
    assert.equal(result.state,expected);
    assert.equal(result.selected,expected==='ready');
    assert.deepEqual(client.calls,['adb:a','ui:a']);
  }
});

test('ADB failure prevents component probing and cannot accidentally be selected', async () => {
  for (const state of ['offline','disconnected','unauthorized','unavailable']) {
    const client = probes({ adb: state });
    const row = await probePreflightRow(initPreflightRow(device('a'),'a'),1,true,client);
    assert.equal(row.state,'unavailable');
    assert.equal(row.selected,false);
    assert.deepEqual(client.calls,['adb:a']);
  }
});

test('stale registration, occupied or disabled device never executes any probe', async () => {
  const problematic = [
    undefined, device('a',{connection:{enabled:false,adb_port:1,connection_mode:'tcp'}}),
    device('a',{target_fingerprint:null}), device('a',{availability_reason:'gateway unavailable'}),
    device('a',{active_session:{id:'active',state:'live',actor_user_id:1,expires_at:''}}),
  ];
  for (const item of problematic) {
    const client = probes();
    const row = await probePreflightRow(initPreflightRow(item,'a'),1,true,client);
    assert.equal(row.state,'unavailable');
    assert.equal(row.selected,false);
    assert.deepEqual(client.calls,[]);
  }
});

test('mismatched response ID and rejected RPC both fail closed', async () => {
  const wrong = {
    adbStatus: async()=>({device_id:'different',state:'connected'}),
    componentStatus:async()=>({device_id:'a',state:'ready',executor_id:'x'}),
  };
  assert.equal((await probePreflightRow(initPreflightRow(device('a'),'a'),1,true,wrong)).state,'query_failed');
  const badUI = {
    adbStatus:async()=>({device_id:'a',state:'connected'}),
    componentStatus:async()=>({device_id:'different',state:'ready'}),
  };
  assert.equal((await probePreflightRow(initPreflightRow(device('a'),'a'),1,true,badUI)).state,'query_failed');
  const error = {
    adbStatus:async()=>{throw new Error('timeout');},
    componentStatus:async()=>{throw new Error('should not run');},
  };
  assert.equal((await probePreflightRow(initPreflightRow(device('a'),'a'),1,true,error)).selected,false);
});

test('batch probes have independent results and keep parallel workers bounded', async () => {
  const rows = Array.from({length:9},(_,i)=>initPreflightRow(device('id'+i),'id'+i));
  let inflight = 0, maximum = 0;
  const client = {
    async adbStatus(_owner,id) {
      inflight++; maximum=Math.max(maximum,inflight);
      await new Promise(resolve=>setTimeout(resolve,3));
      inflight--;
      return {device_id:id,state:id==='id4'?'offline':'connected'};
    },
    async componentStatus(_owner,id) {
      return {device_id:id,executor_id:'runner',state:id==='id5'?'not_prepared':'ready'};
    },
  };
  const results = [];
  await probePreflightDevices(rows,1,true,client,row=>results.push(row),()=>true,3);
  assert.equal(results.length,9);
  assert.ok(maximum<=3);
  assert.equal(results.find(row=>row.id==='id4').state,'unavailable');
  assert.equal(results.find(row=>row.id==='id5').state,'needs_preparation');
  assert.equal(selectedReadyIds(results).length,7);
});

test('closing the dialog stops publishing late probe results', async () => {
  let valid=true, publish=0;
  const rows=[initPreflightRow(device('a'),'a'),initPreflightRow(device('b'),'b')];
  const client={
    async adbStatus(_owner,id){valid=false;return {device_id:id,state:'connected'};},
    async componentStatus(_owner,id){return {device_id:id,state:'ready'};},
  };
  await probePreflightDevices(rows,1,false,client,()=>publish++,()=>valid,2);
  assert.equal(publish,0);
});
