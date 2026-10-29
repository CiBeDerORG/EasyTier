<script setup lang="ts">
/** T14: one selected step on this phone; reuse T12/T13 and the ordinary /runs API. */
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue';
import { Button, InputText, Message, Tag } from 'primevue';
import { useToast } from 'primevue/usetoast';
import AutomationFloatingDialog from './AutomationFloatingDialog.vue';
import TrialLocatorCompare from './TrialLocatorCompare.vue';
import { v4 as uuid } from 'uuid';
import AutomationApi, { AutomationApiError, automationErrorMessage } from '../../modules/automation-api';
import { useCardTrialPreparation, type CardTrialPhase } from '../../modules/useCardTrialPreparation';
import type { TrialPreviewHandle } from '../../modules/automation-run-handoff';
import { useCardRunStatus } from '../../modules/useCardRunStatus';
import { trialRunNeedsReconciliation } from '../../modules/automation-run-result';
import { makeSingleStepRun, stepTrialSafety } from '../../modules/automation-single-step';
import { freezeInlineStepRunIntent, type InlineStepRunIntent, type InlineStepRunProgress } from '../../modules/useInlineStepRun';
import { cardStepLabels } from '../../modules/card-step-projection';
import {
    reviewAutomationDeviceCompatibility,
    statelessCompatibility, requiresReuseHierarchy,
    type DeviceCompatibilityReview,
} from '../../modules/automation-device-compatibility';
import type { AutomationDevice, AutomationHierarchy, AutomationRunInput, AutomationStep } from '../../types/automation';
import {diagnosticXPath,normalizeTrialDiagnosis,summarizeTrialDiagnosis,
    type TrialLocatorDiagnosis} from '../../modules/automation-trial-diagnostics';
import {
    readTrialReceipt, reserveTrialReceipt, clearTrialReceipt, findTrialRun,
    type TrialSubmissionReceipt,
} from '../../modules/automation-trial-receipt';

const props=defineProps<{
    visible: boolean;
    portalTarget: HTMLElement | null;
    fullscreen: boolean;
    ownerId: number;
    device: AutomationDevice;
    step: AutomationStep | null;
    previewHandle: () => TrialPreviewHandle;
    runnerAvailable: boolean;
    blocked?: boolean;
    /** Run directly from the card's play button without a risk modal. */
    headless?: boolean;
}>();
const emit=defineEmits<{ close: []; history: [runId: string]; repair: [runId: string, stepId: string];
    progress: [value: InlineStepRunProgress];
    cleanup: [];
    finished: [value: {phase:'succeeded'|'action_sent'|'failed'|'cancelled';
        message:string;diagnosis?:TrialLocatorDiagnosis|null}] }>();
