<script setup lang="ts">
/** Only the hosting PhoneCard may start/stop or display actions in this panel. */
import { computed, onBeforeUnmount, reactive, watch } from 'vue';
import { Button, InputText, Message, Tag } from 'primevue';
import type { FloatingBatchState } from '../../modules/useFloatingBatchRun';
import type { AutomationStepResult } from '../../types/automation';

const props=defineProps<{
    ownerId:number;
    deviceId:string;
    localName?:string;
    localAvailable?:boolean;
    blockReason?:string;
    localMessage?:string;
    stepsCount:number;
    parameterNames:string[];
    runnerAvailable:boolean;
    draftReady:boolean;
    state:FloatingBatchState;
}>();
const emit=defineEmits<{
    startLocal:[parameters:Record<string,string>];
    refresh:[];reconcile:[];newRun:[];stopDevice:[deviceId:string];
}>();
const values=reactive<Record<string,string>>({});
// Run state belongs to the workbench. Never expose another phone's run,
// result, status, cancellation, or request ID inside this phone's console.
const mine=computed(()=>props.state.owner===props.ownerId &&
    props.state.selected.includes(props.deviceId));
const active=computed(()=>['preparing','submitting','running','cancel_requested'].includes(props.state.phase));
const unresolved=computed(()=>mine.value &&
    (!!props.state.pendingReceipt || props.state.phase==='unknown'));
const canRun=computed(()=>props.runnerAvailable && props.draftReady && props.stepsCount>0 &&
    !active.value && !props.state.pendingReceipt && props.state.phase!=='unknown');
const localAllowed=computed(()=>canRun.value && !!props.localAvailable);
const missingParameters=computed(()=>props.parameterNames.some(name=>!values[name]));
const localRow=computed(()=>mine.value ? props.state.devices[props.deviceId] : undefined);
const deviceRun=computed(()=>mine.value ? props.state.run?.devices.find(x=>x.device_id===props.deviceId):undefined);
const failures=computed(()=>deviceRun.value?.steps.filter(s=>['failed','interrupted','outcome_unknown'].includes(s.state))??[]);
function errorText(value:string|{code?:string;message?:string}|undefined){
    if(typeof value==='string')return value;
    return value?[value.code,value.message].filter(Boolean).join(' · '):'';
}
function stepNumber(id:string):number{
    return (props.state.run?.script_snapshot.steps.findIndex(x=>x.id===id)??-1)+1;
}
function stepError(step:AutomationStepResult):string{
    return [errorText(step.error),step.message].filter(Boolean).filter((x,i,a)=>a.indexOf(x)===i).join('；')
        || '执行器未提供该步骤的具体失败原因';
}
const localError=computed(()=>props.localMessage || (mine.value ? props.state.error : ''));
const runOutcome=computed(()=>!mine.value?'idle':props.state.phase==='completed'
    ? (props.state.run?.devices.find(device=>device.device_id===props.deviceId)?.state ?? 'outcome_unknown')
    : props.state.phase);
const outcomeSeverity=computed<'success'|'danger'|'warn'|'secondary'>(()=>{
    if(runOutcome.value==='completed'||runOutcome.value==='succeeded')return 'success';
    if(runOutcome.value==='failed')return 'danger';
    if(runOutcome.value==='partial_failed'||runOutcome.value==='outcome_unknown'||runOutcome.value==='unknown')return 'warn';
    return 'secondary';
});
function collectParameters():Record<string,string>{
    const params:Record<string,string>={};
    for(const name of props.parameterNames)params[name]=values[name]??'';
    return params;
}
function startLocal(){
    if(!localAllowed.value || missingParameters.value)return;
    // Only a local event: the parent freezes [this deviceId], concurrency=1.
    emit('startLocal',collectParameters());
}
function stopLocal(){
    if(!mine.value || !props.state.run ||
        !props.state.run.devices.some(device=>device.device_id===props.deviceId))return;
    emit('stopDevice',props.deviceId);
}
watch(()=>props.ownerId,()=>{for(const key of Object.keys(values))delete values[key];});
onBeforeUnmount(()=>{for(const key of Object.keys(values))delete values[key];});
const label=(state:string)=>{
    const labels:Record<string,string>={
        idle:'未运行',preparing:'本机准备中',submitting:'提交本机任务中',
        running:'本机执行中',cancel_requested:'已请求停止，等待确认',
        completed:'执行成功',partial_failed:'部分步骤失败',failed:'执行失败或准备失败',
        interrupted:'执行中断',unknown:'旧请求结果未知',
        queued:'排队中',viewing:'原画面检查',suspending:'核验原会话',ready:'已准备',
        succeeded:'成功',cancelled:'已取消',outcome_unknown:'结果未知',cleanup_pending:'清理待确认',
        restoring_manual:'正在恢复手控',manual_restored:'已恢复手控',manual_restore_failed:'手控恢复失败',
        binding:'核对手机绑定',adb_check:'检查ADB',connecting:'连接ADB',
        helper_check:'检查控件组件',helper_start:'启动UiAutomator2',
        helper_wait:'初始化控件中',failed_preparation:'初始化失败',
    };
    return labels[state]||state;
};
const terminal=(status:string)=>['completed','succeeded','failed','cancelled','interrupted','partial_failed'].includes(status);
</script>

