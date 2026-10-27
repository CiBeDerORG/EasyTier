<script setup lang="ts">
import type { AutomationStepDeletion } from '../../modules/automation-card-step-delete';
import AutomationFloatingDialog from './AutomationFloatingDialog.vue';

defineProps<{
    plan: AutomationStepDeletion | null;
    portalTarget: HTMLElement | null;
    fullscreen: boolean;
}>();
const emit=defineEmits<{ cancel: []; confirm: [plan: AutomationStepDeletion] }>();
</script>

<template>
    <AutomationFloatingDialog :visible="!!plan" :portal-target="portalTarget"
        :fullscreen="fullscreen" title="确认删除步骤与条件范围"
        @close="emit('cancel')">
        <p v-if="plan?.reason==='parent-condition'">
            这一步是所属分支的最后一项，单独删除会使 Then/Else 结构无效。
            可以改为删除<strong>所属条件块</strong>，但需要你明确确认。
        </p>
        <p v-else>删除条件节点时，会连同所属 Then/Else 范围内的子步骤一起删除。</p>
        <p v-if="plan" class="delete-count">本次将删除 <strong>{{ plan.affectedIds.length }}</strong> 个步骤，包含当前选中的步骤。</p>
        <p class="delete-hint">只修改共享工作草稿，不会修改已保存脚本版本与历史运行快照。删除后可以短时撤销。</p>
        <template #footer>
            <button type="button" @click="emit('cancel')">取消</button>
            <button type="button" class="confirm-delete"
                :disabled="!plan" @click="plan && emit('confirm',plan)">
                {{ plan?.reason==='parent-condition' ? '删除所属条件块' : '确认删除' }}
            </button>
        </template>
    </AutomationFloatingDialog>
</template>

<style scoped>
p{font-size:12px;line-height:1.6;margin:0 0 10px}
.delete-count{color:var(--p-red-500)}
.delete-hint{color:var(--console-muted);font-size:11px}
button{padding:7px 10px;border-radius:6px;border:1px solid var(--console-border);background:var(--console-panel-raised);color:inherit;cursor:pointer}
.confirm-delete{border-color:var(--p-red-500);color:var(--p-red-500)}
</style>
