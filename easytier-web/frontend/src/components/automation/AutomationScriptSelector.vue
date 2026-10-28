<script setup lang="ts">
import { ref, watch } from 'vue';
import type { AutomationScript } from '../../types/automation';

const props=defineProps<{
    scripts: readonly AutomationScript[];
    selectedId: string | null;
    selectedName: string;
    count: number;
    busy: boolean;
    loading: boolean;
    error?: string;
    syncLabel?: string;
    formalPending?: boolean;
}>();
const emit=defineEmits<{
    refresh: [];
    create: [name: string];
    select: [id: string];
    clone: [id: string];
    publish: [];
    remove: [id:string];
    reconcile: [];
}>();
const expanded=ref(false);
const draftName=ref('');
const collapsed=ref(false);
function toggle(){
    if (props.busy) return;
    expanded.value=!expanded.value;
    if(expanded.value)emit('refresh');
}
function submitNew(){
    const name=draftName.value.trim();
    if(!name || props.busy)return;
    emit('create',name);
}
function select(id:string){
    if(props.busy)return;
    emit('select',id);expanded.value=false;
}
function clone(id:string){
    if(props.busy)return;
    emit('clone',id);
}
function keyDown(event: KeyboardEvent){
    if(event.key==='Escape' && expanded.value){event.stopPropagation();expanded.value=false;}
}
watch(()=>props.selectedId,(id,old)=>{if(id && id!==old){draftName.value='';expanded.value=false;}});
</script>
<template>
    <div class="automation-script-selector" @keydown="keyDown">
        <div class="automation-script-line">
            <button type="button" class="script-trigger" :title="selectedName || '选择自动化脚本'"
                :aria-expanded="expanded" aria-haspopup="listbox" :disabled="busy" @click="toggle">
                <span>{{ selectedName || '选择自动化脚本' }}</span>
                <i :class="expanded ? 'pi pi-chevron-up' : 'pi pi-chevron-down'" aria-hidden="true" />
            </button>
            <button type="button" class="script-count" :aria-expanded="!collapsed"
                :title="collapsed ? '展开脚本步骤' : '收起脚本步骤'" @click="collapsed=!collapsed">
                {{ count }}/100 步 <i :class="collapsed ? 'pi pi-angle-down' : 'pi pi-angle-up'" aria-hidden="true" />
            </button>
        </div>
        <div v-if="expanded" class="script-popover">
            <!-- The first row starts immediately with creation: no duplicated current title. -->
            <form class="script-create-row" @submit.prevent="submitNew">
                <input v-model="draftName" aria-label="新自动化脚本名称" maxlength="128"
                    placeholder="输入新自动化名称" :disabled="busy" />
                <button type="submit" :disabled="busy || !draftName.trim()">新建</button>
            </form>
            <div class="script-options" role="listbox" aria-label="已保存的自动化脚本">
                <div v-for="script in scripts" :key="script.id" class="script-option">
                    <button type="button" role="option" :aria-selected="script.id===selectedId"
                        :title="script.name" :disabled="busy" @click="select(script.id)">
                        <span class="check">{{ script.id===selectedId ? '✓' : '' }}</span>
                        <span class="option-name">{{ script.name }}</span>
                    </button>
                    <button type="button" class="clone-button" :title="`克隆：${script.name}`"
                        :aria-label="`克隆 ${script.name}`" :disabled="busy" @click.stop="clone(script.id)">克隆</button>
                    <button type="button" class="delete-button" :title="`删除正式脚本：${script.name}`"
                        :aria-label="`删除正式脚本 ${script.name}`" :disabled="busy || formalPending"
                        @click.stop="emit('remove',script.id)"><i class="pi pi-trash" aria-hidden="true" /></button>
                </div>
                <p v-if="!loading && !scripts.length" class="script-empty">暂无已保存脚本，可以在上方新建。</p>
                <p v-if="loading" class="script-empty">正在读取脚本列表…</p>
            </div>
        </div>
        <div class="script-manage-actions">
            <button type="button" :disabled="busy || formalPending"
                :title="selectedId?'将当前已同步草稿更新到正式脚本下一修订':'从当前草稿发布新正式脚本'"
                @click="emit('publish')">
                <i class="pi pi-save" aria-hidden="true" />
                {{ selectedId?'更新正式版本':'发布为正式脚本' }}
            </button>
            <button v-if="formalPending" type="button" @click="emit('reconcile')">
                <i class="pi pi-history" aria-hidden="true" />只读核对旧写入
            </button>
        </div>
        <p class="script-sync-label" :title="syncLabel || ''">{{ syncLabel }}</p>
        <p v-if="error" class="script-error" role="alert">{{ error }}</p>
        <slot v-if="!collapsed" />
    </div>
</template>
<style scoped>
.automation-script-selector{display:flex;flex-direction:column;gap:7px;min-width:0}
.automation-script-line{display:flex;align-items:center;justify-content:space-between;gap:5px;min-width:0}
.script-trigger{display:flex;gap:5px;align-items:center;justify-content:space-between;
    width:56%;min-width:0;max-width:56%;border:1px solid var(--console-border);
    border-radius:6px;background:var(--console-panel-raised);color:inherit;padding:6px;cursor:pointer;font-size:12px}
.script-trigger>span{white-space:nowrap;overflow:hidden;text-overflow:ellipsis;min-width:0}
.script-trigger>.pi{font-size:9px;flex:0 0 auto}
.script-count{border:0;background:transparent;color:inherit;white-space:nowrap;
    font-size:11px;padding:4px 0;cursor:pointer}
.script-count>.pi{font-size:10px}
.script-popover{width:56%;max-width:56%;border:1px solid var(--console-border);border-radius:8px;
    background:var(--console-panel);box-shadow:0 4px 16px #0007;overflow:hidden}
.script-create-row{display:flex;gap:3px;padding:5px;border-bottom:1px solid var(--console-border)}
.script-create-row input{min-width:0;width:100%;font-size:11px;padding:5px 3px;border:1px solid var(--console-border);
    background:var(--console-panel-raised);color:inherit;border-radius:4px}
.script-create-row button{font-size:11px;padding:3px 5px;border:1px solid var(--console-border);
    border-radius:4px;background:var(--console-panel-raised);color:inherit;cursor:pointer}
.script-options{max-height:160px;overflow-y:auto}
.script-option{display:flex;gap:2px;min-width:0;align-items:center;padding:2px}
.script-option>button:first-child{display:flex;gap:4px;min-width:0;flex:1;align-items:center;
    padding:5px 0;border:0;background:transparent;color:inherit;font-size:11px;text-align:left;cursor:pointer}
.script-option>button[aria-selected=true]{font-weight:600}
.check{width:11px;flex:0 0 11px}
.option-name{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.clone-button,.delete-button{flex:0 0 auto;border:1px solid var(--console-border);border-radius:4px;
    padding:3px 4px;color:inherit;background:transparent;font-size:10px;cursor:pointer}
.delete-button{color:var(--p-red-500);padding:3px 6px}
.script-manage-actions{display:flex;gap:6px;flex-wrap:wrap}
.script-manage-actions button{border:1px solid var(--console-border);border-radius:6px;
    padding:5px 7px;background:var(--console-panel-raised);color:inherit;font-size:10px;cursor:pointer}
.script-manage-actions button i{margin-right:4px}
.script-empty,.script-sync-label,.script-error{margin:0;font-size:10px;line-height:1.4;color:var(--console-muted)}
.script-error{color:var(--p-red-500)}
button:disabled{opacity:.55;cursor:default}
</style>
