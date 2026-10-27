/** P0 regression: no user text/selector/body in persisted recovery metadata. */
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  reserveTrialReceipt,readTrialReceipt,clearTrialReceipt,findTrialRun,
} from '../src/modules/automation-trial-receipt.ts';

const ownerId=42,deviceId='phone-a',stepId='step-one';
const first='6c8d4c9c-5296-4d8c-8451-aabffad66001';
const second='6c8d4c9c-5296-4d8c-8451-aabffad66002';
const proof=(requestId=first)=>({ownerId,deviceId,stepId,requestId,createdAt:'2026-10-27T13:00:00Z'});
const store=()=>{
  const items=new Map();
  return {
    getItem:key=>items.get(key)??null,
    setItem:(key,value)=>items.set(key,value),
    removeItem:key=>items.delete(key),
    entries:()=>[...items.entries()],
  };
};

test('P0 in-flight receipt survives hide/reopen and cannot be overwritten by new UUID',()=>{
  const storage=store();
  reserveTrialReceipt(storage,proof());
  assert.deepEqual(readTrialReceipt(storage,ownerId,deviceId),proof());
  // A component teardown, route change and new component instance use the same storage.
  assert.throws(()=>reserveTrialReceipt(storage,proof(second)),/未确认/);
  assert.deepEqual(readTrialReceipt(storage,ownerId,deviceId),proof());
  // Only the exact original ID can be cleared.
  clearTrialReceipt(storage,proof(second));
  assert.equal(readTrialReceipt(storage,ownerId,deviceId).requestId,first);
  clearTrialReceipt(storage,proof());
  assert.equal(readTrialReceipt(storage,ownerId,deviceId),null);
});

test('P0 never persists the run payload, parameters or selector text',()=>{
  const storage=store();
  reserveTrialReceipt(storage,proof());
  const written=JSON.stringify(storage.entries());
  assert.ok(written.includes(first));
  for(const forbidden of ['parameters','secret password','steps','resource_id','selector','text']){
    assert.equal(written.includes(forbidden),false,forbidden+' was written to storage');
  }
});

test('P0 stale account/other device cannot steal or clear a trial receipt',()=>{
  const storage=store();
  reserveTrialReceipt(storage,proof());
  assert.equal(readTrialReceipt(storage,99,deviceId),null);
  assert.equal(readTrialReceipt(storage,ownerId,'phone-b'),null);
  clearTrialReceipt(storage,{...proof(),ownerId:99});
  assert.equal(readTrialReceipt(storage,ownerId,deviceId).requestId,first);
});

test('P0 corrupt storage and failed writes fail CLOSED, never overwrite a previous trial',()=>{
  const storage=store();
  reserveTrialReceipt(storage,proof());
  const [[key]]=storage.entries();
  storage.setItem(key,'not JSON');
  assert.throws(()=>readTrialReceipt(storage,ownerId,deviceId),/损坏/);
  assert.throws(()=>reserveTrialReceipt(storage,proof(second)),/损坏/);
  assert.throws(()=>reserveTrialReceipt({
    getItem:()=>null,setItem:()=>{throw new Error('Storage disabled')},removeItem:()=>{},
  },proof()),/Storage disabled/);
});

test('P0 only an exact server run request_id AND same device can resolve a lost POST',()=>{
  const otherDevice={request_id:first,devices:[{device_id:'phone-b'}]};
  const otherRequest={request_id:second,devices:[{device_id:deviceId}]};
  const correct={id:'run-verified',request_id:first,devices:[{device_id:deviceId}]};
  assert.equal(findTrialRun([otherDevice,otherRequest],proof()),null);
  assert.equal(findTrialRun([otherDevice,otherRequest,correct],proof()),correct);
  assert.equal(findTrialRun([{id:'legacy',devices:[{device_id:deviceId}]}],proof()),null);
});
