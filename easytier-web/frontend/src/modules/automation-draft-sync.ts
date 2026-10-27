/**
 * T11 - single shared workspace, account-scoped server draft synchronization.
 * No browser storage: selectors, steps and runtime parameters are never
 * persisted in localStorage/sessionStorage/IndexedDB.
 */
import { reactive, watch, type WatchStopHandle } from 'vue';
import { v4 as uuid } from 'uuid';
import type { AutomationStep, AutomationDraftRecord } from '../types/automation';
import AutomationApi, { AutomationApiError, automationErrorMessage } from './automation-api';
import { emptyAutomationDraft, type AutomationDraftWorkspace } from './automation-draft';

export type DraftSyncPhase = 'loading' | 'idle' | 'saving' | 'synced' | 'unsynced' | 'conflict';
export interface DraftSummary {
    id: string; name: string; revision: number; updatedAt: string; count: number;
}
export interface DraftSyncState {
    ownerId: number;
    id: string | null;
    revision: number | null;
    phase: DraftSyncPhase;
    message: string;
    entries: DraftSummary[];
    initialized: boolean;
}
export interface DraftSnapshot {
    name: string; description: string; steps: AutomationStep[];
    script_id: string | null; script_revision: number | null;
}
interface RequestInFlight {
    owner: number; id: string | null; revision: number | null;
    requestId: string; payload: DraftSnapshot; fingerprint: string;
}
export function draftFingerprint(workspace: Pick<AutomationDraftWorkspace,'name'|'description'|'steps'|'editingId'|'revision'>): string {
    return JSON.stringify({
        name: workspace.name, description: workspace.description, steps: workspace.steps,
        script_id: workspace.editingId ?? null, script_revision: workspace.revision ?? null,
    });
}
function toSummary(record: AutomationDraftRecord): DraftSummary {
    return {id:record.id,name:record.name,revision:record.revision,
        updatedAt:record.updated_at,count:record.steps.length};
}
function snapshotOf(workspace: AutomationDraftWorkspace): DraftSnapshot {
    // Immutable wire body: retries must use the original request_id AND body.
    return JSON.parse(draftFingerprint(workspace)) as DraftSnapshot;
}
const initialFingerprint=JSON.stringify({name:'',description:'',steps:[],script_id:null,script_revision:null});

/** Mutations are serialized per live owner/workspace. A failed request retains
 * the exact payload and UUID for a safe manual or 'online' retry. */
