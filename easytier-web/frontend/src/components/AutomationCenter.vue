<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue';
import { Button, Dialog, InputText, Message, Textarea } from 'primevue';
import { useI18n } from 'vue-i18n';
import { onBeforeRouteLeave, useRouter } from 'vue-router';
import { v4 as uuid } from 'uuid';
import { DiscoveryApi, type EndpointEvent } from '../modules/discovery-api';
import ApiClient from '../modules/api';
import { getApiBase } from '../modules/api-host';
import type { AssetDevice, AssetNode, GatewayGroup } from '../types/node-assets';
import DiscoveryDialog from './node-assets/DiscoveryDialog.vue';
import AutomationApi, { AutomationApiError, automationErrorMessage } from '../modules/automation-api';
import type { AutomationContext, AutomationControl, AutomationControlReply, AutomationDevice, AutomationRun, AutomationStep, AutomationMediaSettings, AutomationComponentStatus, AutomationPhoneConnectionState } from '../types/automation';
import PhoneCard from './automation/PhoneCard.vue';
import { menuToInspectorAction } from '../modules/automation-inspector-actions';
import ConnectionDialog from './automation/ConnectionDialog.vue';
import ScriptLibrary from './automation/ScriptLibrary.vue';
import { useFloatingBatchRun, freezeFloatingBatchIntent } from '../modules/useFloatingBatchRun';
import RunHistory from './automation/RunHistory.vue';
import ExecutorSettings from './automation/ExecutorSettings.vue';
import { automationMessages } from './automation/messages';
import { analyzeAutomationFlow } from '../modules/automation-flow';
import { appendAutomationDraftSteps, appendPickedAutomationStep, emptyAutomationDraft } from '../modules/automation-draft';
import { createAutomationDraftSync } from '../modules/automation-draft-sync';
import { createAutomationScriptWorkspace, type AutomationScriptAction } from '../modules/automation-script-workspace';
import { releaseOwnedTrialPreviews } from '../modules/automation-run-handoff';
import {
    beginPicker, readyPicker, stopPicker, setPickerIntent, invalidatePickerSnapshot,
    pickerMatches, pickerCanApply,
    type PickerBinding, type PickerContext, type PickerIntent, type PickerVideoPoint, type PickerMenuAction,
} from '../modules/automation-picker-state';
import {
    createAutomationRepairRequest, createCardRepairContext, cardRepairWorkspace,
    planAutomationRepair, type CardRepairContext, type AutomationRepairRequest,
} from '../modules/automation-run-repair';
import { replaceAutomationStepTarget, stepCanPickTarget, type StepTarget } from '../modules/automation-step-target';
import { updateCardDraftStep } from '../modules/automation-card-step-edit';
import { planAutomationStepDeletion } from '../modules/automation-card-step-delete';
import { canAccessAutomationCard } from '../modules/automation-floating-contract';
import { locateReadOnlyStep, type LocatorDemoResult } from '../modules/automation-locator-demo';
import type { AutomationStepType } from '../types/automation';
import {
    beginCapturedGesture, observeCapturedGesture, finishCapturedGesture,
    type CapturedGesture, type GestureRecordingResult,
} from '../modules/automation-gesture';

defineOptions({ name: 'AutomationCenter' });

interface PhoneHandle {
    endpointChanged(event: EndpointEvent): Promise<void>;
    open(quality: 'wall' | 'focus', settings: AutomationMediaSettings): Promise<void>;
    close(): Promise<void>;
    cancelRestoreIntent(): void;
    send(message: AutomationControl): Promise<AutomationControlReply>;
    unlock(): Promise<{ unlock_state: string }>;
    getState(): { phase: string; ready: boolean; viewerOnly?: boolean; width: number; height: number };
    getStream(): MediaStream | null;
    getSessionId(): string | null;
    enterViewOnly(): Promise<void>;
    resumeManual(): Promise<void>;
    cancelPointer(): void;
    resetPointer(): void;
}
interface SyncSession { masterId: string; deviceIds: string[]; blocked: string[]; geometry: Record<string, { width: number; height: number }> }
const { t } = useI18n({ useScope: 'local', messages: automationMessages });
const router = useRouter();
const api = new AutomationApi();
const discoveryApi = new DiscoveryApi();
const assetApi = new ApiClient(getApiBase());
const sourceIssues = ref<{device_id:string;machine_id:string;name:string;reason:string}[]>([]);
let stopEvents: (() => void) | undefined;
let eventOwner = 0;
function subscribeEndpoints() {
    if (eventOwner === ownerId.value && stopEvents) return;
    stopEvents?.(); eventOwner = ownerId.value;
    stopEvents = discoveryApi.subscribe(eventOwner, event => {
        if (disposed || event.owner_user_id !== ownerId.value || tab.value !== 'workbench') return;
        void cards.get(event.device_id)?.endpointChanged(event);
        // Only a committed endpoint event refreshes the list; no polling.
        void loadDevices();
    }, () => { stopEvents = undefined; if (!disposed) notice.value = '设备变更通知已断开，点击刷新重新订阅。'; });
}
const context = ref<AutomationContext | null>(null);
const accounts = ref<{ id: number; username: string }[]>([]);
const ownerId = ref(0);
const devices = ref<AutomationDevice[]>([]);
const selectedIds = ref<string[]>([]);
const phases = ref<Record<string, string>>({});
const tab = ref<'workbench' | 'scripts' | 'history' | 'settings'>('workbench');
const settingsVisited = ref(false);
const tabs = computed(() => context.value?.can_manage_settings
    ? ['workbench', 'scripts', 'history', 'settings'] as const
    : ['workbench', 'scripts', 'history'] as const);
const busy = ref(false); const connecting = ref(false); const error = ref(''); const notice = ref('');
const phoneConnecting = ref(false);
const componentChecking = ref(false);
const componentStatuses = ref<Record<string, AutomationComponentStatus>>({});
const componentStatusResults = ref<{ id: string; name: string; state: string; message?: string }[]>([]);
const componentStateLabel: Record<string, string> = {
    ready: '已就绪', not_prepared: '未准备', preparing: '准备中',
    adb_unavailable: 'ADB离线', unavailable: '控件不可用',
    checking: '查询中', query_failed: '查询失败',
};
const phoneConnections = ref<Record<string, AutomationPhoneConnectionState>>({});
const adbActionVersions = new Map<string, number>();
let statusQueryGeneration = 0;
const phoneConnectionErrors = ref<Record<string, string>>({});
const discoveryGateway = ref<AssetDevice | null>(null);
const discoveryBusy = ref(false);
const catalogNodes = ref<AssetNode[]>([]);
const catalogGroups = ref<GatewayGroup[]>([]);
let catalogOwner = 0;
let catalogGeneration = 0;
let initialSelectionDone = false;
let initialConnectDone = false;
const switchingTab = ref(false);
const quality = ref<'wall' | 'focus'>('wall');
const mediaPreferenceKey = 'easytier.automation.media.v1';
const mediaSettings = ref<AutomationMediaSettings>({
    max_size: 960, video_bit_rate: 2_000_000, max_fps: 30,
    low_latency: true, screen_off: false,
});
function restoreMediaPreferences() {
    try {
        const saved = JSON.parse(window.localStorage.getItem(mediaPreferenceKey) ?? 'null') as Partial<AutomationMediaSettings> | null;
        if (!saved || typeof saved !== 'object') return;
        const permittedSizes = [0, 640, 960, 1280, 1920, 2560, 3840];
        const permittedBitrates = [1, 2, 4, 8, 12, 16, 20, 24].map(mbps => mbps * 1_000_000);
        const permittedFps = [15, 24, 30, 60, 90, 120];
        if (permittedSizes.includes(saved.max_size as number)) mediaSettings.value.max_size = saved.max_size!;
        if (permittedBitrates.includes(saved.video_bit_rate as number)) mediaSettings.value.video_bit_rate = saved.video_bit_rate!;
        if (permittedFps.includes(saved.max_fps as number)) mediaSettings.value.max_fps = saved.max_fps!;
        if (typeof saved.low_latency === 'boolean') mediaSettings.value.low_latency = saved.low_latency;
        if (typeof saved.screen_off === 'boolean') mediaSettings.value.screen_off = saved.screen_off;
    } catch { /* Browser storage may be disabled; session defaults remain usable. */ }
}
watch(mediaSettings, value => {
    try { window.localStorage.setItem(mediaPreferenceKey, JSON.stringify(value)); }
    catch { /* Storage is optional. */ }
}, { deep: true });
const filters = ref({ node: '', group: '', type: '', keyword: '' });
const applied = ref({ ...filters.value });
const connectionDevice = ref<AutomationDevice | null>(null);
const globalOpen = ref(false);
const syncPreparation = ref(false); const syncMaster = ref(''); const syncTargets = ref<string[]>([]);
const sync = ref<SyncSession | null>(null);
const recordingId = ref<string | null>(null); const recordingPaused = ref(false); const recordedSteps = ref<AutomationStep[]>([]);
const draftWorkspace = reactive(emptyAutomationDraft(0));
const draftSync = createAutomationDraftSync(api,draftWorkspace);
const floatingBatch = useFloatingBatchRun(api);
// A reload may have an unknown POST; reconcile by GET before a new submit.
watch(ownerId,owner=>{
    floatingBatch.forgetOwner();
    if(owner>0)void floatingBatch.reconcile(owner);
});
const draftSyncState=draftSync.state;
const draftSyncLabel=computed(()=>{
    const state=draftSyncState;
    if(state.phase==='loading')return '正在检查服务端草稿…';
    if(state.phase==='saving')return '工作草稿保存中…';
    if(state.phase==='synced')return '工作草稿已同步 · 修订 '+(state.revision??'—');
    if(state.phase==='conflict')return '草稿版本冲突 · 本地修改未覆盖服务端';
    if(state.phase==='unsynced')return '工作草稿未同步 · 请检查状态';
    return '工作草稿未创建';
});
const chosenDraftId=ref('');
watch(()=>draftSyncState.entries.map(item=>item.id).join(','),()=>{
    chosenDraftId.value=draftSyncState.id??draftSyncState.entries[0]?.id??'';
});
function hasLocalDraftData():boolean {
    return !!(draftWorkspace.steps.length || draftWorkspace.name.trim() ||
        draftWorkspace.description.trim() || draftWorkspace.editingId);
}
function warnBeforeDraftReplacement():boolean {
    if(draftSync.canLeave() && !cardEditorDirty.value)return true;
    if(!hasLocalDraftData() && !cardEditorDirty.value)return true;
    const situation=!draftSyncState.initialized
        ? '工作草稿仍在加载或同步状态尚未确认。'
        : draftSyncState.phase==='conflict'
            ? '多页面修订冲突，当前修改尚未安全合并到服务器。'
            : draftSyncState.phase==='saving' || draftSyncState.phase==='unsynced'
                ? '当前修改尚未确认保存到服务器。'
                : '卡片内仍有未提交的参数修改。';
    return window.confirm(situation+
        '切换内容可能丢失本页的未落库修改；已确认保存在服务端的旧草稿不会被删除。是否放弃并继续？');
}
async function restoreSavedDraft(id=chosenDraftId.value) {
    if(!id||!warnBeforeDraftReplacement())return;
    const okay=await draftSync.restore(id);
    if(!okay) {error.value=draftSyncState.message||'恢复服务端草稿失败。';return;}
    stopAllPicking();closeInspector();cardStepEditor.value=null;
    chosenDraftId.value=id;
    notice.value='已从服务端恢复草稿，步骤 ID 与顺序保持不变；需要正式执行时仍要保存脚本。';
}
async function discardSavedDraft() {
    const id=chosenDraftId.value;
    if(!id || !window.confirm('删除此服务端工作草稿？如果这是当前草稿，编辑区也会清空。此操作不可撤销。'))return;
    if(!warnBeforeDraftReplacement())return;
    if(!await draftSync.discard(id)) {error.value=draftSyncState.message||'草稿删除失败';return;}
    stopAllPicking();closeInspector();
    notice.value='服务端工作草稿已删除。';
}
function forkCurrentDraft() {
    if(!window.confirm('将当前本地步骤另存为新的工作草稿？原服务端草稿及其修订不会被覆盖。'))return;
    draftSync.fork();
    notice.value='已转为独立工作草稿，正在尝试保存；请确认同步状态。';
}
function beforeUnloadDraft(event:BeforeUnloadEvent) {
    // beforeunload is the only synchronous protection against a hard refresh.
    // Do not skip it while the very first server draft load is still pending.
    if((hasLocalDraftData() && !draftSync.canLeave()) || cardEditorDirty.value) {
        event.preventDefault();
        event.returnValue='';
    }
}
onBeforeRouteLeave(async()=>{
    // Vue Router changes do not fire browser beforeunload.
    if(cardEditorDirty.value &&
        !window.confirm('当前卡片有未保存的参数修改。退出将放弃这些局部修改，确定继续？'))
        return false;
    if(hasLocalDraftData() && !draftSync.canLeave()){
        // An explicit SPA exit can await one safe save; the sync module reuses
        // the same request ID/body after any uncertain network outcome.
        if(draftSyncState.initialized && draftSyncState.phase!=='conflict')
            await draftSync.flush();
        if(!draftSync.canLeave() &&
            !window.confirm('工作草稿仍未确认保存，离开此页面可能丢失本地修改。已确认保存在服务端的草稿不受影响。仍要离开吗？'))
            return false;
    }
    return true;
});
const repairRequest = ref<AutomationRepairRequest | null>(null);
const cardRepairContext = ref<CardRepairContext | null>(null);
let cardRepairEpoch=0;
watch(ownerId, id => Object.assign(draftWorkspace, emptyAutomationDraft(id)), { immediate: true });
// Loading/saving a different script revision invalidates picker intents but
// never deletes any steps. The persistent draft is still out of scope (M3).
watch(() => [draftWorkspace.ownerId, draftWorkspace.editingId, draftWorkspace.revision] as const, () => {
    // This watcher executes after pickerContexts is initialized at runtime.
    if (typeof stopAllPicking === 'function') stopAllPicking();
});
const inspectorDeviceId = ref<string | null>(null);
const inspectorOpenGeneration = ref(0);
interface PickerEditScope {
    ownerId: number; stepId: string; stepType: AutomationStepType; targetFingerprint: string;
    editingId?: string; revision?: number; origin: 'scripts' | 'card';
    binding?: PickerBinding; generation?: number; snapshotGeneration?: number;
    /** Detect changes to the original step during the asynchronous hierarchy read. */
    stepSnapshot?: string;
}
const inspectorEditScope = ref<PickerEditScope | null>(null);
const cardStepEditor = ref<{ deviceId: string; stepId: string } | null>(null);
const cardEditorDirty = ref(false);
/** One owner-level Script API projection for all phone-card floating windows.
 * The authoritative mutable steps remain ONLY in draftWorkspace/draftSync. */
const scriptWorkspace = createAutomationScriptWorkspace(
    api, ownerId, devices, draftWorkspace, draftSync, cardEditorDirty, () => {
        stopAllPicking(); closeInspector();
        cardStepEditor.value=null; cardEditorDirty.value=false;
        notice.value='已切换到已保存脚本，并正在建立独立工作草稿。';
    },
);
function refreshScriptList(deviceId:string) {
    if(!devices.value.some(device=>device.id===deviceId && device.owner_user_id===ownerId.value))return;
    void scriptWorkspace.refresh();
}
function handleFloatingScriptAction(action:AutomationScriptAction) {
    void scriptWorkspace.perform(action);
}
// No second draft or mutation: this is only ephemeral per-card presentation.
interface CardDemoState {
    token: number; stepId: string; loading: boolean;
    result: LocatorDemoResult | null; error: string;
}
const locatorDemos = reactive<Record<string,CardDemoState>>({});
const locatorDemoTimers = new Map<string,ReturnType<typeof setTimeout>>();
let locatorDemoEpoch = 0;
function clearCardDemo(deviceId:string) {
    const timer=locatorDemoTimers.get(deviceId);
    if(timer) clearTimeout(timer);
    locatorDemoTimers.delete(deviceId);
    delete locatorDemos[deviceId];
    ++locatorDemoEpoch;
}
function clearAllCardDemos() {
    for(const id of Object.keys(locatorDemos)) clearCardDemo(id);
    for(const timer of locatorDemoTimers.values()) clearTimeout(timer);
    locatorDemoTimers.clear();
}
watch(() => [draftWorkspace.ownerId, draftWorkspace.editingId, draftWorkspace.revision], () => {
    cardStepEditor.value = null;
    cardEditorDirty.value = false;
    clearAllCardDemos();
});
const inspectorReplaceStep = computed(() => inspectorEditScope.value
    ? { id: inspectorEditScope.value.stepId, type: inspectorEditScope.value.stepType }
    : null);
