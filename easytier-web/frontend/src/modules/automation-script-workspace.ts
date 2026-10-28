/**
 * T26–T28: a UI-level adapter around the EXISTING single owner draft, Script API
 * and CAS synchronizer. No second script store, preview or action runner.
 */
import { computed, reactive, watch, type Ref } from 'vue';
import type { AutomationDevice, AutomationScript, AutomationStep } from '../types/automation';
import AutomationApi, { AutomationApiError, automationErrorMessage } from './automation-api';
import { draftFingerprint, type createAutomationDraftSync } from './automation-draft-sync';
import type { AutomationDraftWorkspace } from './automation-draft';
import { v4 as uuid } from 'uuid';

export type ScriptSwitchDecision = 'save' | 'discard';
export type AutomationScriptAction =
    | { kind: 'create'; deviceId: string; name: string; decision?: ScriptSwitchDecision }
    | { kind: 'select'; deviceId: string; scriptId: string; revision: number; decision?: ScriptSwitchDecision }
    | { kind: 'clone'; deviceId: string; scriptId: string; revision: number; name: string; decision?: ScriptSwitchDecision };

type DraftSync = ReturnType<typeof createAutomationDraftSync>;
export type ScriptGuard = 'clean' | 'loading' | 'unsynced' | 'conflict' | 'editor';
export interface FormalScriptWriteReceipt {
    owner: number;
    kind: 'publish' | 'create' | 'delete';
    scriptId: string;
    revision: number;
    /** No script text, selector, secret, parameter or HTTP body is persisted. */
    expectedName?: string;
    expectedStepIds?: string[];
    createdAt: string;
}
function formalReceiptKey(owner:number):string {
    return 'easytier.automation.formal-write.v1.'+owner;
}
function readFormalReceipt(owner:number):FormalScriptWriteReceipt|null {
    const value=window.sessionStorage.getItem(formalReceiptKey(owner));
    if(!value)return null;
    const row:unknown=JSON.parse(value);
    if(!row || typeof row!=='object')throw new Error('脚本写操作凭据损坏，不允许再次发布或删除。');
    const proof=row as Partial<FormalScriptWriteReceipt>;
    if(proof.owner!==owner || !['publish','create','delete'].includes(proof.kind??'') ||
        typeof proof.scriptId!=='string' || !Number.isSafeInteger(proof.revision) ||
        (proof.revision??-1)<(proof.kind==='create'?0:1) || typeof proof.createdAt!=='string')
        throw new Error('脚本写操作身份不完整，禁止新发请求。');
    return proof as FormalScriptWriteReceipt;
}
function reserveFormalReceipt(proof:FormalScriptWriteReceipt):void {
    if(readFormalReceipt(proof.owner))throw new Error('有尚未确认的正式脚本写操作，请先只读核验。');
    window.sessionStorage.setItem(formalReceiptKey(proof.owner),JSON.stringify(proof));
    const stored=readFormalReceipt(proof.owner);
    if(!stored || stored.scriptId!==proof.scriptId || stored.revision!==proof.revision)
        throw new Error('无法确认写操作身份已保存，禁止发送。');
}
function clearFormalReceipt(proof:FormalScriptWriteReceipt):void {
    const current=readFormalReceipt(proof.owner);
    if(!current || current.scriptId!==proof.scriptId ||
        current.revision!==proof.revision || current.kind!==proof.kind)
        throw new Error('正式脚本写操作身份已改变，禁止清除保护。');
    window.sessionStorage.removeItem(formalReceiptKey(proof.owner));
}

function copySteps(steps: AutomationStep[], regenerateIds = false): AutomationStep[] {
    return steps.map(step => ({
        ...step,
        id: regenerateIds ? uuid() : step.id,
        selector: step.selector ? { ...step.selector } : undefined,
        scroll_container: step.scroll_container ? { ...step.scroll_container } : undefined,
    }));
}
function utf8Length(input: string) { return new TextEncoder().encode(input).length; }

