/** T48: real floating-dialog ordering composable, with no mocked UI ordering.
 * Pure Vue reactivity only: no WebRTC, ADB, browser, fullscreen or mounting.
 * Browser keyboard/pointer/fullscreen behavior still needs separate E2E.
 */
import test from 'node:test';
import assert from 'node:assert/strict';
import { nextTick } from 'vue';
import { useAutomationFloatingDialogOrder } from '../src/modules/automation-floating-dialog-stack.ts';

test('T48 child dialogs register only on raise, and only top window owns Escape', async()=>{
  const first=useAutomationFloatingDialogOrder();
  const second=useAutomationFloatingDialogOrder();
  assert.equal(first.isTopmost(),false);
  assert.equal(second.isTopmost(),false);
  try {
    first.raise();
    await nextTick();
    assert.equal(first.isTopmost(),true);
    assert.equal(second.isTopmost(),false);
    const z1=first.zIndex.value;
    second.raise();
    await nextTick();
    assert.equal(first.isTopmost(),false);
    assert.equal(second.isTopmost(),true);
    assert.ok(second.zIndex.value>z1,'latest raised dialog must render above older');

    first.raise();
    await nextTick();
    assert.equal(first.isTopmost(),true,'pointer/focus raises the existing dialog');
    assert.equal(second.isTopmost(),false);
    assert.ok(first.zIndex.value>second.zIndex.value);
  }finally{
    first.unregister();
    second.unregister();
  }
});

test('T48 closing topmost dialog returns focus eligibility to surviving window',async()=>{
  const a=useAutomationFloatingDialogOrder();
  const b=useAutomationFloatingDialogOrder();
  try{
    a.raise();
    b.raise();
    b.unregister();
    await nextTick();
    assert.equal(a.isTopmost(),true);
    assert.equal(b.isTopmost(),false);
    const before=a.zIndex.value;
    a.raise();
    await nextTick();
    assert.ok(a.zIndex.value>before);
  }finally{
    a.unregister();b.unregister();
  }
  assert.equal(a.isTopmost(),false);
  assert.equal(b.isTopmost(),false);
});

test('T48 duplicate unregister is idempotent and never steals another dialog',()=>{
  const original=useAutomationFloatingDialogOrder();
  const other=useAutomationFloatingDialogOrder();
  try{
    original.raise();
    other.raise();
    original.unregister();
    original.unregister();
    assert.equal(other.isTopmost(),true);
    assert.equal(original.isTopmost(),false);
  }finally{
    original.unregister();other.unregister();
  }
});
