/** P1 regression: executor reports success in more than one step enum. */
import test from 'node:test';
import assert from 'node:assert/strict';
import { summarizeCardRun } from '../src/modules/automation-run-result.ts';
const run=(stepState,deviceState='completed',runState='completed')=>({
  id:'run-42',state:runState,devices:[{
    device_id:'phone-1',state:deviceState,steps:[{step_id:'step-1',state:stepState}],
  }],
});
test('P1 native Android action_sent is neither failure nor verified business outcome',()=>{
  const state=summarizeCardRun(run('action_sent'),'phone-1','step-1');
  assert.equal(state.phase,'action_sent');
  assert.match(state.label,/尚未验证/);
});
test('P1 UiAutomator assertion and condition success states are accepted',()=>{
  for(const state of ['assertion_passed','condition_passed','passed','action_completed','completed','succeeded']){
    assert.equal(summarizeCardRun(run(state),'phone-1','step-1').phase,'succeeded',state);
  }
});
test('P1 unknown / failed outcomes take precedence over action_sent',()=>{
  assert.equal(summarizeCardRun(run('action_sent','outcome_unknown','outcome_unknown'),
    'phone-1','step-1').phase,'unknown');
  assert.equal(summarizeCardRun(run('action_sent','failed','failed'),
    'phone-1','step-1').phase,'failed');
  assert.equal(summarizeCardRun(run('failed','failed','failed'),
    'phone-1','step-1').phase,'failed');
});
test('P1 action sent during active script is running rather than completed',()=>{
  assert.equal(summarizeCardRun(run('action_sent','running','running'),
    'phone-1','step-1').phase,'running');
});
test('P1 missing or unrelated device is never marked succeeded',()=>{
  assert.notEqual(summarizeCardRun(run('assertion_passed'),'phone-2','step-1').phase,'succeeded');
  assert.notEqual(summarizeCardRun(run('assertion_passed'),'phone-1','missing').phase,'succeeded');
});