const textDialog = ref(false); const textTargets = ref<string[]>([]); const inputText = ref(''); const sendingText = ref(false);
const results = ref<{ id: string; name: string; state: string; message?: string }[]>([]);
const unlockingIds = ref<string[]>([]);
const focusRunId = ref<string>(); const historyKey = ref(0);
const fullscreenActive = ref(false);
// The fullscreen element is a browser top-layer boundary: dialogs attached
// to document.body are invisible behind it. Teleport text input into the
// active phone card while fullscreen, and back into the page afterward.
const textDialogTarget = computed<HTMLElement | 'body'>(() =>
    fullscreenActive.value && document.fullscreenElement instanceof HTMLElement
        ? document.fullscreenElement : 'body');
const cards = new Map<string, PhoneHandle>();
// T01: picker modes belong to a live card/session; steps stay in ONE draftWorkspace.
const pickerContexts = reactive<Record<string, PickerContext>>({});
const activePickingDeviceNames = computed(()=>Object.entries(pickerContexts)
    .filter(([,picker])=>picker.phase==='on' || picker.phase==='preparing')
    .map(([id])=>devices.value.find(device=>device.id===id)?.name??id));
// Last right-click is scoped to one session/generation. T03/T04 will use it
// for menu placement and selecting the candidate in a fresh hierarchy.
const pickerPoints = reactive<Record<string, {
    point: PickerVideoPoint; generation: number; snapshotGeneration: number;
    action: PickerMenuAction;
}>>({});
// Immutable acceptance scope captured when ONE Inspector is opened. No
// event may reconstruct its token from the live picker state at submit time.
interface InspectorAppendScope {
    token: number;
    deviceId: string;
    binding: PickerBinding;
    generation: number;
    snapshotGeneration: number;
    submitted: boolean;
}
const inspectorAppendScope = ref<InspectorAppendScope | null>(null);
let pickerEpoch = 0;
function pickerBinding(id: string): PickerBinding | null {
    const device = devices.value.find(item => item.id === id);
    const card = cards.get(id);
    const sessionId = card?.getSessionId();
    if (!device || !card?.getState().ready || !sessionId || !device.target_fingerprint
        || device.owner_user_id !== ownerId.value || draftWorkspace.ownerId !== ownerId.value) return null;
    return {
        ownerId: ownerId.value, deviceId: id,
        targetFingerprint: device.target_fingerprint, sessionId,
        draftOwnerId: draftWorkspace.ownerId,
        draftScriptId: draftWorkspace.editingId ?? null,
        draftRevision: draftWorkspace.revision ?? null,
    };
}
function stopCardPicking(id: string) {
    delete pickerPoints[id];
    const current = pickerContexts[id];
    if (current && current.phase !== 'off')
        pickerContexts[id] = stopPicker(current, ++pickerEpoch);
}
function stopAllPicking() {
    for (const id of Object.keys(pickerContexts)) stopCardPicking(id);
}
function armCardPicking(id: string, intent: PickerIntent = {kind:'append'}): boolean {
    const binding = pickerBinding(id);
    if (!binding || !!sync.value || !!recordingId.value) return false;
    const current = pickerContexts[id];
    if (current?.phase === 'on' && pickerMatches(current, binding)) {
        pickerContexts[id] = setPickerIntent(current, intent, ++pickerEpoch);
    } else {
        const started = beginPicker(binding, ++pickerEpoch, intent);
        // Card/session readiness is confirmed here; UI hierarchy readiness is
        // separately confirmed by the Inspector after a fresh hierarchy read.
        pickerContexts[id] = readyPicker(started, started.generation);
    }
    return true;
}
function toggleCardPicking(id: string) {
    const current = pickerContexts[id];
    if (current && current.phase !== 'off') {
        stopCardPicking(id);
        if (inspectorDeviceId.value === id &&
            (!inspectorEditScope.value || inspectorEditScope.value.origin === 'card')) closeInspector();
        notice.value = '已退出该手机的拾取模式；当前脚本步骤不会丢失。';
        return;
    }
    if (!armCardPicking(id)) {
        error.value = '无法开启拾取：请先打开手机实时画面，并结束群控同步或录制。';
        return;
    }
    notice.value = '拾取已开启：可正常左键操作手机，在画面上右键查看控件。';
}
async function onCardPickAction(id: string, point: PickerVideoPoint, action: PickerMenuAction) {
    if (action === 'exit') { stopCardPicking(id); return; }
    clearCardDemo(id);
    const current = pickerContexts[id], binding = pickerBinding(id);
    if (!current || !binding || !pickerCanApply(current, binding,
        current.generation, current.snapshotGeneration, current.intent)) {
        error.value = '拾取会话已失效，请重新开启拾取模式。';
        return;
    }
    // Keep the original menu cursor separate from the live video. T04 will
    // resolve it against a newly captured hierarchy before confirming a step.
    pickerPoints[id] = { point, generation: current.generation,
        snapshotGeneration: current.snapshotGeneration, action };
    const valid = () => {
        const latest = pickerContexts[id], latestBinding = pickerBinding(id);
        return !!latest && !!latestBinding && pickerCanApply(latest, latestBinding,
            current.generation, current.snapshotGeneration, current.intent);
    };
    if (action === 'refresh') {
        notice.value = '正在重新读取控件树（只读，不操作手机）…';
        try {
            const hierarchy = await api.hierarchy(ownerId.value, id);
            if (!valid()) return;
            if (hierarchy.device_id !== id || hierarchy.target_fingerprint !== binding.targetFingerprint)
                throw new Error('设备绑定在刷新控件树期间发生变化。');
            notice.value = '控件树读取完成：' + hierarchy.snapshot.nodes.length + ' 个节点；再次右键可配置步骤。';
        } catch (cause) { if (valid()) error.value = automationErrorMessage(cause); }
        return;
    }
    // The Inspector now resides in the SAME phone-screen fullscreen element.
    // Do not exit fullscreen, re-open media or alter the picker generation.
    if (!valid()) return;
    const token = ++inspectorOpenGeneration.value;
    inspectorAppendScope.value = null;
    openInspector(id);
    if (inspectorDeviceId.value !== id) return;
    // Fullscreen exit, opening the dialog and further ADB callbacks may
    // invalidate the captured picker; never silently bind a new session.
    if (!valid()) { closeInspector(); return; }
    const captured = pickerPoints[id];
    if (!captured || captured.generation !== current.generation ||
        captured.snapshotGeneration !== current.snapshotGeneration) {
        closeInspector();
        error.value = '拾取目标已变化，请在手机画面上重新右键选择。';
        return;
    }
    // Store the discriminated intent in a stable local before the async
    // Inspector/renderer branch. Vue reactive property access is not narrowed
    // reliably across these operations by vue-tsc.
    const capturedIntent: PickerIntent = current.intent;
    if (capturedIntent.kind === 'append') {
        inspectorAppendScope.value = {
            token, deviceId: id, binding: { ...binding }, generation: current.generation,
            snapshotGeneration: current.snapshotGeneration, submitted: false,
        };
    } else {
        // In replace mode the dialog may ONLY replace the captured step. It
        // cannot emit an append event, and the old ID survives the operation.
        if (inspectorEditScope.value?.origin !== 'card' ||
            inspectorEditScope.value.stepId !== capturedIntent.stepId) {
            closeInspector();
            error.value = '当前旧步骤已变化，重新拾取已取消。';
            return;
        }
    }
    const labels: Partial<Record<PickerMenuAction,string>> = {
        tap:'点击', long_press:'长按', input_text:'输入文字', wait_for:'等待出现',
        wait_gone:'等待消失', assert:'检查控件', assert_property:'属性断言',
    };
    notice.value = action === 'inspect'
        ? '已打开控件树；右键位置尚未自动匹配控件，请手动确认目标。'
        : '正在配置' + (labels[action] ?? '控件') + '步骤：请在控件树中选择目标并确认对应动作；此时尚未录入脚本。';
}
function reconcilePickerBindings() {
    for (const [id, current] of Object.entries(pickerContexts)) {
        if (current.phase === 'off') continue;
        const binding = pickerBinding(id);
        if (!binding || !pickerMatches(current, binding)) stopCardPicking(id);
    }
}
// T02/T03 consume pickerContexts[id].phase and arm/stop callbacks; T04/T06
// must call pickerCanApply with BOTH generations before committing an async hit.

// Preview sessions opened/adopted from the run dialog, never other users' sessions.
const scriptPreviewIds = new Set<string>();
async function releaseRunPreviews(ids: string[], progress: (message: string) => void): Promise<void> {
    const owner = ownerId.value;
    await releaseOwnedTrialPreviews(api, {
        ownerId: owner,
        requestedDeviceIds: ids.length ? ids : [...scriptPreviewIds],
        ownedPreviewIds: scriptPreviewIds,
        findPreview: id => cards.get(id),
        onProgress: progress,
        stillCurrent: () => !disposed && ownerId.value === owner && tab.value === 'scripts',
        deviceName: id => deviceNames.value[id] ?? id,
    });
}
async function prepareRunDevice(
    id: string, needsHelper: boolean, preview: boolean,
    progress: (message: string) => void, active: () => boolean,
): Promise<{ sessionId?: string; stream?: MediaStream | null }> {
    const owner = ownerId.value;
    const alive = () => {
        if (!active() || disposed || owner !== ownerId.value) throw new Error('试跑目标已切换，停止本次设备准备。');
    };
    const card = cards.get(id);
    if (!card) throw new Error('手机卡片不存在，请刷新工作台。');
    progress('检查最新设备绑定及占用');
    const current = (await api.devices(owner)).items.find(item => item.id === id);
    alive();
    if (!current?.connection.enabled || !current.target_fingerprint || current.availability_reason)
        throw new Error('手机绑定不可用，请先确认 ADB 服务配置。');
    if (current.run_occupied) throw new Error('手机有正在执行或未确认清理的脚本，不能接管。');
    const localId = card.getSessionId();
    if (current.active_session && (!localId || current.active_session.id !== localId))
        throw new Error('手机正在被其他页面或历史投屏会话占用，请先由原会话关闭。');
    progress('检查 ADB 连接');
    let adb = await api.adbStatus(owner,id); alive();
    if (adb.state !== 'connected') {
        if (adb.state === 'unauthorized') throw new Error('ADB 未授权，请先在手机上确认授权。');
        progress('正在连接 ADB（不会自动无线配对）');
        const connected = await api.prepare(owner,id,{action:'connect'}); alive();
        if (connected.state !== 'connected') throw new Error(connected.message || 'ADB 连接失败');
        adb = await api.adbStatus(owner,id); alive();
        if (adb.state !== 'connected') throw new Error('ADB 连接状态仍未确认，请检查手机。');
    }
    if (needsHelper) {
        progress('检查 UiAutomator2 控件服务');
        let component = await api.componentStatus(owner,id); alive();
        if (component.state !== 'ready') {
            // Preparation claims the device exclusively: release only OUR
            // preview first, never close another operator's session.
            if (localId && current.active_session?.id === localId) {
                progress('暂停本页预览以准备控件服务');
                await card.close(); alive();
                if (card.getState().phase === 'cleanup_pending' || card.getSessionId())
                    throw new Error('预览尚未安全关闭，无法初始化控件。');
                scriptPreviewIds.delete(id);
            }
            progress('正在初始化 UiAutomator2（可能需要较长时间）');
            let preparation = await api.startPreparation(owner,id,uuid()); alive();
            // One explicitly initiated preparation; poll only while the dialog
            // is current, never loop on a rejected request or restart a job.
            for (let attempt=0; !['succeeded','failed','cancelled','interrupted','cleanup_pending'].includes(preparation.state) && attempt<75; attempt++) {
                progress('初始化控件：' + (preparation.stage || preparation.state) + '（请勿断开 ADB）');
                await new Promise<void>(resolve => setTimeout(resolve,1200));
                alive();
                preparation = await api.preparationDetail(owner,id,preparation.id);
                alive();
            }
            if (preparation.state !== 'succeeded')
                throw new Error(preparation.state === 'queued' || preparation.state === 'running'
                    ? '控件初始化仍在执行，可查看控件组件进度并点击重新检查；不会重复启动任务。'
                    : preparation.message || '控件初始化失败，请查看执行器准备日志。');
            component = await api.componentStatus(owner,id); alive();
            if (component.state !== 'ready') throw new Error('初始化已结束，但 UiAutomator2 尚未确认就绪。');
        }
    }
    if (preview) {
        // Only one run-dialog video is needed. Never consume all media slots
        // when selecting a large batch of phones.
        await releaseRunPreviews([...scriptPreviewIds].filter(other=>other!==id),progress);
        alive();
        if (!card.getSessionId()) {
            progress('正在打开实时画面');
            await card.open('wall',{...mediaSettings.value,screen_off:false});
            alive();
        }
        if (!card.getSessionId()) throw new Error('实时画面会话创建失败。');
        scriptPreviewIds.add(id);
        let stream = card.getStream();
        for (let n=0; !stream && n<30; n++) {
            progress('等待实时画面视频流（已连接，继续处理中）');
            await new Promise<void>(resolve=>setTimeout(resolve,250));
            alive();
            stream=card.getStream();
        }
        if (!stream) throw new Error('视频流尚未建立，请检查 WebRTC 后重新预检。');
        progress('实时画面与控件已准备，可以检查步骤');
        return {sessionId:card.getSessionId()??undefined,stream};
    }
    if (localId && current.active_session?.id === localId && card.getSessionId()) {
        scriptPreviewIds.add(id);
        return {sessionId:card.getSessionId()??undefined};
    }
    progress('ADB 与控件检查完成');
    return {};
}
let loadGeneration = 0; let workspaceGeneration = 0; let disposed = false;
let gesture: CapturedGesture | null = null;
const gestureConfirmation = ref<Extract<GestureRecordingResult, { kind: 'confirm' }> | null>(null);
let recordingGeneration = 0; let pendingRecordActions = 0;
const runnerAvailable = computed(() => context.value?.available === true && context.value.runner_configured);
const managedAccount = computed(() => accounts.value.find(account => account.id === ownerId.value)?.username ?? String(ownerId.value));
const deviceNames = computed(() => Object.fromEntries(devices.value.map(device => [device.id, device.name])));
const repickingStepIds = computed<Record<string,string>>(() => Object.fromEntries(
    Object.entries(pickerContexts).flatMap(([id,context]) =>
        context.phase === 'on' && context.intent.kind === 'replace'
            ? [[id,context.intent.stepId] as const] : []),
));
const inspectorPickContext = computed(() => {
    const id = inspectorDeviceId.value;
    // Script-library replacement stays independent of any card cursor.
    if (!id || inspectorEditScope.value?.origin === 'scripts' || tab.value !== 'workbench') return null;
    const saved = pickerPoints[id], context = pickerContexts[id], binding = pickerBinding(id);
    const intent: PickerIntent = inspectorEditScope.value?.origin === 'card'
        ? {kind:'replace',stepId:inspectorEditScope.value.stepId,stepType:inspectorEditScope.value.stepType}
        : {kind:'append'};
    return saved && context && binding && pickerCanApply(
        context, binding, saved.generation, saved.snapshotGeneration, intent,
    ) ? saved : null;
});
const inspectorInitialPoint = computed<PickerVideoPoint | null>(() => inspectorPickContext.value?.point ?? null);
const inspectorPreferredAction = computed(() => menuToInspectorAction(inspectorPickContext.value?.action ?? 'inspect'));
const nodeOptions = computed(() => [...new Map([
    ...catalogNodes.value.map(node => [node.machine_id, node.alias || node.hostname || node.machine_id] as const),
    ...devices.value.map(device => [device.machine_id, device.node_name] as const),
]).entries()]);
const groupOptions = computed(() => [...new Map([
    ...catalogGroups.value.filter(group => !filters.value.node || group.machine_id === filters.value.node)
        .map(group => [group.id, group.name] as const),
    ...devices.value.filter(device => device.gateway_group_id && (!filters.value.node || device.machine_id === filters.value.node))
        .map(device => [device.gateway_group_id!, device.gateway_name ?? device.gateway_group_id!] as const),
]).entries()]);
const typeOptions = computed(() => [...new Map(devices.value.filter(device => device.device_type_id).map(device => [device.device_type_id!, device.type_name ?? device.device_type_id!])).entries()]);
// An explicit query already probes ADB status for every visible device in
// parallel. Re-sort reactively as each result arrives, keeping the existing
// gateway/date order stable within the online and other groups.
const sortedDevices = computed(() => [...devices.value].sort((a, b) =>
    Number(phoneConnections.value[a.id] !== 'connected') - Number(phoneConnections.value[b.id] !== 'connected')
    || Number(!a.gateway_group_id) - Number(!b.gateway_group_id)
    || (a.gateway_name ?? '').localeCompare(b.gateway_name ?? '', undefined, { sensitivity: 'base' })
    || (a.gateway_group_id ?? '').localeCompare(b.gateway_group_id ?? '')
    || (b.updated_at ?? '').localeCompare(a.updated_at ?? '')
    || a.id.localeCompare(b.id)));
