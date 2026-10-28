import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {analyzeAutomationFlow} from '../src/modules/automation-flow.ts';
import {emptyAutomationDraft} from '../src/modules/automation-draft.ts';
const read=p=>readFileSync(new URL('../src/'+p,import.meta.url),'utf8');
const timeline=read('components/automation/CardStepTimeline.vue');
const card=read('components/automation/PhoneCard.vue');
const center=read('components/AutomationCenter.vue');

test('OFFLINE-01 empty authorized script draft has a valid edit structure',()=>{
    const draft=emptyAutomationDraft(2);
    assert.equal(draft.steps.length,0);
    assert.equal(analyzeAutomationFlow(draft.steps).problem,undefined);
});
test('OFFLINE-02 action selector stays enabled even after force cleanup or unknown run',()=>{
    assert.match(timeline,/<Select\b[^>]*v-model="insertType"/);
    assert.match(timeline,/:append-to="overlayTarget \?\? 'self'"/);
    assert.doesNotMatch(timeline,/<select[^>]*:disabled=/);
    assert.match(timeline,/const blocked=computed\(\(\)=>!!layout\.value\.problem\)/);
    assert.doesNotMatch(card,/:structure-locked=/);
    assert.doesNotMatch(timeline,/props\.trialActive\|\|!!layout\.value\.problem/);
});
test('OFFLINE-03 insertion is a draft operation, not gated by ADB or running state',()=>{
    const body=center.slice(center.indexOf('function cardStructureAllowed('),
        center.indexOf('function createCardDraftStep('));
    assert.ok(body.length>0);
    for(const term of ['floatingBatch.busy','recordingId','sync.value','inspectorDeviceId',
        'scriptWorkspace.state.busy','cardEditorDirty.value ||']){
        assert.ok(!body.includes(term),'unexpected offline draft restriction: '+term);
    }
    assert.match(center,/function openAppendedStepEditor\(/);
    assert.match(center,/draftWorkspace\.steps=next;/);
    assert.match(center,/const pending=pendingNewDraftEditors\.value\[0\]/);
});
test('OFFLINE-04 offline editing never bypasses owner and flow safety',()=>{
    assert.match(center,/!canAccessAutomationCard\(ownerId\.value,deviceId,device,draftWorkspace\)/);
    assert.match(center,/analyzeAutomationFlow\(draftWorkspace\.steps\)\.problem/);
    assert.match(timeline,/draft\.steps\.length>=100/);
    assert.match(center,/if\(cardEditorDirty\.value && cardStepEditor\.value\)/);
});
