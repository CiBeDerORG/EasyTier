import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const card=readFileSync(new URL('../src/components/automation/PhoneCard.vue',import.meta.url),'utf8');

test('phone card header uses explicit connect icon, not a duplicate picker icon',()=>{
  const header=card.slice(card.indexOf('<header>'),card.indexOf('</header>'));
  assert.ok(header.includes('class="card-icon connect-icon"'));
  assert.ok(header.includes("emit('connect', device.id)"));
  assert.ok(header.includes('pi pi-link'));
  assert.ok(!header.includes('inspector-icon'));
  assert.ok(!header.includes("emit('toggle-picking', device.id)"));
});
test('ADB connection status dot remains informational without duplicate click handler',()=>{
  const header=card.slice(card.indexOf('<header>'),card.indexOf('</header>'));
  assert.ok(header.includes('<span class="adb-status"'));
  assert.ok(header.includes('adb-status-dot'));
  assert.equal((header.match(/emit\('connect', device\.id\)/g)||[]).length,1);
});
test('gateway name is beside phone name and card footer renders actual ADB port',()=>{
  const header=card.slice(card.indexOf('<header>'),card.indexOf('</header>'));
  assert.ok(header.indexOf('class="gateway-name"')>header.indexOf('class="device-name"'));
  assert.ok(header.includes('device.gateway_name'));
  assert.ok(card.includes('device.connection.adb_port > 0'));
  assert.ok(card.includes(':{{ device.connection.adb_port }}'));
  assert.ok(!card.includes('<div class="device-meta">'));
});
test('contextual right-click picking remains available on live phone video',()=>{
  assert.ok(card.includes('@contextmenu="contextPick"'));
  assert.ok(card.includes('automation-panel-open'));
  assert.ok(card.includes('CardPickingOverlay'));
});
