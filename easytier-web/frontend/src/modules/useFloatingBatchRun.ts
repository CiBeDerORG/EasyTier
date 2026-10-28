/**
 * T43–T45: One batch job for the shared workbench, never a second video.
 * No Android device control lives here: only existing PhoneCard viewers,
 * the T12 preparation helper and the server's immutable /runs API.
 */
import { computed, onBeforeUnmount, reactive, watch } from 'vue';
import { v4 as uuid } from 'uuid';
import type { AutomationDevice, AutomationMediaSettings, AutomationRun, AutomationRunInput, AutomationStep } from '../types/automation';
import AutomationApi, { AutomationApiError, automationErrorMessage } from './automation-api';
import { analyzeAutomationFlow } from './automation-flow';
import { useCardTrialPreparation } from './useCardTrialPreparation';
import { requiresAutomationHelper } from './automation-preflight';
import { readBatchReceipt, reserveBatchReceipt, clearBatchReceipt, type BatchSubmissionReceipt } from './automation-batch-receipt';

export interface BatchPhoneHandle {
    open(quality: 'wall' | 'focus', settings: AutomationMediaSettings): Promise<void>;
    getState(): { phase: string; ready: boolean; viewerOnly?: boolean };
    getSessionId(): string | null;
    getStream(): MediaStream | null;
    enterViewOnly(): Promise<void>;
}
export interface FrozenBatchTarget { id: string; name: string; fingerprint: string; }
export interface FrozenBatchIntent {
    readonly owner: number;
    readonly requestId: string;
    readonly name: string;
    readonly steps: AutomationStep[];
    readonly stepHash: string;
    readonly targets: FrozenBatchTarget[];
    readonly parameters: Record<string,string>;
    readonly concurrency: number;
}
export type BatchPhase = 'idle' | 'preparing' | 'submitting' | 'running' | 'cancel_requested' |
    'completed' | 'failed' | 'unknown';
export interface BatchDeviceProgress {
    name: string; phase: string; message: string;
    sessionId?: string; runState?: string; stepId?: string;
    mediaState?: 'live'|'lost'|'unavailable'; mediaMessage?: string;
}
export interface FloatingBatchState {
    owner: number; phase: BatchPhase; error: string; requestId: string;
    selected: string[]; devices: Record<string,BatchDeviceProgress>;
    run: AutomationRun | null; pendingReceipt: BatchSubmissionReceipt | null;
}

export function freezeFloatingBatchIntent(
    owner: number, selected: string[], available: AutomationDevice[],
    steps: AutomationStep[], name: string, parameters: Record<string,string>,
    concurrency: number, requestId=uuid(),
): FrozenBatchIntent {
    const ids=[...new Set(selected)];
    // Existing Rust API validates 1–20 devices; do not silently split a
    // request into multiple runs, or pretend this limit no longer exists.
    if(owner<=0 || ids.length<1 || ids.length>20)
        throw new Error('当前批量 API 一次支持 1～20 台；不自动拆单，以免重复手机动作。');
    if(!steps.length || steps.length>100 || analyzeAutomationFlow(steps).problem)
        throw new Error('脚本为空、超过 100 步或条件分支结构无效。');
    const refs=new Map(available.map(d=>[d.id,d]));
    const targets=ids.map(id=>{
        const device=refs.get(id);
        if(!device || device.owner_user_id!==owner || !device.connection.enabled ||
            !device.target_fingerprint || device.availability_reason || device.run_occupied)
            throw new Error('选中手机未授权、绑定异常或仍被其它运行占用：'+(device?.name??id));
        return {id,name:device.name,fingerprint:device.target_fingerprint};
    });
    const parameterNames=[...new Set(steps.filter(s=>s.type==='input_text' && !!s.parameter)
        .map(s=>s.parameter!.trim()))];
    if(parameterNames.some(key=>!key || !parameters[key]))
        throw new Error('脚本含动态输入参数，请填写当前批量运行所需参数。');
    if(!Number.isSafeInteger(concurrency)||concurrency<1)
        throw new Error('并发数必须是正整数。');
    const copy=JSON.parse(JSON.stringify(steps)) as AutomationStep[];
    return {
        owner,requestId,name:name.trim()||'自动化批量流程',steps:copy,
        stepHash:JSON.stringify(copy),targets,parameters:{...parameters},concurrency,
    };
}