const visibleDevices = computed(() => {
    const query = applied.value; const keyword = query.keyword.trim().toLocaleLowerCase();
    return sortedDevices.value.filter(device => (!query.node || device.machine_id === query.node)
        && (!query.group || device.gateway_group_id === query.group)
        && (!query.type || device.device_type_id === query.type)
        && (!keyword || [device.name, device.model, device.overlay_ip, device.lan_ip].some(value => value?.toLocaleLowerCase().includes(keyword))));
});
const visibleIds = computed(() => new Set(visibleDevices.value.map(device => device.id)));
const liveCount = computed(() => Object.values(phases.value).filter(phase => phase === 'live').length);
const globalTargets = computed(() => sync.value?.deviceIds ?? selectedIds.value);
const globalManualBlocked = computed(() => globalTargets.value.some(id => {
    const card = cards.get(id)?.getState();
    const device = devices.value.find(item => item.id === id);
    return !card?.ready || !!card.viewerOnly || !!device?.run_occupied;
}));
const recordingName = computed(() => deviceNames.value[recordingId.value ?? ''] ?? '');

/** T43: the selector on ANY phone edits the same shared draft but the
 * batch target is ONLY the current workbench checkbox set, never "all". */
const batchDraftReady=computed(()=>scriptWorkspace.guard.value==='clean' &&
    draftSync.canLeave() && !cardEditorDirty.value);
async function runSelectedBatch(parameters:Record<string,string>,concurrency:number) {
    error.value='';
    if(!runnerAvailable.value || !batchDraftReady.value ||
        !!recordingId.value || !!sync.value || !!cardStepEditor.value ||
        !!inspectorDeviceId.value || selectedIds.value.some(id=>unlockingIds.value.includes(id))){
        error.value='批量执行前请保存草稿并结束录制、群控、编辑、控件拾取和解锁操作。';return;
    }
    try{
        const intent=freezeFloatingBatchIntent(
            ownerId.value,selectedIds.value,devices.value,draftWorkspace.steps,
            draftWorkspace.name,parameters,concurrency);
        await floatingBatch.start(intent,id=>cards.get(id),{...mediaSettings.value},()=>
            !disposed && tab.value==='workbench' && ownerId.value===intent.owner);
        if(floatingBatch.state.error)error.value=floatingBatch.state.error;
    }catch(cause){error.value=automationErrorMessage(cause);}
}
function refreshBatch(){void floatingBatch.refresh();}
function reconcileBatch(){void floatingBatch.reconcile(ownerId.value);}
function stopBatch(){void floatingBatch.cancelAll();}
function stopBatchDevice(id:string){void floatingBatch.cancelDevice(id);}

