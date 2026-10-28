<script setup lang="ts">
/** T43–T45: display only the authoritative batch state; never embed video. */
import { computed, onBeforeUnmount, reactive, ref, watch } from 'vue';
import { Button, InputText, Message, Tag } from 'primevue';
import type { FloatingBatchState } from '../../modules/useFloatingBatchRun';
const props=defineProps<{
    ownerId:number;
    selectedCount:number;
    stepsCount:number;
    parameterNames:string[];
    runnerAvailable:boolean;
    draftReady:boolean;
    state:FloatingBatchState;
}>();
const emit=defineEmits<{
    start:[parameters:Record<string,string>,concurrency:number];
    refresh:[]; stop:[]; reconcile:[]; stopDevice:[deviceId:string];
}>();
const concurrency=ref(3);
const values=reactive<Record<string,string>>({});
const active=computed(()=>['preparing','submitting','running','cancel_requested'].includes(props.state.phase));
const unresolved=computed(()=>!!props.state.pendingReceipt || props.state.phase==='unknown');
const allowed=computed(()=>props.runnerAvailable && props.draftReady && props.stepsCount>0 &&
    props.selectedCount>0 && props.selectedCount<=20 && !active.value && !unresolved.value);
const rows=computed(()=>props.state.selected.map(id=>({id,...props.state.devices[id]})));
function start(){
    if(!allowed.value)return;
    const params:Record<string,string>={};
    for(const name of props.parameterNames)params[name]=values[name]??'';
    emit('start',params,Math.max(1,Math.floor(Number(concurrency.value)||1)));
    // Secret parameters stay only in the input until the parent freezes them.
}
watch(()=>props.ownerId,()=>{for(const key of Object.keys(values))delete values[key];});
onBeforeUnmount(()=>{for(const key of Object.keys(values))delete values[key];});
const label=(state:string)=>{
    const texts:Record<string,string>={
        idle:'未运行',preparing:'独立准备中',submitting:'提交整批任务中',
        running:'设备并行执行中',cancel_requested:'已请求停止，等待确认',
        completed:'执行已结束',failed:'准备/提交失败',unknown:'结果未知，禁止再次提交',
        queued:'排队中',viewing:'原画面检查',suspending:'只读移交',ready:'已准备',
        succeeded:'成功',cancelled:'已取消',outcome_unknown:'结果未知',cleanup_pending:'清理待确认',
    };
    return texts[state]||state;
};
const terminal=(status:string)=>['completed','succeeded','failed','cancelled','interrupted','partial_failed'].includes(status);
</script>

