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
    });

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
    return { state, guard, refresh, perform };
}
