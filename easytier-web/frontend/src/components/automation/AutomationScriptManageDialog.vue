<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import AutomationFloatingDialog from './AutomationFloatingDialog.vue';
import type { AutomationScript } from '../../types/automation';
import type { ScriptGuard } from '../../modules/automation-script-workspace';

/** T46: only a presentation/dialog over the unique owner-level draft and
 * revision-CAS Script API; it never creates a second script editor or stream.
 */
const props=defineProps<{
    visible:boolean;
    mode:'publish'|'delete';
    portalTarget:HTMLElement|null;
    fullscreen:boolean;
    draftName:string;
    draftDescription:string;
    stepCount:number;
    source:AutomationScript|null;
    guard:ScriptGuard;
    busy:boolean;saveBusy?:boolean;saveMessage?:string;
    error?:string;
}>();
const emit=defineEmits<{
    close:[];
    publish:[name:string,description:string];
    saveDraft:[];
    remove:[id:string,revision:number];
}>();
const name=ref('');
const description=ref('');
const acknowledged=ref('');
watch(()=>[props.visible,props.mode,props.source?.id] as const,([visible])=>{
    if(!visible)return;
    name.value=props.draftName;
    description.value=props.draftDescription;
    acknowledged.value='';
},{immediate:true});
const allowed=computed(()=>props.guard==='clean'&&!props.busy);
const validName=computed(()=>!!name.value.trim()&&new TextEncoder().encode(name.value.trim()).length<=128);
const validDescription=computed(()=>new TextEncoder().encode(description.value).length<=2000);
function confirm(){
    if(!allowed.value)return;
    if(props.mode==='publish'){
        if(!validName.value||!validDescription.value)return;
        emit('publish',name.value.trim(),description.value);
    } else if(props.source && acknowledged.value===props.source.name)
        emit('remove',props.source.id,props.source.revision);
}
</script>
<template>
    <AutomationFloatingDialog :visible="visible" :portal-target="portalTarget"
        :fullscreen="fullscreen"
        :title="mode==='publish' ? (source?'保存正式脚本修订':'发布新的正式脚本'):'删除正式脚本'"
        :closable="!busy" @close="emit('close')">
        <div class="manage-content">
            <template v-if="mode==='publish'">
                <small>来源：当前账号唯一工作草稿（{{ stepCount }}/100 步）。
                    {{ source?'更新原正式脚本的下一修订，不改变历史运行快照。':'新建正式脚本 ID，保留当前工作草稿直到服务端确认。' }}
                </small>
                <small v-if="source">原正式脚本：{{ source.name }} · 修订 {{ source.revision }}</small>
                <label>脚本名称
                    <input v-model="name" maxlength="128" autocomplete="off" :disabled="busy" />
                </label>
                <label>脚本说明（选填）
                    <textarea v-model="description" rows="3" maxlength="2000" :disabled="busy" />
                </label>
                <small v-if="!validName || !validDescription" class="manage-error">
                    名称不能留空且不超过128字节，说明不超过2000字节。
                </small>
            </template>
            <template v-else>
                <strong>确定永久删除“{{ source?.name }}”吗？</strong>
                <small v-if="source">仅删除当前确认的修订 {{ source.revision }}；若其他页面已更新，数据库会拒绝删除。历史运行快照保留，当前未发布草稿不会被清空。</small>
                <label>请输入完整脚本名称以确认
                    <input v-model="acknowledged" autocomplete="off" :disabled="busy"
                        :placeholder="source?.name??''" />
                </label>
            </template>
            <p v-if="guard!=='clean'" class="manage-error" role="alert">
                {{ guard==='editor'?'步骤编辑尚未确认，先保存步骤。':
                   guard==='conflict'?'工作草稿出现修订冲突，先解决冲突。':
                   guard==='loading'?'正在加载服务器工作草稿，请完成同步后继续。':
                   '工作草稿仍有尚未确认的修改，请先保存并等待同步。' }}
            </p>
            <p v-if="saveMessage" class="manage-hint" role="status">{{ saveMessage }}</p>
            <p v-if="error" class="manage-error" role="alert">{{ error }}</p>
            <div class="manage-actions">
                <button v-if="mode==='publish'" type="button"
                    :disabled="busy || saveBusy || guard==='editor' || guard==='conflict' || guard==='loading'"
                    @click="emit('saveDraft')">
                    {{ saveBusy?'正在保存…':'① 保存工作草稿' }}
                </button>
                <button type="button" :disabled="busy" @click="emit('close')">取消</button>
                <button type="button" :class="{danger:mode==='delete'}"
                    :disabled="!allowed || (mode==='publish'? !validName || !validDescription:
                        !source || acknowledged!==source.name)"
                    @click="confirm">
                    {{ busy?'正在等待服务器确认':mode==='publish'?'② 保存为正式版本':'确认删除此修订' }}
                </button>
            </div>
        </div>
    </AutomationFloatingDialog>
</template>
<style scoped>
.manage-content{display:grid;gap:9px;font-size:12px;line-height:1.6;min-width:0}
.manage-content>small{color:var(--console-muted);overflow-wrap:anywhere}
.manage-hint{font-size:11px;color:var(--console-muted);margin:0;overflow-wrap:anywhere}
.manage-content label{display:grid;gap:5px}
.manage-content input,.manage-content textarea{width:100%;min-width:0;box-sizing:border-box;
    padding:6px 7px;border:1px solid var(--console-border);background:var(--console-panel-raised);
    border-radius:6px;color:inherit;font:inherit;resize:vertical}
.manage-content .manage-error{margin:0;color:var(--p-red-500);font-size:11px;overflow-wrap:anywhere}
.manage-actions{display:flex;justify-content:flex-end;gap:8px;flex-wrap:wrap}
.manage-actions button{border:1px solid var(--console-border);border-radius:6px;padding:6px 9px;
    color:inherit;background:var(--console-panel-raised);cursor:pointer}
.manage-actions button:last-child{background:var(--p-primary-color);color:var(--p-primary-contrast-color)}
.manage-actions button.danger{background:var(--p-red-500);color:white}
.manage-actions button:disabled{opacity:.5;cursor:default}
</style>
