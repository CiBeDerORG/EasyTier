import assert from 'node:assert/strict';
import test from 'node:test';
import {
  businessCheckStepId, businessDeviceState, businessRunState,
} from '../src/modules/automation-business-verification.ts';

const assertion=(id='verify',extras={})=>({
  id,type:'assert',selector:{resource_id:'com.example:id/success'},timeout_ms:10000,...extras,
});
const device=(state,checkState)=>({
  state, steps:checkState ? [{step_id:'verify',state:checkState}]:[],
});
const run=(steps,devices)=>({script_snapshot:{steps},devices});

test('old assertions without explicit marker never become business verified',()=>{
  const legacy=run([assertion()], [device('completed','assertion_passed')]);
  assert.equal(businessCheckStepId(legacy.script_snapshot.steps),null);
  assert.equal(businessRunState(legacy),'not_verified');
  assert.equal(businessDeviceState(legacy,legacy.devices[0]),'not_verified');
});

test('only explicitly marked last assertion verifies business outcome',()=>{
  const marked=run([
    {id:'go',type:'tap',x:0.5,y:0.5,width:800,height:1600},
    assertion('verify',{business_verification:true}),
  ],[device('completed','assertion_passed')]);
  assert.equal(businessCheckStepId(marked.script_snapshot.steps),'verify');
  assert.equal(businessDeviceState(marked,marked.devices[0]),'verified');
  assert.equal(businessRunState(marked),'verified');
});

test('action-sent, skipped and running assertion are never success',()=>{
  const steps=[assertion('verify',{business_verification:true})];
  for(const check of ['action_sent','skipped','running',undefined]){
    const r=run(steps,[device('completed',check)]);
    assert.equal(businessRunState(r),'not_reached',String(check));
  }
  assert.equal(businessRunState(run(steps,[device('running','assertion_passed')])),'pending');
});

test('explicit failed assertion marks business failure; failed earlier action does not',()=>{
  const steps=[assertion('verify',{business_verification:true})];
  assert.equal(businessRunState(run(steps,[device('failed','failed')])),'failed');
  assert.equal(businessRunState(run(steps,[device('failed',undefined)])),'not_reached');
});

test('multi-device mixed, unknown and pending outcomes never appear fully verified',()=>{
  const steps=[assertion('verify',{business_verification:true})];
  assert.equal(businessRunState(run(steps,[device('completed','assertion_passed'),device('failed','failed')])),'partial');
  assert.equal(businessRunState(run(steps,[device('completed','assertion_passed'),device('outcome_unknown','outcome_unknown')])),'outcome_unknown');
  assert.equal(businessRunState(run(steps,[device('queued'),device('running')])),'pending');
});

test('nonterminal and nonassertion markers are invalid and never imply success',()=>{
  assert.equal(businessCheckStepId([assertion('verify',{business_verification:true}),{id:'end',type:'key',key:'HOME'}]),null);
  assert.equal(businessCheckStepId([{id:'key',type:'key',key:'BACK',business_verification:true}]),null);
  assert.equal(businessCheckStepId([assertion('a',{business_verification:true}),assertion('verify',{business_verification:true})]),null);
  assert.equal(businessCheckStepId([{...assertion('verify'),type:'assert_property',
    selector:{resource_id:'com.example:id/check'},property:'checked',expected:true,business_verification:true}]),'verify');
});
