/**
 * AUT-08 integration: exercise the real, unmocked authoring/flow/locator/
 * preflight/outcome/repair modules end-to-end with simulated phone replies.
 * NOT a real Android, FnOS, browser UI, or server integration test.
 */
import assert from 'node:assert/strict';
import test from 'node:test';
import { beginCapturedGesture, observeCapturedGesture, finishCapturedGesture } from '../src/modules/automation-gesture.ts';
import { emptyAutomationDraft, appendAutomationDraftSteps } from '../src/modules/automation-draft.ts';
import { parseFlowTree, compileFlowTree, removeFlowBlock } from '../src/modules/automation-flow-tree.ts';
import { analyzeAutomationFlow } from '../src/modules/automation-flow.ts';
import { replaceAutomationStepTarget } from '../src/modules/automation-step-target.ts';
import { initPreflightRow, probePreflightDevices, selectedReadyIds, requiresAutomationHelper } from '../src/modules/automation-preflight.ts';
import { businessCheckStepId, businessRunState, businessDeviceState } from '../src/modules/automation-business-verification.ts';
import { createAutomationRepairRequest, planAutomationRepair } from '../src/modules/automation-run-repair.ts';

const ids = {
  launch: '00000000-0000-4000-8000-000000000001',
  condition: '00000000-0000-4000-8000-000000000002',
  yes: '00000000-0000-4000-8000-000000000003',
  no: '00000000-0000-4000-8000-000000000004',
  hold: '00000000-0000-4000-8000-000000000005',
  verify: '00000000-0000-4000-8000-000000000006',
};
const point=(x,y)=>({x,y,width:1080,height:2400});
const phone=(id)=>({
  id,name:'Phone '+id,owner_user_id:1,target_fingerprint:'fp-'+id,
  connection:{enabled:true,adb_port:37001,connection_mode:'wireless'},
  active_session:null,availability_reason:null,run_occupied:false,
});
const action=(step_id,state,extra={})=>({step_id,state,...extra});
function makeScript() {
  const draft=emptyAutomationDraft(1);
  const base=[
    {id:ids.launch,type:'launch_app',package:'com.example.demo'},
    {id:ids.condition,type:'if_else',selector:{text:'Confirm?'},then_count:1,else_count:1},
    {id:ids.yes,type:'tap',selector:{resource_id:'demo:id/yes'}},
    {id:ids.no,type:'tap',selector:{resource_id:'demo:id/no'}},
  ];
  assert.equal(appendAutomationDraftSteps(draft,1,base,()=>{throw Error('duplicate ID')}),true);
  const cap=beginCapturedGesture('phone-a',point(400,900),1000);
  assert.ok(cap);
  observeCapturedGesture(cap,point(404,903));
  const classified=finishCapturedGesture(cap,point(401,901),1900,ids.hold);
  assert.equal(classified.kind,'step');
  assert.equal(classified.step.type,'long_press');
  assert.equal(classified.step.duration_ms,900);
  assert.equal(appendAutomationDraftSteps(draft,1,[classified.step],()=>{throw Error('unexpected ID reuse')}),true);
  const assertion={id:ids.verify,type:'assert',selector:{text:'Completed'},timeout_ms:10000,business_verification:true};
  assert.equal(appendAutomationDraftSteps(draft,1,[assertion],()=>{throw Error('unexpected ID reuse')}),true);
  return draft;
}
function simulatedRun(script) {
  const snapshot=JSON.parse(JSON.stringify({script_id:'saved-a',name:'Demo checkout',
    revision:3,steps:script}));
  return {
    id:'run-01',name:snapshot.name,state:'partial_failed',script_snapshot:snapshot,
    devices:[
      {device_id:'phone-a',name:'Phone A',state:'completed',
        steps:[action(ids.launch,'action_sent'),action(ids.condition,'condition_passed',{branch_taken:'then'}),
          action(ids.yes,'action_sent'),action(ids.no,'skipped',{skipped_by_step_id:ids.condition}),
          action(ids.hold,'action_sent'),action(ids.verify,'assertion_passed')]},
      {device_id:'phone-b',name:'Phone B',state:'failed',
        steps:[action(ids.launch,'action_sent'),action(ids.condition,'condition_passed',{branch_taken:'else'}),
          action(ids.yes,'skipped',{skipped_by_step_id:ids.condition}),action(ids.no,'action_sent'),
          action(ids.hold,'action_sent'),
          action(ids.verify,'failed',{message:'Success indicator absent'})]},
    ],
  };
}

