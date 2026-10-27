<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import type { AutomationDraftWorkspace } from '../../modules/automation-draft';
import { projectCardSteps } from '../../modules/card-step-projection';

const props=defineProps<{
    draft: AutomationDraftWorkspace;
    syncLabel?: string;
    pickerActive?: boolean;
}>();
const emit=defineEmits<{
    select: [stepId: string];
    edit: [stepId: string];
    trial: [stepId: string];
}>();

const projected=computed(() => projectCardSteps(props.draft));
const expanded=ref(!!props.pickerActive && props.draft.steps.length > 0);
const manuallyCollapsed=ref(false);
function toggleExpand() {
    expanded.value = !expanded.value;
    manuallyCollapsed.value = !expanded.value;
}
watch(() => props.pickerActive, active => {
    if (active && !manuallyCollapsed.value && props.draft.steps.length) expanded.value=true;
});
watch(() => props.draft.steps.length, (current,previous) => {
    if (current > previous && props.pickerActive && !manuallyCollapsed.value) expanded.value=true;
});
watch(() => [props.draft.ownerId,props.draft.editingId] as const, () => {
    manuallyCollapsed.value=false;
    expanded.value=!!props.pickerActive && props.draft.steps.length > 0;
});
function openEditor(id: string) {
    emit('select',id);
    emit('edit',id);
}
</script>

<template>
    <section class="card-steps" aria-label="当前脚本草稿步骤">
        <div class="card-steps-top">
            <div class="draft-identity">
                <strong :title="projected.title">{{ projected.title }}</strong>
                <span class="draft-badge" :class="projected.saveState">
                    {{ projected.saveLabel }} · {{ syncLabel ?? '工作草稿未确认同步' }}
                </span>
            </div>
            <button type="button" class="fold-button"
                :aria-expanded="expanded" :aria-label="(expanded?'收起':'展开')+'脚本步骤'"
                @click="toggleExpand">
                <span>{{ projected.steps.length }}/100 步</span>
                <i :class="expanded ? 'pi pi-chevron-up' : 'pi pi-chevron-down'" aria-hidden="true" />
            </button>
        </div>
        <template v-if="expanded">
            <p v-if="projected.flowErrorAt" class="flow-warning" role="status">
                步骤 {{ projected.flowErrorAt }} 的条件分支不完整，请进入脚本库修改。
            </p>
            <div v-if="!projected.steps.length" class="no-steps">
                尚无步骤。开启拾取后，在手机画面右键选择控件即可录入。
            </div>
            <ol v-else class="timeline" aria-label="脚本步骤时间线">
                <li v-for="step in projected.steps" :key="step.id" class="timeline-item"
                    :class="{ active: step.id === draft.selectedStepId }">
                    <button class="step-select" type="button"
                        :aria-pressed="step.id === draft.selectedStepId"
                        :title="'选择步骤 '+step.number+'：'+step.label"
                        @click="openEditor(step.id)">
                        <span class="number">{{ step.number }}</span>
                        <span class="step-info" :style="{ paddingLeft: Math.min(step.depth,3)*7+'px' }">
                            <strong>{{ step.label }}</strong>
                            <small v-if="step.branch" class="branch">{{ step.branch }}</small>
                            <small>{{ step.context }}{{ step.incomplete ? ' · 待完善' : '' }}</small>
                        </span>
                    </button>
                    <button type="button" class="step-trial" :aria-label="'试跑步骤 '+step.number"
                        title="在此手机单独试跑步骤（执行前检查并确认）"
                        :disabled="step.incomplete || step.type === 'if_gone' || step.type === 'if_else'"
                        @click="emit('trial',step.id)">
                        <i class="pi pi-play" aria-hidden="true" />
                    </button>
                    <button type="button" class="step-edit" :aria-label="'编辑步骤 '+step.number"
                        title="在当前手机卡片编辑步骤"
                        @click="openEditor(step.id)">
                        <i class="pi pi-pencil" aria-hidden="true" />
                    </button>
                </li>
            </ol>
            <small class="draft-hint">步骤与脚本库实时共享；点击步骤可编辑与重新拾取；绿色播放入口为真实单步试跑，定位演示只读不操作手机。</small>
        </template>
    </section>
</template>

<style scoped>
.card-steps{padding:9px 11px 10px;border-top:1px solid var(--console-border);min-width:0;background:var(--console-panel)}
.card-steps-top{display:flex;align-items:center;justify-content:space-between;gap:6px;min-width:0}
.draft-identity{display:flex;flex:1 1 auto;min-width:0;flex-direction:column;gap:4px}
.draft-identity strong{font-size:12px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:100%}
.draft-badge{font-size:10px;color:var(--console-muted)}
.draft-badge.new,.draft-badge.modified{color:var(--p-orange-500)}
.draft-badge.saved{color:var(--p-green-500)}
.fold-button{display:flex;align-items:center;gap:6px;flex:0 0 auto;border:1px solid var(--console-border);background:var(--console-panel-raised);border-radius:7px;color:var(--p-text-color);padding:6px;font-size:11px;cursor:pointer}
 .fold-button:focus-visible,.step-select:focus-visible,.step-edit:focus-visible,.step-trial:focus-visible{outline:2px solid var(--p-primary-color);outline-offset:2px}
.timeline{list-style:none;margin:9px 0 0;padding:0;display:flex;flex-direction:column;gap:3px;max-height:220px;overflow:auto;overscroll-behavior:contain}
.timeline-item{display:flex;align-items:stretch;min-width:0;border:1px solid transparent;border-radius:7px}
.timeline-item.active{background:var(--console-accent-soft);border-color:var(--p-primary-color)}
.step-select{display:flex;align-items:center;min-width:0;gap:7px;flex:1;background:transparent;color:var(--p-text-color);border:0;text-align:left;padding:7px 4px;cursor:pointer}
.number{display:grid;place-items:center;width:24px;height:24px;flex-shrink:0;border-radius:50%;border:1px solid var(--console-border);font-size:11px;font-variant-numeric:tabular-nums}
.timeline-item.active .number{border-color:var(--p-primary-color);color:var(--p-primary-color)}
.step-info{min-width:0;display:flex;flex-direction:column;gap:3px}
.step-info strong{font-size:12px;line-height:1.3;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.step-info small{font-size:10px;color:var(--console-muted);white-space:nowrap;text-overflow:ellipsis;overflow:hidden}
.step-info .branch{color:var(--p-primary-color)}
.step-trial{flex:0 0 auto;background:transparent;color:var(--p-green-500);border:0;border-radius:7px;min-width:30px;cursor:pointer}
.step-trial:hover:not(:disabled){background:var(--console-accent-soft)}
.step-trial:disabled{opacity:.35;cursor:not-allowed}
.step-edit{flex:0 0 auto;background:transparent;color:var(--console-muted);border:0;border-radius:7px;min-width:30px;cursor:pointer}
.step-edit:hover{color:var(--p-primary-color);background:var(--console-accent-soft)}
.no-steps,.flow-warning{margin:9px 0 0;font-size:11px;line-height:1.6;color:var(--console-muted)}
.flow-warning{color:var(--p-orange-500)}
.draft-hint{display:block;margin-top:8px;font-size:10px;line-height:1.5;color:var(--console-muted)}
</style>