export function createAutomationDraftSync(api: AutomationApi, workspace: AutomationDraftWorkspace) {
    const state=reactive<DraftSyncState>({
        ownerId:0,id:null,revision:null,phase:'idle',message:'',
        entries:[],initialized:false,
    });
    let epoch=0, active=false, destroyed=false, scheduled:ReturnType<typeof setTimeout>|null=null;
    let pending:RequestInFlight|null=null;
    let busy:Promise<void>|null=null;
    let confirmed=initialFingerprint;
    let suspended=false;
    let lastIdentity='';
    function currentSnapshot() {return draftFingerprint(workspace);}
    function ownerMatches(owner:number,e:number) {
        return !destroyed && epoch===e && state.ownerId===owner && workspace.ownerId===owner;
    }
    function clearTimer() {if(scheduled!==null){clearTimeout(scheduled);scheduled=null;}}
    function markChanged() {
        if(!active || !state.initialized || !ownerMatches(state.ownerId,epoch) || suspended)return;
        if(currentSnapshot()===confirmed && !pending) {
            state.phase=state.id?'synced':'idle';state.message='';return;
        }
        if(state.phase==='conflict')return;
        if(state.phase!=='saving')state.phase='unsynced';
        if(!pending && state.phase!=='saving') schedule();
    }
    function schedule(delay=450) {
        clearTimer();
        if(!active || pending || busy || state.phase==='conflict')return;
        scheduled=setTimeout(()=>{scheduled=null;void flush();},delay);
    }
    function replaceWorkspace(record:AutomationDraftRecord) {
        suspended=true;
        // Preserve source script identity for an existing saved script.
        // The formal script is STILL only written by /scripts CAS on publish.
        workspace.editingId=record.script_id??undefined;
        workspace.revision=record.script_revision??undefined;
        workspace.name=record.name;workspace.description=record.description;
        workspace.steps=record.steps.map(step=>({...step,selector:step.selector?{...step.selector}:undefined,
            scroll_container:step.scroll_container?{...step.scroll_container}:undefined}));
        workspace.selectedStepId=workspace.steps[0]?.id;
        workspace.baseline=initialFingerprint;
        suspended=false;
        lastIdentity=String(workspace.editingId??'')+':'+String(workspace.revision??'');
        confirmed=currentSnapshot();
        state.id=record.id;state.revision=record.revision;
        state.phase='synced';state.message='';
        const summary=toSummary(record);
        state.entries=[summary,...state.entries.filter(entry=>entry.id!==record.id)];
    }
    async function load(owner:number) {
        const e=++epoch;clearTimer();active=false;pending=null;busy=null;
        state.ownerId=owner;state.initialized=false;state.id=null;
        lastIdentity=String(workspace.editingId??'')+':'+String(workspace.revision??'');
        state.revision=null;state.entries=[];state.message='';
        state.phase=owner>0?'loading':'idle';confirmed=currentSnapshot();
        if(owner<=0)return;
        const initial=currentSnapshot();
        try{
            const response=await api.listDrafts(owner);
            if(!ownerMatches(owner,e))return;
            state.entries=response.items.map(toSummary);
            // Initial restoration is safe only before any user input in the
            // newly opened account has changed the blank workspace.
            if(initial===currentSnapshot() && state.entries.length &&
                initial===initialFingerprint) {
                const latest=await api.getDraft(owner,state.entries[0].id);
                if(!ownerMatches(owner,e))return;
                if(currentSnapshot()===initial) replaceWorkspace(latest);
            }
            if(!ownerMatches(owner,e))return;
            confirmed=state.id?currentSnapshot():initialFingerprint;
            state.initialized=true;active=true;
            if(!state.id) {
                state.phase=currentSnapshot()===initialFingerprint?'idle':'unsynced';
                if(state.phase==='unsynced')schedule();
            }
        }catch(error) {
            if(!ownerMatches(owner,e))return;
            state.initialized=true;active=true;state.phase='unsynced';
            state.message='读取服务器草稿失败：'+automationErrorMessage(error)+'；不会覆盖当前本地内容。';
        }
    }
    function changeFormalIdentity() {
        const identity=String(workspace.editingId??'')+':'+String(workspace.revision??'');
        if(identity===lastIdentity)return;
        lastIdentity=identity;
        if(!active || suspended || !state.initialized)return;
        // Switching formal scripts must not edit the old remote draft.
        // Pending writes are immutable snapshots of the OLD document.
        clearTimer();
        pending=null;state.id=null;state.revision=null;
        confirmed=workspace.editingId && currentSnapshot()===workspace.baseline
            ? currentSnapshot():initialFingerprint;
        state.phase=currentSnapshot()===confirmed?'idle':'unsynced';
        state.message='';
        if(state.phase==='unsynced')schedule();
    }
    const stopWatch:WatchStopHandle=watch(()=>draftFingerprint(workspace),markChanged,{flush:'post'});
    const stopIdentity:WatchStopHandle=watch(()=>[
        workspace.ownerId, workspace.editingId,workspace.revision] as const,
        ([owner])=>{if(owner!==state.ownerId){void load(owner);return;}changeFormalIdentity();},
        {flush:'post',immediate:true});
    function canLeave(): boolean {
        if(!state.initialized || !active) return false;
        return (state.phase==='idle' || state.phase==='synced') &&
            currentSnapshot()===confirmed && !busy && !pending;
    }
    function scheduleFollowup(owner:number,e:number) {
        // This function re-reads the reactive phase after async completion,
        // avoiding TS control-flow retaining the earlier 'saving' assignment.
        if(ownerMatches(owner,e)&&!pending&&state.phase==='unsynced'&&!state.message)
            schedule(150);
    }
    async function flush():Promise<void> {
        clearTimer();
        if(!active || !state.initialized || state.ownerId<=0 ||
            workspace.ownerId!==state.ownerId || state.phase==='conflict')return;
        if(busy)return busy;
        if(!pending && currentSnapshot()===confirmed)return;
        const owner=state.ownerId,e=epoch;
        if(!pending) {
            pending={owner,id:state.id,revision:state.revision,requestId:uuid(),
                payload:snapshotOf(workspace),fingerprint:currentSnapshot()};
        }
        const item=pending;
        state.phase='saving';state.message='';
        const task=(async()=>{
            try{
                const body={request_id:item.requestId,...item.payload};
                const received=item.id
                    ? await api.saveDraft(owner,{...body,revision:item.revision!},item.id)
                    : await api.saveDraft(owner,body);
                if(!ownerMatches(owner,e) || pending!==item)return;
                // T10 retries return compact metadata instead of a duplicate
                // payload; only the original response has name/steps.
                if('request_replayed' in received && received.request_replayed){
                    const latest=await api.getDraft(owner,received.id);
                    if(!ownerMatches(owner,e)||pending!==item)return;
                    if(latest.revision!==received.revision){
                        pending=null;state.phase='conflict';
                        state.message='该草稿已由其他页面更新；本地步骤未覆盖服务器。';
                        return;
                    }
                }
                if(!ownerMatches(owner,e)||pending!==item)return;
                state.id=received.id;state.revision=received.revision;
                confirmed=item.fingerprint;pending=null;
                state.entries=[
                    {id:received.id,name:item.payload.name,revision:received.revision,
                        count:item.payload.steps.length,updatedAt:new Date().toISOString()},
                    ...state.entries.filter(entry=>entry.id!==received.id),
                ];
                state.phase=currentSnapshot()===confirmed?'synced':'unsynced';
                state.message='';
            }catch(error){
                if(!ownerMatches(owner,e)||pending!==item)return;
                if(error instanceof AutomationApiError &&
                    error.status===409 && error.code==='draft_revision_changed'){
                    pending=null;state.phase='conflict';
                    state.message='版本冲突：另一页面已保存更新。当前修改留在此页面，请选择恢复服务端版本或另存新草稿。';
                }else if(error instanceof AutomationApiError &&
                    [401,403,404,422].includes(error.status)){
                    // Validation or permission failures are deterministic;
                    // if data changes, the old request must be discarded.
                    pending=null;state.phase='unsynced';
                    state.message='未同步：'+automationErrorMessage(error);
                }else{
                    // Network failure/timeout is unknown-outcome. Keep UUID
                    // and IDENTICAL body until the same request is retried.
                    state.phase='unsynced';
                    state.message='保存结果未确认：'+automationErrorMessage(error)+
                        '。网络恢复后会以相同请求编号重试。';
                }
            }
        })();
        busy=task;
        await task;
        if(busy===task)busy=null;
        scheduleFollowup(owner,e);
    }
    async function retry():Promise<void>{
        if(!active)return;
        if(state.phase==='conflict')return;
        state.message='';state.phase='unsynced';
        await flush();
    }
    async function refreshList():Promise<void> {
        if(!active)return;
        const owner=state.ownerId,e=epoch;
        try{
            const response=await api.listDrafts(owner);
            if(ownerMatches(owner,e))state.entries=response.items.map(toSummary);
        }catch(error){if(ownerMatches(owner,e))state.message='刷新草稿列表失败：'+automationErrorMessage(error);}
    }
    async function restore(id:string):Promise<boolean>{
        if(!active || !id || state.phase==='saving' || !!busy)return false;
        const owner=state.ownerId,e=epoch;
        // Freeze the local workspace AND server draft identity before GET.
        // A user can append/edit/replace steps while a slow GET is in flight.
        // Never overwrite those edits with a stale response.
        const initial=currentSnapshot(),oldId=state.id,oldRevision=state.revision;
        const oldPending=pending,oldPhase=state.phase;
        try{
            const record=await api.getDraft(owner,id);
            if(!ownerMatches(owner,e))return false;
            if(currentSnapshot()!==initial || state.id!==oldId ||
                state.revision!==oldRevision || pending!==oldPending ||
                state.phase!==oldPhase || !!busy){
                state.message='读取草稿期间，本页步骤或服务器修订状态已变化；已保留当前修改，请重新选择恢复。';
                return false;
            }
            clearTimer();pending=null;replaceWorkspace(record);
            return true;
        }catch(error){
            if(ownerMatches(owner,e))state.message='恢复失败：'+automationErrorMessage(error);
            return false;
        }
    }
    function fork():void {
        if(!active || busy)return;
        clearTimer();pending=null;state.id=null;state.revision=null;
        confirmed=initialFingerprint;state.phase='unsynced';state.message='';
        schedule(30);
    }
    async function discard(id:string):Promise<boolean>{
        if(!active || busy || (id===state.id && !!pending))return false;
        const owner=state.ownerId,e=epoch;
        try{
            const item=state.entries.find(d=>d.id===id);
            if(!item)return false;
            await api.discardDraft(owner,id,{request_id:uuid(),revision:item.revision});
            if(!ownerMatches(owner,e))return false;
            state.entries=state.entries.filter(d=>d.id!==id);
            if(state.id===id) {
                clearTimer();pending=null;suspended=true;
                Object.assign(workspace,emptyAutomationDraft(owner));
                state.id=null;state.revision=null;
                confirmed=initialFingerprint;
                lastIdentity=String(workspace.editingId??'')+':'+String(workspace.revision??'');
                suspended=false;state.phase='idle';state.message='';
            }
            return true;
        }catch(error){
            if(ownerMatches(owner,e))state.message='删除失败：'+automationErrorMessage(error);
            return false;
        }
    }
    function online(){if(active&&state.phase==='unsynced')void retry();}
    window.addEventListener('online',online);
    function dispose(){
        destroyed=true;epoch++;active=false;clearTimer();pending=null;
        window.removeEventListener('online',online);
        stopWatch();stopIdentity();
    }
    return {state,flush,retry,refreshList,restore,fork,discard,canLeave,dispose};
}
