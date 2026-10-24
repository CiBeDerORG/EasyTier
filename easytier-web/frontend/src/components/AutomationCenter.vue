<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { Button, Dialog, InputText, Message, Textarea } from 'primevue';
import { useI18n } from 'vue-i18n';
import { useRouter } from 'vue-router';
import { v4 as uuid } from 'uuid';
import { DiscoveryApi, type EndpointEvent } from '../modules/discovery-api';
import AutomationApi, { AutomationApiError, automationErrorMessage } from '../modules/automation-api';
import type { AutomationContext, AutomationControl, AutomationControlReply, AutomationDevice, AutomationRun, AutomationStep } from '../types/automation';
import PhoneCard from './automation/PhoneCard.vue';
import ConnectionDialog from './automation/ConnectionDialog.vue';
import ScriptLibrary from './automation/ScriptLibrary.vue';
import RunHistory from './automation/RunHistory.vue';
import ExecutorSettings from './automation/ExecutorSettings.vue';
import { automationMessages } from './automation/messages';

interface PhoneHandle {
    endpointChanged(event: EndpointEvent): Promise<void>;
    open(quality: 'wall' | 'focus'): Promise<void>;
    close(): Promise<void>;
    cancelRestoreIntent(): void;
    send(message: AutomationControl): Promise<AutomationControlReply>;
    getState(): { phase: string; ready: boolean; width: number; height: number };
    cancelPointer(): void;
    resetPointer(): void;
}
interface SyncSession { masterId: string; deviceIds: string[]; blocked: string[]; geometry: Record<string, { width: number; height: number }> }
interface Gesture { id: string; x: number; y: number; width: number; height: number; started: number; valid: boolean }
const { t } = useI18n({ useScope: 'local', messages: automationMessages });
const router = useRouter();
const api = new AutomationApi();
const discoveryApi = new DiscoveryApi();
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
const switchingTab = ref(false);
const quality = ref<'wall' | 'focus'>('wall');
const filters = ref({ node: '', group: '', type: '', keyword: '' });
const applied = ref({ ...filters.value });
const connectionDevice = ref<AutomationDevice | null>(null);
const globalOpen = ref(false);
const syncPreparation = ref(false); const syncMaster = ref(''); const syncTargets = ref<string[]>([]);
const sync = ref<SyncSession | null>(null);
const recordingId = ref<string | null>(null); const recordingPaused = ref(false); const recordedSteps = ref<AutomationStep[]>([]);
const recordingDraft = ref<AutomationStep[]>([]);
const textDialog = ref(false); const textTargets = ref<string[]>([]); const inputText = ref(''); const sendingText = ref(false);
const results = ref<{ id: string; name: string; state: string; message?: string }[]>([]);
const focusRunId = ref<string>(); const historyKey = ref(0);
const fullscreenActive = ref(false);
const cards = new Map<string, PhoneHandle>();
let loadGeneration = 0; let workspaceGeneration = 0; let disposed = false;
let gesture: Gesture | null = null;
let recordingGeneration = 0; let pendingRecordActions = 0;
const runnerAvailable = computed(() => context.value?.available === true && context.value.runner_configured);
const managedAccount = computed(() => accounts.value.find(account => account.id === ownerId.value)?.username ?? String(ownerId.value));
const deviceNames = computed(() => Object.fromEntries(devices.value.map(device => [device.id, device.name])));
const nodeOptions = computed(() => [...new Map(devices.value.map(device => [device.machine_id, device.node_name])).entries()]);
const groupOptions = computed(() => [...new Map(devices.value.filter(device => device.gateway_group_id).map(device => [device.gateway_group_id!, device.gateway_name ?? device.gateway_group_id!])).entries()]);
const typeOptions = computed(() => [...new Map(devices.value.filter(device => device.device_type_id).map(device => [device.device_type_id!, device.type_name ?? device.device_type_id!])).entries()]);
const visibleDevices = computed(() => {
    const query = applied.value; const keyword = query.keyword.trim().toLocaleLowerCase();
    return devices.value.filter(device => (!query.node || device.machine_id === query.node)
        && (!query.group || device.gateway_group_id === query.group)
        && (!query.type || device.device_type_id === query.type)
        && (!keyword || [device.name, device.model, device.overlay_ip, device.lan_ip].some(value => value?.toLocaleLowerCase().includes(keyword))));
});
const visibleIds = computed(() => new Set(visibleDevices.value.map(device => device.id)));
const liveCount = computed(() => Object.values(phases.value).filter(phase => phase === 'live').length);
const globalTargets = computed(() => sync.value?.deviceIds ?? selectedIds.value);
const recordingName = computed(() => deviceNames.value[recordingId.value ?? ''] ?? '');

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
async function loadDevices() {
    if (!ownerId.value || !context.value?.available) return;
    const owner = ownerId.value; const request = ++loadGeneration;
    busy.value = true; error.value = '';
    try {
        const response = await api.devices(owner);
        if (disposed || request !== loadGeneration || owner !== ownerId.value) return;
        devices.value = response.items; sourceIssues.value = response.issues ?? []; subscribeEndpoints();
        const currentIds = new Set(devices.value.map(device => device.id));
        selectedIds.value = selectedIds.value.filter(id => currentIds.has(id));
        for (const id of Object.keys(phases.value)) if (!currentIds.has(id)) delete phases.value[id];
    } catch (cause) { if (!disposed && request === loadGeneration) handleError(cause); }
    finally { if (request === loadGeneration) busy.value = false; }
}
async function query() {
    applied.value = { ...filters.value };
    await loadDevices();
    if (sync.value?.deviceIds.some(id => !visibleIds.value.has(id))) await stopSync();
    for (const [id, card] of cards) if (!visibleIds.value.has(id)) await card.close();
}
async function changeOwner(id: number) {
    if (id === ownerId.value || busy.value || connecting.value || switchingTab.value) return;
    stopEvents?.(); stopEvents = undefined;
    ++workspaceGeneration; ++loadGeneration;
    await closeAll();
    ownerId.value = id; devices.value = []; selectedIds.value = []; phases.value = {}; results.value = [];
    connectionDevice.value = null; textDialog.value = false; recordingDraft.value = []; focusRunId.value = undefined;
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
            try { await card.open(quality.value); }
            catch (cause) { results.value.push({ id, name: deviceNames.value[id] ?? id, state: 'failed', message: automationErrorMessage(cause) }); }
        }
    };
    await Promise.all([worker(), worker()]); connecting.value = false;
}
async function closeSelected() {
    const targets = [...selectedIds.value];
    if (sync.value && targets.some(id => sync.value!.deviceIds.includes(id))) await stopSync();
    if (recordingId.value && targets.includes(recordingId.value)) finishRecording(false);
    await Promise.all(targets.map(id => cards.get(id)?.close()));
    await loadDevices();
}
async function closeAll() {
    globalOpen.value = false; syncPreparation.value = false;
    await stopSync();
    if (recordingId.value) finishRecording(false);
    await Promise.all([...cards.values()].map(card => card.close()));
}
function onState(id: string, phase: string) {
    phases.value[id] = phase;
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
    if (sync.value?.deviceIds.includes(id)) { notice.value = t('syncRotation'); void stopSync(); }
    if (recordingId.value === id) { ++recordingGeneration; gesture = null; notice.value = t('recordIncomplete'); }
}
function addStep(step: AutomationStep) {
    if (!recordingId.value || recordingPaused.value) return;
    if (recordedSteps.value.length >= 100) { notice.value = t('recordLimit'); finishRecording(false); return; }
    recordedSteps.value.push(step);
}
async function routeInput(id: string, message: AutomationControl) {
    const activeSync = sync.value;
    if (activeSync && id !== activeSync.masterId) return;
    const targets = activeSync ? activeSync.deviceIds.filter(target => !activeSync.blocked.includes(target)) : [id];
    if (message.type === 'touch' && message.action === 'down' && recordingId.value === id && !recordingPaused.value) {
        gesture = { id, x: message.x!, y: message.y!, width: message.width!, height: message.height!, started: performance.now(), valid: true };
    }
    const activeGesture = gesture;
    const recordEpoch = recordingGeneration;
    const finishesGesture = message.type === 'touch' && message.action === 'up' && activeGesture?.id === id;
    if (finishesGesture) { gesture = null; ++pendingRecordActions; }
    if (message.type === 'touch.release' && activeGesture?.id === id) { activeGesture.valid = false; gesture = null; }
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
    if (message.type === 'touch' && message.action === 'up' && activeGesture?.id === id) {
        if (recordEpoch !== recordingGeneration || recordingId.value !== id) return;
        if (!activeGesture.valid || activeGesture.width !== message.width || activeGesture.height !== message.height) { notice.value = t('recordIncomplete'); return; }
        const elapsed = Math.round(gestureEndedAt - activeGesture.started);
        if (elapsed > 3000) { notice.value = t('recordIncomplete'); return; }
        const duration = Math.max(100, elapsed);
        const distance = Math.hypot((message.x! - activeGesture.x) / activeGesture.width, (message.y! - activeGesture.y) / activeGesture.height);
        const origin = { x: activeGesture.x / activeGesture.width, y: activeGesture.y / activeGesture.height, width: activeGesture.width, height: activeGesture.height };
        addStep(duration < 500 && distance < 0.025
            ? { id: uuid(), type: 'tap', ...origin }
            : { id: uuid(), type: 'swipe', ...origin, to_x: message.x! / activeGesture.width, to_y: message.y! / activeGesture.height, duration_ms: duration });
    }
}
async function sendAction(ids: string[], message: AutomationControl) {
    const targets = [...ids]; error.value = '';
    const mine = workspaceGeneration;
    if (!requireTargets(targets)) return;
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
function localAction(id: string, message: AutomationControl) {
    if (sync.value?.deviceIds.includes(id)) { error.value = t('controlsLocked'); return; }
    if (message.type === 'stop') { void releaseManual([id]); return; }
    void sendAction([id], message);
}
function globalAction(message: AutomationControl) {
    const targets = [...globalTargets.value];
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
    ++recordingGeneration; recordingId.value = id; recordingPaused.value = false; recordedSteps.value = []; gesture = null;
}
function pauseRecording() {
    if (gesture || pendingRecordActions) notice.value = t('recordIncomplete');
    if (recordingId.value) cards.get(recordingId.value)?.cancelPointer();
    ++recordingGeneration; gesture = null; recordingPaused.value = !recordingPaused.value;
}
function finishRecording(edit: boolean) {
    if (gesture || pendingRecordActions) notice.value = t('recordIncomplete');
    ++recordingGeneration;
    const id = recordingId.value; recordingId.value = null; gesture = null;
    if (id) cards.get(id)?.cancelPointer();
    recordingDraft.value = recordedSteps.value.map(step => ({ ...step })); recordedSteps.value = [];
    if (edit) void changeTab('scripts');
}
async function changeTab(next: typeof tab.value) {
    if (next === tab.value || switchingTab.value || connecting.value || sendingText.value) return;
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
        tab.value = next;
        if (next === 'settings') settingsVisited.value = true;
        if (next === 'workbench') await loadDevices();
    } finally { switchingTab.value = false; }
}
async function settingsSaved() {
    try { context.value = await api.context(); }
    catch (cause) { handleError(cause); }
}
function onRunCreated(run: AutomationRun) {
    for (const card of cards.values()) card.cancelRestoreIntent();
    focusRunId.value = run.id; ++historyKey.value; tab.value = 'history';
}
function savedConnection(device: AutomationDevice) {
    devices.value = devices.value.map(item => item.id === device.id ? device : item);
    if (connectionDevice.value?.id === device.id) connectionDevice.value = device;
}
function visibilityChanged() {
    if (document.hidden) { ++workspaceGeneration; notice.value = t('controlDisabledWhileHidden'); void closeAll(); }
}
function fullScreenChanged() { fullscreenActive.value = !!document.fullscreenElement; }
function releaseOnBlur() {
    for (const card of cards.values()) card.cancelPointer();
}
onMounted(async () => {
    document.addEventListener('visibilitychange', visibilityChanged);
    document.addEventListener('fullscreenchange', fullScreenChanged);
    window.addEventListener('blur', releaseOnBlur);
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
    disposed = true; ++workspaceGeneration; ++loadGeneration;
    document.removeEventListener('visibilitychange', visibilityChanged);
    document.removeEventListener('fullscreenchange', fullScreenChanged);
    window.removeEventListener('blur', releaseOnBlur);
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
                    <div class="toolbar-actions"><Button :label="t('selectShown')" text size="small" @click="selectedIds = [...new Set([...selectedIds, ...visibleDevices.map(device => device.id)])]" /><Button :label="t('clearSelection')" text size="small" @click="selectedIds = []" /><label class="quality" :title="t('qualityHint')"><span>{{ t('quality') }}</span><select v-model="quality"><option value="wall">{{ t('wall') }}</option><option value="focus">{{ t('focus') }}</option></select></label><Button :label="t('openSelected')" icon="pi pi-play" size="small" :loading="connecting" :disabled="!runnerAvailable || !selectedIds.length" @click="openSelected" /><Button :label="t('releaseManual')" icon="pi pi-unlock" text size="small" :disabled="!selectedIds.length" @click="releaseManual([...selectedIds])" /><Button :label="t('closeSelected')" icon="pi pi-stop" outlined size="small" :disabled="!selectedIds.length" @click="closeSelected" /></div>
                </div>
                <div v-if="sync" class="activity-banner sync-banner" role="status"><div><strong>{{ t('syncRunning', { count: sync.deviceIds.length }) }}</strong><p>{{ t('master') }}：{{ deviceNames[sync.masterId] }} · {{ t('syncFixed') }}</p></div><Button :label="t('stopSync')" severity="danger" icon="pi pi-stop-circle" @click="stopSync" /></div>
                <div v-if="recordingId" class="activity-banner recording-banner" role="status"><div><strong>{{ recordingPaused ? t('recordPaused') : t('recordRunning', { name: recordingName }) }}</strong><p>{{ t('recordedCount', { count: recordedSteps.length }) }} · {{ t('recordHint') }}</p></div><div class="record-actions"><Button :label="t(recordingPaused ? 'resume' : 'pause')" :icon="recordingPaused ? 'pi pi-play' : 'pi pi-pause'" outlined @click="pauseRecording" /><Button :label="t('editRecording')" severity="danger" icon="pi pi-stop" @click="finishRecording(true)" /></div></div>
                <div v-if="!devices.length && !busy" class="console-panel console-empty-state"><i class="pi pi-mobile" aria-hidden="true" /><h2>{{ t('noAssets') }}</h2><p>{{ t('noAssetsDetail') }}</p><Button :label="t('toAssets')" outlined @click="router.push({ name: 'nodeAssets' })" /></div>
                <div v-else-if="!visibleDevices.length && !busy" class="console-panel console-empty-state"><i class="pi pi-search" aria-hidden="true" /><h2>{{ t('noResults') }}</h2><Button :label="t('reset')" text @click="filters = { node: '', group: '', type: '', keyword: '' }; query()" /></div>
                <div v-if="busy && !devices.length" class="console-panel console-empty-state" role="status"><i class="pi pi-spin pi-spinner" aria-hidden="true" />{{ t('loading') }}</div>
                <div class="phone-grid"><PhoneCard v-for="device in devices" v-show="visibleIds.has(device.id)" :key="`${ownerId}:${device.id}`" :ref="value => setCard(device.id, value)" :device="device" :owner-id="ownerId" :selected="selectedIds.includes(device.id)" :runner-available="runnerAvailable" :sync-active="!!sync" :sync-role="sync?.deviceIds.includes(device.id) ? (sync.masterId === device.id ? 'master' : 'follower') : undefined" :recording="recordingId === device.id" @selection="select" @configure="connectionDevice = $event" @input="routeInput" @action="localAction" @text="id => showText([id])" @record="toggleRecord" @state="onState" @geometry="onGeometry" @stop-sync="stopSync" @refresh="loadDevices" /></div>
                <section v-if="results.length" class="console-panel results-panel"><div class="section-heading"><h2>{{ t('operationResults') }}</h2></div><ul><li v-for="result in results" :key="result.id"><strong>{{ result.name }}</strong><span :class="{ failed: result.state !== 'action_sent' }">{{ result.state === 'action_sent' ? t('actionSent') : result.state === 'sending' ? t('loading') : t('resultUnknown') }}</span><p v-if="result.message">{{ result.message }}</p></li></ul></section>
                <button v-if="globalOpen && !fullscreenActive" class="global-dismiss" :aria-label="t('collapse')" @pointerdown.stop @click.stop="globalOpen = false" />
                <div v-if="devices.length && !fullscreenActive" class="global-controls" @pointerdown.stop @click.stop @keydown.esc="globalOpen = false">
                    <div v-if="globalOpen" class="global-actions" role="group" :aria-label="t('globalControl')"><span class="global-scope">{{ t('globalControl') }} · {{ t('targetCount', { count: globalTargets.length }) }}</span><Button icon="pi pi-arrow-left" rounded :title="t('back')" :aria-label="t('back')" :disabled="!globalTargets.length" @click="globalAction({ type: 'key', key: 'BACK' })" /><Button icon="pi pi-home" rounded :title="t('home')" :aria-label="t('home')" :disabled="!globalTargets.length" @click="globalAction({ type: 'key', key: 'HOME' })" /><Button icon="pi pi-clone" rounded :title="t('recent')" :aria-label="t('recent')" :disabled="!globalTargets.length" @click="globalAction({ type: 'key', key: 'RECENT' })" /><Button icon="pi pi-pencil" rounded :title="t('text')" :aria-label="t('text')" :disabled="!globalTargets.length" @click="showText([...globalTargets])" /><Button icon="pi pi-power-off" rounded :title="t('wake')" :aria-label="t('wake')" :disabled="!globalTargets.length" @click="globalAction({ type: 'key', key: 'WAKE' })" /><Button icon="pi pi-unlock" rounded :title="t('releaseManual')" :aria-label="t('releaseManual')" :disabled="!globalTargets.length" @click="releaseManual([...globalTargets])" /><Button :icon="sync ? 'pi pi-stop-circle' : 'pi pi-share-alt'" rounded :severity="sync ? 'danger' : 'primary'" :title="t(sync ? 'stopSync' : 'sync')" :aria-label="t(sync ? 'stopSync' : 'sync')" @click="sync ? stopSync() : prepareSync()" /></div>
                    <Button class="global-orb" :icon="globalOpen ? 'pi pi-times' : 'pi pi-sliders-h'" rounded :aria-label="t('globalControl')" :aria-expanded="globalOpen" @click="globalOpen = !globalOpen" /><span class="target-badge" aria-hidden="true">{{ globalTargets.length }}</span>
                </div>
            </div>
            <p v-if="liveCount" class="manual-release-hint">{{ t('tabReleaseHint') }}</p>
            <ScriptLibrary v-if="tab === 'scripts' && ownerId" :key="ownerId" :owner-id="ownerId" :selected-device-ids="selectedIds" :device-names="deviceNames" :recorded-steps="recordingDraft" @run-created="onRunCreated" @recording-consumed="recordingDraft = []" />
            <RunHistory v-else-if="tab === 'history' && ownerId" :key="`${ownerId}:${historyKey}`" :owner-id="ownerId" :focus-run-id="focusRunId" />
            <ExecutorSettings v-if="settingsVisited && context.can_manage_settings" v-show="tab === 'settings'" :live-count="liveCount" @saved="settingsSaved" @close-screens="closeAll" />
        </template>
        <ConnectionDialog :device="connectionDevice" :owner-id="ownerId" :runner-available="runnerAvailable" @close="connectionDevice = null" @saved="savedConnection" />
        <Dialog v-model:visible="syncPreparation" modal :header="t('syncTitle')" :style="{ width: '500px', maxWidth: 'calc(100vw - 24px)' }"><div class="modal-content"><Message severity="info" :closable="false">{{ t('syncHint') }}</Message><p>{{ t('targetCount', { count: syncTargets.length }) }}</p><ul><li v-for="id in syncTargets" :key="id">{{ deviceNames[id] ?? id }}</li></ul><label>{{ t('master') }}<select v-model="syncMaster"><option v-for="id in syncTargets" :key="id" :value="id">{{ deviceNames[id] ?? id }}</option></select></label></div><template #footer><Button :label="t('cancel')" text @click="syncPreparation = false" /><Button :label="t('sync')" icon="pi pi-share-alt" @click="beginSync" /></template></Dialog>
        <Dialog v-model:visible="textDialog" modal :header="t('textTitle')" :closable="!sendingText" :close-on-escape="!sendingText" :style="{ width: '520px', maxWidth: 'calc(100vw - 24px)' }"><div class="modal-content"><p>{{ t('targetCount', { count: textTargets.length }) }} · {{ textTargets.map(id => deviceNames[id] ?? id).join('、') }}</p><Textarea v-model="inputText" :placeholder="t('textPlaceholder')" rows="5" maxlength="2000" :disabled="sendingText" autofocus /><p class="muted">{{ t('textHint') }}</p></div><template #footer><Button :label="t('cancel')" text :disabled="sendingText" @click="textDialog = false; inputText = ''" /><Button :label="t('send')" icon="pi pi-send" :loading="sendingText" :disabled="!inputText" @click="sendText" /></template></Dialog>
    </section>
</template>

<style scoped>
.automation-tabs button{white-space:nowrap}
.workbench-view{display:flex;flex-direction:column;gap:20px}.manual-release-hint{font-size:12px;color:var(--console-muted)}.automation-center{position:relative;padding-bottom:70px}.owner-caption{display:flex;align-items:center;gap:8px;font-size:12px;color:var(--console-muted);white-space:nowrap}.owner-caption strong{color:var(--p-text-color)}.workspace-header{display:flex;align-items:center;justify-content:space-between;gap:16px;flex-wrap:wrap}.automation-tabs{display:flex;gap:4px;padding:4px;background:var(--console-panel-raised);border:1px solid var(--console-border);border-radius:10px}.automation-tabs button{padding:9px 17px;border:0;border-radius:7px;color:var(--console-muted);background:transparent;cursor:pointer;font-weight:550}.automation-tabs button.active{background:var(--console-accent-soft);color:var(--p-primary-color);box-shadow:inset 0 0 0 1px color-mix(in srgb,var(--p-primary-color) 25%,transparent)}.owner-select{display:flex;align-items:center;gap:9px;font-size:12px}.filters{padding:18px;display:flex;flex-wrap:wrap;align-items:flex-end;gap:12px}.filters label{display:flex;flex-direction:column;gap:7px;font-size:11px;color:var(--console-muted);flex:1;min-width:145px}.filters .keyword-field{flex:1.5;min-width:180px}select{padding:9px 10px;border:1px solid var(--console-border-strong);border-radius:7px;background:var(--console-panel);color:var(--p-text-color);max-width:100%;min-width:0;font-size:13px}.workbench-toolbar{display:flex;align-items:center;justify-content:space-between;gap:14px;flex-wrap:wrap}.summary{display:flex;gap:16px;font-size:12px;color:var(--console-muted)}.summary strong{color:var(--p-text-color);margin-left:5px;font-size:16px}.toolbar-actions{display:flex;align-items:center;gap:8px;flex-wrap:wrap}.quality{display:flex;align-items:center;gap:7px;color:var(--console-muted);font-size:11px}.quality select{font-size:12px;padding:7px 9px}.phone-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(230px,1fr));gap:18px}.activity-banner{display:flex;align-items:center;justify-content:space-between;gap:14px;padding:16px 18px;border:1px solid var(--p-primary-color);border-radius:12px;background:var(--console-accent-soft)}.activity-banner strong{font-size:14px}.activity-banner p{color:var(--console-muted);font-size:12px;margin-top:5px}.recording-banner{border-color:var(--p-red-400);background:color-mix(in srgb,var(--p-red-400) 7%,var(--console-panel))}.record-actions{display:flex;gap:8px;flex-shrink:0}.results-panel ul{padding:8px 18px}.results-panel li{display:grid;grid-template-columns:1fr auto;gap:5px 16px;padding:10px 0;border-bottom:1px solid var(--console-border);font-size:12px}.results-panel li:last-child{border:0}.results-panel p{grid-column:1 / -1;color:var(--console-muted);overflow-wrap:anywhere}.results-panel .failed{color:var(--p-orange-500)}.global-controls{position:fixed;bottom:30px;right:30px;z-index:46;display:flex;flex-direction:column;align-items:flex-end;gap:10px}.global-controls :deep(.p-button){width:46px;height:46px;border-radius:50%;box-shadow:0 4px 20px #0003;border:1px solid color-mix(in srgb,var(--p-primary-color) 45%,var(--console-border));backdrop-filter:blur(12px)}.global-controls :deep(.global-orb){width:56px;height:56px;box-shadow:0 4px 28px color-mix(in srgb,var(--p-primary-color) 35%,transparent)}.global-actions{display:grid;grid-template-columns:46px 46px;gap:8px;justify-items:end}.global-scope{grid-column:1 / -1;font-size:11px;white-space:nowrap;padding:6px 10px;background:var(--console-panel);border:1px solid var(--console-border);border-radius:20px;box-shadow:var(--console-shadow)}.target-badge{position:absolute;bottom:40px;right:-3px;min-width:22px;height:22px;padding:1px 5px;border:2px solid var(--console-panel);background:var(--p-primary-contrast-color);color:var(--p-primary-color);font-size:11px;font-weight:700;border-radius:20px;text-align:center;pointer-events:none}.global-dismiss{position:fixed;inset:0;background:transparent;z-index:45;border:0;cursor:default}.modal-content{display:flex;flex-direction:column;gap:16px}.modal-content label{display:flex;flex-direction:column;gap:8px;font-size:13px}.modal-content p,.modal-content li{font-size:12px}.modal-content ul{padding-left:18px;list-style:disc}.modal-content :deep(textarea){width:100%}
@media(max-width:700px){.page-heading{align-items:flex-start;flex-direction:column}.workspace-header{align-items:stretch;flex-direction:column}.owner-select{justify-content:space-between}.owner-select select{flex:1}.automation-tabs button{flex:1;padding:9px 12px}.filters{padding:14px}.phone-grid{grid-template-columns:repeat(auto-fill,minmax(220px,1fr))}.toolbar-actions{width:100%}.activity-banner{align-items:flex-start;flex-direction:column}.record-actions{flex-wrap:wrap}.global-controls{bottom:20px;right:18px}}
</style>
