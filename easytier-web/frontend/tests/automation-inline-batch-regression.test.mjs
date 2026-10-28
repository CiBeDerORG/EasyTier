/**
 * FINAL V3 / T48: pure contract checks for frozen inline steps and batch
 * request-identity receipts. Writing a test file is NOT permission to run it.
 *
 * Does not open a browser, WebRTC, ADB, or send any automation API request.
 */
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  readBatchReceipt, reserveBatchReceipt, clearBatchReceipt,
} from '../src/modules/automation-batch-receipt.ts';
import {
  freezeInlineStepRunIntent, inlineStepIntentStillCurrent,
  inlineStepMayAutoSubmit,
} from '../src/modules/useInlineStepRun.ts';

const owner=23;
const id='123e4567-e89b-42d3-a456-426614174000';
function memoryStorage() {
  const records=new Map();
  return {
    records,
    getItem(key){return records.has(key)?records.get(key):null;},
    setItem(key,value){records.set(key,String(value));},
    removeItem(key){records.delete(key);},
  };
}
const receipt=()=>({
  ownerId:owner, requestId:id, deviceIds:['phone-a','phone-b'],
  createdAt:'2026-10-28T00:00:00.000Z',
});

test('T48 batch receipt fences one UUID and never clears another batch identity',()=>{
  const storage=memoryStorage();
  const first=receipt();
  assert.equal(readBatchReceipt(storage,owner),null);
  reserveBatchReceipt(storage,first);
  assert.deepEqual(readBatchReceipt(storage,owner),first);
  assert.equal(readBatchReceipt(storage,owner+1),null,'owner must be isolated');
  assert.throws(()=>reserveBatchReceipt(storage,{...first,requestId:'123e4567-e89b-42d3-a456-426614174001'}),/未核对/);
  assert.throws(()=>clearBatchReceipt(storage,{...first,requestId:'123e4567-e89b-42d3-a456-426614174001'}),/身份已变化/);
  assert.deepEqual(readBatchReceipt(storage,owner),first,'different receipt must remain');
  clearBatchReceipt(storage,first);
  assert.equal(readBatchReceipt(storage,owner),null);
});

test('T48 invalid owner, missing target and damaged persisted JSON fail closed',()=>{
  const storage=memoryStorage();
  const key='easytier.automation.batch.pending.v1.'+owner;
  storage.setItem(key,'{"ownerId":23,"requestId":"broken"}');
  assert.throws(()=>readBatchReceipt(storage,owner),/不完整/);
  assert.throws(()=>reserveBatchReceipt(storage,receipt()),/不完整/);
  storage.setItem(key,'not valid json');
  assert.throws(()=>readBatchReceipt(storage,owner));
  storage.setItem(key,JSON.stringify({...receipt(),ownerId:owner+1}));
  assert.throws(()=>readBatchReceipt(storage,owner),/不完整/);
  storage.setItem(key,JSON.stringify({...receipt(),deviceIds:[]}));
  assert.throws(()=>readBatchReceipt(storage,owner),/不完整/);
  assert.equal(storage.records.has(key),true,'invalid proof is never silently discarded');
});

test('T48 batch storage persists only request metadata; no steps or private inputs',()=>{
  const storage=memoryStorage();
  reserveBatchReceipt(storage,receipt());
  const raw=[...storage.records.values()].join('');
  for(const disallowed of ['password','selector','steps','parameters','secret value']){
    assert.equal(raw.includes(disallowed),false,disallowed+' must not be persisted');
  }
  assert.equal(raw.includes(id),true);
  assert.equal(raw.includes('phone-a'),true);
});

test('T48 inline ▶ freezes the original step/request and rejects stale edits and phones',()=>{
  const source={id:'step-1',type:'tap',selector:{resource_id:'demo:id/start'}};
  const intent=freezeInlineStepRunIntent(owner,'phone-a','fingerprint-a',source,id);
  assert.ok(intent);
  assert.equal(Object.isFrozen(intent),true);
  assert.equal(intent.requestId,id);
  assert.equal(inlineStepIntentStillCurrent(intent,owner,'phone-a','fingerprint-a',source),true);
  assert.equal(inlineStepIntentStillCurrent(intent,owner,'phone-b','fingerprint-a',source),false);
  assert.equal(inlineStepIntentStillCurrent(intent,owner,'phone-a','rebound',source),false);
  assert.equal(inlineStepIntentStillCurrent(intent,owner+1,'phone-a','fingerprint-a',source),false);
  source.selector.resource_id='demo:id/changed';
  assert.equal(intent.step.selector.resource_id,'demo:id/start','frozen intent keeps original deep snapshot');
  assert.equal(inlineStepIntentStillCurrent(intent,owner,'phone-a','fingerprint-a',source),false);
  assert.equal(freezeInlineStepRunIntent(owner,'phone-a','fingerprint-a',null,id),null);
});

test('T48 no automatic POST for actions, parameters or unconfirmed matching',()=>{
  assert.equal(inlineStepMayAutoSubmit('read_only',{status:'compatible'},undefined),true);
  assert.equal(inlineStepMayAutoSubmit('read_only',{status:'compatible'},'password'),false);
  assert.equal(inlineStepMayAutoSubmit('read_only',{status:'blocked'},undefined),false);
  assert.equal(inlineStepMayAutoSubmit('read_only',null,undefined),false);
  assert.equal(inlineStepMayAutoSubmit('needs_consent',{status:'compatible'},undefined),false);
  assert.equal(inlineStepMayAutoSubmit('mutating',{status:'compatible'},undefined),false);
});
