import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const load=p=>readFileSync(new URL('../src/'+p,import.meta.url),'utf8');
test('forced device cleanup is reachable even while preview is blocked',()=>{
 const card=load('components/automation/PhoneCard.vue');
 // Verify the visible recovery action is independent of the preview being live.
 // The executor may have unfinished work even when the screen cannot open.
 const clearButton = card.match(/<Button v-if="executorTaskStatus\?\.has_tasks \|\| device\.run_occupied"[\s\S]*?@click="emit\('force-cleanup',device\.id\)" \/>/);
 assert.ok(clearButton, 'occupied phone must offer executor cleanup before preview opens');
 assert.match(clearButton[0], /:label="forceCleanupBusy \? '正在清除执行器任务' : t\('clearExecutorTasks'\)"/);
 assert.match(clearButton[0], /:disabled="!runnerAvailable \|\| forceCleanupBusy \|\| batchVideoProtected"/);
 assert.doesNotMatch(clearButton[0], /session\.state\.phase\s*===\s*'live'/);
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