<template>
    <section class="batch-box" aria-label="选中手机完整脚本运行">
        <div class="batch-head">
            <strong>运行全流程</strong>
            <Tag :value="label(state.phase)" :severity="state.phase==='completed'?'success':
                state.phase==='unknown'||state.phase==='failed'?'warn':'secondary'" />
        </div>
        <small>只向工作台勾选的 {{ selectedCount }} 台手机提交。每机继续观看各自卡片原实时画面；无第二条投屏。</small>
        <div class="batch-actions">
            <label>并行数
                <input v-model.number="concurrency" type="number" min="1" step="1"
                    :disabled="active" aria-label="任务并发设备数" />
            </label>
            <Button label="▶ 运行所选完整脚本" size="small" icon="pi pi-play"
                :disabled="!allowed || parameterNames.some(name=>!values[name])" @click="start" />
        </div>
        <div v-if="parameterNames.length" class="batch-parameters">
            <label v-for="name in parameterNames" :key="name">本次运行参数 · {{ name }}
                <InputText v-model="values[name]" type="password" autocomplete="off"
                    :disabled="active" placeholder="不会保存进执行记录" />
            </label>
        </div>
        <Message v-if="selectedCount>20" severity="warn" :closable="false">
            当前后端一次最多 20 台。请缩小选中范围；不会自动拆成多次执行。
        </Message>
        <Message v-if="!draftReady && stepsCount>0" severity="warn" :closable="false">
            工作草稿仍有未保存编辑或同步冲突，先完成保存再执行当前不可变快照。
        </Message>
        <Message v-if="state.error" severity="warn" :closable="false">{{ state.error }}</Message>
        <div v-if="state.requestId" class="batch-receipt">
            <small>请求：<code>{{ state.requestId }}</code></small>
            <small v-if="state.run">运行：<code>{{ state.run.id }}</code></small>
        </div>
        <div v-if="rows.length" class="batch-devices" aria-live="polite">
            <div v-for="device in rows" :key="device.id" class="batch-device">
                <strong>{{ device.name || device.id }}</strong>
                <small>{{ label(device.phase || 'queued') }}</small>
                <small>{{ device.message }}</small>
                <small v-if="device.sessionId">原视频会话：<code>{{ device.sessionId.slice(0,8) }}</code></small>
                <small v-if="device.mediaState==='lost'" class="media-alert" role="alert">
                    {{ device.mediaMessage }}
                </small>
                <small v-else-if="device.mediaState==='live'">视频：原画面正常</small>
                <small v-if="device.stepId">当前步骤：{{ device.stepId }}</small>
                <small v-if="device.runState && !terminal(device.runState)">尚未确认完成或清理</small>
                <Button v-if="state.run && state.phase==='running' && device.runState &&
                    ['dispatch_pending','queued','accepted','starting','running'].includes(device.runState) &&
                    device.phase!=='cancel_requested'"
                    label="停止此手机" icon="pi pi-stop" severity="danger" outlined size="small"
                    @click="emit('stopDevice',device.id)" />
            </div>
        </div>
        <div class="batch-footer" v-if="state.run || unresolved">
            <Button v-if="state.run" label="刷新状态" icon="pi pi-refresh" outlined size="small" @click="emit('refresh')" />
            <Button v-if="state.run && (state.phase==='running'||state.phase==='cancel_requested')"
                label="请求停止全部" icon="pi pi-stop-circle" severity="danger" outlined size="small"
                :disabled="state.phase==='cancel_requested'" @click="emit('stop')" />
            <Button v-if="unresolved" label="只读查询原请求" icon="pi pi-history"
                size="small" outlined @click="emit('reconcile')" />
        </div>
        <small class="batch-hint">停止是请求，不等于动作已结束；不自动重发未知操作。单机停止只撤回对应 run/device，其它手机继续执行；实际停止和清理状态由服务器返回。</small>
    </section>
</template>

<style scoped>
.batch-box{display:grid;gap:8px;margin:7px 0;padding:10px;border:1px solid var(--console-border);border-radius:9px;min-width:0;background:var(--console-panel-raised)}
.batch-head{display:flex;justify-content:space-between;gap:6px;align-items:center;font-size:12px}
.batch-box small{font-size:10px;line-height:1.45;color:var(--console-muted);overflow-wrap:anywhere}
.batch-actions,.batch-footer{display:flex;align-items:center;gap:6px;flex-wrap:wrap}
.batch-actions label{font-size:10px;display:flex;align-items:center;gap:4px}
.batch-actions input{width:48px;min-width:0;padding:4px;background:var(--console-panel);color:inherit;border:1px solid var(--console-border);border-radius:4px}
.batch-parameters{display:grid;gap:6px}.batch-parameters label{display:grid;gap:4px;font-size:11px}
.batch-receipt{display:grid;gap:2px}.batch-receipt code{word-break:break-all;font-size:9px}
.batch-devices{display:grid;gap:5px;max-height:150px;overflow-y:auto}
.batch-device{display:grid;gap:2px;border-top:1px solid var(--console-border);padding-top:5px}
.batch-device strong{font-size:11px}
.batch-hint{border-top:1px solid var(--console-border);padding-top:7px}
.batch-device .media-alert{color:var(--p-red-500)}
</style>
