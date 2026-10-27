<script setup lang="ts">
import AutomationFloatingDialog from './AutomationFloatingDialog.vue';
import type { ScriptGuard, ScriptSwitchDecision } from '../../modules/automation-script-workspace';
defineProps<{
    visible: boolean;
    portalTarget: HTMLElement | null;
    fullscreen: boolean;
    guard: ScriptGuard;
    busy: boolean;
}>();
const emit=defineEmits<{
    cancel: [];
    decide: [decision: ScriptSwitchDecision];
}>();
</script>
<template>
    <AutomationFloatingDialog :visible="visible" :portal-target="portalTarget"
        :fullscreen="fullscreen" title="切换脚本前确认" :closable="!busy"
        @close="emit('cancel')">
        <p v-if="guard==='editor'">当前步骤还有未确认的参数修改。请先保存编辑，或者明确放弃这些修改。</p>
        <p v-else-if="guard==='conflict'">当前工作草稿与服务器版本冲突；不能直接覆盖，请保留当前内容或明确放弃本地未同步修改。</p>
        <p v-else-if="guard==='loading'">服务器草稿状态尚未完成读取，请先等待。</p>
        <p v-else>当前工作草稿尚未确认同步。可先保存旧草稿再继续，也可以明确放弃未同步修改；旧服务器草稿不会被删除。</p>
        <template #footer>
            <button type="button" :disabled="busy" @click="emit('cancel')">留在当前</button>
            <button type="button" :disabled="busy || guard==='loading' || guard==='editor' || guard==='conflict'"
                @click="emit('decide','save')">保存当前并继续</button>
            <button type="button" :disabled="busy || guard==='loading'" class="script-discard"
                @click="emit('decide','discard')">放弃未确认修改</button>
        </template>
    </AutomationFloatingDialog>
</template>
<style scoped>
p{margin:0;font-size:12px;line-height:1.6}
button{padding:7px 9px;border-radius:6px;border:1px solid var(--console-border);
    background:var(--console-panel-raised);color:inherit;cursor:pointer}
button:disabled{opacity:.5;cursor:default}
.script-discard{color:var(--p-red-500)}
</style>
