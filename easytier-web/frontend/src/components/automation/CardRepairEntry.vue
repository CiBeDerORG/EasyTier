<script setup lang="ts">
/**
 * T15: explicit path from immutable failed run to ONE existing card step.
 * This component never sends phone actions; T14 owns manual trial confirmation.
 */
import { computed, ref, watch } from 'vue';
import { Button, Checkbox, Message, Tag } from 'primevue';
import type { AutomationStep } from '../../types/automation';
import type { CardRepairContext } from '../../modules/automation-run-repair';

const props=defineProps<{
    repair: CardRepairContext;
    step: AutomationStep;
    stepNumber: number;
    editing: boolean;
    runnerAvailable: boolean;
    blocked: boolean;
    saved: boolean;
}>();
const emit=defineEmits<{
    edit: [stepId: string];
    repick: [stepId: string];
    trial: [stepId: string];
    dismiss: [];
}>();
const unknown=computed(()=>props.repair.resultState==='outcome_unknown');
const acknowledged=ref(false);
watch(()=>[props.repair.runId,props.repair.stepId,props.repair.deviceId] as const,()=>{
    acknowledged.value=false;
});
const changed=computed(()=>JSON.stringify(props.step)!==props.repair.sourceStepFingerprint);
const copyHint=computed(()=>{
    if(props.repair.sourceKind==='current') return '已定位当前工作草稿里对应的原步骤，保留其他步骤与未发布的修改，不覆盖历史记录。';
    if(props.repair.sourceKind==='saved') return '已确认正式脚本修订与历史快照一致；本次修复将在共享工作草稿中修改，旧运行记录不变。';
    if(props.repair.sourceReason==='updated') return '原正式脚本已有新修订，已从旧快照建立独立草稿，不会覆盖新版脚本。';
    if(props.repair.sourceReason==='deleted') return '原正式脚本已删除，已从历史快照建立独立草稿。';
    return '来源是内联单步试跑，已从历史快照建立独立草稿。';
});
const allowTrial=computed(()=>props.runnerAvailable && !props.blocked &&
    props.saved && changed.value && (!unknown.value || acknowledged.value));
</script>

<template>
    <section class="card-repair" aria-label="试跑失败修复记录">
        <header>
            <strong>第 {{ stepNumber }} 步 · 失败回修</strong>
            <Tag :value="unknown?'结果未确认':repair.resultState==='interrupted'?'执行中断':'执行失败'"
                 :severity="unknown?'warn':'danger'"/>
            <Button icon="pi pi-times" text rounded size="small" aria-label="关闭回修提示"
                :disabled="unknown && !acknowledged"
                :title="unknown && !acknowledged ? '请先核对手机实际状态，再关闭未知结果提醒' : '关闭回修提示'"
                @click="emit('dismiss')"/>
        </header>
        <p>来源运行：<code>{{ repair.runId }}</code></p>
        <p>步骤：<code>{{ repair.stepId }}</code></p>
        <p>{{ copyHint }}</p>
        <p>{{ changed ? '已检测到该步骤与失败时的快照不同，准备保存后再次人工验证。' :
            '此步骤仍与失败时的历史快照相同；请先编辑或重新拾取，避免直接重复旧动作。' }}</p>
        <p v-if="changed && !saved" role="status">该修复尚未完成草稿保存或存在冲突；请保存并确认最新修订后才能再次执行。</p>
        <Message v-if="unknown" severity="warn" :closable="false">
            上次动作结果未知，修复不代表动作没有发生。请先核对手机当前实际状态，确认执行器清理完成后才能人工发起新的试跑。
        </Message>
        <label v-if="unknown" class="confirm">
            <Checkbox v-model="acknowledged" binary/>
            <span>我已人工核对手机当前状态，理解再次试跑可能重复执行动作。</span>
        </label>
        <div class="actions">
            <Button label="编辑步骤" icon="pi pi-pencil" outlined size="small"
                :disabled="blocked" @click="emit('edit',repair.stepId)"/>
            <Button label="重新拾取" icon="pi pi-search" outlined size="small"
                :disabled="blocked" @click="emit('repick',repair.stepId)"/>
            <Button label="再次验证（新试跑）" icon="pi pi-play" size="small"
                :disabled="!allowTrial || editing" @click="emit('trial',repair.stepId)"/>
        </div>
        <small>再次验证会创建独立运行记录；历史快照、旧运行 ID 和失败结果保持不变。</small>
    </section>
</template>

<style scoped>
.card-repair{display:grid;gap:8px;padding:11px 12px;border-top:1px solid var(--console-border);background:var(--console-panel);font-size:11px;min-width:0}
.card-repair header{display:flex;align-items:center;gap:8px}
.card-repair header strong{flex:1;font-size:12px}
.card-repair p{margin:0;line-height:1.5;color:var(--console-muted);overflow-wrap:anywhere}
.card-repair code{font:inherit;overflow-wrap:anywhere}
.confirm{display:flex;align-items:flex-start;gap:7px;line-height:1.5}
.actions{display:flex;gap:6px;flex-wrap:wrap}
.actions :deep(.p-button){font-size:11px;padding:7px}
.card-repair small{color:var(--console-muted);line-height:1.5}
</style>