function setCard(id: string, value: unknown) {
    if (value) cards.set(id, value as PhoneHandle); else cards.delete(id);
}
function select(id: string, checked: boolean) {
    selectedIds.value = checked ? [...new Set([...selectedIds.value, id])] : selectedIds.value.filter(item => item !== id);
}
function handleError(cause: unknown) {
    error.value = automationErrorMessage(cause);
    if (cause instanceof AutomationApiError && [401, 403].includes(cause.status)) {
        void closeAll();
        if (cause.status === 401) void router.push({ name: 'login' });
    }
}
async function loadGatewayCatalog(owner: number) {
    if (catalogOwner === owner) return;
    catalogOwner = owner;
    const epoch = ++catalogGeneration;
    try {
        const nodes = await assetApi.assetsNodes(owner);
        if (disposed || owner !== ownerId.value || epoch !== catalogGeneration) return;
        const groups = await Promise.all(nodes.items.map(async node => {
            const answer = await assetApi.assetsGroups({ owner_user_id: owner, machine_id: node.machine_id });
            return answer.items;
        }));
        if (disposed || owner !== ownerId.value || epoch !== catalogGeneration) return;
        catalogNodes.value = nodes.items;
        catalogGroups.value = groups.flat();
    } catch (cause) {
        if (!disposed && epoch === catalogGeneration) {
            catalogOwner = 0; // A later manual refresh may safely retry discovery catalogs.
            notice.value = t('discoveryCatalogUnavailable');
        }
    }
}
async function loadDevices() {
    if (!ownerId.value || !context.value?.available) return;
    const owner = ownerId.value; const request = ++loadGeneration;
    busy.value = true; error.value = '';
    try {
        const response = await api.devices(owner);
        if (disposed || request !== loadGeneration || owner !== ownerId.value) return;
        // A cached component state belongs to one exact ADB target revision.
        const previousTargets = new Map(devices.value.map(item => [item.id, item.target_fingerprint]));
        devices.value = response.items; sourceIssues.value = response.issues ?? []; subscribeEndpoints();
        reconcilePickerBindings();
        for (const id of Object.keys(componentStatuses.value)) {
            const device = response.items.find(item => item.id === id);
            if (!device || previousTargets.get(id) !== device.target_fingerprint) {
                delete componentStatuses.value[id];
                delete phoneConnections.value[id];
                delete phoneConnectionErrors.value[id];
                adbActionVersions.set(id, (adbActionVersions.get(id) ?? 0) + 1);
            }
        }
        void loadGatewayCatalog(owner);
        const currentIds = new Set(devices.value.map(device => device.id));
        selectedIds.value = initialSelectionDone
            ? selectedIds.value.filter(id => currentIds.has(id))
            : response.items.map(device => device.id);
        initialSelectionDone = true;
        for (const id of Object.keys(phases.value)) if (!currentIds.has(id)) delete phases.value[id];
        for (const id of Object.keys(phoneConnections.value)) if (!currentIds.has(id)) {
            delete phoneConnections.value[id]; delete phoneConnectionErrors.value[id];
        }
        if (!initialConnectDone && runnerAvailable.value && response.items.length) {
            initialConnectDone = true;
            void connectPhones([...selectedIds.value]);
        }
    } catch (cause) { if (!disposed && request === loadGeneration) handleError(cause); }
    finally { if (request === loadGeneration) busy.value = false; }
}
function queryVisibleDeviceStatuses() {
    if (!runnerAvailable.value) return;
    // Read-only, per-device ADB and UIA probes start independently.
    // A timeout on one device cannot delay another device's status update.
    const owner = ownerId.value;
    const workspace = workspaceGeneration;
    const generation = ++statusQueryGeneration;
    for (const device of [...visibleDevices.value]) {
        const { id, target_fingerprint: fingerprint } = device;
        const valid = () => !disposed && owner === ownerId.value &&
            workspace === workspaceGeneration && generation === statusQueryGeneration &&
            devices.value.some(item => item.id === id && item.target_fingerprint === fingerprint);
        const actionVersion = adbActionVersions.get(id) ?? 0;
        if (phoneConnections.value[id] !== 'connecting') phoneConnections.value[id] = 'checking';
        delete phoneConnectionErrors.value[id];
        componentStatuses.value[id] = { device_id:id, executor_id:'', state:'checking' };
        void api.adbStatus(owner, id).then(response => {
            if (!valid() || response.device_id !== id || (adbActionVersions.get(id) ?? 0) !== actionVersion) return;
            if (phoneConnections.value[id] !== 'connecting') {
                phoneConnections.value[id] = response.state;
                if (response.code) phoneConnectionErrors.value[id] = response.code;
            }
        }).catch(cause => {
            if (!valid() || (adbActionVersions.get(id) ?? 0) !== actionVersion) return;
            if (phoneConnections.value[id] !== 'connecting') {
                phoneConnections.value[id] = 'query_failed';
                phoneConnectionErrors.value[id] = automationErrorMessage(cause);
            }
        });
        void api.componentStatus(owner, id).then(response => {
            if (!valid() || response.device_id !== id) return;
            componentStatuses.value[id] = response;
        }).catch(() => {
            if (!valid()) return;
            componentStatuses.value[id] = { device_id:id, executor_id:'', state:'query_failed' };
        });
    }
}
async function query() {
    if(floatingBatch.busy.value){
        notice.value='批量脚本正在原画面运行，请先结束任务再切换筛选（避免误关视频）。';return;
    }
    applied.value = { ...filters.value };
    await loadDevices();
    queryVisibleDeviceStatuses();
    if (sync.value?.deviceIds.some(id => !visibleIds.value.has(id))) await stopSync();
    for (const [id, card] of cards) if (!visibleIds.value.has(id)) { stopCardPicking(id); await card.close(); }
}
async function changeOwner(id: number) {
    if (id === ownerId.value || busy.value || connecting.value || switchingTab.value) return;
    if(floatingBatch.busy.value || floatingBatch.state.phase==='unknown'){
        notice.value='批量运行或结果未知期间不能切换账号；请确认停止或只读核实原请求。';return;
    }
    if(!warnBeforeDraftReplacement())return;
    repairRequest.value = null;
    cardRepairContext.value = null; ++cardRepairEpoch;
    if (gestureConfirmation.value) { notice.value = '请先确认滑动或拖拽，才能切换管理账号。'; return; }
    stopEvents?.(); stopEvents = undefined;
    ++workspaceGeneration; ++loadGeneration; ++statusQueryGeneration;
    adbActionVersions.clear();
    await closeAll();
    closeInspector();
    stopAllPicking();
    ownerId.value = id; devices.value = []; selectedIds.value = []; phases.value = {}; results.value = [];
    componentStatuses.value = {}; componentStatusResults.value = [];
    initialSelectionDone = false; initialConnectDone = false;
    ++catalogGeneration; catalogOwner = 0; catalogNodes.value = []; catalogGroups.value = [];
    phoneConnections.value = {}; phoneConnectionErrors.value = {}; discoveryGateway.value = null;
    connectionDevice.value = null; textDialog.value = false; focusRunId.value = undefined;
    unlockingIds.value = [];
    filters.value = { node: '', group: '', type: '', keyword: '' }; applied.value = { ...filters.value };
    await loadDevices();
}
function unavailable(ids: string[]) {
    return ids.filter(id => !cards.get(id)?.getState().ready);
}
function requireTargets(ids: string[], minimum = 1): boolean {
    if (ids.length < minimum) { error.value = minimum === 2 ? t('syncMinimum') : t('selectionRequired'); return false; }
    const invalid = unavailable(ids);
    if (invalid.length) { error.value = `${t('targetsUnavailable')} ${invalid.map(id => deviceNames.value[id] ?? id).join('、')}`; return false; }
    return true;
}
async function connectPhones(ids: string[]) {
    if (!runnerAvailable.value || phoneConnecting.value || !ids.length) return;
    const owner = ownerId.value; const epoch = workspaceGeneration;
    phoneConnecting.value = true;
    const targets = [...new Set(ids)].filter(id => {
        const device = devices.value.find(item => item.id === id);
        return device?.connection.enabled && !device.availability_reason;
    });
    const queue = [...targets];
    for (const id of targets) {
        adbActionVersions.set(id, (adbActionVersions.get(id) ?? 0) + 1);
        phoneConnections.value[id] = 'connecting';
        delete phoneConnectionErrors.value[id];
    }
    // Explicit ADB connect only. Never silently pair, retry credentials or
    // open a video session. At most two requests can be in flight.
    const worker = async () => {
        while (queue.length && epoch === workspaceGeneration && !disposed) {
            const id = queue.shift()!;
            try {
                const result = await api.prepare(owner, id, { action: 'connect' });
                if (epoch !== workspaceGeneration || disposed) return;
                if (result.state !== 'connected') throw new Error(result.message || t('actionFailed'));
                phoneConnections.value[id] = 'connected';
            } catch (cause) {
                if (epoch !== workspaceGeneration || disposed) return;
                phoneConnections.value[id] = 'failed';
                phoneConnectionErrors.value[id] = automationErrorMessage(cause);
            }
        }
    };
    try { await Promise.all([worker(), worker()]); }
    finally { phoneConnecting.value = false; }
}
async function querySelectedComponentStatuses() {
    if (!runnerAvailable.value || componentChecking.value || !selectedIds.value.length) return;
    const owner = ownerId.value, generation = workspaceGeneration;
    const targets = [...new Set(selectedIds.value)]
        .map(id => devices.value.find(device => device.id === id))
        .filter((device): device is AutomationDevice => !!device);
    if (!targets.length) return;
    componentChecking.value = true;
    componentStatusResults.value = targets.map(device => ({
        id: device.id, name: device.name, state: 'checking',
    }));
    const queue = [...targets];
    const worker = async () => {
        while (queue.length && generation === workspaceGeneration && !disposed) {
            const device = queue.shift()!;
            try {
                // Explicit read-only status query. Never prepare, pair or start UiAutomator.
                const response = await api.componentStatus(owner, device.id);
                if (disposed || generation !== workspaceGeneration || owner !== ownerId.value) return;
                if (devices.value.find(item => item.id === device.id)?.target_fingerprint !== device.target_fingerprint) continue;
                if (response.device_id !== device.id) throw new Error('执行器返回了不匹配的手机状态');
                componentStatuses.value[device.id] = response;
                const entry = componentStatusResults.value.find(item => item.id === device.id);
                if (entry) entry.state = response.state;
            } catch (cause) {
                if (disposed || generation !== workspaceGeneration || owner !== ownerId.value) return;
                const entry = componentStatusResults.value.find(item => item.id === device.id);
                if (entry) { entry.state = 'query_failed'; entry.message = automationErrorMessage(cause); }
            }
        }
    };
    try { await Promise.all([worker(), worker(), worker()]); }
    finally { componentChecking.value = false; }
}
async function discoverAdb() {
    const group = filters.value.group || applied.value.group;
    if (!group || !ownerId.value || discoveryBusy.value) return;
    const groupRecord = catalogGroups.value.find(entry => entry.id === group);
    const machineId = groupRecord?.machine_id ?? devices.value.find(device => device.gateway_group_id === group)?.machine_id;
    if (!machineId) { error.value = t('discoveryGatewayMissing'); return; }
    discoveryBusy.value = true; error.value = '';
    try {
        const scope = { owner_user_id: ownerId.value, machine_id: machineId };
        let gateway: AssetDevice | undefined;
        for (let pageNumber = 1; pageNumber <= 20 && !gateway; ++pageNumber) {
            const page = await assetApi.assetsDevices(scope, { group_id: group, page: pageNumber, page_size: 100 });
            gateway = page.items.find(device => device.is_gateway_self && (device.semantic_kinds?.includes('openwrt') || device.semantic_kind === 'openwrt'));
            if (pageNumber * 100 >= page.total) break;
        }
        if (!gateway) throw new Error(t('discoveryGatewayMissing'));
        discoveryGateway.value = gateway;
    } catch (cause) { handleError(cause); }
    finally { discoveryBusy.value = false; }
}
async function openSelected() {
    if (connecting.value || !runnerAvailable.value) return;
    if (!selectedIds.value.length) { error.value = t('selectionRequired'); return; }
    const targets = [...selectedIds.value]; const mine = workspaceGeneration;
    const invalid = targets.filter(id => {
        const device = devices.value.find(item => item.id === id);
        return !device?.connection.enabled || device.availability_reason || (device.active_session && !cards.get(id)?.getState().ready);
    });
    if (invalid.length) { error.value = `${t('targetsUnavailable')} ${invalid.map(id => deviceNames.value[id] ?? id).join('、')}`; return; }
    connecting.value = true; error.value = ''; results.value = [];
    const queue = targets.filter(id => !cards.get(id)?.getState().ready);
    // At most two explicit starts in flight; this queue never retries a failed device.
    const worker = async () => {
        while (queue.length && mine === workspaceGeneration && !disposed) {
            const id = queue.shift()!; const card = cards.get(id);
            if (!card) continue;
            try { await card.open(quality.value, { ...mediaSettings.value }); }
            catch (cause) { results.value.push({ id, name: deviceNames.value[id] ?? id, state: 'failed', message: automationErrorMessage(cause) }); }
        }
    };
    await Promise.all([worker(), worker()]); connecting.value = false;
}
async function toggleDeviceScreen(id: string) {
    const card = cards.get(id);
    if (!card || !runnerAvailable.value) return;
    const state = card.getState();
    if (['opening', 'connecting'].includes(state.phase)) return;
    if (state.ready || state.phase === 'live') {
        await card.close().then(() => loadDevices()).catch(handleError);
    } else {
        const device = devices.value.find(item => item.id === id);
        if (!device?.connection.enabled || device.availability_reason || device.active_session) {
            error.value = t('targetsUnavailable');
            return;
        }
        try { await card.open(quality.value, { ...mediaSettings.value }); }
        catch (cause) { handleError(cause); }
    }
}
async function closeSelected() {
    const targets = [...selectedIds.value];
    if(floatingBatch.busy.value && targets.some(id=>floatingBatch.state.selected.includes(id))){
        notice.value='批量任务运行中，不能关闭选中设备原实时画面；请先请求停止并确认清理。';return;
    }
    if (sync.value && targets.some(id => sync.value!.deviceIds.includes(id))) await stopSync();
    if (recordingId.value && targets.includes(recordingId.value)) finishRecording(false);
    await Promise.all(targets.map(id => cards.get(id)?.close()));
    await loadDevices();
}
async function closeAll() {
    clearAllCardDemos();
    stopAllPicking();
    closeInspector();
    globalOpen.value = false; syncPreparation.value = false;
    await stopSync();
    if (recordingId.value) finishRecording(false);
    await Promise.all([...cards.values()].map(card => card.close()));
    scriptPreviewIds.clear();
}
function onState(id: string, phase: string) {
    if (phase !== 'live') clearCardDemo(id);
    if (phase !== 'live') stopCardPicking(id);
    if (phase !== 'live' && inspectorDeviceId.value === id) closeInspector();
    phases.value[id] = phase;
    // T45: media loss is NOT the same as action execution failure.
    // Keep the run's server status intact; display a separate viewer warning.
    if(floatingBatch.state.selected.includes(id))floatingBatch.inspectMedia();
    if (phase === 'live') phoneConnections.value[id] = 'connected';
    if (phase !== 'live' && sync.value?.deviceIds.includes(id)) {
        if (id === sync.value.masterId) { notice.value = t('syncMasterLost'); void stopSync(); }
        else if (!sync.value.blocked.includes(id)) sync.value.blocked.push(id);
    }
    if (phase !== 'live' && recordingId.value === id) finishRecording(false);
}
function prepareSync() {
    error.value = '';
    if (recordingId.value) { error.value = t('recordingExists'); return; }
    const targets = [...selectedIds.value];
    if (!requireTargets(targets, 2)) return;
    syncTargets.value = targets; syncMaster.value = targets[0]; syncPreparation.value = true; globalOpen.value = false;
}
function beginSync() {
    if (!requireTargets(syncTargets.value, 2)) return;
    const geometry: SyncSession['geometry'] = {};
    for (const id of syncTargets.value) {
        const dimensions = cards.get(id)!.getState();
        if (!dimensions.width || !dimensions.height) { error.value = t('notReady'); return; }
        geometry[id] = { width: dimensions.width, height: dimensions.height };
    }
    const master = geometry[syncMaster.value];
    if (!master || Object.values(geometry).some(item => (item.width > item.height) !== (master.width > master.height))) { error.value = t('syncRotation'); return; }
    sync.value = { masterId: syncMaster.value, deviceIds: [...syncTargets.value], blocked: [], geometry };
    for (const id of syncTargets.value) stopCardPicking(id);
    if (inspectorDeviceId.value && syncTargets.value.includes(inspectorDeviceId.value)) closeInspector();
    syncPreparation.value = false; gesture = null;
}
async function stopSync() {
    const current = sync.value; sync.value = null; gesture = null;
    if (!current) return;
    for (const id of current.deviceIds) cards.get(id)?.resetPointer();
    const releases = await Promise.allSettled(current.deviceIds.map(id => cards.get(id)?.send({ type: 'stop' }) ?? Promise.reject(new Error('Control session unavailable'))));
    if (releases.some(result => result.status === 'rejected')) notice.value = t('releaseUnknown');
}
async function releaseManual(ids: string[]) {
    const targets = [...ids];
    if (targets.some(id => unlockingIds.value.includes(id))) { error.value = t('unlockBusy'); return false; }
    if (recordingId.value && targets.includes(recordingId.value)) finishRecording(false);
    if (sync.value && targets.some(id => sync.value!.deviceIds.includes(id))) await stopSync();
    for (const id of targets) cards.get(id)?.resetPointer();
    const responses = await Promise.allSettled(targets.map(id => cards.get(id)?.send({ type: 'stop' }) ?? Promise.reject(new Error(t('notReady')))));
    results.value = responses.map((response, index) => ({ id: targets[index], name: deviceNames.value[targets[index]] ?? targets[index], state: response.status === 'fulfilled' ? 'action_sent' : 'outcome_unknown', message: response.status === 'rejected' ? automationErrorMessage(response.reason) : t('manualReleased') }));
    const success = responses.every(response => response.status === 'fulfilled');
    if (!success) error.value = t('releaseUnknown');
    return success;
}
function onGeometry(id: string) {
    clearCardDemo(id);
    if (pickerContexts[id]?.phase === 'on') {
        pickerContexts[id] = invalidatePickerSnapshot(pickerContexts[id]);
        delete pickerPoints[id];
    }
    if (sync.value?.deviceIds.includes(id)) { notice.value = t('syncRotation'); void stopSync(); }
    if (recordingId.value === id) { ++recordingGeneration; gesture = null; notice.value = t('recordIncomplete'); }
}
function addStep(step: AutomationStep) {
    if (!recordingId.value || recordingPaused.value) return;
    if (draftWorkspace.steps.length + recordedSteps.value.length >= 100) { notice.value = t('recordLimit'); finishRecording(false); return; }
    recordedSteps.value.push(step);
}
async function routeInput(id: string, message: AutomationControl) {
    const activeSync = sync.value;
    if (activeSync && id !== activeSync.masterId) return;
    const targets = activeSync ? activeSync.deviceIds.filter(target => !activeSync.blocked.includes(target)) : [id];
    for(const target of targets) clearCardDemo(target);
    // While a motion is waiting for a swipe/drag decision, do not send
    // another pointer stream and accidentally record an out-of-order action.
    if (gestureConfirmation.value && message.type === 'touch') {
        notice.value = '先确认刚才的滑动或拖拽类型，再继续操作手机。';
        return;
    }
    if (message.type === 'touch' && message.action === 'down' && recordingId.value === id && !recordingPaused.value) {
        gesture = beginCapturedGesture(id, message, performance.now());
    } else if (message.type === 'touch' && message.action === 'move' && gesture?.deviceId === id) {
        observeCapturedGesture(gesture, message);
    }
    const activeGesture = gesture;
    const recordEpoch = recordingGeneration;
    const finishesGesture = message.type === 'touch' && message.action === 'up' && activeGesture?.deviceId === id;
    if (finishesGesture) { gesture = null; ++pendingRecordActions; }
    if (message.type === 'touch.release' && activeGesture?.deviceId === id) { activeGesture.valid = false; gesture = null; }
    const gestureEndedAt = performance.now();
    const responses = await Promise.allSettled(targets.map(async target => {
        const card = cards.get(target); if (!card) throw new Error(t('notReady'));
        let command = { ...message };
        if (activeSync && message.type === 'touch') {
            const expected = activeSync.geometry[target]; const actual = card.getState();
            if (actual.width !== expected.width || actual.height !== expected.height) { onGeometry(target); throw new Error(t('syncRotation')); }
            command = { ...message, x: Math.min(actual.width - 1, Math.round(message.x! / message.width! * actual.width)), y: Math.min(actual.height - 1, Math.round(message.y! / message.height! * actual.height)), width: actual.width, height: actual.height };
        }
        return card.send(command);
    }));
    if (finishesGesture) --pendingRecordActions;
    const failure = responses.find(response => response.status === 'rejected');
    if (failure?.status === 'rejected') {
        if (activeGesture) activeGesture.valid = false;
        error.value = automationErrorMessage(failure.reason);
        if (activeSync && sync.value === activeSync) {
            responses.forEach((response, index) => { if (response.status === 'rejected' && !activeSync.blocked.includes(targets[index])) activeSync.blocked.push(targets[index]); });
            if (activeSync.blocked.includes(activeSync.masterId)) void stopSync();
        }
    }
    if (message.type === 'touch.release') return;
    if (finishesGesture && activeGesture) {
        if (recordEpoch !== recordingGeneration || recordingId.value !== id) return;
        if (!activeGesture.valid || recordingPaused.value) { notice.value = t('recordIncomplete'); return; }
        const result = finishCapturedGesture(activeGesture, message, gestureEndedAt, uuid());
        if (result.kind === 'invalid') {
            notice.value = result.reason === 'returned_to_origin'
                ? '轨迹返回起点，无法通过单条直线手势复现；这次操作没有录入。'
                : '手势超时或采集不完整，这次操作没有录入。';
        } else if (result.kind === 'step') {
            addStep(result.step);
        } else {
            gestureConfirmation.value = result;
        }
    }
}
function confirmRecordedMotion(choice: 'swipe' | 'drag' | 'discard') {
    const pending = gestureConfirmation.value;
    if (!pending) return;
    gestureConfirmation.value = null;
    if (choice !== 'discard') {
        if (recordingId.value && !recordingPaused.value) {
            addStep(choice === 'swipe' ? pending.swipe : pending.drag);
        } else notice.value = '录制会话已结束，未保存这一条手势。';
    } else {
        notice.value = '已忽略这一条未确认的移动手势。';
    }
}
async function sendAction(ids: string[], message: AutomationControl) {
    const targets = [...ids]; error.value = '';
    for(const target of targets) clearCardDemo(target);
    const mine = workspaceGeneration;
    if (!requireTargets(targets)) return;
    if (targets.some(id => unlockingIds.value.includes(id))) { error.value = t('unlockBusy'); return; }
    if (recordingId.value && (targets.length !== 1 || targets[0] !== recordingId.value)) { error.value = t('recordingExists'); return; }
    results.value = targets.map(id => ({ id, name: deviceNames.value[id] ?? id, state: 'sending' }));
    const recordEpoch = recordingGeneration;
    const recordAction = targets.length === 1 && targets[0] === recordingId.value;
    if (recordAction) ++pendingRecordActions;
    const responses = await Promise.allSettled(targets.map(id => cards.get(id)!.send(message)));
    if (recordAction) --pendingRecordActions;
    if (mine !== workspaceGeneration || disposed) return;
    results.value = responses.map((response, index) => ({ id: targets[index], name: deviceNames.value[targets[index]] ?? targets[index], state: response.status === 'fulfilled' ? 'action_sent' : 'outcome_unknown', message: response.status === 'rejected' ? automationErrorMessage(response.reason) : undefined }));
    if (recordEpoch === recordingGeneration && targets.length === 1 && targets[0] === recordingId.value && responses[0].status === 'fulfilled') {
        if (message.type === 'key') addStep({ id: uuid(), type: 'key', key: message.key });
        // Never persist manually entered text in a recording; the editor supplies the parameter.
        if (message.type === 'input_text') addStep({ id: uuid(), type: 'input_text', parameter: `input_${recordedSteps.value.length + 1}` });
    }
}
const unlockMessages: Record<string, string> = {
    unlock_confirmed: '已确认手机解锁成功',
    already_unlocked: '手机已处于解锁状态，没有输入密码',
    wake_sent: '屏幕已唤醒；未配置开屏密码，请在 ADB 服务中保存',
    lock_state_unknown: '锁屏状态不明确，为避免误输入密码已停止',
    lock_state_changed: '锁屏状态发生变化，已停止密码输入',
    unlock_size_unknown: '无法取得正确的屏幕尺寸，已停止解锁',
    unlock_swipe_failed: '锁屏上滑失败，未输入密码',
    unlock_skipped: '解锁中断，最终状态尚未确认',
    unlock_sent: '已发送密码及确认键，但尚未验证解锁成功',
};
async function unlockPhones(ids: string[]) {
    error.value = '';
    const targets = [...new Set(ids)];
    if (!requireTargets(targets)) return;
    if (sync.value || recordingId.value) {
        error.value = sync.value ? t('controlsLocked') : t('recordingExists');
        return;
    }
    if (targets.some(id => unlockingIds.value.includes(id))) return;
    const epoch = workspaceGeneration;
    unlockingIds.value = [...new Set([...unlockingIds.value, ...targets])];
    results.value = targets.map(id => ({ id, name: deviceNames.value[id] ?? id, state: 'sending' }));
    try {
        const responses = await Promise.allSettled(targets.map(id => cards.get(id)!.unlock()));
        if (disposed || epoch !== workspaceGeneration) return;
        results.value = responses.map((response, i) => ({
            id: targets[i], name: deviceNames.value[targets[i]] ?? targets[i],
            state: response.status === 'fulfilled' ? 'action_sent' : 'outcome_unknown',
            message: response.status === 'fulfilled'
                ? unlockMessages[response.value.unlock_state] ?? '解锁结果未知，请检查手机'
                : automationErrorMessage(response.reason) + '；不要自动重试，先检查手机状态',
        }));
    } finally {
        if (epoch === workspaceGeneration) unlockingIds.value = unlockingIds.value.filter(id => !targets.includes(id));
    }
}
function localAction(id: string, message: AutomationControl) {
    if (sync.value?.deviceIds.includes(id)) { error.value = t('controlsLocked'); return; }
    if (unlockingIds.value.includes(id)) return;
    if (message.type === 'stop') { void releaseManual([id]); return; }
    void sendAction([id], message);
}
function globalAction(message: AutomationControl) {
    const targets = [...globalTargets.value];
    if (globalManualBlocked.value) { error.value = '所选手机含只读观看、脚本占用或控制未就绪设备，不能批量发送手控指令。'; return; }
    if (targets.some(id => unlockingIds.value.includes(id))) return;
    if (sync.value?.blocked.length) { error.value = `${t('targetsUnavailable')} ${sync.value.blocked.map(id => deviceNames.value[id] ?? id).join('、')}`; return; }
    void sendAction(targets, message);
}
function showText(ids: string[]) {
    if (!requireTargets(ids)) return;
    textTargets.value = [...ids]; inputText.value = ''; textDialog.value = true; globalOpen.value = false;
}
async function sendText() {
    if (!inputText.value || sendingText.value) return;
    if (new TextEncoder().encode(inputText.value).length > 2000) { error.value = t('textTooLong'); return; }
    sendingText.value = true;
    try { await sendAction([...textTargets.value], { type: 'input_text', text: inputText.value }); textDialog.value = false; inputText.value = ''; }
    finally { sendingText.value = false; }
}
function toggleRecord(id: string) {
    if (recordingId.value === id) { finishRecording(true); return; }
    if (recordingId.value || sync.value) { error.value = recordingId.value ? t('recordingExists') : t('controlsLocked'); return; }
    if (!requireTargets([id])) return;
    if (draftWorkspace.steps.length >= 100 || analyzeAutomationFlow(draftWorkspace.steps).problem) {
        error.value = '当前脚本草稿已满或分支范围无效，请先在脚本编辑器处理。';
        return;
    }
    stopCardPicking(id);
    if (inspectorDeviceId.value === id) closeInspector();
    ++recordingGeneration; recordingId.value = id; recordingPaused.value = false; recordedSteps.value = []; gesture = null;
}
function pauseRecording() {
    if (gestureConfirmation.value) { notice.value = '请先确认滑动或拖拽，再暂停录制。'; return; }
    if (gesture || pendingRecordActions) notice.value = t('recordIncomplete');
    if (recordingId.value) cards.get(recordingId.value)?.cancelPointer();
    ++recordingGeneration; gesture = null; recordingPaused.value = !recordingPaused.value;
}
function openInspector(id: string) {
    if (sync.value || recordingId.value) { error.value = '请先结束群控同步或脚本录制，再拾取控件。'; return; }
    const card = cards.get(id);
    const stream = card?.getStream();
    if (!card?.getState().ready || !stream) { error.value = '请先打开这台手机的实时画面，再拾取控件。'; return; }
    // A right-click may already have armed this exact session. Do not bump
    // its generation again or invalidate the cursor captured by T02.
    const current = pickerContexts[id], binding = pickerBinding(id);
    if ((!current || current.phase !== 'on' || !binding || !pickerMatches(current, binding))
        && !armCardPicking(id)) {
        error.value = '无法开启拾取：设备会话、账号或草稿上下文已变化。'; return;
    }
    const pickedIntent = current?.intent;
    if (pickedIntent?.kind === 'replace') {
        // Capture narrowed scalar fields before entering Array.find's callback.
        // The reactive union itself is not narrowed inside that closure.
        const replaceStepId = pickedIntent.stepId;
        const replaceStepType = pickedIntent.stepType;
        const item = draftWorkspace.steps.find(step => step.id === replaceStepId);
        if (!item || item.type !== replaceStepType || !stepCanPickTarget(item) || !binding) {
            error.value = '重新拾取的原步骤已变化，请重新选择步骤。';
            return;
        }
        inspectorEditScope.value = {
            ownerId: ownerId.value, origin: 'card', stepId: item.id,
            stepType: item.type, targetFingerprint: binding.targetFingerprint,
            editingId: draftWorkspace.editingId, revision: draftWorkspace.revision,
            binding: {...binding}, generation: current.generation,
            snapshotGeneration: current.snapshotGeneration, stepSnapshot: JSON.stringify(item),
        };
    } else inspectorEditScope.value = null;
    inspectorDeviceId.value = id;
    // No Inspector MediaStream attachment: it only consumes this card's
    // current native video dimensions and draws on its original main video.
}
/** Legacy script-tab request: return to the ORIGINAL phone card for repicking.
 * It must not open a second video, create a control lease or select stale pixels. */
