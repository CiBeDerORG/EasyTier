/** T46/T47 static regression: legacy scripts tab is only a read-only
 * compatibility view and may never become a second owner of script actions.
 * Separate browser and owner/CAS tests exercise actual runtime behavior.
 */
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
const old=readFileSync(new URL('../src/components/automation/ScriptLibrary.vue',import.meta.url),'utf8');
const center=readFileSync(new URL('../src/components/AutomationCenter.vue',import.meta.url),'utf8');

test('T46 old tab does not contain an editable script or secondary stream',()=>{
  for(const pattern of [
    /<ScriptStepEditor\b/,
    /<video\b/,
    /new RTCPeerConnection\(/,
    /api\.saveScript\(/,
    /api\.deleteScript(?:Versioned)?\(/,
    /api\.createRun\(/,
    /<Dialog\b/,
    /@click="save"/,
  ]) assert.equal(pattern.test(old),false,'retired write/media UI must not return: '+pattern);
});

test('T46 compatibility keeps authoritative server list and old uncertain receipt recovery',()=>{
  for(const pattern of [
    /api\.listScripts\(/,
    /readTrialReceipt\(/,
    /findTrialRun\(/,
    /clearTrialReceipt\(/,
    /emit\('to-card'/,
    /emit\('run-created'/,
  ]) assert.ok(pattern.test(old),'safety-compatible read-only route must remain: '+pattern);
});

test('T47 compatibility targets original PhoneCard and canonical owner draft only',()=>{
  assert.match(center,/async function openLegacyScriptOnCard\(deviceId:string,scriptId\?:string,revision\?:number\)/);
  assert.match(center,/card\.openAutomationPanel\(\)/);
  assert.match(center,/scriptWorkspace\.perform\(\{kind:'select',deviceId,scriptId,revision\}\)/);
  assert.doesNotMatch(center,/@pick-step-target="openScriptInspector"/);
  assert.doesNotMatch(center,/function openScriptInspector\(/);
  assert.match(center,/@run-created="onRunCreated"/);
});
