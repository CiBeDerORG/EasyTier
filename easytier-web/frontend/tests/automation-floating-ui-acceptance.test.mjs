import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
const load=name=>readFileSync(new URL('../src/components/automation/'+name,import.meta.url),'utf8');
const step=load('ScriptStepEditor.vue');
const inline=load('AutomationInlineSelect.vue');
const card=load('PhoneCard.vue');
const editor=load('CardStepEditor.vue');
const timeline=load('CardStepTimeline.vue');
const componentPanel=load('PhoneAutomationPanel.vue');
const dialog=load('AutomationFloatingDialog.vue');
const scripts=load('AutomationScriptSelector.vue');

test('UIA-01 all automation editing options are inline, no cross-window portal',()=>{
    assert.ok((step.match(/<AutomationInlineSelect\b/g)||[]).length>=10);
    assert.doesNotMatch(step,/<Select\b/);
    assert.match(timeline,/<AutomationInlineSelect :model-value="insertType"/);
    assert.match(editor,/:overlay-target="overlayTarget"/);
    assert.match(card,/:overlay-target="automationSubwindowHost"/);
    assert.match(inline,/class="choice-list"/);
    assert.doesNotMatch(inline,/<Teleport/);
    assert.match(inline,/@click.stop="toggle"/);
    assert.match(inline,/@click.stop="choose\(option.value\)"/);
});
test('UIA-02 normal and fullscreen panels use one actual editing tree',()=>{
    assert.match(card,/<Teleport to="body" :disabled="isFullscreen">/);
    assert.match(card,/:fullscreen="isFullscreen"/);
    assert.match(editor,/:key="fullscreen \? 'fullscreen-menu' : 'page-menu'"/);
    assert.match(inline,/pointer-events:auto/);
    assert.match(componentPanel,/:append-to="portalTarget \?\? 'body'"/);
});
test('UIA-03 expanded inline option list handles Escape before its editor',()=>{
    assert.match(dialog,/automation-inline-choice/);
    assert.match(inline,/event.key==='Escape'/);
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
