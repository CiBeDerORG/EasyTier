<script setup lang="ts">
import { ref, watch } from 'vue';
import type { AutomationScript } from '../../types/automation';
import AutomationFloatingDialog from './AutomationFloatingDialog.vue';

const props=defineProps<{
    source: AutomationScript | null;
    portalTarget: HTMLElement | null;
    fullscreen: boolean;
    busy: boolean;
}>();
const emit=defineEmits<{
    cancel: [];
    confirm: [scriptId: string, revision: number, name: string];
}>();
const name=ref('');
const initialName=ref('');
watch(()=>props.source?.id,id=>{
    if(!id)return;
    initialName.value=`${props.source?.name ?? ''} - 副本`;
    name.value=initialName.value;
},{immediate:true});
function submit(){
    if(!props.source || props.busy || !name.value.trim())return;
    emit('confirm',props.source.id,props.source.revision,name.value.trim());
}
</script>
<template>
    <AutomationFloatingDialog :visible="!!source" :portal-target="portalTarget"
        :fullscreen="fullscreen" title="克隆脚本" :dirty="name !== initialName"
        :closable="!busy" @close="emit('cancel')">
        <form class="clone-form" @submit.prevent="submit">
            <label for="automation-script-clone-name">新脚本名称</label>
            <input id="automation-script-clone-name" v-model="name"
                :disabled="busy" maxlength="128" autocomplete="off" />
            <small>副本会创建为独立脚本，复制已保存的步骤，不复制执行记录。</small>
        </form>
        <template #footer>
            <button type="button" :disabled="busy" @click="emit('cancel')">取消</button>
            <button type="button" :disabled="busy || !name.trim()" @click="submit">确认克隆</button>
        </template>
    </AutomationFloatingDialog>
</template>
<style scoped>
.clone-form{display:flex;flex-direction:column;gap:7px;min-width:0}
.clone-form input{border:1px solid var(--console-border);background:var(--console-panel-raised);
    color:inherit;padding:7px;border-radius:6px;min-width:0;width:100%;box-sizing:border-box}
.clone-form small{color:var(--console-muted)}
button{padding:7px 11px;border-radius:6px;border:1px solid var(--console-border);
    background:var(--console-panel-raised);color:inherit;cursor:pointer}
button:disabled{opacity:.5;cursor:default}
</style>