const DONE=new Set(['completed','succeeded','failed','cancelled','interrupted','partial_failed']);
export function useFloatingBatchRun(api: AutomationApi = new AutomationApi()) {
    const state=reactive<FloatingBatchState>({
        owner:0,phase:'idle',error:'',requestId:'',selected:[],
        devices:{},run:null,pendingReceipt:null,
    });
    const preparation=useCardTrialPreparation(api);
    let version=0;
    let timer:ReturnType<typeof setTimeout>|null=null;
    let disposed=false;
    let cardResolver:((id:string)=>BatchPhoneHandle|undefined)|null=null;
    const busy=computed(()=>['preparing','submitting','running','cancel_requested'].includes(state.phase));
    // Per-device preparation updates are independent: a slow UiAutomator2
    // install on A must not hide B's ready/failed state.
    watch(preparation.state, observations=>{
        if(state.phase!=='preparing')return;
        for(const target of state.selected){
            const p=observations[state.owner+':'+target];
            const row=state.devices[target];
            if(!p || !row || row.phase==='ready' || row.phase==='viewing' || row.phase==='suspending')continue;
            state.devices[target]={...row,phase:p.phase,message:p.message};
        }
    },{deep:true});
    function stopTimer(){if(timer!==null){clearTimeout(timer);timer=null;}}
    function still(owner:number,epoch:number){return !disposed&&version===epoch&&state.owner===owner;}
    function progress(id:string,phase:string,message:string,sessionId?:string){
        const row=state.devices[id];if(!row)return;
        state.devices[id]={...row,phase,message,...(sessionId?{sessionId}:{})};
    }
    function inspectMedia(){
        for(const did of state.selected){
            const row=state.devices[did];if(!row)continue;
            const card=cardResolver?.(did);
            if(!card || !row.sessionId) {
                row.mediaState='unavailable';
                row.mediaMessage='此页面未持有可核对的原视频会话；请到原手机卡片确认';
                continue;
            }
            const live=card.getSessionId()===row.sessionId &&
                card.getState().phase==='live' && !!card.getStream();
            row.mediaState=live?'live':'lost';
            row.mediaMessage=live?'原实时画面持续播放':'原实时画面已经中断或会话身份发生变化，请检查手机卡片';
        }
    }
    function acceptRun(run: AutomationRun, intent?: FrozenBatchIntent) {
        const expected=intent?.targets.map(t=>t.id) ?? state.selected;
        if(!run.id || run.devices.length!==expected.length ||
            expected.some(id=>!run.devices.some(d=>d.device_id===id)))
            throw new Error('服务端返回的运行设备列表与本次冻结目标不一致。');
        // Rust normalizes/validates step JSON before persisting it, so
        // comparing full JSON text would reject legitimate accepted runs.
        // The globally unique request UUID plus immutable ordered step IDs
        // and exact target set bind this acknowledgement to our one POST.
        if(intent && (run.request_id!==intent.requestId ||
            run.script_snapshot.steps.length!==intent.steps.length ||
            run.script_snapshot.steps.some((step,i)=>step.id!==intent.steps[i].id)))
            throw new Error('运行请求身份或步骤列表不一致，禁止按此回执继续提交。');
        state.run=run;
        for(const target of run.devices){
            const old=state.devices[target.device_id];
            state.devices[target.device_id]={
                name:old?.name??target.name,
                phase:target.state,message:target.steps.find(s=>s.step_id===target.current_step)?.message??target.state,
                sessionId:old?.sessionId,runState:target.state,
                mediaState:old?.mediaState,mediaMessage:old?.mediaMessage,
                stepId:target.steps.find(step=>step.state==='running')?.step_id ??
                    (typeof target.current_step==='string'?target.current_step:undefined),
            };
        }
        inspectMedia();
        state.phase=DONE.has(run.state)?'completed'
            :run.state==='outcome_unknown'||run.state==='cleanup_pending'?'unknown'
            :run.state==='cancel_requested'?'cancel_requested':'running';
    }
    async function refresh(){
        if(!state.run || !['running','cancel_requested','unknown','completed'].includes(state.phase))return;
        const owner=state.owner,id=state.run.id,epoch=version;
        try {
            const fresh=await api.getRun(owner,id);
            if(!still(owner,epoch) || fresh.id!==id)return;
            acceptRun(fresh);
            state.error='';
            if(state.phase==='running'||state.phase==='cancel_requested'){
                stopTimer();timer=setTimeout(()=>void refresh(),2000);
            }
        }catch(error){
            if(!still(owner,epoch))return;
            state.error='只读查询批量运行进度失败：'+automationErrorMessage(error);
            // Query failures do not imply stopped execution or authorize replay.
            if(state.phase==='running'||state.phase==='cancel_requested'){
                stopTimer();timer=setTimeout(()=>void refresh(),5000);
            }
        }
    }
    async function reconcile(owner:number):Promise<void>{
        let receipt:BatchSubmissionReceipt|null;
        try{receipt=readBatchReceipt(window.sessionStorage,owner);}
        catch(error){state.error=automationErrorMessage(error);state.phase='unknown';return;}
        if(!receipt)return;
        state.pendingReceipt=receipt;state.owner=owner;state.requestId=receipt.requestId;
        state.selected=[...receipt.deviceIds];state.phase='unknown';
        try {
            const results=await api.listRuns(owner);
            const match=results.items.find(run=>run.request_id===receipt!.requestId &&
                run.devices.length===receipt!.deviceIds.length &&
                receipt!.deviceIds.every(id=>run.devices.some(d=>d.device_id===id)));
            if(!match){
                state.error='未在最近执行历史中查到原请求 '+receipt.requestId+
                    '，不能证明未执行；保留保护，请人工查看执行器日志。';return;
            }
            acceptRun(match);
            clearBatchReceipt(window.sessionStorage,receipt);
            state.pendingReceipt=null;state.error='';
            if(state.phase==='running'||state.phase==='cancel_requested')void refresh();
        }catch(error){
            state.phase='unknown';
            state.error='只读恢复失败，未发送任何新动作：'+automationErrorMessage(error);
        }
    }
    async function start(
        intent:FrozenBatchIntent,
        resolveCard:(id:string)=>BatchPhoneHandle|undefined,
        media:AutomationMediaSettings,
        allow:()=>boolean,
    ):Promise<void>{
        if(busy.value || state.pendingReceipt)throw new Error('存在运行中或结果未核实的批量任务。');
        // Never re-POST a previous ambiguous action after page refresh.
        const old=readBatchReceipt(window.sessionStorage,intent.owner);
        if(old){
            state.pendingReceipt=old;state.phase='unknown';state.error='旧批量运行结果尚未核实，请先只读查询原请求。';
            return;
        }
        const epoch=++version;
        cardResolver=resolveCard;
        stopTimer();
        state.owner=intent.owner;state.phase='preparing';state.error='';
        state.requestId=intent.requestId;state.run=null;
        state.selected=intent.targets.map(t=>t.id);
        state.devices=Object.fromEntries(intent.targets.map(t=>[t.id,{
            name:t.name,phase:'queued',message:'排队等待当前手机原画面',
        }]));
        const valid=()=>still(intent.owner,epoch)&&allow();
        try {
            // A/B prepare independently (bounded concurrency), but any failure
            // blocks the ONE atomic batch POST. Other viewers remain untouched.
            const todo=[...intent.targets];
            const workers=Array.from({length:Math.min(3,todo.length)},async()=>{
                while(todo.length && valid()){
                    const target=todo.shift()!;
                    const card=resolveCard(target.id);
                    if(!card)throw new Error(target.name+' 的原手机卡片未就绪');
                    progress(target.id,'viewing','确认原卡片观看身份');
                    const pre=(await api.devices(intent.owner)).items.find(d=>d.id===target.id);
                    if(!valid())return;
                    if(!pre || pre.owner_user_id!==intent.owner || pre.target_fingerprint!==target.fingerprint ||
                        pre.run_occupied || pre.availability_reason)
                        throw new Error(target.name+' 授权/绑定已变化或被占用');
                    let sid=card.getSessionId();
                    if(pre.active_session && pre.active_session.id!==sid)
                        throw new Error(target.name+' 已被别的网页投屏控制，禁止抢占');
                    if(!sid){
                        progress(target.id,'viewing','打开该手机唯一原实时画面');
                        await card.open('wall',{...media,screen_off:false});
                        if(!valid())return;
                        sid=card.getSessionId();
                    }
                    if(!sid || card.getState().phase!=='live' || !card.getStream())
                        throw new Error(target.name+' 的原视频未确认连续可用');
                    const before=(await api.devices(intent.owner)).items.find(d=>d.id===target.id);
                    if(!valid())return;
                    if(before?.active_session?.id!==sid || before.target_fingerprint!==target.fingerprint)
                        throw new Error(target.name+' 原卡片会话已改变');
                    if(before.active_session.state!=='view_only'){
                        progress(target.id,'suspending','只释放手控写权限，不关闭视频',sid);
                        await card.enterViewOnly();
                    }
                    if(!valid())return;
                    const viewer=(await api.devices(intent.owner)).items.find(d=>d.id===target.id);
                    if(!valid())return;
                    if(viewer?.active_session?.id!==sid ||
                        viewer.active_session.state!=='view_only' ||
                        viewer.target_fingerprint!==target.fingerprint || !card.getStream())
                        throw new Error(target.name+' 尚未确认只读移交，禁止准备脚本');
                    progress(target.id,'preparing','检查 ADB / UiAutomator2',sid);
                    inspectMedia();
                    const p=await preparation.run({
                        ownerId:intent.owner,deviceId:target.id,targetFingerprint:target.fingerprint,
                        steps:intent.steps,ownedSessionId:sid,
                    });
                    if(!valid())return;
                    if(p?.phase!=='ready')throw new Error(target.name+'：'+(p?.message||'设备准备结果未确认'));
                    progress(target.id,'ready','同一原视频准备完成，等待其它手机',sid);
                }
            });
            const results=await Promise.allSettled(workers);
            if(!valid())return;
            const fault=results.find((r):r is PromiseRejectedResult=>r.status==='rejected');
            if(fault)throw fault.reason;
            if(Object.values(state.devices).some(d=>d.phase!=='ready'))
                throw new Error('部分手机尚未完成安全准备，未提交整批脚本');
            const fresh=await api.devices(intent.owner);
            if(!valid())return;
            for(const target of intent.targets){
                const dev=fresh.items.find(item=>item.id===target.id);
                const sid=state.devices[target.id]?.sessionId;
                const card=resolveCard(target.id);
                if(!dev || dev.owner_user_id!==intent.owner || dev.target_fingerprint!==target.fingerprint ||
                    dev.run_occupied || !sid || dev.active_session?.id!==sid ||
                    dev.active_session.state!=='view_only' || card?.getSessionId()!==sid || !card?.getStream())
                    throw new Error(target.name+' 原视频/只读权限或目标指纹在提交前变化，整批取消');
            }
            const body:AutomationRunInput={
                request_id:intent.requestId,name:intent.name,steps:JSON.parse(intent.stepHash),
                device_ids:intent.targets.map(t=>t.id),parameters:{...intent.parameters},concurrency:intent.concurrency,
            };
            const proof:BatchSubmissionReceipt={
                ownerId:intent.owner,requestId:intent.requestId,
                deviceIds:[...body.device_ids],createdAt:new Date().toISOString(),
            };
            reserveBatchReceipt(window.sessionStorage,proof);
            state.pendingReceipt=proof;state.phase='submitting';
            // Single POST for the whole immutable batch, never per-device POST.
            try {
                const run=await api.createRun(intent.owner,body);
                if(!valid())return;
                acceptRun(run,intent);
                clearBatchReceipt(window.sessionStorage,proof);
                state.pendingReceipt=null;
                if(state.phase==='running'||state.phase==='cancel_requested')void refresh();
            }catch(error){
                if(!valid())return;
                // 400/422 prove validation failed, not that a phone action ran.
                if(error instanceof AutomationApiError && [400,422].includes(error.status)){
                    clearBatchReceipt(window.sessionStorage,proof);state.pendingReceipt=null;
                    state.phase='failed';
                }else state.phase='unknown';
                throw error;
            }
        }catch(error){
            if(!valid())return;
            if(state.phase!=='unknown')state.phase=state.pendingReceipt?'unknown':'failed';
            state.error=automationErrorMessage(error)+(state.pendingReceipt
                ? '；可能已接收，必须只读查询，绝不自动再次发送。'
                : '；本次未提交完整脚本动作。');
        }
    }
    async function cancelDevice(deviceId:string){
        const run=state.run;
        const old=state.devices[deviceId];
        if(!run || !old || !['running','cancel_requested'].includes(state.phase) ||
            !run.devices.some(device=>device.device_id===deviceId) ||
            ['cancel_requested','cancelled','completed','succeeded','failed','interrupted'].includes(old.phase))
            return;
        const owner=state.owner,epoch=version;
        progress(deviceId,'cancel_requested','已请求停止此手机；等待执行器清理和真实结果');
        try {
            const result=await api.cancelRunDevice(owner,run.id,deviceId);
            if(!still(owner,epoch))return;
            acceptRun(result);
            if(state.phase==='running'||state.phase==='cancel_requested')void refresh();
        }catch(error){
            if(!still(owner,epoch))return;
            // Never automatically retry a possibly accepted stop action.
            progress(deviceId,'outcome_unknown','单机停止回执未知；请仅查询执行历史');
            state.error='单机停止结果未确认，请勿重复请求：'+automationErrorMessage(error);
            if(state.phase==='running'||state.phase==='cancel_requested')void refresh();
        }
    }
    async function cancelAll(){
        const run=state.run;
        if(!run || !['running','cancel_requested'].includes(state.phase))return;
        const owner=state.owner,epoch=version;
        state.phase='cancel_requested';
        try{
            const result=await api.cancelRun(owner,run.id);
            if(!still(owner,epoch))return;
            acceptRun(result);
            // Cancellation requested != stopped; keep polling actual cleanup.
            if(state.phase==='running')state.phase='cancel_requested';
            if(state.phase==='cancel_requested')void refresh();
        }catch(error){
            if(!still(owner,epoch))return;
            state.phase='unknown';state.error='停止请求结果未确认，不能自动重发：'+automationErrorMessage(error);
        }
    }
    function forgetOwner(){
        ++version;stopTimer();cardResolver=null;state.owner=0;state.phase='idle';state.error='';
        state.selected=[];state.devices={};state.run=null;state.pendingReceipt=null;
        // Do not delete persistent request proof.
    }
    onBeforeUnmount(()=>{disposed=true;forgetOwner();preparation.dispose();});
    return {state,busy,start,refresh,reconcile,cancelAll,cancelDevice,inspectMedia,forgetOwner};
}