<template>
    <section class="batch-box" aria-label="本机完整脚本运行">
        <div class="batch-head">
            <strong>本机运行全流程</strong>
            <Tag :value="label(runOutcome)" :severity="outcomeSeverity" />
        </div>
        <small>仅以当前 {{ localName || '手机' }} 为执行目标，不采用工作台勾选的其他设备，也不会向其他手机发送执行或停止指令。保留原实时画面。</small>
        <div class="batch-actions">
            <Button label="▶ 本机运行全流程" size="small" icon="pi pi-play"
                :disabled="!localAllowed || missingParameters" @click="startLocal" />
        </div>
        <div v-if="parameterNames.length" class="batch-parameters">
            <label v-for="name in parameterNames" :key="name">本次运行参数 · {{ name }}
                <InputText v-model="values[name]" type="password" autocomplete="off"
                    :disabled="active" placeholder="不会保存进执行记录" />
            </label>
        </div>
        <Message v-if="!draftReady && stepsCount>0" severity="warn" :closable="false">
            {{ blockReason || '请先保存已确认的步骤与工作草稿，等待服务器确认后再运行。' }}
        </Message>
        <Message v-if="draftReady && !runnerAvailable" severity="warn" :closable="false">
            执行器未配置或不可用，请先到自动化中心设置页检查执行器。
        </Message>
        <Message v-if="localError" severity="warn" :closable="false">{{ localError }}</Message>
        <section v-if="mine && failures.length" class="batch-failures" aria-label="本机失败步骤与错误">
            <strong>执行失败详情 · {{ failures.length }} 项</strong>
            <details v-for="step in failures" :key="step.step_id" open>
                <summary>第 {{ stepNumber(step.step_id) }} 步 · {{ step.state }}</summary>
                <p>{{ stepError(step) }}</p><small>步骤 ID：{{ step.step_id }}</small>
            </details>
        </section>
        <section v-else-if="mine && deviceRun?.error" class="batch-failures">
            <strong>设备执行错误</strong><p>{{errorText(deviceRun.error)}}</p>
        </section>
        <Message v-if="!mine && active" severity="secondary" :closable="false">
            当前工作台已有未完成任务，本机暂不能提交新的执行请求。
        </Message>
        <div v-if="mine && state.requestId" class="batch-receipt">
            <small>本机请求：<code>{{ state.requestId }}</code></small>
            <small v-if="state.run">运行：<code>{{ state.run.id }}</code></small>
        </div>
        <div v-if="localRow" class="batch-devices" aria-live="polite">
            <div class="batch-device">
                <strong>{{ localName || deviceId }}</strong>
                <small>{{ label(localRow.phase || 'queued') }}</small>
                <small>{{ localRow.message }}</small>
                <small v-if="localRow.sessionId">原视频会话：<code>{{ localRow.sessionId.slice(0,8) }}</code></small>
                <small v-if="localRow.mediaState==='lost'" class="media-alert" role="alert">
                    {{ localRow.mediaMessage }}
                </small>
                <small v-else-if="localRow.mediaState==='live'">视频：原画面正常</small>
                <small v-if="localRow.stepId">当前步骤：{{ localRow.stepId }}</small>
                <small v-if="localRow.runState && !terminal(localRow.runState)">尚未确认完成或清理</small>
                <Button v-if="mine && state.run && state.phase==='running' && localRow.runState &&
                    ['dispatch_pending','queued','accepted','starting','running'].includes(localRow.runState) &&
                    localRow.phase!=='cancel_requested'"
                    label="停止本机" icon="pi pi-stop" severity="danger" outlined size="small"
                    @click="stopLocal" />
            </div>
        </div>
        <div class="batch-footer" v-if="mine && (state.run || unresolved)">
            <Button v-if="state.run" label="刷新本机状态" icon="pi pi-refresh" outlined size="small" @click="emit('refresh')" />
            <Button v-if="unresolved" label="查询本机原请求" icon="pi pi-history"
                size="small" outlined @click="emit('reconcile')" />
            <Button v-if="unresolved" label="结束旧请求跟踪，准备新运行" icon="pi pi-replay"
                size="small" outlined @click="emit('newRun')" />
            <small v-if="unresolved">先核查旧任务及设备占用；正在执行的任务不会重发。</small>
        </div>
        <small class="batch-hint">手控开关保持原状态。脚本执行期间由执行器独占当前手机动作；停止只是请求，需等待服务端确认实际结果。</small>
    </section>
</template>

<style scoped>
.batch-box{display:grid;gap:8px;margin:7px 0;padding:10px;border:1px solid var(--console-border);border-radius:9px;min-width:0;background:var(--console-panel-raised)}
.batch-head{display:flex;justify-content:space-between;gap:6px;align-items:center;font-size:12px}
.batch-box small{font-size:10px;line-height:1.45;color:var(--console-muted);overflow-wrap:anywhere}
.batch-actions,.batch-footer{display:flex;align-items:center;gap:6px;flex-wrap:wrap}
.batch-parameters{display:grid;gap:6px}.batch-parameters label{display:grid;gap:4px;font-size:11px}
.batch-receipt{display:grid;gap:2px}.batch-receipt code{word-break:break-all;font-size:9px}
.batch-devices{display:grid;gap:5px;max-height:150px;overflow-y:auto}
.batch-device{display:grid;gap:2px;border-top:1px solid var(--console-border);padding-top:5px}
.batch-device strong{font-size:11px}
.batch-hint{border-top:1px solid var(--console-border);padding-top:7px}
.batch-device .media-alert{color:var(--p-red-500)}
.batch-failures{display:grid;gap:6px;padding:8px;background:color-mix(in srgb,var(--p-red-500) 7%,transparent);border:1px solid var(--p-red-500);border-radius:8px}
.batch-failures>strong{color:var(--p-red-500);font-size:12px}
.batch-failures details{font-size:11px;border-top:1px solid var(--console-border);padding-top:5px}
.batch-failures summary{cursor:pointer;font-weight:600}
.batch-failures p{white-space:pre-wrap;overflow-wrap:anywhere;margin:5px 0;font-size:11px}
</style>
