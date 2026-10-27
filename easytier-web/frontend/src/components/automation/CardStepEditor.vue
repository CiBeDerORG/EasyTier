<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { Button, Message } from 'primevue';
import type { AutomationStep } from '../../types/automation';
import { stepCanPickTarget } from '../../modules/automation-step-target';
import type { LocatorDemoResult } from '../../modules/automation-locator-demo';
import ScriptStepEditor from './ScriptStepEditor.vue';

const props = defineProps<{
    step: AutomationStep; stepNumber: number; repicking?: boolean;
    demoLoading?: boolean; demoResult?: LocatorDemoResult | null; demoError?: string;
    demoAvailable?: boolean;
}>();
const emit = defineEmits<{
    save: [stepId: string, previous: AutomationStep, next: AutomationStep];
    repick: [stepId: string];
    demo: [stepId: string];
    'clear-demo': [];
    close: [];
    'dirty-change': [dirty: boolean];
}>();
function copy(step: AutomationStep): AutomationStep {
    return {...step, selector: step.selector ? {...step.selector}:undefined,
        scroll_container:step.scroll_container?{...step.scroll_container}:undefined};
}
const original=ref<AutomationStep>(copy(props.step));
const edited=ref<AutomationStep>(copy(props.step));
const fingerprint=(step:AutomationStep)=>JSON.stringify(step);
const dirty=computed(()=>fingerprint(edited.value)!==fingerprint(original.value));
watch(dirty, value=>emit('dirty-change',value),{immediate:true});
const conflict=computed(()=>fingerprint(props.step)!==fingerprint(original.value));
const warning=ref('');
watch(()=>props.step.id,()=>{original.value=copy(props.step);edited.value=copy(props.step);warning.value='';});
watch(()=>props.step, value=>{
    if (!dirty.value) {original.value=copy(value);edited.value=copy(value);warning.value='';}
},{deep:true});
function updateDraft(next:AutomationStep) {
    if(next.id!==original.value.id || next.type!==original.value.type) return;
    edited.value=copy(next);
    warning.value='';
}
function save() {
    warning.value='';
    if (conflict.value) {warning.value='此步骤已被其他操作修改，请取消后重新打开以获取最新内容。';return;}
    if (!dirty.value) {emit('close');return;}
    emit('save',props.step.id,copy(original.value),copy(edited.value));
}
function repick() {
    warning.value='';
    if (dirty.value || conflict.value) {
        warning.value='请先保存或取消参数修改，再重新拾取控件。';
        return;
    }
    emit('repick',props.step.id);
}
</script>

<template>
    <section class="card-step-editor" aria-label="卡片步骤编辑">
        <header>
            <div><strong>编辑第 {{ stepNumber }} 步</strong><small>在当前卡片修改参数，不离开实时画面</small></div>
            <button type="button" class="close-editor" aria-label="关闭步骤编辑" @click="emit('close')">
                <i class="pi pi-times" aria-hidden="true"/>
            </button>
        </header>
        <Message v-if="warning" severity="warn" :closable="false">{{ warning }}</Message>
        <Message v-if="conflict" severity="warn" :closable="false">步骤在编辑期间已变化。当前修改不会自动覆盖新的草稿内容。</Message>
        <div class="edit-fields">
            <ScriptStepEditor :model-value="edited" lock-action-type @update:model-value="updateDraft"/>
        </div>
        <Message v-if="demoError" severity="warn" :closable="false">{{ demoError }}</Message>
        <Message v-if="demoResult" :severity="demoResult.status === 'one' ? 'success' : demoResult.status === 'none' || demoResult.status === 'many' ? 'warn' : 'info'" :closable="false">
            <strong>只读定位结果 · {{ demoResult.count }} 个匹配</strong>
            <span class="demo-message">{{ demoResult.message }}</span>
        </Message>
        <div class="actions">
            <Button label="取消" text size="small" @click="emit('close')"/>
            <Button label="定位演示（只读）" icon="pi pi-eye" outlined size="small"
                :loading="demoLoading" :disabled="!demoAvailable || dirty || conflict || repicking || demoLoading"
                @click="emit('demo', step.id)"/>
            <Button v-if="demoResult || demoError" label="清除高亮" icon="pi pi-times" text size="small"
                @click="emit('clear-demo')"/>
            <Button v-if="stepCanPickTarget(step)" :label="repicking?'等待右键拾取…':'重新拾取控件'"
                icon="pi pi-search" outlined size="small" :disabled="dirty || conflict || repicking" @click="repick"/>
            <Button label="保存到草稿" icon="pi pi-check" size="small" :disabled="!dirty || conflict" @click="save"/>
        </div>
        <p class="tip">保存只修改本步骤，不执行手机动作；重新拾取后会保留动作参数和步骤编号。</p>
    </section>
</template>

<style scoped>
.card-step-editor{padding:10px 12px;border-top:1px solid var(--console-border);background:var(--console-panel);min-width:0;display:grid;gap:10px}
header{display:flex;justify-content:space-between;align-items:flex-start;gap:8px}
header>div{min-width:0;display:flex;flex-direction:column;gap:3px}
header strong{font-size:12px}
header small,.tip{color:var(--console-muted);font-size:11px;line-height:1.5}
.close-editor{background:transparent;color:var(--console-muted);border:0;padding:5px;cursor:pointer;border-radius:5px}
.close-editor:focus-visible{outline:2px solid var(--p-primary-color)}
.edit-fields{overflow:auto;max-height:350px;min-width:0;padding:3px 2px}
.edit-fields :deep(.step-editor){gap:10px}
.edit-fields :deep(.field){font-size:12px}
.edit-fields :deep(.selector-fields){padding:8px;gap:8px}
.actions{display:flex;align-items:center;justify-content:flex-end;gap:5px;flex-wrap:wrap}
.actions :deep(.p-button){font-size:11px;padding:7px}
.tip{margin:0}
.demo-message{display:block;margin-top:4px;font-size:11px;line-height:1.5}
</style>
