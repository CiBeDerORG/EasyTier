<script setup lang="ts">
/** T14: one selected step on this phone; reuse T12/T13 and the ordinary /runs API. */
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Button, Checkbox, Dialog, InputText, Message, Tag } from 'primevue';
import { v4 as uuid } from 'uuid';
import AutomationApi, { AutomationApiError, automationErrorMessage } from '../../modules/automation-api';
import { useCardTrialPreparation, type CardTrialPhase } from '../../modules/useCardTrialPreparation';
import { releaseOwnedTrialPreviews, type TrialPreviewHandle } from '../../modules/automation-run-handoff';
import { useCardRunStatus } from '../../modules/useCardRunStatus';
import { makeSingleStepRun, stepTrialSafety } from '../../modules/automation-single-step';
import { cardStepLabels } from '../../modules/card-step-projection';
import {
    reviewAutomationDeviceCompatibility, compatibilityStillCurrent,
    statelessCompatibility, requiresReuseHierarchy,
    type DeviceCompatibilityReview,
} from '../../modules/automation-device-compatibility';
import type { AutomationDevice, AutomationRunInput, AutomationStep } from '../../types/automation';
import {
    readTrialReceipt, reserveTrialReceipt, clearTrialReceipt, findTrialRun,
    type TrialSubmissionReceipt,
} from '../../modules/automation-trial-receipt';

const props=defineProps<{
    visible: boolean;
    ownerId: number;
    device: AutomationDevice;
    step: AutomationStep | null;
    previewHandle: () => TrialPreviewHandle;
    runnerAvailable: boolean;
    blocked?: boolean;
}>();
const emit=defineEmits<{ close: []; history: [runId: string]; repair: [runId: string, stepId: string] }>();
const api=new AutomationApi();
const preparation=useCardTrialPreparation(api);
const result=useCardRunStatus(api);
const currentRun=result.run;
const runView=result.view;
const runError=result.error;
const runPolling=result.polling;
const repairableRun=computed(()=>!!currentRun.value && !!runView.value &&
    ['failed','interrupted','outcome_unknown'].includes(runView.value.stepState));
type PanelPhase = 'idle' | 'releasing' | 'preparing' | 'ready' | 'submitting' | 'observing' | 'failed' | 'unknown';
const phase=ref<PanelPhase>('idle');
const warning=ref('');
const consent=ref(false);
const compatibilityConsent=ref(false);
const compatibilityReview=ref<DeviceCompatibilityReview | null>(null);
const parameterValue=ref('');
const frozen=ref<AutomationStep | null>(null);
const targetFingerprint=ref('');
const pending=ref<AutomationRunInput | null>(null);
const uncertain=ref(false);
const unresolvedReceipt=ref<TrialSubmissionReceipt|null>(null);
const receiptFailure=ref('');
const checkingReceipt=ref(false);
function loadReceipt() {
    try {
        unresolvedReceipt.value=readTrialReceipt(window.sessionStorage,props.ownerId,props.device.id);
        receiptFailure.value='';
    }catch(cause){
        unresolvedReceipt.value=null;
        receiptFailure.value='不能安全读取本页历史试跑请求：'+automationErrorMessage(cause);
    }
}
let epoch=0;
let disposed=false;
const busy=computed(()=>['releasing','preparing','submitting'].includes(phase.value));
const prepKey=computed(()=>props.ownerId+':'+props.device.id);
const prepProgress=computed(()=>preparation.state[prepKey.value]);
const safety=computed(()=>frozen.value ? stepTrialSafety(frozen.value) : 'unsupported');
const parameter=computed(()=>frozen.value?.type==='input_text' ? frozen.value.parameter : undefined);
const usable=computed(()=>!!frozen.value && !!frozen.value.id &&
    safety.value!=='unsupported' && !(frozen.value.type==='key' && !frozen.value.key) &&
    !(frozen.value.type==='input_text' && !frozen.value.parameter));