export function createAutomationScriptWorkspace(
    api: AutomationApi,
    ownerId: Ref<number>,
    devices: Ref<AutomationDevice[]>,
    workspace: AutomationDraftWorkspace,
    draftSync: DraftSync,
    editorDirty: Ref<boolean>,
    onSelected: () => void,
) {
    const state = reactive({
        items: [] as AutomationScript[],
        busy: false, loading: false, error: '',
        formalPending: null as FormalScriptWriteReceipt|null,
    });
    let listGeneration = 0;
    let actionGeneration = 0;
    /** Network-unknown create/clone is NOT safe to repeat without a receipt. */
    let uncertainAction: { owner: number; kind: 'create' | 'clone'; name: string; source?: string } | null = null;

    const guard = computed<ScriptGuard>(() => {
        if (editorDirty.value) return 'editor';
        if (!draftSync.state.initialized || draftSync.state.ownerId !== ownerId.value) return 'loading';
        if (draftSync.state.phase === 'conflict') return 'conflict';
        return draftSync.canLeave() ? 'clean' : 'unsynced';
    });
    watch(ownerId, () => {
        ++listGeneration; ++actionGeneration;
        state.items = []; state.loading = false;
        state.error = ''; state.busy = false; uncertainAction = null;
        try { state.formalPending=readFormalReceipt(ownerId.value); }
        catch(cause){state.error='正式脚本历史操作核对失败：'+automationErrorMessage(cause);
            // Fail closed, even if storage content cannot be parsed.
            state.formalPending={owner:ownerId.value,kind:'publish',scriptId:'unknown',
                revision:1,createdAt:''};}
    }, {immediate:true});

    async function refresh(): Promise<boolean> {
        const owner = ownerId.value, generation = ++listGeneration;
        if (!owner) return false;
        state.loading = true;
        try {
            const response = await api.listScripts(owner);
            if (generation !== listGeneration || ownerId.value !== owner) return false;
            state.items = response.items;
            state.error = '';
            return true;
        } catch (cause) {
            if (generation === listGeneration && ownerId.value === owner)
                state.error = '读取脚本列表失败：' + automationErrorMessage(cause);
            return false;
        } finally {
            if (generation === listGeneration) state.loading = false;
        }
    }
    function authorized(deviceId: string, owner: number): boolean {
        return !!owner && workspace.ownerId === owner &&
            devices.value.some(device => device.id === deviceId && device.owner_user_id === owner);
    }
    function stillCurrent(owner: number, deviceId: string, generation: number, snapshot: string): boolean {
        return ownerId.value === owner && actionGeneration === generation &&
            authorized(deviceId, owner) && draftFingerprint(workspace) === snapshot;
    }
    function normalizedName(name: string): string | null {
        const trimmed = name.trim();
        if (!trimmed || utf8Length(trimmed) > 128 || trimmed.includes('\0')) {
            state.error = '脚本名称不能为空且最多 128 字节。';
            return null;
        }
        return trimmed;
    }
    function containsName(name: string): boolean {
        return state.items.some(script => script.name.trim().toLocaleLowerCase() === name.toLocaleLowerCase());
    }
    async function perform(input: AutomationScriptAction): Promise<void> {
        if (state.busy) return;
        const owner = ownerId.value;
        // Preserve unknown old formal writes when switching or cloning scripts.
        if(state.formalPending || readFormalReceipt(owner)){
            state.error='有未核实的正式脚本写操作，请先只读核对，避免重复修改服务器。';return;
        }
        if (!authorized(input.deviceId, owner)) { state.error = '当前手机或账号已变化，请重新打开自动化浮窗。'; return; }
        const generation = ++actionGeneration;
        state.busy = true; state.error = '';
        try {
            if (guard.value !== 'clean') {
                if (!input.decision) { state.error = '先确认当前未保存草稿的处理方式。'; return; }
                if (input.decision === 'save') {
                    if (editorDirty.value) { state.error = '步骤参数仍未确认，请先保存编辑或选择明确放弃。'; return; }
                    await draftSync.flush();
                    if (!draftSync.canLeave()) {
                        state.error = draftSync.state.message || '工作草稿尚未完成服务端同步，当前脚本保持不变。';
                        return;
                    }
                } else if (draftSync.state.phase === 'saving') {
                    state.error = '服务器草稿仍在保存中，请等待回执再进行切换。';
                    return;
                }
            }
            const snapshot = draftFingerprint(workspace);
            if (!stillCurrent(owner, input.deviceId, generation, snapshot)) return;
            if (!await refresh() || !stillCurrent(owner, input.deviceId, generation, snapshot)) return;
            let selected: AutomationScript | null = null;
            const beforeIds = new Set(state.items.map(item => item.id));
            if (input.kind === 'select') {
                selected = state.items.find(script => script.id === input.scriptId) ?? null;
                if (!selected) { state.error = '脚本已经删除或无权访问，请刷新列表。'; return; }
                if (selected.revision !== input.revision) { state.error = '脚本修订已变化，请重新查看并选择。'; return; }
                if (selected.id === workspace.editingId && selected.revision === workspace.revision) return;
            } else {
                const name = normalizedName(input.name);
                if (!name) return;
                if (containsName(name)) { state.error = '该名称已有脚本，请换一个名称。'; return; }
                const signature = input.kind === 'create' ? undefined : input.scriptId;
                if (uncertainAction?.owner === owner && uncertainAction.kind === input.kind &&
                    uncertainAction.name === name && uncertainAction.source === signature) {
                    state.error = '上次新建结果仍未知，请先核对服务器列表，不要再次提交同一请求。';
                    return;
                }
                let from: AutomationScript | null = null;
                if (input.kind === 'clone') {
                    from = state.items.find(script => script.id === input.scriptId) ?? null;
                    if (!from || from.revision !== input.revision) {
                        state.error = '克隆源脚本已修改，请刷新并重新确认副本内容。';
                        return;
                    }
                }
                const body = {
                    name, description: from?.description ?? '',
                    steps: from ? copySteps(from.steps, true) : [],
                };
                try {
                    selected = await api.saveScript(owner, body);
                } catch (cause) {
                    const definitive = cause instanceof AutomationApiError &&
                        [400,401,403,404,409,422].includes(cause.status);
                    if (!definitive) {
                        uncertainAction = { owner, kind: input.kind, name, source: signature };
                        // An uncertain response is not an invitation to send a second POST.
                        // Reconcile by looking for a new ID in the fresh server list.
                        if (await refresh() && stillCurrent(owner, input.deviceId, generation, snapshot)) {
                            const matches = state.items.filter(script =>
                                !beforeIds.has(script.id) && script.name === name);
                            if (matches.length === 1) {
                                selected = matches[0];
                                uncertainAction = null;
                            }
                        }
                    }
                    if (!selected) {
                        state.error = definitive
                            ? automationErrorMessage(cause)
                            : '创建结果尚未确认。未自动重试，请刷新脚本列表核对后再操作。';
                        return;
                    }
                }
                if (!stillCurrent(owner, input.deviceId, generation, snapshot)) return;
                state.items = [selected, ...state.items.filter(item => item.id !== selected!.id)];
            }
            if (!selected || !stillCurrent(owner, input.deviceId, generation, snapshot)) return;
            if (!draftSync.adoptSavedScript(selected, input.decision === 'discard')) {
                state.error = '旧草稿发生更新，未切换工作区；新建的脚本可在列表中重新选择。';
                return;
            }
            onSelected();
            state.error = '';
        } catch (cause) {
            if (ownerId.value === owner && generation === actionGeneration)
                state.error = automationErrorMessage(cause);
        } finally {
            if (generation === actionGeneration) state.busy = false;
        }
    }
    function ensureFormalScope(deviceId:string): {owner:number; scriptId?:string; revision?:number} {
        const owner=ownerId.value, scriptId=workspace.editingId,revision=workspace.revision;
        if(!authorized(deviceId,owner) || (scriptId && (!revision ||
            !Number.isSafeInteger(revision) || revision<1)))
            throw new Error('请选择当前账号有效的正式脚本修订。');
        if(editorDirty.value || guard.value!=='clean'||!draftSync.canLeave())
            throw new Error('正式发布前请完成步骤编辑及服务端工作草稿同步；不能覆盖未保存的修改。');
        if(state.formalPending || readFormalReceipt(owner))
            throw new Error('已有未知结果的正式脚本写操作，请先只读核对。');
        return {owner,scriptId,revision};
    }
    async function publish(deviceId:string,formalName:string,formalDescription:string):Promise<void> {
        if(state.busy)return;
        let scope:{owner:number;scriptId?:string;revision?:number};
        try{scope=ensureFormalScope(deviceId);}
        catch(cause){state.error=automationErrorMessage(cause);return;}
        const {owner,scriptId,revision}=scope;
        const desiredName=normalizedName(formalName);
        if(!desiredName)return;
        if(utf8Length(formalDescription)>2000){
            state.error='脚本说明不能超过 2000 字节。';return;
        }
        const snapshot=draftFingerprint(workspace), generation=++actionGeneration;
        const cloned=copySteps(workspace.steps),expectedIds=cloned.map(s=>s.id);
        if(cloned.length>100 || cloned.some(s=>!s.id)){
            state.error='当前脚本步骤编号无效，禁止正式保存。';return;
        }
        state.busy=true;state.error='';
        let proof:FormalScriptWriteReceipt|null=null;
        try{
            const latest=await api.listScripts(owner);
            if(!stillCurrent(owner,deviceId,generation,snapshot)||!draftSync.canLeave())return;
            const source=latest.items.find(s=>s.id===scriptId);
            if(scriptId && (!source || source.revision!==revision))
                throw new Error('正式脚本已经被其他窗口修改或删除；未发布，请刷新并核对修订。');
            if(!scriptId && latest.items.some(s=>s.name===desiredName))
                throw new Error('已有同名正式脚本，请选择该脚本或使用不同名称发布。');
            const payload={name:desiredName,description:formalDescription,steps:cloned};
            if(source && source.name===payload.name && source.description===payload.description &&
                JSON.stringify(source.steps)===JSON.stringify(payload.steps)){
                state.error='当前正式脚本没有需要发布的修改。';return;
            }
            proof={owner,kind:scriptId?'publish':'create',scriptId:scriptId??('new:'+uuid()),
                revision:revision??0,expectedName:desiredName,
                expectedStepIds:expectedIds,createdAt:new Date().toISOString()};
            reserveFormalReceipt(proof);
            state.formalPending=proof;
            const saved=await api.saveScript(owner,
                scriptId?{...payload,revision}:payload,scriptId);
            if((scriptId && saved.id!==scriptId) ||
                saved.revision!==(scriptId?revision!+1:1) ||
                saved.steps.length!==expectedIds.length ||
                saved.steps.some((s,i)=>s.id!==expectedIds[i]))
                throw new Error('正式脚本写入回执身份/步骤不匹配，不允许再次发布。');
            // Commit has happened; only clear the pending proof if THIS exact
            // result is matched. If the page changed, keep it for read-only recovery.
            if(!stillCurrent(owner,deviceId,generation,snapshot))return;
            clearFormalReceipt(proof);state.formalPending=null;
            state.items=[saved,...state.items.filter(s=>s.id!==saved.id)];
            if(!draftSync.adoptSavedScript(saved)){
                state.error='正式脚本已保存，但工作草稿切换未确认。请只读刷新并重新选择。';return;
            }
            onSelected();state.error='';
        }catch(cause){
            if(ownerId.value!==owner||generation!==actionGeneration)return;
            if(proof && cause instanceof AutomationApiError &&
                [400,401,403,404,409,422].includes(cause.status)){
                // These are definitive server rejections of this PUT.
                clearFormalReceipt(proof);state.formalPending=null;
            }
            state.error=automationErrorMessage(cause)+(state.formalPending
                ? '；结果可能已提交，已锁定正式脚本写操作，请只读核对，不要重试。':'');
        }finally{
            if(ownerId.value===owner&&generation===actionGeneration)state.busy=false;
        }
    }
    async function remove(deviceId:string,scriptId:string,revision:number):Promise<void> {
        if(state.busy)return;
        const owner=ownerId.value;
        if(!authorized(deviceId,owner)||!scriptId||!Number.isSafeInteger(revision)||revision<1){
            state.error='删除请求不属于当前授权设备或有效修订。';return;
        }
        if(guard.value!=='clean'||!draftSync.canLeave()){
            state.error='有未同步的工作草稿，先保存/解决冲突再删除正式脚本。';return;
        }
        if(state.formalPending || readFormalReceipt(owner)){
            state.error='旧正式脚本写操作结果尚未知，请先核实再删除。';return;
        }
        const generation=++actionGeneration,snapshot=draftFingerprint(workspace);
        state.busy=true;state.error='';
        let proof:FormalScriptWriteReceipt|null=null;
        try {
            const latest=await api.listScripts(owner);
            if(!stillCurrent(owner,deviceId,generation,snapshot)||!draftSync.canLeave())return;
            const script=latest.items.find(item=>item.id===scriptId);
            if(!script||script.revision!==revision)
                throw new Error('准备删除的正式脚本修订已变化，请刷新列表后重新确认。');
            proof={owner,kind:'delete',scriptId,revision,createdAt:new Date().toISOString()};
            reserveFormalReceipt(proof);state.formalPending=proof;
            const response=await api.deleteScriptVersioned(owner,scriptId,revision);
            if(!response.deleted||response.id!==scriptId||response.revision!==revision)
                throw new Error('删除回执身份不匹配，请只读核对。');
            if(!stillCurrent(owner,deviceId,generation,snapshot))return;
            clearFormalReceipt(proof);state.formalPending=null;
            state.items=state.items.filter(item=>item.id!==scriptId);
            if(workspace.editingId===scriptId){
                // This is a copy of editable steps, not an overwrite of old
                // server history. Detach the deleted formal ID and fork a new
                // CAS work draft so the operator's current steps are retained.
                draftSync.fork();
                workspace.editingId=undefined; workspace.revision=undefined;
                workspace.baseline=JSON.stringify({name:workspace.name,
                    description:workspace.description,steps:workspace.steps});
                onSelected();
            }
            state.error='';
        }catch(cause){
            if(ownerId.value!==owner||generation!==actionGeneration)return;
            if(proof && cause instanceof AutomationApiError &&
                [400,401,403,404,409,422].includes(cause.status)){
                clearFormalReceipt(proof);state.formalPending=null;
            }
            state.error=automationErrorMessage(cause)+(state.formalPending
                ? '；删除结果未知，保留请求身份；请先只读核对，禁止再次删除。':'');
        }finally{
            if(ownerId.value===owner&&generation===actionGeneration)state.busy=false;
        }
    }
    async function reconcileFormal():Promise<void> {
        if(state.busy)return;
        const owner=ownerId.value;
        state.busy=true;state.error='';
        try{
            const proof=readFormalReceipt(owner);
            if(!proof){state.formalPending=null;return;}
            const received=await api.listScripts(owner);
            if(owner!==ownerId.value)return;
            const matches=received.items.filter(item=>proof.kind==='create'
                ? item.name===proof.expectedName && item.revision===1 &&
                    item.steps.length===proof.expectedStepIds?.length &&
                    item.steps.every((s,i)=>s.id===proof.expectedStepIds?.[i])
                : item.id===proof.scriptId);
            const script=matches[0];
            // Never infer a missing POST from list pagination or a stale
            // revision. Only an unambiguous creation or exact CAS revision
            // and step-ID set is enough to clear the persisted refusal.
            const confirmed=proof.kind==='delete'
                ? !script
                : !!script && matches.length===1 &&
                    script.revision===(proof.kind==='create'?1:proof.revision+1) &&
                    script.name===proof.expectedName &&
                    script.steps.length===proof.expectedStepIds?.length &&
                    script.steps.every((s,i)=>s.id===proof.expectedStepIds?.[i]);
            if(!confirmed){
                state.formalPending=proof;
                state.error='服务端尚不能唯一确认原'+(proof.kind==='delete'?'删除':'发布')+
                    '请求结果；继续阻断重复修改，请人工核对版本与历史记录。';
                return;
            }
            if(proof.kind==='delete' && workspace.editingId===proof.scriptId){
                if(!draftSync.canLeave())
                    throw new Error('删除已经在服务器确认，但本页草稿仍在同步；先完成草稿同步再重新核对以安全解绑。');
                draftSync.fork();
                workspace.editingId=undefined;workspace.revision=undefined;
                workspace.baseline=JSON.stringify({name:workspace.name,
                    description:workspace.description,steps:workspace.steps});
                onSelected();
            }
            clearFormalReceipt(proof);state.formalPending=null;
            state.items=received.items;
            state.error='已只读核对原请求，无新的正式写入；请重新选择当前正式修订。';
            // Reconciliation never silently overwrites locally edited steps.
        }catch(cause){if(owner===ownerId.value){
            state.error='只读核对失败：'+automationErrorMessage(cause);
        }}finally{if(owner===ownerId.value)state.busy=false;}
    }
    return { state, guard, refresh, perform, publish, remove, reconcileFormal };
}
