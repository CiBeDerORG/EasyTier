import assert from 'node:assert/strict';
import test from 'node:test';
import { analyzeAutomationFlow, canInsertBranchStep, insertBranchStep } from '../src/modules/automation-flow.ts';

let counter = 0;
const key = () => ({ id: String(++counter), type: 'key', key: 'HOME' });
const ifElse = (then_count, else_count) => ({
  id: String(++counter), type: 'if_else', then_count, else_count, selector: { text: 'Dialog' },
});
const ifGone = skip_count => ({
  id: String(++counter), type: 'if_gone', skip_count, selector: { text: 'Dialog' },
});

test('nested Then/Else stays within three levels and exposes exact ancestry', () => {
  const script = [ifElse(5, 1), ifGone(4), ifElse(1, 2), key(), key(), key(), key()];
  const model = analyzeAutomationFlow(script);
  assert.equal(model.problem, undefined);
  assert.deepEqual(model.entries[3].ancestors.map(a => [a.conditionIndex, a.lane]),
    [[0, 'then'], [1, 'then'], [2, 'then']]);
  assert.deepEqual(model.entries[5].ancestors.map(a => [a.conditionIndex, a.lane]),
    [[0, 'then'], [1, 'then'], [2, 'else']]);
  assert.deepEqual(model.entries[6].ancestors.map(a => [a.conditionIndex, a.lane]),
    [[0, 'else']]);
});

test('crossed condition and fourth level are rejected', () => {
  const crossed = [ifElse(2, 1), key(), ifGone(1), key()];
  assert.equal(analyzeAutomationFlow(crossed).problem?.reason, 'crossing');
  const tooDeep = [ifGone(5), ifGone(4), ifGone(3), ifGone(2), key(), key()];
  assert.equal(analyzeAutomationFlow(tooDeep).problem?.reason, 'too_deep');
});

test('inserting in an inner Else expands only that lane and all enclosing Then scopes', () => {
  const script = [ifElse(5, 1), ifGone(4), ifElse(1, 2), key(), key(), key(), key()];
  const layout = analyzeAutomationFlow(script);
  assert.equal(canInsertBranchStep(script, layout, 2, 'else'), true);
  const newStep = key();
  const result = insertBranchStep(script, layout, 2, 'else', newStep);
  assert.ok(result);
  assert.equal(result[0].then_count, 6);
  assert.equal(result[1].skip_count, 5);
  assert.equal(result[2].then_count, 1);
  assert.equal(result[2].else_count, 3);
  assert.equal(result[6].id, newStep.id);
  assert.equal(analyzeAutomationFlow(result).problem, undefined);
  assert.equal(script[0].then_count, 5, 'editor insertion must not mutate the saved draft');
});

test('branch insertion fails closed on invalid or full branches', () => {
  const full = [ifGone(20), ...Array.from({ length: 20 }, key)];
  const layout = analyzeAutomationFlow(full);
  assert.equal(layout.problem, undefined);
  assert.equal(canInsertBranchStep(full, layout, 0, 'then'), false);
  assert.equal(insertBranchStep(full, layout, 0, 'then', key()), null);
  assert.equal(canInsertBranchStep([key()], analyzeAutomationFlow([key()]), 0, 'then'), false);
});