const started=computed(()=>!!result.run.value || !!pending.value);
const label=computed(()=>frozen.value ? cardStepLabels[frozen.value.type] : '未选择步骤');
const preparePhases:Record<CardTrialPhase,string>={
    queued:'准备排队中',binding:'核对设备绑定',adb_check:'查询 ADB',connecting:'连接 ADB',
    helper_check:'检查控件组件',helper_start:'提交控件初始化',helper_wait:'初始化控件中',
    ready:'准备就绪',failed:'准备失败',unknown:'准备结果未知',cancelled:'已停止准备',
};
function snapshot(step:AutomationStep):AutomationStep {
    return JSON.parse(JSON.stringify(step)) as AutomationStep;
}
function clearSecrets(){parameterValue.value='';}
function resetForOpen(){
    loadReceipt();
    if(receiptFailure.value || unresolvedReceipt.value) {
        result.reset();
        pending.value=null;uncertain.value=true;consent.value=false;
        compatibilityReview.value=null;compatibilityConsent.value=false;
        clearSecrets();phase.value='unknown';
        frozen.value=props.step ? snapshot(props.step) : null;
        targetFingerprint.value=props.device.target_fingerprint??'';
        warning.value=receiptFailure.value ||
            '发现上一条试跑请求尚未核实（请求编号 '+unresolvedReceipt.value!.requestId+
            '）。只能先查询执行记录并核对手机现场，禁止产生第二条动作。';
        return;
    }
    if(pending.value && uncertain.value) {
        // Preserve request_id/body but never replay it on a DIFFERENT step.
        phase.value='unknown';
        warning.value=pending.value.steps?.[0]?.id===props.step?.id
            ? '上一条试跑请求结果未确认。只能用相同请求编号人工重查，不会自动创建新动作。'
            : '其他步骤的试跑请求结果仍未确认。请返回原步骤核实记录，不能在这里重新执行。';
        return;
    }
    result.reset();pending.value=null;uncertain.value=false;consent.value=false;
    compatibilityReview.value=null;compatibilityConsent.value=false;
    clearSecrets();phase.value='idle';warning.value='';
    frozen.value=props.step ? snapshot(props.step) : null;
    targetFingerprint.value=props.device.target_fingerprint ?? '';
}
function valid(mine:number, owner:number, deviceId:string, original:string):boolean {
    return !disposed && props.visible && mine===epoch &&
        props.ownerId===owner && props.device.id===deviceId &&
        props.device.target_fingerprint===targetFingerprint.value &&
        !!props.step && JSON.stringify(props.step)===original && !props.blocked;
}
function checkReadyToPrepare(): string | null {
    if(!props.runnerAvailable)return '执行器当前不可用。';
    if(props.blocked)return '请先退出录制、群控或当前未保存步骤编辑。';
    if(!targetFingerprint.value || !props.device.connection.enabled || props.device.availability_reason)
        return '目标设备绑定无效，请检查 ADB 服务。';
    if(!usable.value)return '此步骤尚未配置完整、是条件分支节点或包含不安全的字面输入。';
    return null;
}
async function prepare(){
    loadReceipt();
    if(receiptFailure.value || unresolvedReceipt.value) {
        phase.value='unknown';
        warning.value=receiptFailure.value || '该手机尚有未核实的试跑请求，请先按请求编号只读查询执行记录。';
        return;
    }
    if(busy.value || pending.value || result.run.value)return;
    const problem=checkReadyToPrepare();
    if(problem){warning.value=problem;return;}
    const mine=++epoch,owner=props.ownerId,id=props.device.id;
    const source=JSON.stringify(props.step);
    const chosen=frozen.value!;
    warning.value='';consent.value=false;
    compatibilityReview.value=null;compatibilityConsent.value=false;
    try {
        phase.value='releasing';
        const owned=new Set([id]); // This card's own handle ONLY, never a foreign session.
        await releaseOwnedTrialPreviews(api,{
            ownerId:owner,requestedDeviceIds:[id],ownedPreviewIds:owned,
            findPreview:()=>props.previewHandle(),
            onProgress:msg=>{if(valid(mine,owner,id,source))warning.value=msg;},
            stillCurrent:()=>valid(mine,owner,id,source),
            deviceName:()=>props.device.name,
        });
        if(!valid(mine,owner,id,source))return;
        // Always re-read the device after handoff. T12 will run a second
        // authorization/binding check before any helper side effects.
        const fresh=await api.devices(owner);
        if(!valid(mine,owner,id,source))return;
        const row=fresh.items.find(item=>item.id===id);
        if(!row || row.owner_user_id!==owner || row.target_fingerprint!==targetFingerprint.value ||
            row.active_session || row.run_occupied)
            throw new Error('设备会话、目标绑定或占用状态已变化，不能自动准备。');
        phase.value='preparing';warning.value='';
        const prepared=await preparation.run({
            ownerId:owner,deviceId:id,targetFingerprint:targetFingerprint.value,steps:[chosen],
        });
        if(!valid(mine,owner,id,source))return;
        if(prepared?.phase==='ready'){
            // Prepared helper/ADB is not evidence that saved coordinates fit
            // this phone. Native-only steps do not require UiAutomator2.
            const latestDevice=(await api.devices(owner)).items.find(item=>item.id===id);
            if(!valid(mine,owner,id,source))return;
            if(!latestDevice)throw new Error('无法确认当前手机授权及绑定。');
            const checked=requiresReuseHierarchy([chosen])
                ? reviewAutomationDeviceCompatibility(owner,latestDevice,
                    await api.hierarchy(owner,id),[chosen])
                : statelessCompatibility(owner,latestDevice,[chosen]);
            if(!valid(mine,owner,id,source))return;
            compatibilityReview.value=checked;
            if(checked.status==='blocked'){
                phase.value='failed';
                warning.value='原步骤的坐标与当前手机不匹配。请重新拾取或修改步骤，执行已阻止。';
            }else {phase.value='ready';warning.value='';}
        }
        else {
            phase.value=prepared?.phase==='unknown'?'unknown':'failed';
            warning.value=prepared?.message || '无法确认设备准备状态。';
        }
    }catch(cause){
        if(valid(mine,owner,id,source)){
            phase.value='failed';warning.value=automationErrorMessage(cause);
        }
    }
}
async function submit(){
    loadReceipt();
    if(receiptFailure.value || unresolvedReceipt.value) {
        phase.value='unknown';warning.value=receiptFailure.value || '旧试跑尚未核实，禁止新请求。';return;
    }
    if(phase.value!=='ready' || pending.value || !frozen.value || !usable.value)return;
    if(!compatibilityReview.value || compatibilityReview.value.status==='blocked'){
        warning.value='当前手机兼容性核验尚未通过，不能执行此步骤。';return;
    }
    if(compatibilityReview.value.status==='confirm' && !compatibilityConsent.value){
        warning.value='请确认当前手机的坐标/定位风险后再执行。';return;
    }
    if(safety.value==='confirm_required' && !consent.value) {
        warning.value='请明确确认此步骤可能修改手机状态或保存截图证据。';return;
    }
    if(parameter.value && (!parameterValue.value ||
        new TextEncoder().encode(parameterValue.value).length>2000)) {
        warning.value='请输入本次试跑参数（最多 2000 字节）。';return;
    }
    const owner=props.ownerId,id=props.device.id,mine=++epoch;
    const original=JSON.stringify(props.step);
    const payload=makeSingleStepRun(frozen.value,id,uuid(),
        parameter.value ? {[parameter.value]:parameterValue.value} : {},consent.value);
    if(!payload){warning.value='步骤不支持脱离上下文单独执行。';return;}
    pending.value=payload; // Freeze request ID and body across uncertain HTTP outcomes.
    phase.value='submitting';warning.value='';
    try{
        if(!valid(mine,owner,id,original))throw new Error('提交前账号、设备或脚本步骤发生变化。');
        const latest=await api.devices(owner);
        if(!valid(mine,owner,id,original))return;
        const row=latest.items.find(item=>item.id===id);
        if(!row || row.owner_user_id!==owner || !row.connection.enabled ||
            row.availability_reason || row.target_fingerprint!==targetFingerprint.value ||
            row.active_session || row.run_occupied)
            throw new Error('设备不再空闲或绑定变更，未提交试跑。');
        const rechecked=requiresReuseHierarchy([frozen.value])
            ? reviewAutomationDeviceCompatibility(owner,row,
                await api.hierarchy(owner,id),[frozen.value])
            : statelessCompatibility(owner,row,[frozen.value]);
        if(!valid(mine,owner,id,original))return;
        if(rechecked.status==='blocked'){
            pending.value=null;compatibilityReview.value=rechecked;
            compatibilityConsent.value=false;phase.value='failed';
            warning.value='提交时手机画面尺寸不符，已中止本次执行。';return;
        }
        if(!compatibilityStillCurrent(compatibilityReview.value,owner,row,[frozen.value]) ||
            JSON.stringify(rechecked.issues)!==JSON.stringify(compatibilityReview.value?.issues)){
            pending.value=null;compatibilityReview.value=rechecked;
            compatibilityConsent.value=false;phase.value='ready';
            warning.value='手机页面/定位状态变化，请重新核对兼容性风险并确认。';return;
        }
        clearSecrets();
        await sendFrozen(mine,owner,id,original,payload);
    }catch(cause){
        if(valid(mine,owner,id,original)){
            pending.value=null;phase.value='failed';warning.value=automationErrorMessage(cause);
        }
    }
}
async function sendFrozen(mine:number,owner:number,id:string,original:string,payload:AutomationRunInput){
    const proof:TrialSubmissionReceipt={
        ownerId:owner,deviceId:id,stepId:payload.steps?.[0]?.id??'',requestId:payload.request_id,
        createdAt:new Date().toISOString(),
    };
    let dispatched=false;
    try {
        // Persist only UUID and device/step metadata BEFORE sending the POST.
        // Never serialize the POST body, inline parameters, selectors or text.
        reserveTrialReceipt(window.sessionStorage,proof);
        unresolvedReceipt.value=proof;receiptFailure.value='';
        dispatched=true;
        // Server create_run() still owns definitive BEGIN IMMEDIATE admission.
        const created=await api.createRun(owner,payload);
        if(!valid(mine,owner,id,original))return; // receipt remains until read-only reconciliation
        if(!created.id || !created.devices.some(d=>d.device_id===id)) {
            uncertain.value=true;phase.value='unknown';
            warning.value='服务器响应没有可核实的目标运行记录；请求身份已保留，请只读查询历史。';return;
        }
        clearTrialReceipt(window.sessionStorage,proof);
        unresolvedReceipt.value=null;
        pending.value=null;uncertain.value=false;phase.value='observing';
        result.observe(owner,created,id,payload.steps?.[0]?.id??'');
        warning.value='';
    }catch(cause){
        if(!valid(mine,owner,id,original))return;
        // Only explicit server validation/admission errors prove no run was
        // created. A generic network error, 408, 429 or 5xx never does.
        // 401/403 after an earlier accepted attempt, or 409 on an
        // idempotency conflict, does NOT prove the original run never existed.
        // Clear only explicit input validation failures; every other error
        // keeps the persisted guard for manual read-only reconciliation.
        const definitive=dispatched && cause instanceof AutomationApiError &&
            [400,422].includes(cause.status);
        if(definitive){
            try{clearTrialReceipt(window.sessionStorage,proof);unresolvedReceipt.value=null;}
            catch{receiptFailure.value='请求虽被拒绝，但未能清理本地保护凭据。';}
        }
        const unknown=dispatched && !definitive;
        uncertain.value=unknown;phase.value=unknown?'unknown':'failed';
        warning.value=(dispatched?'': '无法安全保存试跑请求身份，未发送动作：')+
            automationErrorMessage(cause)+
            (unknown?'。动作可能已被服务器接收；请按原请求编号只读核对，不能新建请求。':'');
        if(!unknown)pending.value=null;
    }
}
async function recheckTrialReceipt(){
    if(checkingReceipt.value)return;
    loadReceipt();
    if(!unresolvedReceipt.value || receiptFailure.value)return;
    const proof=unresolvedReceipt.value;
    checkingReceipt.value=true;
    try{
        // LIST is read-only. Never send a new POST to learn whether an old
        // UUID was admitted. For >100 runs or eventual consistency, keep
        // the guard and ask the operator to check server logs.
        const listed=await api.listRuns(proof.ownerId);
        const matched=findTrialRun(listed.items,proof);
        if(!matched){
            warning.value='服务器近期运行记录未找到请求 '+proof.requestId+
                '；不能据此认定动作没有发生，保护仍然生效。请核对设备现场和服务器日志。';
            return;
        }
        if(!matched.script_snapshot.steps.some(step=>step.id===proof.stepId))
            throw new Error('同一请求编号的历史步骤与保存的步骤身份不匹配，不能解除保护。');
        clearTrialReceipt(window.sessionStorage,proof);
        loadReceipt();
        if(props.ownerId!==proof.ownerId || props.device.id!==proof.deviceId || !props.visible)return;
        pending.value=null;uncertain.value=false;phase.value='observing';
        result.observe(proof.ownerId,matched,proof.deviceId,proof.stepId);
        warning.value='已通过只读历史记录找回原试跑；没有提交第二次动作。';
    }catch(cause){
        warning.value='查询旧试跑失败，保护仍然有效：'+automationErrorMessage(cause);
    }finally{checkingReceipt.value=false;}
}
async function recheckPreparation(){
    if(phase.value!=='unknown' || !!pending.value || !frozen.value)return;
    const owner=props.ownerId,id=props.device.id,mine=++epoch,original=JSON.stringify(props.step);
    phase.value='preparing';warning.value='只读复核此前准备状态，不会发起新的连接或初始化请求。';
    try {
        const state=await preparation.reconcile({
            ownerId:owner,deviceId:id,targetFingerprint:targetFingerprint.value,steps:[frozen.value],
        });
        if(!valid(mine,owner,id,original))return;
        if(state?.phase==='ready') {
            // A recovered preparation is NOT a recovered T16 device review.
            // This read-only check is mandatory before any new run request.
            const latestDevice=(await api.devices(owner)).items.find(item=>item.id===id);
            if(!valid(mine,owner,id,original))return;
            if(!latestDevice)throw new Error('原授权设备已消失，不能继续试跑。');
            const checked=requiresReuseHierarchy([frozen.value])
                ? reviewAutomationDeviceCompatibility(owner,latestDevice,
                    await api.hierarchy(owner,id),[frozen.value])
                : statelessCompatibility(owner,latestDevice,[frozen.value]);
            if(!valid(mine,owner,id,original))return;
            compatibilityReview.value=checked;
            compatibilityConsent.value=false;consent.value=false;
            if(checked.status==='blocked'){
                phase.value='failed';
                warning.value='设备准备已恢复，但屏幕坐标与当前手机不兼容；请重新拾取。';
            }else{
                phase.value='ready';
                warning.value=checked.status==='confirm'
                    ? '准备已恢复，请重新人工确认本手机的定位和坐标风险。'
                    : '准备状态及只读设备核验均已恢复。';
            }
        }else{
            compatibilityReview.value=null;
            phase.value=state?.phase==='unknown'?'unknown':'failed';
            warning.value=state?.message||'准备状态无法确认。';
        }
    }catch(cause){
        if(valid(mine,owner,id,original)){
            phase.value='unknown';warning.value=automationErrorMessage(cause);
        }
    }
}
function openHistory(){
    const id=result.run.value?.id;
    if(id)emit('history',id);
}
function openRepair(){
    const view=runView.value;
    if(repairableRun.value && view?.runId && view.stepId)
        emit('repair',view.runId,view.stepId);
}
function hide(){
    if(phase.value==='submitting' && unresolvedReceipt.value) {
        uncertain.value=true;phase.value='unknown';
        warning.value='请求已经发送或结果尚未确认。关闭弹窗不会解除试跑保护。';
    }
    ++epoch;
    if(phase.value==='preparing' || phase.value==='releasing')
        void preparation.cancel(props.ownerId,props.device.id);
    result.stop();
    clearSecrets();
    emit('close');
}
watch(()=>props.visible,open=>{
    ++epoch;
    if(open) {
        if(result.run.value && result.view.value?.stepId===props.step?.id &&
            result.view.value?.runId===result.run.value.id) result.resume();
        else resetForOpen();
    } else {
        if(phase.value==='preparing' || phase.value==='releasing')
            void preparation.cancel(props.ownerId,props.device.id);
        result.stop();clearSecrets();
    }
},{immediate:true});
watch(()=>[props.ownerId,props.device.id] as const,(_current, previous)=>{
    ++epoch;
    if(previous) void preparation.cancel(previous[0],previous[1]);
    result.reset();pending.value=null;uncertain.value=false;clearSecrets();
    compatibilityReview.value=null;compatibilityConsent.value=false;
    phase.value='idle';
});
onBeforeUnmount(()=>{
    disposed=true;++epoch;result.stop();preparation.dispose();clearSecrets();
});
</script>

