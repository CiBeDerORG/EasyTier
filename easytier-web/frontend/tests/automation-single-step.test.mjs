import assert from 'node:assert/strict';
import test from 'node:test';
import {
  stepTrialSafety, isolateAutomationStep, makeSingleStepRun,
} from '../src/modules/automation-single-step.ts';

const id='11111111-1111-4111-8111-111111111111';
const key={id,type:'key',key:'HOME'};
const check={id,type:'assert',selector:{resource_id:'demo:id/done'},timeout_ms:1000,business_verification:true};

test('only presence/property assertions and waits are read-only',()=>{
  for (const type of ['wait_for','wait_gone','assert','assert_property'])
    assert.equal(stepTrialSafety({id,type}),'read_only',type);
  for (const type of ['key','tap','long_press','swipe','drag','input_text','launch_app','scroll_find','screenshot'])
    assert.equal(stepTrialSafety({id,type}),'confirm_required',type);
  assert.equal(stepTrialSafety({id,type:'if_else'}),'unsupported');
  assert.equal(stepTrialSafety({id,type:'if_gone'}),'unsupported');
});

test('single assertion strips only the business marker and preserves UUID and selector',()=>{
  const isolated=isolateAutomationStep(check);
  assert.equal(isolated.id,id);
  assert.equal(isolated.type,'assert');
  assert.equal(isolated.business_verification,undefined);
  assert.deepEqual(isolated.selector,check.selector);
  assert.notStrictEqual(isolated.selector,check.selector);
  isolated.selector.resource_id='changed';
  assert.equal(check.selector.resource_id,'demo:id/done');
  const body=makeSingleStepRun(check,'phone-a','request-a',{},false);
  assert.deepEqual(body.device_ids,['phone-a']);
  assert.equal(body.concurrency,1);
  assert.equal(body.steps.length,1);
  assert.equal(body.steps[0].business_verification,undefined);
  assert.equal(check.business_verification,true);
});

test('mutating actions require explicit confirmation, never implicit run',()=>{
  assert.equal(makeSingleStepRun(key,'phone-a','request-a',{},false),null);
  assert.equal(makeSingleStepRun(key,'phone-a','request-a',{},true).steps[0].key,'HOME');
  assert.equal(makeSingleStepRun(key,'phone-a','request-a',{},true).steps.length,1);
  assert.equal(makeSingleStepRun({id,type:'screenshot'},'phone-a','request-a',{},false),null);
});

test('standalone condition nodes are rejected even with user confirmation',()=>{
  for(const type of ['if_gone','if_else']){
    assert.equal(isolateAutomationStep({id,type}),null);
    assert.equal(makeSingleStepRun({id,type},'phone-a','request-a',{},true),null);
  }
});

test('sensitive input parameter name survives, but runtime value is not added to step',()=>{
  const original={id,type:'input_text',parameter:'login_code',
    selector:{resource_id:'demo:id/text'}};
  const params={login_code:'temporary-private-value'};
  assert.equal(makeSingleStepRun(original,'phone-a','request-a',params,false),null);
  const body=makeSingleStepRun(original,'phone-a','request-a',params,true);
  assert.equal(body.parameters.login_code,params.login_code);
  assert.equal(body.steps[0].parameter,'login_code');
  assert.equal(body.steps[0].text,undefined);
  assert.equal(body.steps[0].selector.resource_id,'demo:id/text');
  params.login_code='mutated';
  assert.equal(body.parameters.login_code,'temporary-private-value');
});

test('does not carry other script steps, branch ranges or live references',()=>{
  const source={...key,selector:{text:'X'},scroll_container:{resource_id:'demo:id/list'}};
  const body=makeSingleStepRun(source,'phone-1','request-1',{},true);
  source.selector.text='Y';source.scroll_container.resource_id='changed';
  assert.equal(body.steps[0].selector.text,'X');
  assert.equal(body.steps[0].scroll_container.resource_id,'demo:id/list');
  assert.equal(makeSingleStepRun(key,'','request-a',{},true),null);
  assert.equal(makeSingleStepRun(key,'phone-a','',{},true),null);
});