const api=new AutomationApi();
const toast=useToast();
function toastTrialError(message:string){
    toast.add({severity:'error',summary:'自动化单步执行',detail:message.trim()||'单步执行失败，未提供具体原因。',life:7000});
}
function failTrial(message:string,nextPhase:PanelPhase='failed'){
    phase.value=nextPhase;
    warning.value=message;
    toastTrialError(message);
}
const validationFieldLabels:Record<string,string>={
    body:'请求内容',request_id:'请求编号',name:'任务名称',steps:'步骤列表',
    'steps.id':'步骤 ID','steps.type':'步骤类型',title:'步骤标题',
    command:'ADB 命令',timeout_ms:'命令超时时间',device_ids:'目标手机',
    parameters:'运行参数',package:'应用包名',key:'导航按键',
};
function chineseValidationMessage(message:string):string{
    const known:Record<string,string>={
        'Some fields are invalid.':'请求字段校验失败。',
        'The request contains unsupported fields.':'请求中包含当前动作不支持的字段。',
        'Enter an ADB shell command.':'请填写 ADB shell 命令。',
        'Enter one adb shell command without host options, shell interpreters, pipes or redirects.':'只允许一条 adb shell 命令，不能包含宿主机参数、shell 解释器、管道或重定向。',
        'ADB command timeout must be 1000–20000 ms.':'ADB 命令超时时间必须在 1000～20000 毫秒之间。',
        'Each step requires a distinct UUID.':'每个步骤都必须有独立的 UUID。',
        'Each step requires a distinct canonical UUID.':'步骤 ID 必须是规范 UUID，且不能重复。',
        'Step title must be 1–80 visible characters.':'步骤标题必须为 1～80 个可见字符。',
        'Choose a supported step.':'步骤类型不受支持。',
        'Use between 1 and 100 steps.':'步骤数量必须在 1～100 步之间。',
        'The script is too large.':'脚本内容超过允许大小。',
        'A text value is required.':'该字段必须填写文本。',
        'The value is empty, too long, or contains unsupported characters.':'该字段为空、超长或包含不支持的字符。',
        'Choose a positive integer.':'该字段必须是正整数。',
        'Select between 1 and 20 devices.':'请选择 1～20 台手机。',
        'Use valid device identifiers.':'目标手机标识无效。',
        'A script parameter is missing.':'脚本引用的运行参数缺失。',
        'Parameter names and text values are invalid or too long.':'参数名称或参数值格式不正确，或长度超限。',
    };
    return known[message] ?? message;
}
function trialErrorDetails(cause:unknown,payload?:AutomationRunInput):string[]{
    if(cause instanceof AutomationApiError){
        const entries=Object.entries(cause.fieldErrors??{})
            .filter(([,message])=>typeof message==='string' && !!message.trim())
            .map(([field,message])=>(validationFieldLabels[field]??field)+'：'+chineseValidationMessage(message));
        if(entries.length)return entries.map(entry=>'服务器拒绝本次执行：'+entry);
        const serverMessage=chineseValidationMessage(cause.message);
        if(cause.status===422){
            const step=payload?.steps?.[0];
            const snapshot=step?.type==='adb_command'
                ? '当前提交：步骤 ID='+String(step.id??'空')+'；动作类型='+String(step.type)
                    +'；超时时间='+String(step.timeout_ms??'未填写')+' 毫秒；ADB 命令='+String(step.command??'未填写')
                : '当前提交：步骤 ID='+String(step?.id??'空')+'；动作类型='+String(step?.type??'未填写');
            return ['服务器拒绝本次执行。'+serverMessage+'；服务端没有返回具体字段原因。'+snapshot+'。'];
        }
        return [serverMessage+'（错误代码：'+cause.code+'）'];
    }
    return [automationErrorMessage(cause)];
}
function trialErrorMessage(cause:unknown,payload?:AutomationRunInput):string{
    return trialErrorDetails(cause,payload).join('；');
}
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
const compatibilityReview=ref<DeviceCompatibilityReview | null>(null);
const trialDiagnosis=ref<TrialLocatorDiagnosis|null>(null);
async function readTrialDiagnosis(
    owner:number,device:AutomationDevice,step:AutomationStep,
    hierarchy?:AutomationHierarchy,
):Promise<TrialLocatorDiagnosis|null>{
    if(!step.selector)return null;
    const expression=diagnosticXPath(step.selector);
    if(!expression)return null;
    const shot=hierarchy??await api.hierarchy(owner,device.id);
    const report=await api.verifyLocator(owner,device.id,{
        xpath:expression,width:shot.snapshot.width,height:shot.snapshot.height,
        preview_only:true,diagnose:true,
    });
    if(report.device_id!==device.id || report.target_fingerprint!==device.target_fingerprint)
        throw new Error('定位诊断时设备身份已变化，无法使用结果。');
    return normalizeTrialDiagnosis(step.selector,report,shot.captured_at);
}
async function reviewStepOnPhone(owner:number, device:AutomationDevice, step:AutomationStep):Promise<DeviceCompatibilityReview>{
    trialDiagnosis.value=null;
    if(!requiresReuseHierarchy([step]))return statelessCompatibility(owner,device,[step]);
    const hierarchy=await api.hierarchy(owner,device.id);
    const xpathVerified:Record<string,boolean>={};
    if(step.selector){
        // Read-only analysis of the CURRENT XML supplies both the exact count
        // and capped full-attribute/parent rows when multiple targets match.
        const diagnosis=await readTrialDiagnosis(owner,device,step,hierarchy);
        trialDiagnosis.value=diagnosis;
        if(step.selector.xpath)xpathVerified[step.id]=diagnosis?.count===1;
    }
    const review=reviewAutomationDeviceCompatibility(owner,device,hierarchy,[step],Date.now(),
        {requireUniqueSelectors:true,xpathVerified});
    // Native selectors and XPath must both be unique in the SAME live raw
    // Android XML. A zero-result wait_gone is the one intentional exception.
    if(step.selector && trialDiagnosis.value &&
        (trialDiagnosis.value.count>1 ||
         (trialDiagnosis.value.count===0 && step.type!=='wait_gone'))){
        review.status='blocked';
        review.issues.push({stepId:step.id,stepNumber:1,severity:'block',
            message:summarizeTrialDiagnosis(trialDiagnosis.value)});
    }
    return review;
}
const parameterValue=ref('');
const frozen=ref<AutomationStep | null>(null);
const lockedIntent=ref<InlineStepRunIntent | null>(null);
const targetFingerprint=ref('');
const handedOffSessionId=ref<string | null>(null);
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
    !(frozen.value.type==='input_text' && !frozen.value.parameter && !('text' in frozen.value)));
