<script setup lang="ts">
/**
 * T46/T47 — read-only compatibility entry for old "scripts" bookmarks.
 *
 * The parent owns the ONE AutomationDraftWorkspace, all script CRUD and the
 * one original PhoneCard video. This page must not mutate steps, save scripts,
 * dispatch runs, or create a second editor/player.
 *
 * Keep the T17 legacy trial-receipt recovery: an unknown old action may only
 * be resolved from read-only history, never automatically replayed.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Button } from 'primevue';
import { useI18n } from 'vue-i18n';
import AutomationApi, { automationErrorMessage } from '../../modules/automation-api';
import type { AutomationDevice, AutomationRun, AutomationScript } from '../../types/automation';
import type { AutomationDraftWorkspace } from '../../modules/automation-draft';
import { readTrialReceipt, clearTrialReceipt, findTrialRun,
    type TrialSubmissionReceipt } from '../../modules/automation-trial-receipt';

const props=defineProps<{
    ownerId:number;
    selectedDeviceIds:string[];
    pickerDevices:{id:string;name:string}[];
    runDeviceSnapshots:AutomationDevice[];
    draft:AutomationDraftWorkspace;
    draftSyncLabel?:string;
    draftSyncMessage?:string;
}>();
const emit=defineEmits<{
    'run-created':[run:AutomationRun];
    'to-card':[deviceId:string,scriptId?:string,revision?:number];
}>();
const api=new AutomationApi();
const {locale}=useI18n();
const text=(zh:string,en:string)=>locale.value==='cn'||locale.value.startsWith('zh')?zh:en;
const scripts=ref<AutomationScript[]>([]);
const loading=ref(false);
const error=ref('');
const notice=ref('');
const checking=ref(false);
const pendingReceipts=ref<TrialSubmissionReceipt[]>([]);
let epoch=0,disposed=false;
const available= computed(()=>new Set(props.pickerDevices.map(d=>d.id)));
const targetDevice=computed(()=>props.selectedDeviceIds.find(id=>available.value.has(id))??
    props.pickerDevices[0]?.id);
const deviceName=computed(()=>props.pickerDevices.find(d=>d.id===targetDevice.value)?.name??'');
function checkHistoricalReceipts(){
    try{
        pendingReceipts.value=props.runDeviceSnapshots.map(d=>
            readTrialReceipt(window.sessionStorage,props.ownerId,d.id))
            .filter((p):p is TrialSubmissionReceipt=>p!==null);
        if(pendingReceipts.value.length)
            notice.value='检测到历史试跑有未核实请求，请先只读查询；本页不会发起任何手机动作。';
    }catch(cause){
        error.value='旧请求身份读取失败：'+automationErrorMessage(cause)+
            '；不能证明原动作未发生，请人工核对。';
    }
}
async function reload(){
    const n=++epoch,owner=props.ownerId;
    if(!owner)return;
    loading.value=true;
    try{
        const result=await api.listScripts(owner);
        if(!disposed&&n===epoch&&props.ownerId===owner){
            scripts.value=result.items;error.value='';
        }
    }catch(cause){
        if(!disposed&&n===epoch)error.value='读取正式脚本失败：'+automationErrorMessage(cause);
    }finally{if(n===epoch)loading.value=false;}
}
async function recoverOldRequests(){
    if(checking.value||!pendingReceipts.value.length)return;
    const owner=props.ownerId,n=epoch,receipts=[...pendingReceipts.value];
    checking.value=true;
    try{
        const list=await api.listRuns(owner);
        if(disposed||n!==epoch||owner!==props.ownerId)return;
        const matches=receipts.map(receipt=>({
            receipt,run:findTrialRun(list.items,receipt),
        }));
        if(matches.some(item=>!item.run||!item.run.script_snapshot.steps.some(
            step=>step.id===item.receipt.stepId))){
            error.value='未能从服务端执行历史唯一确认原请求；继续保留保护凭据，绝不再次运行。';
            return;
        }
        for(const item of matches)clearTrialReceipt(window.sessionStorage,item.receipt);
        pendingReceipts.value=[];
        notice.value='已只读核对旧请求对应的历史执行，未发送新的手机动作。';
        error.value='';
        if(matches[0]?.run)emit('run-created',matches[0].run);
    }catch(cause){
        if(!disposed&&n===epoch)error.value='旧请求只读核对失败：'+automationErrorMessage(cause);
    }finally{checking.value=false;}
}
function openCard(script?:AutomationScript){
    if(!targetDevice.value){
        error.value='暂无当前账号授权的可用手机，请返回工作台选择设备。';return;
    }
    emit('to-card',targetDevice.value,script?.id,script?.revision);
}
watch(()=>props.ownerId,()=>{
    ++epoch;scripts.value=[];pendingReceipts.value=[];notice.value='';error.value='';
    checkHistoricalReceipts();void reload();
},{immediate:true});
watch(()=>props.runDeviceSnapshots.map(d=>d.id).join(','),checkHistoricalReceipts);
onBeforeUnmount(()=>{disposed=true;++epoch;});
</script>

<template>
    <section class="legacy-script-library" :aria-label="text('旧脚本入口（只读）','Legacy scripts (read only)')">
        <header class="console-panel legacy-header">
            <div>
                <h2>{{ text('脚本管理已迁至手机原画面','Script editing moved to the original phone card') }}</h2>
                <p>此处仅保留旧链接、已保存脚本目录和历史请求只读核对，不再存在第二套步骤编辑器、正式写入入口或投屏播放器。</p>
                <small v-if="draftSyncLabel" class="draft-label">{{ draftSyncLabel }}{{ draftSyncMessage ? ' · '+draftSyncMessage : '' }}</small>
            </div>
            <Button :label="text('打开原手机脚本浮窗','Open script panel on phone card')"
                icon="pi pi-mobile" :disabled="!targetDevice" @click="openCard()" />
        </header>
        <p v-if="error" role="alert" class="legacy-error">{{ error }}</p>
        <p v-if="notice" role="status" class="legacy-notice">{{ notice }}</p>
        <div v-if="pendingReceipts.length" class="console-panel receipt-warning">
            <strong>旧版本未确认的试跑请求：{{ pendingReceipts.length }}</strong>
            <p>只核对后端历史；即便未检索到对应记录，也不允许推断动作未执行。</p>
            <Button outlined icon="pi pi-history" :loading="checking"
                label="从历史只读核对" @click="recoverOldRequests" />
        </div>
        <section class="console-panel">
            <div class="legacy-title">
                <h3>{{ text('已保存脚本（只读）','Saved scripts (read only)') }} · {{ scripts.length }}</h3>
                <Button text icon="pi pi-refresh" :loading="loading" :aria-label="text('刷新脚本列表','Refresh scripts')" @click="reload" />
            </div>
            <p v-if="targetDevice" class="legacy-tip">打开目标：{{ deviceName }}。切换脚本、克隆、删除、步骤编辑和 ▶ 运行均在这台手机的原画面浮窗进行；未同步草稿不会被此页覆盖。</p>
            <p v-else class="legacy-tip">没有可用手机卡片，脚本目录仅供查看。</p>
            <p v-if="!scripts.length&&!loading" class="legacy-tip">暂无正式脚本，可在原手机脚本浮窗中新建。</p>
            <ul class="legacy-list">
                <li v-for="script in scripts" :key="script.id" :class="{selected:script.id===draft.editingId}">
                    <div class="script-info">
                        <strong>{{ script.name }} <span v-if="script.id===draft.editingId">✓</span></strong>
                        <small>{{ script.steps.length }} 步 · 修订 {{ script.revision }} · {{ script.description }}</small>
                    </div>
                    <Button size="small" outlined icon="pi pi-external-link"
                        :disabled="!targetDevice" label="在原卡片打开" @click="openCard(script)" />
                </li>
            </ul>
        </section>
    </section>
</template>

<style scoped>
.legacy-script-library{display:grid;gap:14px;max-width:1180px;min-width:0}
.legacy-header{display:flex;gap:12px;justify-content:space-between;align-items:center;padding:18px;flex-wrap:wrap}
.legacy-header>div{flex:1;min-width:240px}
.legacy-header h2{font-size:16px;margin:0 0 8px}
.legacy-header p,.legacy-tip,.receipt-warning p{font-size:12px;color:var(--console-muted);line-height:1.6}
.draft-label{font-size:11px;color:var(--console-muted)}
.legacy-error{color:var(--p-red-500);font-size:12px}
.legacy-notice{color:var(--p-primary-color);font-size:12px}
.receipt-warning,.legacy-script-library>.console-panel{padding:16px}
.legacy-title{display:flex;align-items:center;justify-content:space-between;gap:8px}
.legacy-title h3{font-size:14px;margin:0}
.legacy-list{list-style:none;padding:0;margin:10px 0 0;display:grid;gap:5px}
.legacy-list li{display:flex;align-items:center;justify-content:space-between;gap:12px;border:1px solid var(--console-border);border-radius:8px;padding:10px;min-width:0;flex-wrap:wrap}
.legacy-list li.selected{border-color:var(--p-primary-color)}
.script-info{display:grid;gap:6px;flex:1;min-width:180px}
.script-info strong{overflow-wrap:anywhere;font-size:13px}
.script-info strong span{color:var(--p-primary-color)}
.script-info small{color:var(--console-muted);font-size:11px;overflow-wrap:anywhere}
</style>