test('AUT-08 pipeline: record -> branch editor -> repick -> preflight -> two phones -> repair snapshot',async()=>{
  const draft=makeScript();
  const parsed=parseFlowTree(draft.steps);
  assert.ok(parsed,'script must be a valid nested editor tree');
  const roundTrip=compileFlowTree(parsed);
  assert.ok(roundTrip);
  assert.deepEqual(roundTrip.map(s=>s.id),draft.steps.map(s=>s.id));
  assert.equal(analyzeAutomationFlow(roundTrip).problem,undefined);
  assert.equal(businessCheckStepId(roundTrip),ids.verify);
  assert.equal(roundTrip.find(s=>s.id===ids.hold).duration_ms,900);

  // Replace a target in the Then lane; never change action, order or UUID.
  const repicked=replaceAutomationStepTarget(roundTrip,ids.yes,'tap',
    {selector:{resource_id:'demo:id/new-yes'}});
  assert.ok(repicked);
  assert.equal(repicked.length,roundTrip.length);
  assert.deepEqual(repicked.map(s=>s.id),roundTrip.map(s=>s.id));
  assert.equal(repicked[2].type,'tap');
  assert.equal(roundTrip[2].selector.resource_id,'demo:id/yes','original immutable');
  assert.equal(repicked[2].selector.resource_id,'demo:id/new-yes');

  const three=['phone-a','phone-b','phone-c'].map(id=>initPreflightRow(phone(id),id));
  const trace=[],updates=[],active=new Set();let max=0;
  const api={
    async adbStatus(_owner,id){
      active.add(id);max=Math.max(max,active.size);
      await new Promise(resolve=>setTimeout(resolve,1));active.delete(id);
      trace.push('adb:'+id);
      return {device_id:id,state:id==='phone-c'?'offline':'connected'};
    },
    async componentStatus(_owner,id){trace.push('ui:'+id);return {device_id:id,state:'ready',executor_id:'executor-A'};},
  };
  assert.equal(requiresAutomationHelper(repicked),true);
  await probePreflightDevices(three,1,true,api,row=>updates.push(row),()=>true,2);
  assert.ok(max<=2,'bounded parallel probe');
  assert.equal(updates.length,3);
  assert.deepEqual(selectedReadyIds(updates).sort(),['phone-a','phone-b']);
  assert.equal(updates.find(x=>x.id==='phone-c').state,'unavailable');
  assert.equal(trace.includes('ui:phone-c'),false,'offline phone must not bootstrap helper');

  // Simulated immutable run snapshot: one phone verifies business, one fails.
  const source=simulatedRun(repicked);
  assert.equal(businessDeviceState(source,source.devices[0]),'verified');
  assert.equal(businessDeviceState(source,source.devices[1]),'failed');
  assert.equal(businessRunState(source),'partial');

  const failure=source.devices[1].steps.find(s=>s.step_id===ids.verify);
  const request=createAutomationRepairRequest(source,source.devices[1],failure,1,'repair-uuid');
  assert.ok(request);
  assert.equal(request.stepId,ids.verify);
  assert.equal(request.revision,3);

  // User has since edited the saved script; fork from history, never mutate
  // the newer revision or the original execution evidence.
  const current={id:'saved-a',revision:4,name:'Demo checkout',description:'',
    steps:[...repicked,{id:'00000000-0000-4000-8000-000000000007',type:'key',key:'BACK'}]};
  const plan=planAutomationRepair(request,[current]);
  assert.equal(plan.kind,'copy');
  assert.equal(plan.reason,'updated');
  assert.equal(plan.stepId,ids.verify);
  const repaired=replaceAutomationStepTarget(plan.steps,ids.verify,'assert',
    {selector:{resource_id:'demo:id/success-result'}});
  assert.ok(repaired);
  assert.equal(businessCheckStepId(repaired),ids.verify);
  assert.equal(repaired.find(s=>s.id===ids.verify).selector.resource_id,'demo:id/success-result');
  assert.equal(current.revision,4);
  assert.equal(current.steps.find(s=>s.id===ids.verify).selector.text,'Completed');
  assert.equal(source.script_snapshot.steps.find(s=>s.id===ids.verify).selector.text,'Completed');
  assert.equal(source.devices[1].steps.at(-1).state,'failed');

  // Only a brand new run can prove the repair; never rewrite the previous run.
  const retry=simulatedRun(repaired);
  retry.id='run-02';
  retry.devices=retry.devices.map(d=>({
    ...d,state:'completed',steps:d.steps.map(s=>s.step_id===ids.verify
      ? action(ids.verify,'assertion_passed') : s),
  }));
  assert.equal(businessRunState(retry),'verified');
  assert.equal(businessRunState(source),'partial');
});

test('AUT-08 failures: unready phone, branch corruption, unknown action cannot become success',async()=>{
  const draft=makeScript();
  const failedRemoval=removeFlowBlock(draft.steps,ids.yes);
  assert.equal(failedRemoval,null,'the sole action of a lane cannot be deleted');
  const noBusiness={...simulatedRun(draft.steps),script_snapshot:{steps:draft.steps.map(step=>{
    const {business_verification: marker,...plain}=step;
    return plain;
  })}};
  assert.equal(businessRunState(noBusiness),'not_verified','legacy assertions do not prove business');
  const unknown=simulatedRun(draft.steps);
  unknown.devices[1].state='outcome_unknown';
  unknown.devices[1].steps[unknown.devices[1].steps.length-1].state='outcome_unknown';
  assert.equal(businessRunState(unknown),'outcome_unknown');
  const request=createAutomationRepairRequest(unknown,unknown.devices[1],
    unknown.devices[1].steps.at(-1),1,'unknown-1');
  assert.equal(request.resultState,'outcome_unknown');
  assert.equal(planAutomationRepair(request,[]).kind,'copy');

  const busy=initPreflightRow({...phone('phone-d'),run_occupied:true},'phone-d');
  assert.equal(busy.state,'unavailable');
  const selected=selectedReadyIds([busy]);
  assert.deepEqual(selected,[],'cannot submit a new run to an occupied device');
});
