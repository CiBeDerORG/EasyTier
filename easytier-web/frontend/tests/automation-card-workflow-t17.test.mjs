/**
 * T17 · AC-01..08/12 module-level workflow contract (simulated video/tree).
 * This is not a browser pointer, WebRTC, backend, or real Android E2E test.
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  beginPicker, readyPicker, pickerCanApply, pickerMatches,
  setPickerIntent, invalidatePickerSnapshot, stopPicker,
} from '../src/modules/automation-picker-state.ts';
import { matchPickerPoint } from '../src/modules/automation-pick-engine.ts';
import {
  emptyAutomationDraft, appendPickedAutomationStep,
} from '../src/modules/automation-draft.ts';
import { projectCardSteps } from '../src/modules/card-step-projection.ts';
import { updateCardDraftStep } from '../src/modules/automation-card-step-edit.ts';
import { replaceAutomationStepTarget } from '../src/modules/automation-step-target.ts';
import { locateReadOnlyStep } from '../src/modules/automation-locator-demo.ts';

const owner=23, phoneId='phone-a', fingerprint='bound-a', now=Date.parse('2026-10-27T10:00:00Z');
const firstId='00000000-0000-4000-8000-000000000001';
const secondId='00000000-0000-4000-8000-000000000002';
const binding=(deviceId=phoneId,sessionId='session-1')=>({
  ownerId:owner,deviceId,targetFingerprint:fingerprint,sessionId,
  draftOwnerId:owner,draftScriptId:null,draftRevision:null,
});
const node=(id,values={})=>({
  id,parent_id:null,depth:1,class_name:'android.widget.Button',
  package:'demo.app',resource_id:'',text:'',description:'',clickable:true,
  enabled:true,scrollable:false,selected:false,checked:false,password:false,
  bounds:{left:100,top:200,right:900,bottom:650},...values,
});
const tree=(nodes=[node(1,{resource_id:'demo:id/confirm'})],extras={})=>({
  device_id:phoneId,target_fingerprint:fingerprint,captured_at:new Date(now).toISOString(),
  snapshot:{width:1080,height:2400,truncated:false,nodes},...extras,
});
const point=(x=275,y=225)=>({x,y,width:540,height:1200});

test('AC01/02 picker has explicit ready/off states and rejects stale session/geometry',()=>{
  const first=beginPicker(binding(),10);
  assert.equal(first.phase,'preparing');
  assert.equal(pickerCanApply(first,binding(),10,0,{kind:'append'}),false);
  const on=readyPicker(first,10);
  assert.equal(on.phase,'on');
  assert.equal(pickerCanApply(on,binding(),10,0,{kind:'append'}),true);
  assert.equal(pickerMatches(on,binding(phoneId,'another-session')),false);
  assert.equal(pickerCanApply(on,binding(phoneId,'another-session'),10,0,{kind:'append'}),false);
  assert.equal(pickerCanApply(on,binding('phone-b'),10,0,{kind:'append'}),false);
  const rotated=invalidatePickerSnapshot(on);
  assert.equal(pickerCanApply(rotated,binding(),10,0,{kind:'append'}),false);
  const stopped=stopPicker(rotated,11);
  assert.equal(stopped.phase,'off');
  assert.equal(pickerCanApply(stopped,binding(),11,2,{kind:'append'}),false);
  assert.equal(readyPicker(first,9),first,'late response cannot resurrect a picker');
});

test('AC02/03 real video pixel -> read-only hit -> unique selector, reject black bars/stale tree',()=>{
  const selected=matchPickerPoint(tree(),point(),{targetFingerprint:fingerprint,nowMs:now});
  assert.equal(selected.status,'matched');
  assert.equal(selected.nodeId,1);
  assert.ok(selected.locatorCandidates.some(x=>x.unique && x.selector.resource_id==='demo:id/confirm'));
  assert.equal(matchPickerPoint(tree(),point(-1,25),{targetFingerprint:fingerprint,nowMs:now}).status,'outside');
  assert.equal(matchPickerPoint(tree(),{x:275,y:225,width:1200,height:540},
    {targetFingerprint:fingerprint,nowMs:now}).status,'stale');
  assert.equal(matchPickerPoint(tree(),point(),{targetFingerprint:'rebound',nowMs:now}).status,'stale');
  assert.equal(matchPickerPoint(tree(),point(),{targetFingerprint:fingerprint,nowMs:now+30000}).status,'stale');
  assert.equal(matchPickerPoint(tree([node(1),node(1)]),point(),
    {targetFingerprint:fingerprint,nowMs:now}).status,'invalid');
  assert.equal(matchPickerPoint(tree([node(1)],{snapshot:{width:1080,height:2400,truncated:true,nodes:[node(1)]}}),
    point(),{targetFingerprint:fingerprint,nowMs:now}).status,'truncated');
});

test('AC04/05/06 inspector confirmations append one step only, keep session/picker on',()=>{
  let n=0;
  const newId=()=>String(++n);
  const draft=emptyAutomationDraft(owner);
  const current=readyPicker(beginPicker(binding(),3),3);
  const action={id:firstId,type:'tap',selector:{resource_id:'demo:id/confirm'}};
  assert.equal(pickerCanApply(current,binding(),3,0,{kind:'append'}),true);
  assert.equal(appendPickedAutomationStep(draft,owner,action,newId),true);
  assert.equal(appendPickedAutomationStep(draft,owner,action,newId),false,'double confirmation must be idempotent');
  for(const [id,key] of [[secondId,'HOME'],['step-3','BACK'],['step-4','RECENT']]){
    assert.equal(appendPickedAutomationStep(draft,owner,{id,type:'key',key},newId),true);
  }
  assert.deepEqual(draft.steps.map(s=>s.id),[firstId,secondId,'step-3','step-4']);
  assert.equal(current.phase,'on','confirming a step must not exit the picker');
  assert.equal(draft.selectedStepId,'step-4');
  const projection=projectCardSteps(draft);
  assert.equal(projection.steps.length,4);
  assert.deepEqual(projection.steps.map(s=>s.number),[1,2,3,4]);
  assert.equal(projection.saveState,'new');
  assert.equal(appendPickedAutomationStep(draft,99,{id:'foreign',type:'key',key:'BACK'},newId),false);
});

test('AC07 replacement preserves step ID and unselected steps, rejects concurrent edit',()=>{
  const draft=emptyAutomationDraft(owner);
  const before={id:firstId,type:'tap',selector:{resource_id:'demo:id/old'},
    duration_ms:230,timeout_ms:5000};
  const next={id:secondId,type:'key',key:'BACK'};
  draft.steps=[structuredClone(before),structuredClone(next)];
  const picker=readyPicker(beginPicker(binding(),1),1);
  const replacing=setPickerIntent(picker,{kind:'replace',stepId:firstId,stepType:'tap'},2);
  assert.equal(pickerCanApply(replacing,binding(),2,1,{kind:'append'}),false);
  assert.equal(pickerCanApply(replacing,binding(),2,1,
    {kind:'replace',stepId:firstId,stepType:'tap'}),true);
  assert.equal(pickerCanApply(replacing,binding(),2,1,
    {kind:'replace',stepId:secondId,stepType:'tap'}),false);
  const target=replaceAutomationStepTarget(draft.steps,firstId,'tap',
    {selector:{resource_id:'demo:id/new'}});
  assert.ok(target);
  assert.equal(target[0].id,firstId);
  assert.equal(target[0].duration_ms,230);
  assert.deepEqual(target[1],next);
  assert.equal(updateCardDraftStep(draft,owner,firstId,before,target[0]),true);
  assert.equal(updateCardDraftStep(draft,owner,firstId,before,target[0]),false,'stale CAS cannot overwrite newer step');
  assert.equal(updateCardDraftStep(draft,77,firstId,target[0],before),false);
  assert.equal(draft.steps[0].selector.resource_id,'demo:id/new');
  assert.equal(draft.steps.length,2);
  assert.equal(draft.selectedStepId,firstId);
});

test('AC08 read-only locator demo has unique/missing/many/stale states; no control transport',()=>{
  const step={id:firstId,type:'assert',selector:{resource_id:'demo:id/check'}};
  const opts={deviceId:phoneId,targetFingerprint:fingerprint,
    videoWidth:540,videoHeight:1200,nowMs:now};
  const only=locateReadOnlyStep(tree([node(1,{resource_id:'demo:id/check'})]),step,opts);
  assert.equal(only.status,'one');
  assert.ok(only.bounds);
  const missing=locateReadOnlyStep(tree([]),step,opts);
  assert.equal(missing.status,'none');
  const duplicated=locateReadOnlyStep(tree([
    node(1,{resource_id:'demo:id/check'}),node(2,{resource_id:'demo:id/check'}),
  ]),step,opts);
  assert.equal(duplicated.status,'many');
  assert.equal(locateReadOnlyStep(tree([]),step,{...opts,targetFingerprint:'wrong'}).status,'stale');
  assert.equal(locateReadOnlyStep(tree([]),{...step,selector:{xpath:'//button'}},opts).status,'unsupported');
});

test('AC12 disconnect/switching viewed phone never deletes confirmed in-memory steps',()=>{
  const draft=emptyAutomationDraft(owner);
  const step={id:firstId,type:'key',key:'HOME'};
  assert.equal(appendPickedAutomationStep(draft,owner,step,()=>secondId),true);
  let picker=readyPicker(beginPicker(binding(),5),5);
  picker=stopPicker(picker,6); // WebRTC disconnected
  picker=readyPicker(beginPicker(binding('phone-b','session-b'),7),7);
  assert.equal(picker.phase,'on');
  assert.deepEqual(draft.steps,[step]);
  assert.equal(draft.ownerId,owner);
  assert.equal(projectCardSteps(draft).steps[0].id,firstId);
});
