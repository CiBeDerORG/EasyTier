<script setup lang="ts">
import { computed } from 'vue';
import type { AutomationDraftWorkspace } from '../../modules/automation-draft';
import { projectCardSteps } from '../../modules/card-step-projection';
import type { InlineStepRunProgress } from '../../modules/useInlineStepRun';
import InlineStepRunStatus from './InlineStepRunStatus.vue';

const props=defineProps<{
    draft: AutomationDraftWorkspace;
    undoLabel?: string;
    pickerActive?: boolean;
    trialProgress?: InlineStepRunProgress | null;
}>();
const emit=defineEmits<{
    edit: [stepId: string];
    trial: [stepId: string];
    delete: [stepId: string];
    undo: [];
}>();
const projected=computed(()=>projectCardSteps(props.draft));
</script>

<template>
    <!-- The ONLY visible timeline: parent ScriptSelector owns script name/count/collapse. -->
    <section class="card-steps" aria-label="当前脚本步骤">
        <div v-if="undoLabel" class="undo-line" role="status">
            <span>{{ undoLabel }}</span>
            <button type="button" @click="emit('undo')">撤销删除</button>
        </div>
        <p v-if="projected.flowErrorAt" class="flow-warning" role="status">
            第 {{ projected.flowErrorAt }} 步条件分支不完整，请先修复结构。
        </p>
        <div v-if="!projected.steps.length" class="no-steps">
            暂无步骤。开启控件拾取后，可在原手机画面右键录入。
        </div>
        <ol v-else class="timeline" aria-label="脚本步骤时间线">
            <li v-for="step in projected.steps" :key="step.id" class="timeline-item"
                :class="{ active: step.id === draft.selectedStepId }">
                <button class="step-select" type="button"
                    :aria-pressed="step.id === draft.selectedStepId"
                    :title="'编辑第 '+step.number+' 步：'+step.label" @click="emit('edit',step.id)">
                    <span class="number">{{ step.number }}</span>
                    <span class="step-info" :style="{ paddingLeft: Math.min(step.depth,3)*6+'px' }">
                        <strong>{{ step.label }}</strong>
                        <small v-if="step.branch" class="branch">{{ step.branch }}</small>
                        <small>{{ step.context }}{{ step.incomplete ? ' · 待完善' : '' }}</small>
                        <InlineStepRunStatus v-if="trialProgress?.stepId === step.id"
                            :progress="trialProgress" />
                    </span>
                </button>
                <button type="button" class="step-trial" :aria-label="'试跑第 '+step.number+' 步"
                    title="当前手机单步骤真实试跑"
                    :disabled="step.incomplete || step.type === 'if_gone' || step.type === 'if_else'"
                    @click.stop="emit('trial',step.id)">
                    <i class="pi pi-play" aria-hidden="true" />
                </button>
                <button type="button" class="step-edit" :aria-label="'编辑第 '+step.number+' 步"
                    title="编辑步骤" @click.stop="emit('edit',step.id)">
                    <i class="pi pi-pencil" aria-hidden="true" />
                </button>
                <button type="button" class="step-delete" :aria-label="'删除第 '+step.number+' 步"
                    title="删除此步骤或其所属条件块" @click.stop="emit('delete',step.id)">
                    <i class="pi pi-trash" aria-hidden="true" />
                </button>
            </li>
        </ol>
        <small class="draft-hint">步骤与所有授权手机共用同一工作草稿。▶ 为真实试跑，定位演示只读。</small>
    </section>
</template>

<style scoped>
.card-steps{padding:3px 0 0;min-width:0}
.timeline{list-style:none;margin:6px 0 0;padding:0;display:flex;flex-direction:column;gap:3px;max-height:260px;overflow-y:auto;overscroll-behavior:contain}
.timeline-item{display:flex;align-items:stretch;min-width:0;border:1px solid var(--console-border);border-radius:7px}
.timeline-item.active{background:var(--console-accent-soft);border-color:var(--p-primary-color)}
.step-select{display:flex;align-items:center;min-width:0;gap:5px;flex:1;background:transparent;color:inherit;border:0;text-align:left;padding:6px 3px;cursor:pointer}
.number{display:grid;place-items:center;width:20px;height:20px;flex:0 0 20px;border-radius:50%;border:1px solid var(--console-border);font-size:10px;font-variant-numeric:tabular-nums}
.timeline-item.active .number{border-color:var(--p-primary-color);color:var(--p-primary-color)}
.step-info{min-width:0;display:flex;flex-direction:column;gap:2px}
.step-info strong{font-size:11px;line-height:1.25;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.step-info small{font-size:10px;color:var(--console-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.step-info .branch{color:var(--p-primary-color)}
.step-trial,.step-edit,.step-delete{flex:0 0 25px;width:25px;padding:0;border:0;border-radius:5px;background:transparent;cursor:pointer;font-size:12px}
.step-trial{color:var(--p-green-500)}
.step-trial:disabled{opacity:.35;cursor:not-allowed}
.step-edit{color:var(--console-muted)}
.step-edit:hover{color:var(--p-primary-color)}
.step-delete{color:var(--p-red-500)}
.timeline-item>button:hover:not(:disabled){background:var(--console-accent-soft)}
.timeline-item>button:focus-visible,.undo-line button:focus-visible{outline:2px solid var(--p-primary-color);outline-offset:1px}
.no-steps,.flow-warning{margin:8px 0;font-size:11px;line-height:1.5;color:var(--console-muted)}
.flow-warning{color:var(--p-orange-500)}
.draft-hint{display:block;margin-top:6px;font-size:10px;line-height:1.4;color:var(--console-muted)}
.undo-line{display:flex;align-items:center;justify-content:space-between;gap:6px;font-size:10px;color:var(--console-muted)}
.undo-line span{min-width:0;flex:1}
.undo-line button{white-space:nowrap;border:1px solid var(--console-border);border-radius:5px;padding:3px 5px;background:var(--console-panel-raised);color:inherit;cursor:pointer}
</style>
