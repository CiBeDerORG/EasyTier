import assert from 'node:assert/strict';
import test from 'node:test';
import { appendAutomationDraftSteps, emptyAutomationDraft } from '../src/modules/automation-draft.ts';

const step = id => ({id, type:'key', key:'BACK'});
test('inspector then recorder append into the very same editor workspace', () => {
  const draft = emptyAutomationDraft(24);
  const a = step('picker');
  const b = step('recorded');
  assert.equal(appendAutomationDraftSteps(draft,24,[a],()=> 'unused'),true);
  assert.equal(appendAutomationDraftSteps(draft,24,[b],()=> 'unused'),true);
  assert.deepEqual(draft.steps.map(v=>v.id),['picker','recorded']);
  assert.equal(draft.selectedStepId,'recorded');
  assert.equal(draft.name,'自动化草稿');
});
test('replayed step ID is regenerated and does not mutate the input', () => {
  const draft = emptyAutomationDraft(3), input = step('existing');
  assert.equal(appendAutomationDraftSteps(draft,3,[input],()=> 'unique'),true);
  assert.equal(appendAutomationDraftSteps(draft,3,[input],()=> 'unique'),true);
  assert.deepEqual(draft.steps.map(s=>s.id),['existing','unique']);
  assert.equal(input.id,'existing');
});
test('cross-account append and too-many steps are fail-closed', () => {
  const draft = emptyAutomationDraft(11);
  assert.equal(appendAutomationDraftSteps(draft,12,[step('foreign')],()=> 'new'),false);
  assert.deepEqual(draft.steps,[]);
  draft.steps = Array.from({length:100},(_,i)=>step('item'+i));
  assert.equal(appendAutomationDraftSteps(draft,11,[step('overflow')],()=> 'new'),false);
  assert.equal(draft.steps.length,100);
});
test('append is only valid outside existing branches and refuses a malformed draft', () => {
  const draft = emptyAutomationDraft(11);
  draft.steps = [{id:'cond',type:'if_else',then_count:1,else_count:1,selector:{text:'a'}},step('a'),step('b')];
  assert.equal(appendAutomationDraftSteps(draft,11,[step('after')],()=> 'new'),true);
  assert.equal(draft.steps.at(-1).id,'after');
  assert.equal(draft.steps[0].then_count,1);
  draft.steps[0].then_count=17;
  assert.equal(appendAutomationDraftSteps(draft,11,[step('invalid')],()=> 'new'),false);
});
