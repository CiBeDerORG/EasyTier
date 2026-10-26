import assert from 'node:assert/strict';
import test from 'node:test';
import {
  createAutomationRepairRequest, planAutomationRepair, cloneRepairSteps,
} from '../src/modules/automation-run-repair.ts';

const steps = [
  {id:'a',type:'tap',selector:{resource_id:'demo:id/submit'}},
  {id:'b',type:'assert',selector:{text:'Done'},timeout_ms:10000,business_verification:true},
];
const history=(snapshot={script_id:'script-a',revision:3,name:'Checkout',steps})=>({
  id:'run-1',name:'Checkout',script_snapshot:snapshot,
});
const device=(state='failed')=>({
  device_id:'phone-1',state:'failed',
  steps:[{step_id:'a',state},{step_id:'b',state:'skipped'}],
});
const request=()=>createAutomationRepairRequest(history(),device(),device().steps[0],17,'request-1');
const saved=(revision=3,body=steps)=>({
  id:'script-a',revision,name:'Checkout',description:'saved description',steps:body,
});

test('failure request refers to exact historical step and phone',()=>{
  const r=request();
  assert.ok(r);
  assert.equal(r.runId,'run-1');
  assert.equal(r.stepId,'a');
  assert.equal(r.ownerId,17);
  assert.equal(r.scriptId,'script-a');
  assert.equal(r.revision,3);
  assert.notStrictEqual(r.snapshotSteps,steps);
  r.snapshotSteps[0].selector.resource_id='rewritten';
  assert.equal(steps[0].selector.resource_id,'demo:id/submit');
});

test('same saved revision and identical script remains editable in place',()=>{
  const savedScript=saved();
  const p=planAutomationRepair(request(),[savedScript]);
  assert.equal(p.kind,'saved');
  assert.strictEqual(p.script,savedScript);
  assert.equal(p.stepId,'a');
  assert.equal(p.sourceRunId,'run-1');
});

test('different revision, deleted script and inline trial all make a new draft',()=>{
  const newer=planAutomationRepair(request(),[saved(4)]);
  assert.equal(newer.kind,'copy');
  assert.equal(newer.reason,'updated');
  assert.equal(newer.sourceRevision,3);
  assert.equal(newer.stepId,'a');
  assert.notStrictEqual(newer.steps,steps);
  assert.equal(planAutomationRepair(request(),[]).reason,'deleted');

  const t=createAutomationRepairRequest(history({name:'Inline trial',steps}),
    device(),device().steps[0],17,'request-2');
  const inline=planAutomationRepair(t,[]);
  assert.equal(inline.kind,'copy');
  assert.equal(inline.reason,'inline');
});

test('same revision but script bytes differ never overwrites saved script',()=>{
  const mutated=saved(3,[{...steps[0],selector:{text:'Other'}},steps[1]]);
  assert.equal(planAutomationRepair(request(),[mutated]).kind,'copy');
});

test('queued, skipped, success and unknown step IDs are not offered repair',()=>{
  for(const state of ['queued','running','skipped','action_sent','assertion_passed']) {
    const d=device(state);
    assert.equal(createAutomationRepairRequest(history(),d,d.steps[0],17,'request'),null);
  }
  assert.equal(createAutomationRepairRequest(history(),device(),{step_id:'nonexistent',state:'failed'},17,'request'),null);
  assert.equal(createAutomationRepairRequest(history(),device(),device().steps[0],0,'request'),null);
  assert.equal(createAutomationRepairRequest(history(),device(),{step_id:'a',state:'failed'},17,''),null);
});

test('outcome unknown is repairable but preserves that explicit warning state',()=>{
  const d=device('outcome_unknown');
  const r=createAutomationRepairRequest(history(),d,d.steps[0],17,'request');
  assert.equal(r.resultState,'outcome_unknown');
  assert.equal(planAutomationRepair(r,[saved()]).kind,'saved');
});

test('conditional, selector and scroll properties are cloned without touching history',()=>{
  const s=cloneRepairSteps([
    {id:'cond',type:'if_else',selector:{text:'Dialog'},then_count:1,else_count:1},
    {id:'find',type:'scroll_find',selector:{text:'Submit'},scroll_container:{resource_id:'demo:id/list'}},
    {id:'back',type:'key',key:'BACK'},
  ]);
  s[0].selector.text='Wrong';
  s[1].scroll_container.resource_id='other';
  assert.equal(s[0].then_count,1);
  assert.equal(s[1].type,'scroll_find');
});

test('malformed duplicate step identifiers fail closed',()=>{
  const r={...request(),stepId:'a',snapshotSteps:[steps[0],steps[0]]};
  assert.equal(planAutomationRepair(r,[saved()]),null);
});