async function openScriptInspector(stepId: string, stepType: AutomationStepType, deviceId: string) {
    const owner=ownerId.value, scriptId=draftWorkspace.editingId, revision=draftWorkspace.revision;
    const step=draftWorkspace.steps.find(item=>item.id===stepId);
    const device=devices.value.find(item=>item.id===deviceId);
    if (tab.value!=='scripts' || !step || step.type!==stepType ||
        !stepCanPickTarget(step) || !device || device.owner_user_id!==owner ||
        !device.connection.enabled || !device.target_fingerprint || sync.value || recordingId.value) {
        error.value='脚本、设备或授权信息不匹配，不能定位旧控件。';return;
    }
    await changeTab('workbench');
    if (tab.value!=='workbench' || ownerId.value!==owner ||
        draftWorkspace.editingId!==scriptId || draftWorkspace.revision!==revision ||
        !draftWorkspace.steps.some(item=>item.id===stepId && item.type===stepType)) return;
    await nextTick();
    const card=cards.get(deviceId);
    if(!card?.getState().ready || !card.getStream()) {
        notice.value='已返回自动化中心：请打开目标手机卡片的原实时画面，再编辑步骤并重新拾取。';
        return;
    }
    editCardStep(deviceId,stepId);
    if(cardStepEditor.value?.deviceId!==deviceId || cardStepEditor.value.stepId!==stepId) return;
    if(!armCardPicking(deviceId,{kind:'replace',stepId,stepType})) {
        error.value='无法启动这台手机的原画面拾取，当前设备会话或授权已变化。';return;
    }
    card.openAutomationPanel();
    notice.value='已切换到原手机实时画面与步骤浮窗，请在该画面右键选择需要重新定位的控件。';
}
function closeInspector() {
    const id = inspectorDeviceId.value;
    const previous = inspectorEditScope.value;
    if (id) delete pickerPoints[id];
    inspectorAppendScope.value = null;
    inspectorDeviceId.value = null;
    inspectorEditScope.value = null;
    // Cancel/confirm both end ONE replace intent, without closing the live
    // screen, disabling picking or altering the original step on cancel.
    if (id && previous?.origin === 'card') {
        const current = pickerContexts[id];
        if (current?.phase === 'on' && current.intent.kind === 'replace' &&
            current.intent.stepId === previous.stepId && current.generation === previous.generation)
            pickerContexts[id] = setPickerIntent(current, {kind:'append'}, ++pickerEpoch);
    }
}
function replaceInspectorTarget(stepId: string, target: StepTarget) {
    const scope = inspectorEditScope.value;
    const id = inspectorDeviceId.value;
    const device = devices.value.find(item => item.id === id);
    const currentStep = draftWorkspace.steps.find(item => item.id === stepId);
    const basicValid = !!scope && !!device && device.target_fingerprint === scope.targetFingerprint
        && device.owner_user_id === ownerId.value
        && scope.stepId === stepId && scope.ownerId === ownerId.value
        && draftWorkspace.ownerId === scope.ownerId
        && draftWorkspace.editingId === scope.editingId
        && draftWorkspace.revision === scope.revision
        && currentStep?.type === scope.stepType;
    if (!basicValid || !scope || !device || !id) {
        error.value = '脚本、账号或设备上下文已发生变化，未修改草稿。';
        closeInspector();
        return;
    }
    if (scope.origin === 'card') {
        const context = pickerContexts[id], binding = pickerBinding(id), point = pickerPoints[id];
        const intent: PickerIntent = {kind:'replace',stepId,stepType:scope.stepType};
        if (tab.value !== 'workbench' || !scope.binding || !binding ||
            !pickerMatches(context,scope.binding) ||
            !pickerCanApply(context,binding,scope.generation ?? -1,scope.snapshotGeneration ?? -1,intent) ||
            !point || point.generation !== scope.generation ||
            point.snapshotGeneration !== scope.snapshotGeneration ||
            !cardStepEditor.value || cardStepEditor.value.deviceId !== id ||
            cardStepEditor.value.stepId !== stepId ||
            JSON.stringify(currentStep) !== scope.stepSnapshot) {
            error.value = '设备会话、步骤参数或拾取位置已改变，未替换旧步骤。';
            closeInspector(); return;
        }
    } else if (tab.value !== 'scripts') {
        error.value = '脚本库上下文已变化，未替换旧步骤。';
        closeInspector(); return;
    }
    const next = replaceAutomationStepTarget(draftWorkspace.steps, stepId, scope.stepType, target);
    if (!next) {
        error.value = '无法替换该步骤定位：请检查动作类型及唯一控件条件。';
        return;
    }
    draftWorkspace.steps = next;
    draftWorkspace.selectedStepId = stepId;
    closeInspector();
    notice.value = '已更新第 '+(next.findIndex(item=>item.id===stepId)+1)+' 步的定位目标，原步骤ID、操作参数和条件范围均保留。';
}
function addInspectorStep(step: AutomationStep, token: number, acknowledge: (accepted: boolean) => void) {
    const scope = inspectorAppendScope.value;
    if (!scope || scope.token !== token || scope.submitted ||
        scope.deviceId !== inspectorDeviceId.value || inspectorEditScope.value ||
        tab.value !== 'workbench') {
        acknowledge(false); // late/duplicate submit must never affect a newer Inspector
        return;
    }
    const active = pickerContexts[scope.deviceId];
    const binding = pickerBinding(scope.deviceId);
    const click = pickerPoints[scope.deviceId];
    if (!active || !binding || !pickerMatches(active, scope.binding) ||
        !pickerCanApply(active, binding, scope.generation, scope.snapshotGeneration, {kind:'append'}) ||
        !click || click.generation !== scope.generation ||
        click.snapshotGeneration !== scope.snapshotGeneration) {
        acknowledge(false);
        error.value = '本次拾取对应的设备、画面或脚本已经变化，未录入步骤；请重新右键拾取。';
        closeInspector();
        return;
    }
    if (!appendPickedAutomationStep(draftWorkspace, scope.binding.ownerId, step, uuid)) {
        acknowledge(false);
        error.value = '无法录入：步骤重复、脚本超过 100 步或分支结构无效。请检查草稿后重新确认。';
        return;
    }
    // Consume this opening token BEFORE the Vue dialog is torn down; a
    // double click or late event cannot append a second copy of this step.
    scope.submitted = true;
    acknowledge(true);
    const index = draftWorkspace.steps.findIndex(item => item.id === step.id);
    closeInspector(); // closes only the modal; leaves live video + picker ON
    notice.value = `已录入第 ${index + 1} 步 · ${step.type}。手机拾取仍开启，可继续在画面上右键添加下一步。`;
}
function selectCardStep(deviceId: string, stepId: string): boolean {
    const cardDevice = devices.value.find(item => item.id === deviceId);
    if (!canAccessAutomationCard(ownerId.value, deviceId, cardDevice, draftWorkspace) ||
        !draftWorkspace.steps.some(step => step.id === stepId)) return false;
    if (locatorDemos[deviceId]?.stepId !== stepId) clearCardDemo(deviceId);
    draftWorkspace.selectedStepId = stepId;
    return true;
}
function editCardStep(deviceId: string, stepId: string) {
    const previous=cardStepEditor.value;
    if (previous && (previous.deviceId !== deviceId || previous.stepId !== stepId) && cardEditorDirty.value) {
        error.value = '当前步骤有未保存的参数，请先在浮动编辑窗保存，或明确选择放弃修改。';
        return;
    }
    if (!selectCardStep(deviceId, stepId)) {
        error.value = '步骤或手机归属已变化，请重新打开脚本草稿。';
        return;
    }
    if (previous && (previous.deviceId !== deviceId || previous.stepId !== stepId))
        closeCardStepEditor(previous.deviceId);
    cardStepEditor.value={deviceId,stepId};
    cardEditorDirty.value=false;
}

/** Only one shared draft mutation path, regardless of which phone requests it. */
interface DeletedStepUndo {
    owner: number;
    scriptId: string | undefined;
    revision: number | undefined;
    beforeSteps: AutomationStep[];
    beforeSelected: string | undefined;
    afterFingerprint: string;
    count: number;
}
const deletedStepUndo = ref<DeletedStepUndo | null>(null);
let deletedStepUndoTimer: ReturnType<typeof setTimeout> | undefined;
function clearDeletedStepUndo() {
    if (deletedStepUndoTimer !== undefined) clearTimeout(deletedStepUndoTimer);
    deletedStepUndoTimer = undefined;
    deletedStepUndo.value = null;
}
watch(() => [draftWorkspace.ownerId,draftWorkspace.editingId,draftWorkspace.revision] as const,
    clearDeletedStepUndo);
