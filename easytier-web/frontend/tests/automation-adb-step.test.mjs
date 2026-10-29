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


test('built-in ADB presets can be hidden per account and restored', () => {
  const ui = read('components/automation/ScriptStepEditor.vue');
  const api = read('modules/automation-api.ts');
  assert.ok(ui.includes('builtinAdbCommands.filter(item => !hiddenBuiltinIds.value.includes(item.id)'));
  assert.ok(ui.includes('删除选中的内置命令'));
  assert.ok(ui.includes('恢复已删除内置命令'));
  assert.ok(ui.includes('当前步骤命令仍保留'));
  assert.ok(api.includes('hiddenAdbPresets(ownerId: number)'));
  assert.ok(api.includes('hideAdbPreset(ownerId: number, id: string)'));
  assert.ok(api.includes('restoreAdbPreset(ownerId: number, id: string)'));
});

test('ADB single-step play does not depend on editor, UI helper or preview session', () => {
  const card = read('components/automation/PhoneCard.vue');
  const panel = read('components/automation/CardTrialPanel.vue');
  const preflight = read('modules/automation-preflight.ts');
  assert.doesNotMatch(card, /!!props\.editingStepId\s*\|\|\s*!props\.device\.target_fingerprint/);
  assert.match(panel, /if\(chosen\.type==='adb_command'\)/);
  assert.match(panel, /if\(frozen\.value\.type==='adb_command'\)/);
  assert.match(panel, /await sendFrozen\(mine,owner,id,original,payload\)/);
  assert.match(panel, /phase==='failed'/);
  assert.match(preflight, /'adb_command'/);
});