<template>
    <Dialog :visible="visible" modal append-to="body" header="卡片单步骤试跑"
        :style="{width:'530px',maxWidth:'calc(100vw - 24px)'}"
        @update:visible="open => {if(!open)hide();}">
        <div class="card-trial-content">
            <div class="trial-source">
                <strong>{{ device.name }} · {{ label }}</strong>
                <small>仅试跑当前选择的 1 个步骤；设备 {{ device.id.slice(0,8) }}</small>
                <small>只读定位演示与真实试跑不同：试跑会在服务器上创建独立执行记录。</small>
            </div>
            <Message v-if="warning" severity="warn" :closable="false">{{ warning }}</Message>
            <Message v-if="!usable" severity="warn" :closable="false">
                条件分支节点、未设置动作或非参数化输入不能从此处直接试跑。请先编辑并保存步骤。
            </Message>
            <div class="trial-progress" aria-live="polite">
                <strong>设备准备</strong>
                <Tag :value="phase==='ready'?'就绪':phase==='unknown'?'结果未知':phase==='releasing'?'释放控制中':phase==='preparing'?'准备中':phase==='failed'?'失败':'尚未准备'"
                    :severity="phase==='ready'?'success':phase==='unknown'||phase==='failed'?'warn':'secondary'"/>
                <p v-if="prepProgress">{{ preparePhases[prepProgress.phase] }} · {{ prepProgress.message }}</p>
                <p v-else>准备会检查最新账号授权、ADB 连接与所需控件组件。不会自动进行无线 ADB 配对。</p>
            </div>
            <div v-if="compatibilityReview" class="trial-progress" aria-live="polite">
                <strong>手机匹配：{{ compatibilityReview.viewport.width ? compatibilityReview.viewport.width+' × '+compatibilityReview.viewport.height : '仅ADB操作' }}</strong>
                <Tag :value="compatibilityReview.status==='blocked'?'尺寸不兼容':compatibilityReview.status==='confirm'?'需要确认定位风险':'只读核验通过'"
                    :severity="compatibilityReview.status==='blocked'?'danger':compatibilityReview.status==='confirm'?'warn':'success'"/>
                <p v-for="item in compatibilityReview.issues" :key="item.stepId+item.message">
                    第 {{ item.stepNumber }} 步：{{ item.message }}
                </p>
                <label v-if="phase==='ready' && compatibilityReview.status==='confirm'" class="trial-check">
                    <Checkbox v-model="compatibilityConsent" binary input-id="card-trial-compatibility"/>
                    <span>我已核对当前手机的页面、定位和坐标风险，此确认仅用于本次试跑。</span>
                </label>
            </div>
            <div v-if="phase==='ready' && safety==='confirm_required'" class="trial-check">
                <Checkbox v-model="consent" input-id="card-trial-consent" binary />
                <label for="card-trial-consent">我确认此步骤可能操作手机或持久保存截图证据，仅执行本次所选步骤。</label>
            </div>
            <label v-if="phase==='ready' && parameter" class="param-input">
                本次输入参数：{{ parameter }}
                <InputText v-model="parameterValue" type="password" autocomplete="off"
                    placeholder="仅用于本次运行，不保存到脚本草稿"/>
            </label>
            <div v-if="runView" class="trial-progress" aria-live="polite">
                <strong>执行状态</strong>
                <Tag :value="runView.label"
                    :severity="runView.phase==='succeeded'?'success':runView.phase==='failed'?'danger':runView.phase==='unknown'?'warn':'info'"/>
                <p>执行记录：<code>{{ runView.runId }}</code></p>
                <p>步骤：<code>{{ runView.stepId }}</code></p>
                <p>设备状态：{{ runView.deviceState }} · 步骤状态：{{ runView.stepState }}</p>
                <p v-if="runView.detail">{{ runView.detail }}</p>
                <p v-if="runError">{{ runError }}</p>
            </div>
            <div class="trial-actions">
                <Button label="关闭" text size="small" @click="hide"/>
                <Button v-if="currentRun" label="刷新结果" outlined size="small"
                    :loading="runPolling" @click="result.refresh()"/>
                <Button v-if="repairableRun" label="定位修复本步骤" icon="pi pi-wrench" outlined size="small"
                    @click="openRepair"/>
                <Button v-if="currentRun" label="在执行记录查看" icon="pi pi-history" size="small"
                    @click="openHistory"/>
                <Button v-if="phase==='unknown' && !pending && !unresolvedReceipt && !receiptFailure"
                    label="只读复核准备状态" outlined size="small"
                    :disabled="busy" @click="recheckPreparation"/>
                <Button v-if="unresolvedReceipt || receiptFailure" label="只读查询旧请求"
                    outlined size="small" :loading="checkingReceipt"
                    :disabled="!!receiptFailure || checkingReceipt" @click="recheckTrialReceipt"/>
                <Button v-if="!started && phase!=='ready'" label="自动准备试跑设备"
                    icon="pi pi-link" size="small" :loading="busy"
                    :disabled="!runnerAvailable || !!blocked || !usable || busy || phase==='unknown'"
                    @click="prepare"/>
                <Button v-if="phase==='ready' && !started" label="确认执行此步骤"
                    icon="pi pi-play" size="small"
                    :disabled="!compatibilityReview || compatibilityReview.status==='blocked' ||
                        (compatibilityReview.status==='confirm'&&!compatibilityConsent) ||
                        (safety==='confirm_required'&&!consent) || (!!parameter&&!parameterValue)"
                    @click="submit"/>
            </div>
            <small class="trial-note">准备成功不代表执行成功；单步骤成功不代表整套业务验证完成。未知动作结果不会自动重试。</small>
        </div>
    </Dialog>
</template>

<style scoped>
.card-trial-content{display:grid;gap:13px;font-size:12px}
.trial-source,.trial-progress{display:grid;gap:6px;padding:12px;border:1px solid var(--console-border);border-radius:9px;background:var(--console-panel-raised);overflow-wrap:anywhere}
.trial-source strong,.trial-progress strong{font-size:13px}
.trial-source small,.trial-progress p,.trial-note{font-size:11px;color:var(--console-muted);margin:0;line-height:1.6}
.trial-progress code{overflow-wrap:anywhere}
.trial-check{display:flex;align-items:flex-start;gap:9px;line-height:1.55}
.trial-check label{cursor:pointer}
.param-input{display:grid;gap:7px}
.param-input :deep(input){width:100%}
.trial-actions{display:flex;justify-content:flex-end;gap:7px;flex-wrap:wrap}
.trial-note{display:block}
</style>