onBeforeUnmount(clearDeletedStepUndo);
const deletedStepUndoLabel = computed(() => {
    const undo=deletedStepUndo.value;
    return undo && undo.owner===ownerId.value &&
        undo.scriptId===draftWorkspace.editingId &&
        undo.revision===draftWorkspace.revision &&
        undo.afterFingerprint===JSON.stringify(draftWorkspace.steps)
            ? '已删除 '+undo.count+' 步，可撤销'
            : '';
});
function deleteCardDraftStep(
    deviceId:string,stepId:string,expectedFingerprint:string,confirmedTargetId:string,
    expectedOwner:number,expectedScriptId:string|null,expectedScriptRevision:number|null,
) {
    const device=devices.value.find(item=>item.id===deviceId);
    if(ownerId.value!==expectedOwner ||
        (draftWorkspace.editingId??null)!==expectedScriptId ||
        (draftWorkspace.revision??null)!==expectedScriptRevision ||
        !canAccessAutomationCard(ownerId.value,deviceId,device,draftWorkspace) ||
        !draftSyncState.initialized || draftSyncState.ownerId!==ownerId.value ||
        draftSyncState.phase==='loading' || draftSyncState.phase==='conflict') {
        error.value='当前手机或草稿修订状态不可用，未删除步骤。';return;
    }
    if(cardEditorDirty.value) {
        error.value='有未保存的步骤参数，请先保存或放弃，再删除步骤。';return;
    }
    const preview=planAutomationStepDeletion(draftWorkspace.steps,stepId);
    if(!preview || preview.fingerprint!==expectedFingerprint ||
        preview.targetId!==confirmedTargetId) {
        error.value='确认删除期间条件范围或步骤已变化，未修改草稿；请重新点击删除。';return;
    }
    const beforeSteps=JSON.parse(JSON.stringify(draftWorkspace.steps)) as AutomationStep[];
    const beforeSelected=draftWorkspace.selectedStepId;
    clearDeletedStepUndo();
    if(cardStepEditor.value && preview.affectedIds.includes(cardStepEditor.value.stepId))
        closeCardStepEditor(cardStepEditor.value.deviceId);
    for(const id of Object.keys(locatorDemos)) {
        if(preview.affectedIds.includes(locatorDemos[id]?.stepId??''))clearCardDemo(id);
    }
    draftWorkspace.steps=preview.nextSteps;
    draftWorkspace.selectedStepId=preview.nextSteps.some(step=>step.id===beforeSelected)
        ? beforeSelected
        : preview.nextSteps[Math.min(beforeSteps.findIndex(step=>step.id===stepId),preview.nextSteps.length-1)]?.id;
    deletedStepUndo.value={
        owner:ownerId.value,scriptId:draftWorkspace.editingId,
        revision:draftWorkspace.revision,beforeSteps,beforeSelected,
        afterFingerprint:JSON.stringify(draftWorkspace.steps),count:preview.affectedIds.length,
    };
    deletedStepUndoTimer=setTimeout(clearDeletedStepUndo,10000);
    notice.value='已从当前工作草稿删除 '+preview.affectedIds.length+' 个步骤；正在由同一 CAS 草稿同步器保存。';
}
function undoCardDraftStepDeletion(deviceId:string) {
    const device=devices.value.find(item=>item.id===deviceId),undo=deletedStepUndo.value;
    if(cardEditorDirty.value) {
        error.value='当前编辑窗口还有未确认参数，不能撤销以免覆盖新内容；请先保存或放弃修改。';
        return;
    }
    if(!undo || !canAccessAutomationCard(ownerId.value,deviceId,device,draftWorkspace) ||
        !draftSyncState.initialized || draftSyncState.phase==='conflict' ||
        undo.owner!==ownerId.value || undo.scriptId!==draftWorkspace.editingId ||
        undo.revision!==draftWorkspace.revision ||
        undo.afterFingerprint!==JSON.stringify(draftWorkspace.steps)) {
        error.value='草稿已发生其它修改、授权变化或修订冲突，不能覆盖新内容来撤销。';
        clearDeletedStepUndo();return;
    }
    draftWorkspace.steps=undo.beforeSteps;
    draftWorkspace.selectedStepId=undo.beforeSelected;
    clearDeletedStepUndo();
    notice.value='已撤销上一次步骤删除，原步骤 ID 与条件范围已恢复；正在同步工作草稿。';
}
function markCardStepDirty(deviceId: string, dirty: boolean) {
    if (cardStepEditor.value?.deviceId === deviceId) cardEditorDirty.value=dirty;
}
function closeCardStepEditor(deviceId: string, discardConfirmed = false): boolean {
    if (cardStepEditor.value?.deviceId !== deviceId) return true;
    if (cardEditorDirty.value && !discardConfirmed) {
        error.value='当前参数尚未保存，请在浮动编辑窗选择继续编辑或明确放弃修改。';
        return false;
    }
    cardStepEditor.value=null;
    cardEditorDirty.value=false;
    clearCardDemo(deviceId);
    if (inspectorDeviceId.value === deviceId && inspectorEditScope.value?.origin === 'card') {
        closeInspector();
    }
    const picker=pickerContexts[deviceId];
    if (picker?.phase === 'on' && picker.intent.kind === 'replace')
        pickerContexts[deviceId]=setPickerIntent(picker,{kind:'append'},++pickerEpoch);
    return true;
}
function saveCardStep(deviceId:string,stepId:string,expected:AutomationStep,updated:AutomationStep) {
    const device=devices.value.find(item=>item.id===deviceId);
    if (!device || device.owner_user_id!==ownerId.value ||
        cardStepEditor.value?.deviceId!==deviceId ||
        cardStepEditor.value.stepId!==stepId ||
        !updateCardDraftStep(draftWorkspace,ownerId.value,stepId,expected,updated)) {
        error.value='无法保存：该步骤已改变、存在编辑冲突或条件分支不合法，请重新打开步骤。';
        return;
    }
    clearCardDemo(deviceId);
    cardEditorDirty.value=false;
    closeCardStepEditor(deviceId);
    notice.value='步骤参数已更新至当前脚本草稿；正式保存及试跑前仍需完成脚本检查。';
}
function repickCardStep(deviceId:string,stepId:string) {
    clearCardDemo(deviceId);
    const item=draftWorkspace.steps.find(step=>step.id===stepId);
    const device=devices.value.find(d=>d.id===deviceId);
    if (!item || !stepCanPickTarget(item) || !device || device.owner_user_id!==ownerId.value ||
        cardStepEditor.value?.deviceId!==deviceId || cardStepEditor.value.stepId!==stepId ||
        tab.value!=='workbench') {
        error.value='此步骤或手机当前不能重新拾取，请先选择有效步骤。';return;
    }
    if (inspectorDeviceId.value) closeInspector();
    if (!armCardPicking(deviceId,{kind:'replace',stepId,stepType:item.type})) {
        error.value='请先打开目标手机画面，并结束录制或群控，再重新拾取。';return;
    }
    notice.value='已进入第 '+(draftWorkspace.steps.findIndex(step=>step.id===stepId)+1)+
        ' 步的重新拾取模式：下一次在原手机画面右键选控件并确认，仅替换原定位，不新增步骤。';
}
/** UI-only preview: calls only the authenticated hierarchy GET endpoint. */
async function demoCardStep(deviceId:string,stepId:string) {
    const device=devices.value.find(item=>item.id===deviceId);
    const card=cards.get(deviceId);
    const step=draftWorkspace.steps.find(item=>item.id===stepId);
    const owner=ownerId.value;
    const video=card?.getState(), sessionId=card?.getSessionId();
    if (tab.value!=='workbench' || !device || device.owner_user_id!==owner ||
        !device.target_fingerprint || !device.connection.enabled ||
        draftWorkspace.ownerId!==owner || !step || !video?.ready ||
        !video.width || !video.height || !sessionId ||
        cardStepEditor.value?.deviceId!==deviceId ||
        cardStepEditor.value.stepId!==stepId || cardEditorDirty.value) {
        error.value='只读定位演示不可用：请保持当前手机画面与步骤编辑，并先保存参数修改。';
        return;
    }
    clearCardDemo(deviceId);
    const token=++locatorDemoEpoch, binding=device.target_fingerprint;
    const originalStep=JSON.stringify(step);
    locatorDemos[deviceId]={token,stepId,loading:true,result:null,error:''};
    const stillCurrent=()=>{
        const state=locatorDemos[deviceId];
        const live=cards.get(deviceId),current=live?.getState();
        const active=devices.value.find(item=>item.id===deviceId);
        return !disposed && state?.token===token && ownerId.value===owner &&
            tab.value==='workbench' && draftWorkspace.ownerId===owner &&
            cardStepEditor.value?.deviceId===deviceId &&
            cardStepEditor.value?.stepId===stepId && !cardEditorDirty.value &&
            active?.owner_user_id===owner && active.target_fingerprint===binding &&
            live?.getSessionId()===sessionId && !!current?.ready &&
            current.width===video.width && current.height===video.height &&
            JSON.stringify(draftWorkspace.steps.find(item=>item.id===stepId))===originalStep;
    };
    try {
        const hierarchy=await api.hierarchy(owner,deviceId);
        if(!stillCurrent()) return;
        const result=locateReadOnlyStep(hierarchy,step,{
            deviceId,targetFingerprint:binding,
            videoWidth:video.width,videoHeight:video.height,
        });
        locatorDemos[deviceId]={token,stepId,loading:false,result,error:''};
        if(result.status==='one') {
            locatorDemoTimers.set(deviceId,setTimeout(()=>{
                if(locatorDemos[deviceId]?.token===token) clearCardDemo(deviceId);
            },12000));
        }
    } catch(cause) {
        if(!stillCurrent()) return;
        locatorDemos[deviceId]={token,stepId,loading:false,result:null,
            error:'读取控件树失败：'+automationErrorMessage(cause)};
    }
}
function editInspectorDraft() {
    const id=inspectorDeviceId.value;
    closeInspector();
    if(id && tab.value==='workbench') {
        cards.get(id)?.openAutomationPanel();
        notice.value='已打开本手机原画面上的自动化脚本浮窗，可继续编辑已有步骤。';
    }
}
function finishRecording(edit: boolean) {
    if (gestureConfirmation.value) { notice.value = '请先确认刚才的滑动或拖拽，才能结束录制。'; return; }
    if (gesture || pendingRecordActions) notice.value = t('recordIncomplete');
    ++recordingGeneration;
    const id = recordingId.value; recordingId.value = null; gesture = null;
    if (id) cards.get(id)?.cancelPointer();
    if (recordedSteps.value.length) {
        if (appendAutomationDraftSteps(draftWorkspace, ownerId.value, recordedSteps.value, uuid)) {
            notice.value = `已将 ${recordedSteps.value.length} 个录制步骤追加到当前脚本草稿。`;
            recordedSteps.value = [];
        } else {
            error.value = '录制步骤未能合并到草稿，请检查草稿上限和分支结构；录制数据暂时保留。';
        }
    }
    if (edit) void changeTab('scripts');
}
async function changeTab(next: typeof tab.value) {
    if (next === tab.value || switchingTab.value || connecting.value || sendingText.value) return;
    if(next!=='workbench' && (floatingBatch.busy.value || floatingBatch.state.phase==='unknown')){
        notice.value='批量准备或执行期间保留工作台各机原视频。结果未知请先只读核实。';return;
    }
    if (tab.value === 'workbench' && cardEditorDirty.value &&
        !window.confirm('当前卡片步骤存在未保存的参数修改，确定切换页面并放弃这些修改吗？')) return;
    if (gestureConfirmation.value && tab.value === 'workbench') {
        notice.value = '还有待确认的滑动或拖拽，确认后再切换页面。';
        return;
    }
    if (next !== 'workbench') closeInspector();
    switchingTab.value = true;
    try {
        if (tab.value === 'workbench') {
            for (const card of cards.values()) card.cancelRestoreIntent();
            globalOpen.value = false; syncPreparation.value = false;
            if (recordingId.value) finishRecording(false);
            await stopSync();
            const live = [...cards.entries()].filter(([, card]) => card.getState().ready).map(([id]) => id);
            if (!await releaseManual(live)) return;
        }
        // Switching tabs in this page is safe for the single shared draft.
        // Even an unconfirmed server write must remain visible and retryable;
        // only the transient card editor can be explicitly discarded.
        tab.value = next;
        if(hasLocalDraftData() && !draftSync.canLeave())
            notice.value='工作草稿仍保留在本页，服务端保存尚未确认；切回工作台或脚本库可继续编辑/重试同步。';
        clearAllCardDemos();
        cardStepEditor.value = null;
        cardEditorDirty.value = false;
        stopAllPicking();
        if (next === 'settings') settingsVisited.value = true;
        if (next === 'workbench') await loadDevices();
    } finally { switchingTab.value = false; }
}
async function settingsSaved() {
    try { context.value = await api.context(); }
    catch (cause) { handleError(cause); }
}
async function onRepairStep(request: AutomationRepairRequest) {
    if (request.ownerId !== ownerId.value || switchingTab.value) return;
    // Switching away from history must not cause a manual phone action.
    // The editor independently checks latest saved revisions before changes.
    repairRequest.value = request;
    await changeTab('scripts');
    if (tab.value !== 'scripts') repairRequest.value = null;
}

/**
 * T15: resolve an immutable historical failure to one authorized phone card.
 * Never reuse a newer formal script revision, and never overwrite a current
 * draft just because a saved script happened to share the same name.
 */
async function onRepairCard(request: AutomationRepairRequest) {
    if (switchingTab.value || request.ownerId !== ownerId.value) return;
    const owner=ownerId.value, mine=++cardRepairEpoch;
    const current=()=>!disposed && ownerId.value===owner && mine===cardRepairEpoch;
    error.value='';
    try {
        // Source-of-truth records, not the old history UI's cached steps.
        const [run, scripts, inventory]=await Promise.all([
            api.getRun(owner,request.runId),api.listScripts(owner),api.devices(owner),
        ]);
        if(!current())return;
        const device=run.devices.find(item=>item.device_id===request.deviceId);
        const outcome=device?.steps.find(item=>item.step_id===request.stepId);
        if(!device || !outcome) throw new Error('历史记录中已无法定位原失败步骤。');
        const verified=createAutomationRepairRequest(run,device,outcome,owner,request.requestId);
        if(!verified || verified.resultState!==request.resultState ||
            JSON.stringify(verified.snapshotSteps)!==JSON.stringify(request.snapshotSteps))
            throw new Error('历史运行结果或步骤快照已更新；请先刷新执行记录再定位修复。');
        const target=inventory.items.find(item=>item.id===request.deviceId && item.owner_user_id===owner);
        if(!target)throw new Error('原手机不在当前账号的授权设备列表中；请先恢复授权。');
        const plan=planAutomationRepair(verified,scripts.items);
        const context=plan && createCardRepairContext(verified,plan);
        if(!plan || !context)throw new Error('历史脚本快照无法生成可信的修复草稿。');

        // A one-step inline run normally originates from this exact shared
        // card draft; retain its other steps and unpublished edits.
        const source=verified.snapshotSteps[0];
        const sameCardDraft=!verified.scriptId && verified.snapshotSteps.length===1 &&
            source?.id===request.stepId &&
            draftWorkspace.ownerId===owner &&
            draftWorkspace.steps.some(item=>item.id===source.id && JSON.stringify(item)===JSON.stringify(source));
        if(sameCardDraft) context.sourceKind='current';
        if(!sameCardDraft) {
            if(!draftSyncState.initialized || draftSyncState.phase==='saving')
                throw new Error('当前工作草稿仍在同步，请等待保存状态确定后再切换修复草稿。');
            if(cardEditorDirty.value &&
                !window.confirm('卡片步骤编辑有未保存的修改，确定放弃这些局部修改吗？'))return;
            if(!warnBeforeDraftReplacement())return;
            if(!window.confirm('将从历史执行快照创建新的修复工作草稿，替换当前编辑区内容。已确认保存的旧工作草稿保留在服务器。是否继续？'))return;
        }
        if(!current())return;
        if(tab.value!=='workbench') await changeTab('workbench');
        if(!current() || tab.value!=='workbench')return;
        // The tab's loadDevices() has completed; do not point a historical
        // snapshot at a device that is no longer authorized.
        if(!devices.value.some(item=>item.id===context.deviceId &&
            item.owner_user_id===owner))
            throw new Error('原手机的授权/设备绑定已变化，不能打开卡片回修。');
        stopAllPicking();
        closeInspector();
        clearAllCardDemos();
        cardStepEditor.value=null;cardEditorDirty.value=false;
        if(!sameCardDraft) {
            // Reset the SERVER draft identity first, otherwise T11 would
            // update the previous work draft in place.
            if(draftSyncState.phase==='saving')
                throw new Error('工作草稿开始同步，请等待确认后重试回修。');
            draftSync.fork();
            Object.assign(draftWorkspace,cardRepairWorkspace(owner,plan));
        }
        draftWorkspace.selectedStepId=context.stepId;
        cardRepairContext.value=context;
        // Show the target even if a previous workbench filter hid it.
        if(!visibleIds.value.has(context.deviceId)) {
            filters.value={node:'',group:'',type:'',keyword:''};
            applied.value={...filters.value};
        }
        await nextTick();
        if(!current() || tab.value!=='workbench')return;
        editCardStep(context.deviceId,context.stepId);
        notice.value=context.sourceKind==='current'
            ? '已回到原手机卡片和失败步骤；当前草稿其他步骤没有修改。请修复后手动再次验证。'
            : '已打开历史失败步骤的独立修复工作草稿；原运行与正式脚本未修改。修复后需要手动再次试跑。';
    }catch(cause){
        if(current())error.value='返回卡片修复失败：'+automationErrorMessage(cause);
    }
}
async function onCardTrialRepair(deviceId:string,runId:string,stepId:string) {
    if(!runId || !stepId || !devices.value.some(d=>d.id===deviceId &&
        d.owner_user_id===ownerId.value))return;
    const owner=ownerId.value, mine=++cardRepairEpoch;
    try {
        const run=await api.getRun(owner,runId);
        if(disposed || owner!==ownerId.value || mine!==cardRepairEpoch)return;
        const device=run.devices.find(d=>d.device_id===deviceId);
        const step=device?.steps.find(s=>s.step_id===stepId);
        const request=device && step
            ? createAutomationRepairRequest(run,device,step,owner,uuid()) : null;
        if(!request)throw new Error('本次试跑未返回可回修的失败步骤，先查看执行历史。');
        await onRepairCard(request);
    }catch(cause){
        if(!disposed && owner===ownerId.value && mine===cardRepairEpoch)
            error.value='无法打开失败步骤：'+automationErrorMessage(cause);
    }
}
function repickCardRepairStep(deviceId:string,stepId:string) {
    const context=cardRepairContext.value;
    if(!context || context.deviceId!==deviceId || context.stepId!==stepId ||
        context.ownerId!==ownerId.value || tab.value!=='workbench')return;
    editCardStep(deviceId,stepId);
    if(cardStepEditor.value?.deviceId===deviceId && cardStepEditor.value.stepId===stepId)
        repickCardStep(deviceId,stepId);
}
function dismissCardRepair(deviceId:string) {
    if(cardRepairContext.value?.deviceId===deviceId)
        cardRepairContext.value=null;
}
function onRunCreated(run: AutomationRun) {
    clearAllCardDemos();
    stopAllPicking();
    closeInspector();
    for (const card of cards.values()) card.cancelRestoreIntent();
    focusRunId.value = run.id; ++historyKey.value; tab.value = 'history';
}
async function onCardTrialHistory(deviceId: string, runId: string) {
    // The card can only navigate on behalf of a device in the current account.
    if (!runId || !devices.value.some(device => device.id===deviceId &&
        device.owner_user_id===ownerId.value)) return;
    await changeTab('history');
    if (tab.value==='history') {
        focusRunId.value=runId;
        ++historyKey.value;
    }
}
function savedConnection(device: AutomationDevice) {
    clearCardDemo(device.id);
    stopCardPicking(device.id);
    if (inspectorDeviceId.value === device.id) closeInspector();
    adbActionVersions.set(device.id, (adbActionVersions.get(device.id) ?? 0) + 1);
    delete phoneConnections.value[device.id];
    delete phoneConnectionErrors.value[device.id];
    delete componentStatuses.value[device.id];
    componentStatusResults.value = componentStatusResults.value.filter(item => item.id !== device.id);
    devices.value = devices.value.map(item => item.id === device.id ? device : item);
    if (connectionDevice.value?.id === device.id) connectionDevice.value = device;
}
function visibilityChanged() {
    if (document.hidden) {
        // A hidden browser tab or another menu must not tear down WebRTC.
        // Only release active pointer gestures to avoid a stuck touch.
        for (const card of cards.values()) card.cancelPointer();
    }
}
function fullScreenChanged() { fullscreenActive.value = !!document.fullscreenElement; }
function releaseOnBlur() {
    for (const card of cards.values()) card.cancelPointer();
}
onMounted(async () => {
    restoreMediaPreferences();
    document.addEventListener('visibilitychange', visibilityChanged);
    document.addEventListener('fullscreenchange', fullScreenChanged);
    window.addEventListener('blur', releaseOnBlur);
    window.addEventListener('beforeunload',beforeUnloadDraft);
    try {
        const value = await api.context(); if (disposed) return;
        context.value = value; ownerId.value = value.actor.id; accounts.value = [value.actor];
        if (value.available && value.can_manage_others) {
            const result = await api.accounts(); if (disposed) return;
            accounts.value = result.items;
        }
        await loadDevices();
    } catch (cause) { if (!disposed) handleError(cause); }
});
onBeforeUnmount(() => { stopEvents?.();
    disposed = true; ++workspaceGeneration; ++loadGeneration; ++catalogGeneration; ++statusQueryGeneration;
    clearAllCardDemos();
    stopAllPicking();
    document.removeEventListener('visibilitychange', visibilityChanged);
    document.removeEventListener('fullscreenchange', fullScreenChanged);
    window.removeEventListener('blur', releaseOnBlur);
    window.removeEventListener('beforeunload',beforeUnloadDraft);
    draftSync.dispose();
    void closeAll();
});
</script>

