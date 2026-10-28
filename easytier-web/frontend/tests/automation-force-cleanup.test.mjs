import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const load=p=>readFileSync(new URL('../src/'+p,import.meta.url),'utf8');
test('forced device cleanup is reachable even while preview is blocked',()=>{
 const card=load('components/automation/PhoneCard.vue');
 assert.match(card,/aria-label="forceCleanupBusy \? '正在强制清理本机占用' : '强制清理本机全部自动化占用'"/);
 assert.match(card,/@click="emit\('force-cleanup',device\.id\)"/);
 const consolePanel=load('components/automation/FloatingBatchRunPanel.vue');
 assert.match(consolePanel,/强制清理本机占用/);
 assert.match(consolePanel,/emit\('forceCleanup'\)/);
});
test('cleanup only clears browser receipt after executor confirms actual cleanup',()=>{
 const root=load('components/AutomationCenter.vue');
 assert.match(root,/await api\.forceCleanup\(owner,id\)/);
 assert.match(root,/if\(result\.cleanup_confirmed\)/);
 assert.match(root,/floatingBatch\.acknowledgeDeviceCleanup\(owner,id\)/);
 const batch=load('modules/useFloatingBatchRun.ts');
 assert.match(batch,/function acknowledgeDeviceCleanup\(owner:number,deviceId:string\)/);
 assert.match(batch,/clearBatchReceipt/);
});
