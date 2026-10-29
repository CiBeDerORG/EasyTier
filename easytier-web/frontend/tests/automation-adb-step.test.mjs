import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { projectCardSteps } from '../src/modules/card-step-projection.ts';
import { builtinAdbCommands } from '../src/modules/automation-adb-presets.ts';

const read = path => readFileSync(new URL('../src/' + path, import.meta.url), 'utf8');
const draft = steps => ({
  ownerId: 1, name: 'diagnostic', description: '', steps,
  editingId: null, revision: null, baseline: '',
});
test('step title replaces action label while keeping action context', () => {
  const titled = projectCardSteps(draft([
    {id: 'a', type:'key', title:'启动首页', key:'HOME'},
    {id: 'b', type:'adb_command', title:'检查分辨率', command:'adb shell wm size', timeout_ms:8000},
  ]));
  assert.equal(titled.steps[0].label, '启动首页');
  assert.equal(titled.steps[1].label, '检查分辨率');
  assert.match(titled.steps[1].context, /ADB Shell/);
  assert.equal(titled.steps[1].incomplete, false);
  const defaults = projectCardSteps(draft([
    {id:'a', type:'key', key:'HOME'},
    {id:'b', type:'adb_command'},
  ]));
  assert.equal(defaults.steps[0].label,'导航按键');
  assert.equal(defaults.steps[1].label,'ADB 命令');
  assert.equal(defaults.steps[1].incomplete,true);
});
test('presets are unique, device-scoped and never use host shell', () => {
  assert.ok(builtinAdbCommands.length >= 8);
  assert.equal(new Set(builtinAdbCommands.map(item => item.id)).size, builtinAdbCommands.length);
  assert.equal(new Set(builtinAdbCommands.map(item => item.command)).size, builtinAdbCommands.length);
  assert.ok(builtinAdbCommands.every(item => item.command.startsWith('adb shell ')));
});
test('step editor lets the owner name actions, manually enter shell and manage account favorites', () => {
  const ui = read('components/automation/ScriptStepEditor.vue');
  assert.match(ui,/步骤标题/);
  assert.match(ui,/modelValue\.title/);
  assert.match(ui,/modelValue\.type === 'adb_command'/);
  assert.match(ui,/手动输入 ADB 命令/);
  assert.match(ui,/saveAdbFavorite\(owner, name, command\)/);
  assert.match(ui,/deleteAdbFavorite\(owner, chosen\.id\)/);
  const card = read('components/automation/PhoneCard.vue');
  assert.match(card,/:step="editingStep" :step-number="editingStepNumber" :owner-id="ownerId"/);
  const timeline = read('components/automation/CardStepTimeline.vue');
  assert.match(timeline,/value:'adb_command'/);
});