<template>
    <section class="console-page automation-center">
        <header class="page-heading"><div><h1>{{ t('title') }}</h1><p class="page-description">{{ t('description') }}</p></div><span v-if="ownerId" class="owner-caption"><i class="pi pi-user" aria-hidden="true" /> {{ t('managing') }} · <strong>{{ managedAccount }}</strong></span></header>
        <Message v-if="sourceIssues.length" severity="warn" :closable="false">有 {{ sourceIssues.length }} 条设备资料需要确认手机类型或 ADB 服务，请在节点设备与服务编辑器中处理。<details><summary>查看待处理设备</summary><div v-for="issue in sourceIssues" :key="issue.device_id"><a :href="`#/h/node-assets?owner=${ownerId}&node=${issue.machine_id}&device=${issue.device_id}`">{{ issue.name }}</a> · {{ issue.reason }}</div></details></Message>
        <Message v-if="error" severity="error" @close="error = ''">{{ error }}</Message>
        <Message v-if="notice" severity="info" @close="notice = ''">{{ notice }}</Message>
        <Message v-if="context && !context.available" severity="warn" :closable="false">{{ t('unavailable') }}</Message>
        <Message v-else-if="context && !context.runner_configured" severity="info" :closable="false">{{ t('noRunner') }} <Button v-if="context.can_manage_settings && tab !== 'settings'" :label="t('settings')" text size="small" @click="changeTab('settings')" /></Message>
        <template v-if="context?.available">
            <div class="console-panel draft-sync-bar" role="status" aria-live="polite">
                <div class="draft-sync-summary">
                    <strong>{{ draftSyncLabel }}</strong>
                    <small v-if="draftSyncState.message" class="draft-sync-message">{{ draftSyncState.message }}</small>
                    <small v-if="draftSyncState.phase === 'unsynced' || draftSyncState.phase === 'saving'">
                        未收到服务端确认前，请勿强制刷新网页；在此页面的切换不会清空共享草稿。
                    </small>
                </div>
                <div class="draft-sync-actions">
                    <select v-model="chosenDraftId" aria-label="选择服务端工作草稿">
                        <option value="">选择服务端工作草稿</option>
                        <option v-for="item in draftSyncState.entries" :key="item.id" :value="item.id">
                            {{ item.name || '未命名草稿' }} · {{ item.count }}步 · 修订{{ item.revision }}
                        </option>
                    </select>
                    <Button label="恢复" size="small" text icon="pi pi-folder-open"
                        :disabled="!chosenDraftId || draftSyncState.phase === 'loading' || draftSyncState.phase === 'saving'"
                        @click="restoreSavedDraft()" />
                    <Button label="刷新草稿" size="small" text icon="pi pi-refresh" @click="draftSync.refreshList()" />
                    <Button label="保存当前" size="small" text icon="pi pi-save"
                        :disabled="draftSyncState.phase === 'loading' || draftSyncState.phase === 'saving' || draftSyncState.phase === 'conflict'"
                        @click="draftSync.retry()" />
                    <Button label="另存新草稿" size="small" text
                        :disabled="draftSyncState.phase === 'saving' || draftSyncState.phase === 'loading'"
                        @click="forkCurrentDraft" />
                    <Button label="删除草稿" size="small" text severity="danger"
                        :disabled="!chosenDraftId || draftSyncState.phase === 'saving'"
                        @click="discardSavedDraft" />
                </div>
            </div>
            <p v-if="draftWorkspace.steps.length" class="draft-reuse-hint" role="status">
                同账号只有一份当前工作草稿：{{ draftWorkspace.name || '未命名草稿' }}
                · {{ draftWorkspace.steps.length }} 步。
                {{ activePickingDeviceNames.length ? '正在拾取：'+activePickingDeviceNames.join('、')+'。' : '当前未选择拾取设备。' }}
                其他授权手机可查看同一草稿，但执行前会重新核验绑定、画面尺寸及控件定位；
                不能无提示复用不匹配的坐标。
            </p>
            <div class="workspace-header">
                <nav class="automation-tabs" :aria-label="t('title')"><button v-for="item in tabs" :key="item" type="button" :class="{ active: tab === item }" :aria-current="tab === item ? 'page' : undefined" :disabled="switchingTab || connecting || sendingText" :title="t('tabReleaseHint')" @click="changeTab(item)">{{ t(item) }}</button></nav>
                <label v-if="context.can_manage_others && tab !== 'settings'" class="owner-select">{{ t('owner') }}<select :value="ownerId" :disabled="busy || connecting || sendingText || switchingTab" @change="changeOwner(Number(($event.target as HTMLSelectElement).value))"><option v-for="account in accounts" :key="account.id" :value="account.id">{{ account.username }}</option></select></label>
            </div>
            <div v-show="tab === 'workbench'" class="workbench-view">
                <form class="console-panel filters" @submit.prevent="query">
                    <label>{{ t('node') }}<select v-model="filters.node"><option value="">{{ t('allNodes') }}</option><option v-for="[id, name] in nodeOptions" :key="id" :value="id">{{ name }}</option></select></label>
                    <label>{{ t('group') }}<select v-model="filters.group"><option value="">{{ t('allGroups') }}</option><option v-for="[id, name] in groupOptions" :key="id" :value="id">{{ name }}</option></select></label>
                    <label>{{ t('deviceType') }}<select v-model="filters.type"><option value="">{{ t('allTypes') }}</option><option v-for="[id, name] in typeOptions" :key="id" :value="id">{{ name }}</option></select></label>
                    <label class="keyword-field">{{ t('search') }}<InputText v-model="filters.keyword" :placeholder="t('keyword')" /></label>
                    <Button type="submit" icon="pi pi-search" :label="t('query')" :loading="busy" />
                </form>
                <div class="workbench-toolbar">
                    <div class="summary"><span>{{ t('registered') }} <strong>{{ visibleDevices.length }}</strong></span><span>{{ t('selected') }} <strong>{{ selectedIds.length }}</strong></span><span>{{ t('live') }} <strong>{{ liveCount }}</strong></span></div>
                    <div class="toolbar-actions"><Button :label="t('selectShown')" text size="small" @click="selectedIds = [...new Set([...selectedIds, ...visibleDevices.map(device => device.id)])]" /><Button :label="t('clearSelection')" text size="small" @click="selectedIds = []" /><label class="quality" :title="t('qualityHint')"><span>{{ t('resolution') }}</span><select v-model.number="mediaSettings.max_size"><option :value="640">640</option><option :value="960">960</option><option :value="1280">1280</option><option :value="1920">1920</option><option :value="2560">2560</option><option :value="3840">3840</option><option :value="0">{{ t('originalResolution') }}</option></select></label><label class="quality"><span>{{ t('bitrate') }}</span><select v-model.number="mediaSettings.video_bit_rate"><option :value="1000000">1 Mbps</option><option :value="2000000">2 Mbps</option><option :value="4000000">4 Mbps</option><option :value="8000000">8 Mbps</option><option :value="12000000">12 Mbps</option><option :value="16000000">16 Mbps</option><option :value="20000000">20 Mbps</option><option :value="24000000">24 Mbps</option></select></label><label class="quality"><span>FPS</span><select v-model.number="mediaSettings.max_fps"><option :value="15">15</option><option :value="24">24</option><option :value="30">30</option><option :value="60">60</option><option :value="90">90</option><option :value="120">120</option></select></label><label class="media-toggle"><input v-model="mediaSettings.low_latency" type="checkbox" />{{ t('lowLatency') }}</label><label class="media-toggle"><input v-model="mediaSettings.screen_off" type="checkbox" />{{ t('screenOff') }}</label><Button v-if="filters.group || applied.group" :label="t('discoverAdb')" icon="pi pi-search" size="small" outlined :loading="discoveryBusy" :disabled="discoveryBusy" @click="discoverAdb" /><span class="screen-action-pair"><Button :label="t('openSelected')" icon="pi pi-play" size="small" :loading="connecting" :disabled="!runnerAvailable || !selectedIds.length" @click="openSelected" /><Button :label="t('closeSelected')" icon="pi pi-stop" outlined size="small" :disabled="!selectedIds.length" @click="closeSelected" /></span><Button :label="t('connect')" icon="pi pi-link" size="small" :loading="phoneConnecting" :disabled="!runnerAvailable || !selectedIds.length" @click="connectPhones([...selectedIds])" /><Button label="查询控件状态" icon="pi pi-microchip" size="small" outlined :loading="componentChecking" :disabled="!runnerAvailable || !selectedIds.length || componentChecking" title="只读查询勾选手机的 UiAutomator 状态，不初始化组件" @click="querySelectedComponentStatuses" /><Button :label="t('releaseManual')" icon="pi pi-ban" text size="small" :disabled="!selectedIds.length" @click="releaseManual([...selectedIds])" /></div>
                </div>
                <section v-if="componentStatusResults.length" class="console-panel component-results" aria-live="polite">
                    <div class="component-results-heading"><strong>控件状态查询结果</strong><Button label="收起" icon="pi pi-times" text size="small" :disabled="componentChecking" @click="componentStatusResults = []" /></div>
                    <div class="component-result-list">
                        <div v-for="item in componentStatusResults" :key="item.id" class="component-result">
                            <strong>{{ item.name }}</strong>
                            <span :class="['component-result-state', item.state]">{{ componentStateLabel[item.state] || item.state }}</span>
                            <small v-if="item.message">{{ item.message }}</small>
                        </div>
                    </div>
                </section>
                <div v-if="sync" class="activity-banner sync-banner" role="status"><div><strong>{{ t('syncRunning', { count: sync.deviceIds.length }) }}</strong><p>{{ t('master') }}：{{ deviceNames[sync.masterId] }} · {{ t('syncFixed') }}</p></div><Button :label="t('stopSync')" severity="danger" icon="pi pi-stop-circle" @click="stopSync" /></div>
                <div v-if="recordingId" class="activity-banner recording-banner" role="status"><div><strong>{{ recordingPaused ? t('recordPaused') : t('recordRunning', { name: recordingName }) }}</strong><p>{{ t('recordedCount', { count: recordedSteps.length }) }} · {{ t('recordHint') }}</p></div><div class="record-actions"><Button :label="t(recordingPaused ? 'resume' : 'pause')" :icon="recordingPaused ? 'pi pi-play' : 'pi pi-pause'" outlined @click="pauseRecording" /><Button :label="t('editRecording')" severity="danger" icon="pi pi-stop" @click="finishRecording(true)" /></div></div>
                <div v-if="!devices.length && !busy" class="console-panel console-empty-state"><i class="pi pi-mobile" aria-hidden="true" /><h2>{{ t('noAssets') }}</h2><p>{{ t('noAssetsDetail') }}</p><Button :label="t('toAssets')" outlined @click="router.push({ name: 'nodeAssets' })" /></div>
                <div v-else-if="!visibleDevices.length && !busy" class="console-panel console-empty-state"><i class="pi pi-search" aria-hidden="true" /><h2>{{ t('noResults') }}</h2><Button :label="t('reset')" text @click="filters = { node: '', group: '', type: '', keyword: '' }; query()" /></div>
                <div v-if="busy && !devices.length" class="console-panel console-empty-state" role="status"><i class="pi pi-spin pi-spinner" aria-hidden="true" />{{ t('loading') }}</div>
                <div class="phone-grid"><PhoneCard v-for="device in sortedDevices" v-show="visibleIds.has(device.id)" :key="`${ownerId}:${device.id}`" :ref="value => setCard(device.id, value)" :device="device" :owner-id="ownerId" :selected="selectedIds.includes(device.id)" :runner-available="runnerAvailable" :component-status="componentStatuses[device.id]" :phone-connection="phoneConnections[device.id]" :phone-connection-error="phoneConnectionErrors[device.id]" :sync-active="!!sync" :sync-role="sync?.deviceIds.includes(device.id) ? (sync.masterId === device.id ? 'master' : 'follower') : undefined" :recording="recordingId === device.id" :unlocking="unlockingIds.includes(device.id)" :draft="draftWorkspace" :draft-sync-label="draftSyncLabel"
                        :scripts="scriptWorkspace.state.items" :scripts-busy="scriptWorkspace.state.busy"
                        :scripts-loading="scriptWorkspace.state.loading" :scripts-error="scriptWorkspace.state.error"
                        :script-guard="scriptWorkspace.guard.value"
                        :selected-device-count="selectedIds.length"
                        :batch-run-state="floatingBatch.state" :batch-draft-ready="batchDraftReady"
                        :undo-delete-label="deletedStepUndoLabel" :editing-dirty="cardEditorDirty"
                        :editing-step-id="cardStepEditor?.deviceId === device.id ? cardStepEditor.stepId : undefined"
                        :repicking-step-id="repickingStepIds[device.id]"
                        :inspector-visible="inspectorDeviceId === device.id"
                        :inspector-commit-token="inspectorOpenGeneration"
                        :inspector-queued-count="draftWorkspace.steps.length"
                        :inspector-replace-step="inspectorDeviceId === device.id ? inspectorReplaceStep : null"
                        :inspector-initial-point="inspectorDeviceId === device.id ? inspectorInitialPoint : null"
                        :inspector-preferred-action="inspectorDeviceId === device.id ? inspectorPreferredAction : null"
                        :repair-context="cardRepairContext?.deviceId === device.id ? cardRepairContext : null"
                        :demo-result="locatorDemos[device.id]?.stepId === cardStepEditor?.stepId ? locatorDemos[device.id]?.result : null"
                        :demo-loading="locatorDemos[device.id]?.loading ?? false" :demo-error="locatorDemos[device.id]?.error"
                        @selection="select" @configure="connectionDevice = $event" @input="routeInput" @action="localAction" @unlock="id => unlockPhones([id])" @text="id => showText([id])" @record="toggleRecord" :picker-phase="pickerContexts[device.id]?.phase ?? 'off'" :picker-error="pickerContexts[device.id]?.error"
                        @toggle-picking="toggleCardPicking" @pick-action="onCardPickAction"
                        @inspector-close="closeInspector" @inspector-add-step="addInspectorStep"
                        @inspector-replace-target="replaceInspectorTarget" @inspector-edit="editInspectorDraft"
                        @script-refresh="refreshScriptList" @script-action="handleFloatingScriptAction"
                        @batch-start="runSelectedBatch" @batch-refresh="refreshBatch"
                        @batch-stop="stopBatch" @batch-stop-device="stopBatchDevice"
                        @batch-reconcile="reconcileBatch"
                        @step-edit="editCardStep" @step-delete="deleteCardDraftStep"
                        @step-undo="undoCardDraftStepDeletion"
                        @trial-history="onCardTrialHistory" @trial-repair="onCardTrialRepair" @repair-repick="repickCardRepairStep" @repair-dismiss="dismissCardRepair" @step-close="closeCardStepEditor"
                        @step-save="saveCardStep" @step-repick="repickCardStep" @step-dirty="markCardStepDirty"
                        @step-demo="demoCardStep" @demo-clear="clearCardDemo" @connect="id => connectPhones([id])" @toggle-screen="toggleDeviceScreen" @state="onState" @geometry="onGeometry" @stop-sync="stopSync" @refresh="loadDevices" /></div>
                <section v-if="results.length" class="console-panel results-panel"><div class="section-heading"><h2>{{ t('operationResults') }}</h2></div><ul><li v-for="result in results" :key="result.id"><strong>{{ result.name }}</strong><span :class="{ failed: result.state !== 'action_sent' }">{{ result.state === 'action_sent' ? t('actionSent') : result.state === 'sending' ? t('loading') : t('resultUnknown') }}</span><p v-if="result.message">{{ result.message }}</p></li></ul></section>
                <button v-if="globalOpen && !fullscreenActive" class="global-dismiss" :aria-label="t('collapse')" @pointerdown.stop @click.stop="globalOpen = false" />
                <div v-if="devices.length && !fullscreenActive" class="global-controls" @pointerdown.stop @click.stop @keydown.esc="globalOpen = false">
                    <div v-if="globalOpen" class="global-actions" role="group" :aria-label="t('globalControl')"><span class="global-scope">{{ t('globalControl') }} · {{ t('targetCount', { count: globalTargets.length }) }}</span><Button icon="pi pi-arrow-left" rounded :title="t('back')" :aria-label="t('back')" :disabled="!globalTargets.length || globalManualBlocked" @click="globalAction({ type: 'key', key: 'BACK' })" /><Button icon="pi pi-home" rounded :title="t('home')" :aria-label="t('home')" :disabled="!globalTargets.length || globalManualBlocked" @click="globalAction({ type: 'key', key: 'HOME' })" /><Button icon="pi pi-clone" rounded :title="t('recent')" :aria-label="t('recent')" :disabled="!globalTargets.length || globalManualBlocked" @click="globalAction({ type: 'key', key: 'RECENT' })" /><Button icon="pi pi-pencil" rounded :title="t('text')" :aria-label="t('text')" :disabled="!globalTargets.length || globalManualBlocked" @click="showText([...globalTargets])" /><Button icon="pi pi-power-off" rounded :title="t('wake')" :aria-label="t('wake')" :disabled="!globalTargets.length || globalManualBlocked" @click="globalAction({ type: 'key', key: 'WAKE' })" /><Button icon="pi pi-unlock" rounded severity="primary" :title="t('unlockPhone')" :aria-label="t('unlockPhone')" :loading="globalTargets.some(id => unlockingIds.includes(id))" :disabled="!globalTargets.length || globalManualBlocked || !!sync || !!recordingId || globalTargets.some(id => unlockingIds.includes(id))" @click="unlockPhones([...globalTargets])" /><Button icon="pi pi-ban" rounded severity="secondary" :title="t('releaseManual')" :aria-label="t('releaseManual')" :disabled="!globalTargets.length || globalManualBlocked || globalTargets.some(id => unlockingIds.includes(id))" @click="releaseManual([...globalTargets])" /><Button :icon="sync ? 'pi pi-stop-circle' : 'pi pi-share-alt'" rounded :severity="sync ? 'danger' : 'primary'" :title="t(sync ? 'stopSync' : 'sync')" :aria-label="t(sync ? 'stopSync' : 'sync')"  :disabled="!sync && globalManualBlocked" @click="sync ? stopSync() : prepareSync()" /></div>
                    <Button class="global-orb" :icon="globalOpen ? 'pi pi-times' : 'pi pi-sliders-h'" rounded :aria-label="t('globalControl')" :aria-expanded="globalOpen" @click="globalOpen = !globalOpen" /><span class="target-badge" aria-hidden="true">{{ globalTargets.length }}</span>
                </div>
            </div>
            <p v-if="liveCount" class="manual-release-hint">{{ t('tabReleaseHint') }}</p>
            <ScriptLibrary v-if="tab === 'scripts' && ownerId" :key="ownerId" :owner-id="ownerId" :selected-device-ids="selectedIds" :device-names="deviceNames" :draft="draftWorkspace" :draft-sync-label="draftSyncLabel" :draft-sync-message="draftSyncState.message"
                :can-replace-workspace="warnBeforeDraftReplacement" :run-device-snapshots="devices"
                :repair-request="repairRequest" :prepare-run-device="prepareRunDevice" :release-run-previews="releaseRunPreviews"
                :picker-devices="devices.filter(device => device.connection.enabled && !!device.target_fingerprint).map(device => ({ id: device.id, name: device.name }))"
                @pick-step-target="openScriptInspector" @run-created="onRunCreated"
                @repair-consumed="id => { if (repairRequest?.requestId === id) repairRequest = null; }" />
            <RunHistory v-else-if="tab === 'history' && ownerId" :key="`${ownerId}:${historyKey}`" :owner-id="ownerId" :focus-run-id="focusRunId" @repair-step="onRepairStep" @repair-card="onRepairCard" />
            <ExecutorSettings v-if="settingsVisited && context.can_manage_settings" v-show="tab === 'settings'" :live-count="liveCount" @saved="settingsSaved" @close-screens="closeAll" />
        </template>
        <DiscoveryDialog v-if="discoveryGateway" :key="discoveryGateway.id" :gateway="discoveryGateway" :owner-id="ownerId" initial-action="adb" @close="discoveryGateway = null" @changed="loadDevices" />
        <ConnectionDialog :device="connectionDevice" :owner-id="ownerId" :runner-available="runnerAvailable" @close="connectionDevice = null" @saved="savedConnection" />
        <Dialog :visible="!!gestureConfirmation" :appendTo="textDialogTarget" modal
            :closable="false" :close-on-escape="false"
            header="录制手势 · 确认操作类型"
            :style="{ width: '470px', maxWidth: 'calc(100vw - 24px)' }">
            <div class="modal-content">
                <p>检测到手指移动。单凭轨迹无法可靠区分页面滑动和拖动物体，请根据刚才实际操作选择。</p>
                <p v-if="gestureConfirmation" class="muted">实测持续 {{ gestureConfirmation.durationMs }} 毫秒；拖拽最短按 300 毫秒重放。</p>
                <p class="muted">确认前不会写入脚本，也不会自动重新执行手机动作。</p>
            </div>
            <template #footer>
                <Button label="不录制" text severity="secondary" @click="confirmRecordedMotion('discard')" />
                <Button label="记为滑动" icon="pi pi-arrows-v" outlined @click="confirmRecordedMotion('swipe')" />
                <Button label="记为拖拽" icon="pi pi-arrows-alt" @click="confirmRecordedMotion('drag')" />
            </template>
        </Dialog>
        <Dialog v-model:visible="syncPreparation" modal :header="t('syncTitle')" :style="{ width: '500px', maxWidth: 'calc(100vw - 24px)' }"><div class="modal-content"><Message severity="info" :closable="false">{{ t('syncHint') }}</Message><p>{{ t('targetCount', { count: syncTargets.length }) }}</p><ul><li v-for="id in syncTargets" :key="id">{{ deviceNames[id] ?? id }}</li></ul><label>{{ t('master') }}<select v-model="syncMaster"><option v-for="id in syncTargets" :key="id" :value="id">{{ deviceNames[id] ?? id }}</option></select></label></div><template #footer><Button :label="t('cancel')" text @click="syncPreparation = false" /><Button :label="t('sync')" icon="pi pi-share-alt" @click="beginSync" /></template></Dialog>
        <Dialog :key="fullscreenActive ? 'fullscreen-text' : 'page-text'" v-model:visible="textDialog" :appendTo="textDialogTarget" modal :header="t('textTitle')" :closable="!sendingText" :close-on-escape="!sendingText" :style="{ width: '520px', maxWidth: 'calc(100vw - 24px)' }"><div class="modal-content"><p>{{ t('targetCount', { count: textTargets.length }) }} · {{ textTargets.map(id => deviceNames[id] ?? id).join('、') }}</p><Textarea v-model="inputText" :placeholder="t('textPlaceholder')" rows="5" maxlength="2000" :disabled="sendingText" autofocus /><p class="muted">{{ t('textHint') }}</p></div><template #footer><Button :label="t('cancel')" text :disabled="sendingText" @click="textDialog = false; inputText = ''" /><Button :label="t('send')" icon="pi pi-send" :loading="sendingText" :disabled="!inputText" @click="sendText" /></template></Dialog>
    </section>
