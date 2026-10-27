/**
 * T17 · AC09/10/11: NO real Android commands.
 * Exercises the exact run handoff and immutable repair modules with fake server
 * replies, including run-occupied and cleanup-unknown failure fences.
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { releaseOwnedTrialPreviews } from '../src/modules/automation-run-handoff.ts';
import { makeSingleStepRun } from '../src/modules/automation-single-step.ts';
import { initPreflightRow, probePreflightRow } from '../src/modules/automation-preflight.ts';
import {
  createAutomationRepairRequest, planAutomationRepair,
  cardRepairWorkspace, createCardRepairContext,
} from '../src/modules/automation-run-repair.ts';

const owner=21;
const phone=(id='a',extra={})=>({
  id,name:'Phone '+id,owner_user_id:owner,target_fingerprint:'fp-'+id,
  connection:{enabled:true,adb_port:38000,connection_mode:'wireless'},
  availability_reason:null,active_session:null,run_occupied:false,...extra,
});
const active=(id)=>({id,state:'live',actor_user_id:owner,expires_at:'2026-10-27'});
function fixture(initial,after,handleOptions={}) {
  let calls=0,closeCount=0,liveId=handleOptions.localId??'my-session';
  let phase=handleOptions.phase??'live';
  const owned=new Set(['a']);
  const api={async devices(){calls++;return {items:calls===1?initial:after};}};
  const handle={
    getSessionId:()=>liveId,
    getState:()=>({phase}),
    async close(){
      closeCount++;
      if(handleOptions.closeError)throw new Error('transport timeout');
      liveId=handleOptions.keepLocalId?'my-session':null;
      phase=handleOptions.afterPhase??'off';
    },
  };
  const options={ownerId:owner,requestedDeviceIds:['a'],ownedPreviewIds:owned,
    findPreview:()=>handle,onProgress:()=>{},stillCurrent:()=>true};
  return {api,options,owned,get closes(){return closeCount;}};
}

test('AC09 control handoff closes only locally-owned preview after authoritative check',async()=>{
  const set=fixture([phone('a',{active_session:active('my-session')})],[phone('a')]);
  await releaseOwnedTrialPreviews(set.api,set.options);
  assert.equal(set.closes,1);
  assert.equal(set.owned.size,0);
});

test('AC09 another browser, another owner or occupied run cannot be forcibly closed',async()=>{
  for(const initial of [
    phone('a',{active_session:active('someone-else')}),
    phone('a',{run_occupied:true}),
    phone('a',{owner_user_id:999}),
  ]){
    const set=fixture([initial],[phone('a')]);
    await assert.rejects(releaseOwnedTrialPreviews(set.api,set.options));
    assert.equal(set.closes,0);
    assert.ok(set.owned.has('a'));
  }
});

test('AC09 server cleanup pending, local cleanup pending, or close timeout prevents submission',async()=>{
  const remote=fixture([phone('a',{active_session:active('my-session')})],[
    phone('a',{active_session:active('my-session')}),
  ]);
  await assert.rejects(releaseOwnedTrialPreviews(remote.api,remote.options),/占用/);
  assert.equal(remote.closes,1);
  assert.equal(remote.owned.size,1);
  for(const overrides of [
    {keepLocalId:true},{afterPhase:'cleanup_pending'},{closeError:true},
  ]){
    const local=fixture([phone('a',{active_session:active('my-session')})],[phone('a')],overrides);
    await assert.rejects(releaseOwnedTrialPreviews(local.api,local.options));
    assert.equal(local.owned.size,1);
  }
});

test('AC09 stale account/session response never releases another phone',async()=>{
  let current=true;
  const f=fixture([phone('a',{active_session:active('my-session')})],[phone('a')]);
  const original=f.api.devices;
  f.api.devices=async ownerId=>{
    const data=await original(ownerId);current=false;return data;
  };
  f.options.stillCurrent=()=>current;
  await assert.rejects(releaseOwnedTrialPreviews(f.api,f.options),/变更/);
  assert.equal(f.closes,0);
});

test('AC09 preflight requires ADB and UI helper but never calls prepare to make a false ready',async()=>{
  const list=[];
  const api={
    async adbStatus(_owner,id){list.push('adb:'+id);return {device_id:id,state:'connected'};},
    async componentStatus(_owner,id){list.push('component:'+id);return {device_id:id,state:'not_prepared'};},
    async prepare(){throw Error('must be explicit');},
  };
  const result=await probePreflightRow(initPreflightRow(phone('a'),'a'),owner,true,api);
  assert.equal(result.state,'needs_preparation');
  assert.equal(result.selected,false);
  assert.deepEqual(list,['adb:a','component:a']);
});

test('AC09/10 side effects require confirmation and only one step/device is submitted',()=>{
  const action={id:'touch-1',type:'tap',selector:{resource_id:'demo:id/pay'}};
  assert.equal(makeSingleStepRun(action,'a','run-req-1',{},false),null);
  const frozen=makeSingleStepRun(action,'a','run-req-1',{},true);
  assert.equal(frozen.request_id,'run-req-1');
  assert.deepEqual(frozen.device_ids,['a']);
  assert.equal(frozen.concurrency,1);
  assert.deepEqual(frozen.steps.map(s=>s.id),['touch-1']);
  assert.equal(makeSingleStepRun({id:'if',type:'if_else'},'a','another',{},true),null);
});

const oldSteps=[
  {id:'tap',type:'tap',selector:{resource_id:'demo:id/old'}},
  {id:'verify',type:'assert',selector:{text:'Paid'},business_verification:true},
];
const historical=()=>({
  id:'run-old',name:'Checkout',state:'failed',
  script_snapshot:{name:'Checkout',script_id:'script-1',revision:3,steps:structuredClone(oldSteps)},
});
const failedDevice=()=>({
  device_id:'a',name:'Phone a',state:'failed',
  steps:[{step_id:'tap',state:'failed',message:'no locator'}],
});

test('AC10 failed run links exact step ID; updated published script must not be overwritten',()=>{
  const run=historical(),device=failedDevice();
  const request=createAutomationRepairRequest(run,device,device.steps[0],owner,'repair-1');
  assert.equal(request.runId,'run-old');
  assert.equal(request.stepId,'tap');
  const newer={id:'script-1',revision:4,name:'Checkout',description:'',
    steps:[...structuredClone(oldSteps),{id:'extra',type:'key',key:'BACK'}]};
  const plan=planAutomationRepair(request,[newer]);
  assert.equal(plan.kind,'copy');
  assert.equal(plan.reason,'updated');
  const ctx=createCardRepairContext(request,plan);
  assert.equal(ctx.sourceKind,'copy');
  assert.equal(ctx.runId,run.id);
  assert.equal(ctx.stepId,'tap');
  const workspace=cardRepairWorkspace(owner,plan);
  assert.equal(workspace.ownerId,owner);
  assert.equal(workspace.editingId,undefined,'fork must not inherit published CAS identity');
  workspace.steps[0].selector.resource_id='demo:id/repair';
  assert.equal(newer.steps[0].selector.resource_id,'demo:id/old');
  assert.equal(run.script_snapshot.steps[0].selector.resource_id,'demo:id/old');
  assert.equal(request.snapshotSteps[0].selector.resource_id,'demo:id/old');
});

test('AC10 same revision can edit current script; unknown action is flagged without automatic replay',()=>{
  const run=historical(),device=failedDevice();
  const request=createAutomationRepairRequest(run,device,device.steps[0],owner,'repair-2');
  const matching={id:'script-1',revision:3,name:'Checkout',description:'saved',steps:structuredClone(oldSteps)};
  const plan=planAutomationRepair(request,[matching]);
  assert.equal(plan.kind,'saved');
  const workspace=cardRepairWorkspace(owner,plan);
  assert.equal(workspace.editingId,'script-1');
  assert.equal(workspace.revision,3);
  assert.equal(workspace.selectedStepId,'tap');
  const unknown=failedDevice();
  unknown.steps[0].state='outcome_unknown';
  const u=createAutomationRepairRequest(run,unknown,unknown.steps[0],owner,'repair-unknown');
  assert.equal(u.resultState,'outcome_unknown');
  assert.equal(createCardRepairContext(u,plan).resultState,'outcome_unknown');
  const succeeded={...unknown,steps:[{step_id:'tap',state:'succeeded'}]};
  assert.equal(createAutomationRepairRequest(run,succeeded,succeeded.steps[0],owner,'x'),null);
});
