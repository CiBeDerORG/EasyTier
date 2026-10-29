/** P1 regression: executor reports success in more than one step enum. */
import test from 'node:test';
import assert from 'node:assert/strict';
import { summarizeCardRun, trialRunNeedsReconciliation } from '../src/modules/automation-run-result.ts';
import { readFileSync } from 'node:fs';
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

test('T40 action_sent is settled, so the next ▶ starts a fresh trial',()=>{
  const sent=summarizeCardRun(run('action_sent'),'phone-1','step-1');
  assert.equal(sent.phase,'action_sent');
  assert.equal(trialRunNeedsReconciliation(sent),false);
  for(const state of ['succeeded','failed','cancelled']){
    assert.equal(trialRunNeedsReconciliation({phase:state}),false,state);
  }
  for(const state of ['queued','running','checking','unknown']){
    assert.equal(trialRunNeedsReconciliation({phase:state}),true,state);
  }
  assert.equal(trialRunNeedsReconciliation(null),true,'missing run data must fail closed');
});

test('T40 panel resets completed run on reopen but resumes unresolved one read-only',()=>{
  const panel=readFileSync(new URL('../src/components/automation/CardTrialPanel.vue',import.meta.url),'utf8');
  const phone=readFileSync(new URL('../src/components/automation/PhoneCard.vue',import.meta.url),'utf8');
  assert.match(panel,/if\(result\.run\.value && trialRunNeedsReconciliation\(runView\.value\)\)/);
  assert.match(panel,/result\.resume\(\);/);
  assert.match(panel,/resetForOpen\(\);/);
  assert.match(phone,/trialProgress\.value=null; \/\/ Never show the previous run/);
});

// Inline trial remains exclusive while the device still says running, even
// after the individual action has been sent. A visible recovery entrypoint
// must be available instead of locking the entire timeline with no feedback.
test('T40 stalled one-step trial keeps buttons exclusive but exposes a recovery action',()=>{
  const timeline=readFileSync(new URL('../src/components/automation/CardStepTimeline.vue',import.meta.url),'utf8');
  const phone=readFileSync(new URL('../src/components/automation/PhoneCard.vue',import.meta.url),'utf8');
  const panel=readFileSync(new URL('../src/components/automation/CardTrialPanel.vue',import.meta.url),'utf8');
  const center=readFileSync(new URL('../src/components/AutomationCenter.vue',import.meta.url),'utf8');
  assert.equal(summarizeCardRun(run('action_sent','running','running'),'phone-1','step-1').phase,'running');
  assert.match(timeline,/v-if="trialActive" class="trial-busy-note"/);
  assert.match(timeline,/@click.stop="emit\('inspect-trial'\)"/);
  assert.match(phone,/@inspect-trial="trialInspectOpen = true"/);
  assert.match(phone,/:headless="!trialInspectOpen"/);
  assert.match(panel,/label="强制停止并清理本机任务"/);
  assert.match(panel,/@click="emit\('cleanup'\)"/);
  assert.match(phone,/@cleanup="emit\('force-cleanup',device.id\)"/);
  assert.match(center,/cards.get\(id\)\?\.resetTrialAfterCleanup\(\)/);
  assert.match(phone,/trialPanelKey.value; \/\/ discard prior run observer only after verified cleanup/);
});

test('T40 authoritative action_sent releases the card-level trial lock',()=>{
  const phone=readFileSync(new URL('../src/components/automation/PhoneCard.vue',import.meta.url),'utf8');
  assert.match(phone,/if\(value\.runId && \(value\.phase==='action_sent' \|\| value\.phase==='succeeded'\)\)/);
  assert.match(phone,/trialStepId\.value=null;\s*trialInspectOpen\.value=false;/);
  assert.match(phone,/if\(value\.phase==='unknown'/);
  assert.match(phone,/run_occupied \|\| device\.active_session/);
});