</template>

<style scoped>
.draft-reuse-hint{margin:0;padding:9px 12px;font-size:12px;line-height:1.6;color:var(--console-muted);border:1px solid var(--console-border);border-radius:9px;background:var(--console-panel)}

.draft-sync-bar{display:flex;justify-content:space-between;align-items:center;gap:10px;padding:10px 14px;flex-wrap:wrap}
.draft-sync-summary{display:flex;flex:1;min-width:180px;flex-direction:column;gap:4px;font-size:12px}
.draft-sync-summary small{color:var(--console-muted);font-size:11px;line-height:1.5}
.draft-sync-summary .draft-sync-message{color:var(--p-orange-500)}
.draft-sync-actions{display:flex;align-items:center;gap:4px;flex-wrap:wrap}
.draft-sync-actions select{max-width:210px;border:1px solid var(--console-border);border-radius:6px;padding:7px;background:var(--console-panel-raised);color:var(--p-text-color);font-size:11px}
.automation-tabs button{white-space:nowrap}
.component-results{padding:12px 16px}.component-results-heading{display:flex;align-items:center;justify-content:space-between;gap:12px;font-size:13px}.component-result-list{display:grid;grid-template-columns:repeat(auto-fit,minmax(190px,1fr));gap:6px 16px;margin-top:8px}.component-result{display:flex;align-items:center;gap:8px;flex-wrap:wrap;font-size:12px;padding:6px 0;border-bottom:1px solid var(--console-border)}.component-result strong{min-width:0;overflow-wrap:anywhere}.component-result small{flex-basis:100%;color:var(--p-red-500);overflow-wrap:anywhere}.component-result-state{margin-left:auto;color:var(--console-muted);white-space:nowrap}.component-result-state.ready{color:var(--p-green-500)}.component-result-state.preparing{color:var(--p-primary-color)}.component-result-state.query_failed,.component-result-state.adb_unavailable,.component-result-state.unavailable{color:var(--p-red-500)}
.workbench-view{display:flex;flex-direction:column;gap:20px}.manual-release-hint{font-size:12px;color:var(--console-muted)}.automation-center{position:relative;padding-bottom:70px}.owner-caption{display:flex;align-items:center;gap:8px;font-size:12px;color:var(--console-muted);white-space:nowrap}.owner-caption strong{color:var(--p-text-color)}.workspace-header{display:flex;align-items:center;justify-content:space-between;gap:16px;flex-wrap:wrap}.automation-tabs{display:flex;gap:4px;padding:4px;background:var(--console-panel-raised);border:1px solid var(--console-border);border-radius:10px}.automation-tabs button{padding:9px 17px;border:0;border-radius:7px;color:var(--console-muted);background:transparent;cursor:pointer;font-weight:550}.automation-tabs button.active{background:var(--console-accent-soft);color:var(--p-primary-color);box-shadow:inset 0 0 0 1px color-mix(in srgb,var(--p-primary-color) 25%,transparent)}.owner-select{display:flex;align-items:center;gap:9px;font-size:12px}.filters{padding:18px;display:flex;flex-wrap:wrap;align-items:flex-end;gap:12px}.filters label{display:flex;flex-direction:column;gap:7px;font-size:11px;color:var(--console-muted);flex:1;min-width:145px}.filters .keyword-field{flex:1.5;min-width:180px}select{padding:9px 10px;border:1px solid var(--console-border-strong);border-radius:7px;background:var(--console-panel);color:var(--p-text-color);max-width:100%;min-width:0;font-size:13px}.workbench-toolbar{display:flex;align-items:center;justify-content:space-between;gap:14px;flex-wrap:wrap}.summary{display:flex;gap:16px;font-size:12px;color:var(--console-muted)}.summary strong{color:var(--p-text-color);margin-left:5px;font-size:16px}.toolbar-actions{display:flex;align-items:center;gap:8px;flex-wrap:wrap}.screen-action-pair{display:inline-flex;align-items:center;gap:5px;flex-wrap:nowrap}.quality,.media-toggle{display:flex;align-items:center;gap:7px;color:var(--console-muted);font-size:11px;white-space:nowrap}.media-toggle input{accent-color:var(--p-primary-color)} .media-toggle{cursor:pointer}.quality select{font-size:12px;padding:7px 9px}.phone-grid{display:flex;flex-wrap:wrap;align-items:flex-start;align-content:flex-start;gap:18px;min-width:0}.activity-banner{display:flex;align-items:center;justify-content:space-between;gap:14px;padding:16px 18px;border:1px solid var(--p-primary-color);border-radius:12px;background:var(--console-accent-soft)}.activity-banner strong{font-size:14px}.activity-banner p{color:var(--console-muted);font-size:12px;margin-top:5px}.recording-banner{border-color:var(--p-red-400);background:color-mix(in srgb,var(--p-red-400) 7%,var(--console-panel))}.record-actions{display:flex;gap:8px;flex-shrink:0}.results-panel ul{padding:8px 18px}.results-panel li{display:grid;grid-template-columns:1fr auto;gap:5px 16px;padding:10px 0;border-bottom:1px solid var(--console-border);font-size:12px}.results-panel li:last-child{border:0}.results-panel p{grid-column:1 / -1;color:var(--console-muted);overflow-wrap:anywhere}.results-panel .failed{color:var(--p-orange-500)}.global-controls{position:fixed;bottom:30px;right:30px;z-index:46;display:flex;flex-direction:column;align-items:flex-end;gap:10px}.global-controls :deep(.p-button){width:46px;height:46px;border-radius:50%;box-shadow:0 4px 20px #0003;border:1px solid color-mix(in srgb,var(--p-primary-color) 45%,var(--console-border));backdrop-filter:blur(12px)}.global-controls :deep(.global-orb){width:56px;height:56px;box-shadow:0 4px 28px color-mix(in srgb,var(--p-primary-color) 35%,transparent)}.global-actions{display:grid;grid-template-columns:46px 46px;gap:8px;justify-items:end}.global-scope{grid-column:1 / -1;font-size:11px;white-space:nowrap;padding:6px 10px;background:var(--console-panel);border:1px solid var(--console-border);border-radius:20px;box-shadow:var(--console-shadow)}.target-badge{position:absolute;bottom:40px;right:-3px;min-width:22px;height:22px;padding:1px 5px;border:2px solid var(--console-panel);background:var(--p-primary-contrast-color);color:var(--p-primary-color);font-size:11px;font-weight:700;border-radius:20px;text-align:center;pointer-events:none}.global-dismiss{position:fixed;inset:0;background:transparent;z-index:45;border:0;cursor:default}.modal-content{display:flex;flex-direction:column;gap:16px}.modal-content label{display:flex;flex-direction:column;gap:8px;font-size:13px}.modal-content p,.modal-content li{font-size:12px}.modal-content ul{padding-left:18px;list-style:disc}.modal-content :deep(textarea){width:100%}
@media(max-width:700px){.page-heading{align-items:flex-start;flex-direction:column}.workspace-header{align-items:stretch;flex-direction:column}.owner-select{justify-content:space-between}.owner-select select{flex:1}.automation-tabs button{flex:1;padding:9px 12px}.filters{padding:14px}.phone-grid{gap:12px}.toolbar-actions{width:100%}.activity-banner{align-items:flex-start;flex-direction:column}.record-actions{flex-wrap:wrap}.global-controls{bottom:20px;right:18px}}
</style>
