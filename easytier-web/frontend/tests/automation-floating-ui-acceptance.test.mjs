import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const load=name=>readFileSync(new URL('../src/components/automation/'+name,import.meta.url),'utf8');
const step=load('ScriptStepEditor.vue');
const card=load('PhoneCard.vue');
const editor=load('CardStepEditor.vue');
const timeline=load('CardStepTimeline.vue');
const componentPanel=load('PhoneAutomationPanel.vue');
const dialog=load('AutomationFloatingDialog.vue');
const scripts=load('AutomationScriptSelector.vue');

test('UIA-01 every step Select and insertion menu is scoped to the phone-owned overlay',()=>{
    const fields=(step.match(/<Select\b/g)||[]).length;
    const scoped=(step.match(/<Select :append-to="overlayTarget \?\? 'self'"/g)||[]).length;
    assert.ok(fields>=10,'audit must include all step selector and boolean dropdowns');
    assert.equal(fields,scoped,'no PrimeVue dropdown may silently fall back to body');
    assert.match(timeline,/<Select :key="fullscreen/);
    assert.match(timeline,/:append-to="overlayTarget \?\? 'self'"/);
    assert.match(editor,/:overlay-target="overlayTarget"/);
    assert.match(card,/:overlay-target="automationSubwindowHost"/);
});
test('UIA-02 fullscreen host owns dialogs and controls across mode switches',()=>{
    assert.match(card,/<Teleport to="body" :disabled="isFullscreen">/);
    assert.match(card,/:fullscreen="isFullscreen"/);
    assert.match(editor,/:key="fullscreen \? 'fullscreen-menu' : 'page-menu'"/);
    assert.match(card,/p-select-overlay/);
    assert.match(componentPanel,/:append-to="portalTarget \?\? 'body'"/);
    assert.doesNotMatch(componentPanel,/append-to="body"/);
});
test('UIA-03 Escape handles expanded select before closing an unsaved editor',()=>{
    assert.match(dialog,/\[role="combobox"\]\[aria-expanded="true"\]/);
    assert.match(dialog,/if\(panel\.value\.querySelector/);
    assert.match(dialog,/discardRequested/);
    assert.match(dialog,/if \(props\.dirty\)/);
});
test('UIA-04 result details and script menus remain valid independently clickable controls',()=>{
    const match=timeline.match(/<button class="step-select"[\s\S]*?<\/button>/);
    assert.ok(match,'step selector must exist');
    assert.doesNotMatch(match[0].slice(match[0].indexOf('>')+1),/<button\b/,
        'no nested button inside step selector; it triggers two actions');
    assert.match(timeline,/class="batch-error-toggle"/);
    assert.match(scripts,/\.script-popover\{width:100%;max-width:100%/);
    assert.match(scripts,/overscroll-behavior:contain/);
});