const started=computed(()=>!!result.run.value || !!pending.value);
const label=computed(()=>frozen.value ? (frozen.value.title?.trim() || cardStepLabels[frozen.value.type]) : '未选择步骤');
const inlineStatus=computed<InlineStepRunProgress|null>(() => {
    const intent=lockedIntent.value;
    if(!props.visible || !intent) return null;
    const run=runView.value;
    let status:InlineStepRunProgress['phase']='preparing';
    let text='开始检查设备';
    if(run){
        status=run.phase==='succeeded'?'succeeded':
            run.phase==='action_sent'?'action_sent':
            run.phase==='failed'?'failed':
            run.phase==='cancelled'?'cancelled':
            run.phase==='unknown'?'unknown':'running';
        text=run.label;
    }else if(phase.value==='submitting'){status='submitting';text='提交本步骤中';}
    else if(phase.value==='observing'){status='running';text='查询本步骤执行进度';}
    else if(phase.value==='unknown'){status='unknown';text='动作或准备结果尚未确认';}
    else if(phase.value==='failed'){status='failed';text='设备准备或安全检查失败';}
    else if(phase.value==='ready'){status='ready';text='待确认后执行本步骤';}
    else if(phase.value==='releasing'){text='核对原实时视频会话';}
    else if(phase.value==='preparing'){text=prepProgress.value?.message||'正在准备控件及ADB';}
    const runDevice=result.run.value?.devices.find(item=>item.device_id===intent.deviceId);
    const actualStep=runDevice?.steps.find(item=>item.step_id===intent.stepId);
    const started=Date.parse(actualStep?.started_at??'');
    const finished=Date.parse(actualStep?.finished_at??'');
    // Prefer durable Executor timestamps. During a running step, expose
    // elapsed wall time only after the server reports actual started_at.
    const elapsedMs=Number.isFinite(started)
        ? Math.max(0,(Number.isFinite(finished)?finished:Date.now())-started)
        : undefined;
    return {
        ownerId:intent.ownerId,deviceId:intent.deviceId,stepId:intent.stepId,
        requestId:pending.value?.request_id??intent.requestId,
        runId:run?.runId??null,phase:status,label:text,elapsedMs,
        message:run?.detail || warning.value || undefined,
    };
});
let finishedEmitted=false;
watch(inlineStatus,value=>{
    if(!value)return;
    emit('progress',value);
    if(!finishedEmitted && (value.phase==='failed'||value.phase==='cancelled'
        ||value.phase==='succeeded'||value.phase==='action_sent')){
        finishedEmitted=true;
        const terminalPhase=value.phase;
        void (async()=>{
            let diag=trialDiagnosis.value;
            // If a real run failed after preflight, the Android page could
            // have changed. Capture diagnostic evidence again without any
            // device action; never retry the failed POST.
            if(value.phase==='failed' && runView.value?.phase==='failed'
                && frozen.value?.selector && (diag===null || diag.count===1)){
                try{
                    diag=await readTrialDiagnosis(props.ownerId,props.device,frozen.value);
                }catch{ /* retain original executor failure details */ }
            }
            const message=diag && diag.count!==1 && value.phase==='failed'
                ? summarizeTrialDiagnosis(diag)+(value.message?'；'+value.message:'')
                : value.message||value.label;
            emit('finished',{phase:terminalPhase,message,diagnosis:terminalPhase==='failed'?diag:null});
        })();
    }
},{immediate:true});
const preparePhases:Record<CardTrialPhase,string>={
    queued:'准备排队中',binding:'核对设备绑定',adb_check:'查询 ADB',connecting:'连接 ADB',
    helper_check:'检查控件组件',helper_start:'提交控件初始化',helper_wait:'初始化控件中',
    ready:'准备就绪',failed:'准备失败',unknown:'准备结果未知',cancelled:'已停止准备',
};
function clearSecrets(){parameterValue.value='';}
function resetForOpen(){
    finishedEmitted=false;
    trialDiagnosis.value=null;
    handedOffSessionId.value=null;
    lockedIntent.value=freezeInlineStepRunIntent(
        props.ownerId,props.device.id,props.device.target_fingerprint??'',props.step,uuid());
    loadReceipt();
    if(receiptFailure.value || unresolvedReceipt.value) {
        result.reset();
        pending.value=null;uncertain.value=true;
        compatibilityReview.value=null;
        clearSecrets();phase.value='unknown';
        frozen.value=lockedIntent.value?.step??null;
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
    result.reset();pending.value=null;uncertain.value=false;
    compatibilityReview.value=null;
    clearSecrets();phase.value='idle';warning.value='';
    frozen.value=lockedIntent.value?.step??null;
    targetFingerprint.value=props.device.target_fingerprint ?? '';
}
function valid(mine:number, owner:number, deviceId:string, original:string):boolean {
    if(disposed || !props.visible)return false;
    if(mine!==epoch){toastTrialError('本次单步操作已过期，旧操作结果已忽略。');return false;}
    if(props.ownerId!==owner){toastTrialError('当前账号已变化，已忽略旧单步操作结果。');return false;}
    if(props.device.id!==deviceId){toastTrialError('当前目标手机已变化，已忽略旧单步操作结果。');return false;}
    if(props.device.target_fingerprint!==targetFingerprint.value){
        toastTrialError('目标手机绑定指纹已变化，已忽略旧单步操作结果。');return false;
    }
    if(!lockedIntent.value){toastTrialError('单步执行请求身份不存在，请关闭后重新打开执行窗口。');return false;}
    if(lockedIntent.value.ownerId!==owner){toastTrialError('执行请求账号与当前账号不一致。');return false;}
    if(lockedIntent.value.deviceId!==deviceId){toastTrialError('执行请求目标手机与当前手机不一致。');return false;}
    if(lockedIntent.value.targetFingerprint!==props.device.target_fingerprint){
        toastTrialError('执行期间目标手机绑定发生变化。');return false;
    }
    if(lockedIntent.value.stepId!==props.step?.id){toastTrialError('当前选中的步骤 ID 已变化。');return false;}
    if(lockedIntent.value.stepJson!==JSON.stringify(props.step)){
        toastTrialError('当前步骤内容已变化，旧执行结果不会应用到新步骤。');return false;
    }
    if(!props.step){toastTrialError('当前步骤已不存在。');return false;}
    if(JSON.stringify(props.step)!==original){toastTrialError('当前步骤内容与提交时不一致，已取消后续处理。');return false;}
    return true;
}
function checkReadyToPrepare(): boolean {
    if(!targetFingerprint.value){failTrial('目标手机缺少设备绑定指纹，请重新查询设备状态。');return false;}
    if(!props.device.connection.enabled){failTrial('目标手机的 ADB 服务未启用，请先启用 ADB 服务。');return false;}
    if(props.device.availability_reason){failTrial('目标手机当前不可用：'+props.device.availability_reason);return false;}
    if(!frozen.value){failTrial('没有选中要执行的步骤。');return false;}
    if(!frozen.value.id){failTrial('当前步骤缺少步骤 ID，请重新添加该步骤。');return false;}
    if(safety.value==='unsupported'){failTrial('该步骤类型不支持单独试跑，请先拆分或调整步骤。');return false;}
    if(frozen.value.type==='key' && !frozen.value.key){failTrial('导航按键未选择，请编辑步骤并选择按键。');return false;}
    if(frozen.value.type==='input_text' && !frozen.value.parameter && !('text' in frozen.value)){failTrial('输入文字步骤没有配置文字或运行参数。');return false;}
    const handle=props.previewHandle();
    if(frozen.value.type!=='adb_command' && !handle.getSessionId()){
        failTrial('请先打开本机卡片的原实时画面。');return false;
    }
    if(frozen.value.type!=='adb_command' && handle.getState().phase!=='live'){
        failTrial('本机实时画面尚未连接，请等待画面进入实时状态。');return false;
    }
    return true;
}
async function prepare(){
    loadReceipt();
    if(receiptFailure.value){failTrial(receiptFailure.value,'unknown');return;}
    if(unresolvedReceipt.value){
        failTrial('该手机尚有未核实的试跑请求（'+unresolvedReceipt.value.requestId+'），请先只读查询执行记录，不能新建第二条动作。','unknown');return;
    }
    if(busy.value){failTrial('当前单步操作仍在处理中，请等待当前操作返回。',phase.value);return;}
    if(pending.value){failTrial('当前步骤已有待确认的试跑请求，请先核对原请求结果。','unknown');return;}
    if(result.run.value){failTrial('当前步骤已有执行记录，请先查看或刷新该记录。',phase.value);return;}
    if(!checkReadyToPrepare())return;
    const mine=++epoch,owner=props.ownerId,id=props.device.id;
    const source=JSON.stringify(props.step);
    const chosen=frozen.value!;
    warning.value='';
    compatibilityReview.value=null;
    try {
        // Pure ADB shell steps do not use UiAutomator2, screen geometry or
        // the preview session. Submit the frozen step to the ordinary run API;
        // the server and executor remain authoritative for task occupancy,
        // authorization, command policy and actual ADB execution.
        if(chosen.type==='adb_command'){
            phase.value='preparing';
            const latest=(await api.devices(owner)).items.find(item=>item.id===id);
            if(!valid(mine,owner,id,source))return;
            if(!latest)throw new Error('设备列表中找不到目标手机，请刷新设备列表。');
            if(latest.owner_user_id!==owner)throw new Error('目标手机不属于当前选择的账号。');
            if(latest.target_fingerprint!==targetFingerprint.value)throw new Error('目标手机绑定指纹已变化，请重新查询后再执行。');
            if(!latest.connection.enabled)throw new Error('目标手机的 ADB 服务未启用。');
            if(latest.availability_reason)throw new Error('目标手机不可用：'+latest.availability_reason);
            if(latest.run_occupied)throw new Error('目标手机已有未结束的执行任务，请先查看执行记录或清理占用。');
            compatibilityReview.value=statelessCompatibility(owner,latest,[chosen]);
            phase.value='ready';
            void submit();
            return;
        }
        phase.value='releasing';
        const handle=props.previewHandle();
        const sid=handle.getSessionId();
        if(!sid)throw new Error('原实时画面会话不存在，请重新打开手机画面。');
        if(handle.getState().phase!=='live')throw new Error('原视频画面尚未处于实时连接状态，请等待连接完成。');
        // Manual permission is solely the user's floating icon setting.
        // The Executor arbitrates script and manual actions by physical lease.
        const before=(await api.devices(owner)).items.find(item=>item.id===id);
        if(!valid(mine,owner,id,source))return;
        if(!before)throw new Error('设备列表中找不到目标手机，请刷新设备列表。');
        if(before.owner_user_id!==owner)throw new Error('目标手机不属于当前账号。');
        if(before.target_fingerprint!==targetFingerprint.value)throw new Error('目标手机绑定指纹已变化，请重新打开画面。');
        if(before.run_occupied)throw new Error('目标手机仍有未结束的执行任务，请先处理任务占用。');
        if(before.active_session?.id!==sid)throw new Error('实时画面会话已变化，请重新打开手机画面。');
        if(!before.active_session || !['active','view_only'].includes(before.active_session.state))
            throw new Error('实时画面会话尚未处于可交接状态。');
        warning.value='正在核对原手机会话与执行器占用；原实时画面持续播放。';
        const fresh=await api.devices(owner);
        if(!valid(mine,owner,id,source))return;
        const row=fresh.items.find(item=>item.id===id);
        if(!row)throw new Error('准备前重新查询设备时，没有找到目标手机。');
        if(row.owner_user_id!==owner)throw new Error('重新查询后发现目标手机账号归属已变化。');
        if(row.target_fingerprint!==targetFingerprint.value)throw new Error('重新查询后发现目标手机绑定已变化。');
        if(row.run_occupied)throw new Error('目标手机在准备期间出现未结束的执行任务。');
        if(row.active_session?.id!==sid)throw new Error('目标手机的实时画面会话在准备期间发生变化。');
        if(!row.active_session || !['active','view_only'].includes(row.active_session.state))
            throw new Error('目标手机的实时画面会话当前不可交接。');
        if(handle.getSessionId()!==sid)throw new Error('当前画面窗口与原会话不一致，请重新打开手机画面。');
        handedOffSessionId.value=sid;
        phase.value='preparing';warning.value='';
        const prepared=await preparation.run({
            ownerId:owner,deviceId:id,targetFingerprint:targetFingerprint.value,
            steps:[chosen],ownedSessionId:sid,
        });
        if(!valid(mine,owner,id,source))return;
        if(prepared?.phase==='ready'){
            // Prepared helper/ADB is not evidence that saved coordinates fit
            // this phone. Native-only steps do not require UiAutomator2.
            const latestDevice=(await api.devices(owner)).items.find(item=>item.id===id);
            if(!valid(mine,owner,id,source))return;
            if(!latestDevice)throw new Error('无法确认当前手机授权及绑定。');
            const checked=await reviewStepOnPhone(owner,latestDevice,chosen);
            if(!valid(mine,owner,id,source))return;
            compatibilityReview.value=checked;
            if(checked.status==='blocked'){
                const detail=trialDiagnosis.value && trialDiagnosis.value.count!==1
                    ? summarizeTrialDiagnosis(trialDiagnosis.value)
                    : checked.issues.filter(i=>i.severity==='block').map(i=>i.message).join('；') || '设备安全校验未通过。';
                failTrial(detail);
            }else {
                phase.value='ready';warning.value='';
                // One deliberate press of ▶ authorizes this one frozen step.
                // Never skip fresh target validation or the durable request receipt.
                if (!parameter.value) void submit();
            }
        }
        else {
            const detail=prepared?.message || '无法确认设备准备状态。';
            failTrial(detail,prepared?.phase==='unknown'?'unknown':'failed');
        }
    }catch(cause){
        if(valid(mine,owner,id,source)){
            failTrial(trialErrorMessage(cause));
        }
    }
}
async function submit(){
    loadReceipt();
    if(receiptFailure.value){failTrial(receiptFailure.value,'unknown');return;}
    if(unresolvedReceipt.value){
        failTrial('上一条试跑请求尚未核实（'+unresolvedReceipt.value.requestId+'），请先只读查询原请求。','unknown');return;
    }
    if(phase.value!=='ready'){failTrial('当前阶段为“'+phase.value+'”，不是待执行状态。',phase.value);return;}
    if(pending.value){failTrial('当前步骤已有待提交或待确认的请求，请先核对原请求。','unknown');return;}
    if(!frozen.value){failTrial('没有冻结的步骤内容，请关闭并重新打开单步执行窗口。');return;}
    if(!frozen.value.id){failTrial('当前步骤缺少步骤 ID，请重新添加该步骤。');return;}
    if(safety.value==='unsupported'){failTrial('该类型的步骤不支持单独执行。');return;}
    if(frozen.value.type==='key' && !frozen.value.key){failTrial('导航按键未选择，请编辑步骤后再执行。');return;}
    if(frozen.value.type==='input_text' && !frozen.value.parameter && !('text' in frozen.value)){
        failTrial('输入文字步骤没有配置固定文字或运行参数。');return;
    }
    if(!usable.value){failTrial('当前步骤尚未配置完整，不能执行。');return;}
    if(!compatibilityReview.value){failTrial('尚未完成当前手机的兼容性核验，请先重新准备步骤。');return;}
    if(compatibilityReview.value.status==='blocked'){
        failTrial(compatibilityReview.value.issues.filter(i=>i.severity==='block').map(i=>i.message).join('；') || '当前手机兼容性核验未通过。');return;
    }
    if(parameter.value && !parameterValue.value.trim()){failTrial('请输入本次运行参数。');return;}
    if(parameter.value && new TextEncoder().encode(parameterValue.value).length>2000){
        failTrial('本次运行参数超过 2000 字节，请缩短后重试。');return;
    }
    const owner=props.ownerId,id=props.device.id,mine=++epoch;
    const original=JSON.stringify(props.step);
    const intent=lockedIntent.value;
    if(!intent){failTrial('单步执行请求身份未初始化，请关闭后重新打开窗口。');return;}
    if(intent.ownerId!==owner){failTrial('执行请求账号与当前账号不一致，请重新打开单步执行窗口。');return;}
    if(intent.deviceId!==id){failTrial('执行请求目标手机与当前手机不一致，请重新打开单步执行窗口。');return;}
    if(intent.targetFingerprint!==props.device.target_fingerprint){failTrial('目标手机绑定已变化，请重新查询设备并打开单步执行。');return;}
    if(intent.stepId!==props.step?.id){failTrial('当前选中的步骤 ID 已变化，请重新打开单步执行窗口。');return;}
    if(intent.stepJson!==JSON.stringify(props.step)){failTrial('步骤内容在准备后发生变化，请重新准备后再执行。');return;}
    const payload=makeSingleStepRun(frozen.value,id,intent.requestId,
        parameter.value ? {[parameter.value]:parameterValue.value} : {},true);
    if(!payload){failTrial('步骤无法转换为单步执行请求，请检查步骤类型和步骤 ID。');return;}
    pending.value=payload; // Freeze request ID and body across uncertain HTTP outcomes.
    phase.value='submitting';warning.value='';
    try{
        if(!valid(mine,owner,id,original)){
            failTrial('提交前账号、设备绑定或步骤内容已发生变化，已取消本次提交。');return;
        }
        // The ADB-only path already checked this device in prepare().
        // Do not repeat hierarchy/preview checks that cannot validate a shell
        // command. create_run() performs authoritative admission atomically.
        if(frozen.value.type==='adb_command'){
            await sendFrozen(mine,owner,id,original,payload);
            return;
        }
        const latest=await api.devices(owner);
        if(!valid(mine,owner,id,original))return;
        const row=latest.items.find(item=>item.id===id);
        const handle=props.previewHandle();
        if(!row)throw new Error('提交前重新查询设备时，没有找到目标手机。');
        if(row.owner_user_id!==owner)throw new Error('目标手机账号归属已变化。');
        if(!row.connection.enabled)throw new Error('目标手机的 ADB 服务已关闭。');
        if(row.availability_reason)throw new Error('目标手机当前不可用：'+row.availability_reason);
        if(row.target_fingerprint!==targetFingerprint.value)throw new Error('目标手机绑定指纹已变化。');
        if(!handedOffSessionId.value)throw new Error('原实时画面会话交接凭据缺失，请重新准备步骤。');
        if(handle.getSessionId()!==handedOffSessionId.value)throw new Error('当前画面窗口已不再对应原实时画面会话。');
        if(row.active_session?.id!==handedOffSessionId.value)throw new Error('服务端记录的实时画面会话已变化。');
        if(!row.active_session || !['active','view_only'].includes(row.active_session.state))
            throw new Error('实时画面会话已关闭或尚未进入可用状态。');
        if(row.run_occupied)throw new Error('目标手机已有未结束的执行任务，请先处理任务占用。');
        const rechecked=await reviewStepOnPhone(owner,row,frozen.value);
        if(!valid(mine,owner,id,original))return;
        if(rechecked.status==='blocked'){
            pending.value=null;compatibilityReview.value=rechecked;
            const detail=trialDiagnosis.value && trialDiagnosis.value.count!==1
                ? summarizeTrialDiagnosis(trialDiagnosis.value)
                : rechecked.issues.filter(i=>i.severity==='block').map(i=>i.message).join('；') || '目标定位未通过实时安全验证。';
            failTrial(detail);return;
        }
        // The just-captured hierarchy and device fingerprint are fresh.
        // A harmless warning difference must not force another risk click.
        if(compatibilityReview.value?.targetFingerprint!==row.target_fingerprint){
            pending.value=null;failTrial('兼容性核验使用的设备指纹与最新设备指纹不一致，试跑已中止。');return;
        }
        clearSecrets();
        await sendFrozen(mine,owner,id,original,payload);
    }catch(cause){
        if(valid(mine,owner,id,original)){
            const failedPayload=pending.value??undefined;
            pending.value=null;failTrial(trialErrorMessage(cause,failedPayload));
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
        const details=trialErrorDetails(cause,payload);
        const prefix=dispatched?'':'无法安全保存试跑请求身份，未发送动作：';
        const suffix=unknown?'。动作可能已被服务器接收；请按原请求编号只读核对，不能新建请求。':'';
        warning.value=prefix+details.join('；')+suffix;
        details.forEach((detail,index)=>toastTrialError(prefix+detail+(index===details.length-1?suffix:'')));
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
        const sid=handedOffSessionId.value;
        if(!sid || props.previewHandle().getSessionId()!==sid)
            throw new Error('原手机实时会话已丢失，不能直接恢复准备状态。');
        const current=(await api.devices(owner)).items.find(item=>item.id===id);
        if(!valid(mine,owner,id,original))return;
        if(current?.active_session?.id!==sid ||
            !['active','view_only'].includes(current.active_session.state))
            throw new Error('原手机会话不再稳定，禁止继续恢复试跑准备。');
        const state=await preparation.reconcile({
            ownerId:owner,deviceId:id,targetFingerprint:targetFingerprint.value,
            steps:[frozen.value],ownedSessionId:sid,
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
                    await api.hierarchy(owner,id),[frozen.value],Date.now(),{requireUniqueSelectors:true})
                : statelessCompatibility(owner,latestDevice,[frozen.value]);
            if(!valid(mine,owner,id,original))return;
            compatibilityReview.value=checked;
            if(checked.status==='blocked'){
                phase.value='failed';
                warning.value='准备已恢复，但控件定位或屏幕坐标仍不安全；请重新拾取。';
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
        if(result.run.value && trialRunNeedsReconciliation(runView.value)) {
            // Keep watching the SAME saved run when it is still executing or
            // uncertain, regardless of which step the user selected.
            // Never create a fresh action while an earlier one is unresolved.
            phase.value=runView.value?.phase==='unknown'?'unknown':'observing';
            warning.value=runView.value?.phase==='unknown'
                ? '上一条试跑结果或清理尚未确认；只能只读查询原运行记录，不会再次发送动作。'
                : '上一条试跑仍在执行；正在继续只读查询原运行记录。';
            result.resume();
        } else {
            // An already settled action_sent/success/failure/cancellation
            // must start with a NEW frozen request ID on the next ▶ click.
            // Reusing the old completed run here made single-step replay
            // appear stuck at "动作已发送".
            resetForOpen();
            if(phase.value==='idle') void nextTick(() => {
                if(props.visible && !disposed && phase.value==='idle') void prepare();
            });
        }
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
    compatibilityReview.value=null;
    phase.value='idle';
});
onBeforeUnmount(()=>{
    disposed=true;++epoch;result.stop();preparation.dispose();clearSecrets();
});
</script>

<template>
    <AutomationFloatingDialog :visible="visible && (!headless || phase==='unknown' || phase==='failed' || (phase==='ready' && !!parameter && !parameterValue) || (trialDiagnosis !== null && trialDiagnosis.count !== 1))" :portal-target="portalTarget"
        :fullscreen="fullscreen" title="卡片单步骤试跑"
        :closable="!busy || phase === 'preparing'" @close="hide">
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
                <Tag :value="phase==='ready'?'就绪':phase==='unknown'?'结果未知':phase==='releasing'?'核验原会话':phase==='preparing'?'准备中':phase==='failed'?'失败':'尚未准备'"
                    :severity="phase==='ready'?'success':phase==='unknown'||phase==='failed'?'warn':'secondary'"/>
                <p v-if="prepProgress">{{ preparePhases[prepProgress.phase] }} · {{ prepProgress.message }}</p>
                <p v-else>准备会检查最新账号授权、ADB 连接与所需控件组件。不会自动进行无线 ADB 配对。</p>
            </div>
            <div v-if="compatibilityReview" class="trial-progress" aria-live="polite">
                <strong>手机匹配：{{ compatibilityReview.viewport.width ? compatibilityReview.viewport.width+' × '+compatibilityReview.viewport.height : '仅ADB操作' }}</strong>
                <Tag :value="compatibilityReview.status==='blocked'
                    ? compatibilityReview.issues.some(i=>i.severity==='block' && i.message.includes('坐标参考画面'))?'尺寸不兼容':'定位校验失败'
                    : compatibilityReview.status==='confirm'?'已检查定位风险':'只读核验通过'"
                    :severity="compatibilityReview.status==='blocked'?'danger':compatibilityReview.status==='confirm'?'warn':'success'"/>
                <p v-for="item in compatibilityReview.issues" :key="item.stepId+item.message">
                    第 {{ item.stepNumber }} 步：{{ item.message }}
                </p>
            </div>
            <TrialLocatorCompare v-if="trialDiagnosis" :diagnosis="trialDiagnosis" />
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
                <Button v-if="currentRun && runView && trialRunNeedsReconciliation(runView)"
                    label="强制停止并清理本机任务" icon="pi pi-stop-circle"
                    severity="danger" outlined size="small"
                    :disabled="!runnerAvailable" @click="emit('cleanup')"/>
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
                        (!!parameter&&!parameterValue)"
                    @click="submit"/>
            </div>
            <small class="trial-note">准备成功不代表执行成功；单步骤成功不代表整套业务验证完成。未知动作结果不会自动重试。</small>
        </div>
    </AutomationFloatingDialog>
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
