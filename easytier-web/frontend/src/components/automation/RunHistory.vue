<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Button, Checkbox, Dialog, InputNumber, InputText, Tag } from 'primevue';
import { useI18n } from 'vue-i18n';
import { v4 as uuid } from 'uuid';
import AutomationApi, { AutomationApiError, automationErrorMessage } from '../../modules/automation-api';
import { analyzeAutomationFlow } from '../../modules/automation-flow';
import { createAutomationRepairRequest, type AutomationRepairRequest } from '../../modules/automation-run-repair';
import { getApiBase } from '../../modules/api-host';
import type { AutomationRun, AutomationRunInput, AutomationRunDevice, AutomationStep, BusinessVerificationStatus } from '../../types/automation';

const props = defineProps<{ ownerId: number; focusRunId?: string }>();
const emit = defineEmits<{ 'repair-step': [request: AutomationRepairRequest]; 'repair-card': [request: AutomationRepairRequest] }>();
const api = new AutomationApi();
const { locale } = useI18n();
const cn = computed(() => locale.value === 'cn' || locale.value.startsWith('zh'));
const text = (zh: string, en: string) => cn.value ? zh : en;
const runs = ref<AutomationRun[]>([]);
const selectedId = ref<string>();
const loading = ref(false);
const working = ref<'refresh' | 'cancel' | 'rerun' | null>(null);
const error = ref('');
const notice = ref('');
const expandedDevices = ref<Set<string>>(new Set());
const cancelVisible = ref(false);
const cancelId = ref<string>();
const rerunVisible = ref(false);
const rerunSource = ref<AutomationRun>();
const rerunDeviceIds = ref<string[]>([]);
const rerunParameters = ref<Record<string, string>>(Object.create(null));
const rerunConcurrency = ref<number | null>(3);
const rerunPending = ref<AutomationRunInput>();
const rerunUncertain = ref(false);
const rerunError = ref('');
const previewVisible = ref(false);
const previewLoading = ref(false);
const previewError = ref('');
const previewUrl = ref('');
const previewTitle = ref('');
let previewRequest = 0;
let previewController: AbortController | undefined;
let scopeRevision = 0;
let requestSequence = 0;
let disposed = false;
let progressTimer: ReturnType<typeof setTimeout> | undefined;
let progressPolling = false;
let progressFailures = 0;
let refreshCursor = 0;
const overviewMode = ref<'runs' | 'devices'>('runs');
const progressStates = new Set(['dispatch_pending', 'queued', 'accepted', 'starting', 'running', 'cancel_requested', 'cleanup_pending']);
const deviceBusyStates = new Set(['starting', 'running', 'accepted']);
const deviceQueuedStates = new Set(['dispatch_pending', 'queued']);
const settledSteps = new Set(['completed', 'succeeded', 'action_sent', 'action_completed', 'assertion_passed', 'condition_passed', 'passed', 'failed', 'cancelled', 'interrupted', 'outcome_unknown', 'skipped']);
const activeRuns = computed(() => runs.value.filter(run => progressStates.has(run.state)));
const activeDevices = computed(() => activeRuns.value.flatMap(run => run.devices.map(device => ({ run, device }))));
const summary = computed(() => ({
    runs: activeRuns.value.length,
    running: activeDevices.value.filter(({device}) => deviceBusyStates.has(device.state)).length,
    queued: activeDevices.value.filter(({device}) => deviceQueuedStates.has(device.state)).length,
    failed: activeDevices.value.filter(({device}) => ['failed','interrupted','outcome_unknown','cleanup_pending'].includes(device.state)).length,
}));
const selected = computed(() => runs.value.find(run => run.id === selectedId.value));
const selectedFlow = computed(() => analyzeAutomationFlow(selected.value?.script_snapshot?.steps ?? []));
function nestedBranchPath(stepId: string): string {
    const definitions = selected.value?.script_snapshot?.steps ?? [];
    const index = definitions.findIndex(step => step.id === stepId);
    if (index < 0) return '';
    const entry = selectedFlow.value.entries[index];
    if (!entry) return '';
    return entry.ancestors.map(part =>
        text('条件', 'If') + ' ' + (part.conditionIndex + 1) + ' / ' + (part.lane === 'then' ? 'Then' : 'Else')).join(' › ');
}
function settledCount(device: AutomationRunDevice): number {
    return device.steps?.filter(step => settledSteps.has(step.state)).length ?? 0;
}
function deviceProgress(run: AutomationRun, device: AutomationRunDevice): string {
    const total = run.script_snapshot?.steps?.length ?? 0;
    return text(`${settledCount(device)}/${total} 步`, `${settledCount(device)}/${total} steps`);
}
function devicePercent(run: AutomationRun, device: AutomationRunDevice): number {
    const total = run.script_snapshot?.steps?.length ?? 0;
    return total ? Math.min(100, Math.round(settledCount(device) / total * 100)) : 0;
}
function focusDevice(run: AutomationRun, id: string) {
    selectRun(run);
    expandedDevices.value = new Set([id]);
}

function stopProgressRefresh() {
    if (progressTimer !== undefined) clearTimeout(progressTimer);
    progressTimer = undefined;
}
function scheduleProgressRefresh() {
    stopProgressRefresh();
    if (disposed || props.ownerId <= 0) return;
    // Discover runs started in another tab even if this tab was idle.
    progressTimer = setTimeout(() => { void pollRunProgress(); }, activeRuns.value.length ? 3500 : 10000);
}
async function pollRunProgress() {
    progressTimer = undefined;
    if (disposed || props.ownerId <= 0) return;
    // Independent tasks must not wait for the currently selected row.
    if (progressPolling || busy.value || loading.value || document.hidden) {
        scheduleProgressRefresh();
        return;
    }
    progressPolling = true;
    const owner = props.ownerId, scope = scopeRevision;
    const current = () => !disposed && owner === props.ownerId && scope === scopeRevision;
    let problems = 0;
    try {
        // Catch tasks started in another tab and receive completed device states.
        const response = await api.listRuns(owner);
        if (!current()) return;
        for (const run of response.items) mergeRun(run);
        const active = activeRuns.value;
        if (active.length) {
            // Bounded fairness: at most six task refreshes per tick. Rotate
            // across all active runs so one long run never starves another.
            const selectedBatch: AutomationRun[] = [];
            for (let i = 0; i < Math.min(6, active.length); i++) {
                selectedBatch.push(active[(refreshCursor + i) % active.length]);
            }
            refreshCursor = (refreshCursor + selectedBatch.length) % active.length;
            const queue = [...selectedBatch];
            await Promise.all(Array.from({length: Math.min(3, queue.length)}, async () => {
                while (queue.length && current()) {
                    const run = queue.shift()!;
                    try {
                        const updated = await api.refreshRun(owner, run.id);
                        if (current()) mergeRun(updated);
                    } catch {
                        // Do not block other runs; keep their latest durable result.
                        problems++;
                    }
                }
            }));
        }
        if (!current()) return;
        progressFailures = problems ? progressFailures + 1 : 0;
        if (progressFailures >= 2) {
            notice.value = text('部分任务未能查询执行器，其他任务仍持续更新；失败任务保留上次记录。', 'Some executor queries failed. Other runs continue updating; failed queries keep their previous result.');
        } else if (notice.value.includes(text('部分任务未能查询执行器', 'Some executor queries failed'))) {
            notice.value = '';
        }
    } catch {
        if (!current()) return;
        if (++progressFailures >= 2) notice.value = text(
            '自动刷新任务列表失败，保留上次执行记录，请检查网络或手动刷新。',
            'Automatic task list refresh failed. Previous records were kept; check the network or refresh manually.');
    } finally {
        progressPolling = false;
        if (current()) scheduleProgressRefresh();
    }
}
watch(() => activeRuns.value.map(run => run.id + ':' + run.state).join('|'), scheduleProgressRefresh, { immediate: true });
const retryableStates = new Set(['failed', 'cancelled', 'interrupted']);
const retryableDevices = computed(() => rerunSource.value?.devices.filter(device => retryableStates.has(device.state) && !hasUnconfirmedOutcome(device)) ?? []);
const parameterNames = computed(() => [...new Set((rerunSource.value?.script_snapshot?.steps ?? [])
    .filter(step => step.type === 'input_text' && step.parameter !== undefined)
    .map(step => step.parameter!.trim()))]);
const busy = computed(() => working.value !== null);
const canCancel = computed(() => !!selected.value && selected.value.devices.some(device =>
    !device.cleanup_confirmed && ['dispatch_pending', 'accepted', 'queued', 'starting', 'running', 'cancel_requested', 'outcome_unknown', 'unknown', 'cleanup_pending'].includes(device.state)));
const canRerun = computed(() => !!selected.value?.script_snapshot?.steps?.length && selected.value.devices.some(device => retryableStates.has(device.state) && !hasUnconfirmedOutcome(device)));

function hasUnconfirmedOutcome(device: AutomationRunDevice): boolean {
    return ['outcome_unknown', 'unknown'].includes(device.state)
        || device.steps.some(step => ['outcome_unknown', 'unknown'].includes(step.state));
}
function cleanupNotice(device: AutomationRunDevice): string {
    if (hasUnconfirmedOutcome(device)) {
        return device.cleanup_confirmed
            ? text('已结束占用，先核对手机实际结果再创建新任务。确认后可到脚本库新建运行；此记录仍保留为动作结果未知。', 'Control has been released. Check the actual phone outcome before creating a new task in the script library. This record keeps its unconfirmed action outcome.')
            : text('动作结果未知，控制进程仍待清理。请取消运行并手动查询结果，确认设备占用已结束。', 'The action outcome is unknown and control cleanup is unconfirmed. Cancel the run and query the executor manually to confirm that control has been released.');
    }
    if (device.cleanup_confirmed) return text('控制进程已清理，已结束设备占用；已有动作结果保持不变。', 'The control process has been cleaned up and device control released. Recorded action outcomes are unchanged.');
    if (['cancel_requested', 'cleanup_pending'].includes(device.state)) return text('控制进程仍待清理，请手动查询或再次请求取消。', 'Control cleanup is pending. Query the executor manually or request cancellation again.');
    return '';
}

function businessLabel(state: BusinessVerificationStatus | undefined): string {
    const labels: Record<BusinessVerificationStatus, [string, string]> = {
        verified: ['业务已验证', 'Business verified'],
        failed: ['业务验证失败', 'Business check failed'],
        not_verified: ['业务未验证', 'Business not verified'],
        pending: ['业务验证待执行', 'Business check pending'],
        not_reached: ['业务条件未到达', 'Business check not reached'],
        outcome_unknown: ['业务结果未知', 'Business outcome unknown'],
        partial: ['部分设备业务未验证', 'Partial verification'],
    };
    const pair=labels[state ?? 'not_verified'];
    return text(pair[0],pair[1]);
}
function businessSeverity(state: BusinessVerificationStatus | undefined): 'success' | 'secondary' | 'warn' | 'danger' | 'info' {
    switch (state) {
        case 'verified': return 'success';
        case 'failed': return 'danger';
        case 'outcome_unknown':
        case 'partial': return 'warn';
        case 'pending': return 'info';
        default: return 'secondary';
    }
}
function statusLabel(state: string): string {
    const labels: Record<string, [string, string]> = {
        dispatch_pending: ['等待派发', 'Awaiting dispatch'], queued: ['排队中', 'Queued'], accepted: ['已接收', 'Accepted'],
        starting: ['准备中', 'Starting'], running: ['执行中', 'Running'], succeeded: ['计划已完成', 'Plan completed'],
        completed: ['计划已完成', 'Plan completed'], action_sent: ['动作已发送', 'Action sent'], dispatched: ['动作已派发', 'Action dispatched'],
        action_completed: ['动作已完成', 'Action completed'], assertion_passed: ['条件检查通过', 'Condition passed'],
        condition_passed: ['条件已判定', 'Condition evaluated'], passed: ['条件检查通过', 'Condition passed'],
        failed: ['失败', 'Failed'], partial_failed: ['部分失败', 'Partially failed'], partial: ['部分完成', 'Partially completed'],
        cancel_requested: ['停止待确认', 'Stop pending'], cancelled: ['已取消', 'Cancelled'], interrupted: ['已中断', 'Interrupted'],
        outcome_unknown: ['结果未确认', 'Outcome unconfirmed'], unknown: ['结果未确认', 'Outcome unconfirmed'],
        incomplete: ['存在未确认结果', 'Unconfirmed results'], revoked: ['权限已撤销', 'Authorization revoked'],
        cleanup_pending: ['清理待确认', 'Cleanup pending'], skipped: ['未执行', 'Not executed'], pending: ['未开始', 'Not started'],
    };
    const label = labels[state];
    return label ? text(label[0], label[1]) : state;
}
function severity(state: string): 'success' | 'info' | 'warn' | 'danger' | 'secondary' {
    if (['assertion_passed', 'condition_passed', 'passed', 'completed', 'succeeded'].includes(state)) return 'success';
    if (['failed', 'partial_failed'].includes(state)) return 'danger';
    if (['outcome_unknown', 'unknown', 'incomplete', 'cancel_requested', 'cleanup_pending', 'interrupted', 'partial'].includes(state)) return 'warn';
    if (['running', 'starting', 'accepted', 'action_sent', 'action_completed', 'dispatched'].includes(state)) return 'info';
    return 'secondary';
}
function message(value: AutomationRunDevice['error']): string {
    if (!value) return '';
    return typeof value === 'string' ? value : value.message || value.code || '';
}
function date(value: string | undefined): string {
    if (!value) return '—';
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString(cn.value ? 'zh-CN' : 'en-US');
}
function mergeRun(run: AutomationRun) {
    const existing = runs.value.find(record => record.id === run.id);
    // A delayed list read or another device's callback must never rewind a
    // newer executor revision on the page.
    if (existing && (existing.executor_revision ?? -1) > (run.executor_revision ?? -1)) return;
    if (!existing) runs.value = [run, ...runs.value];
    else runs.value = runs.value.map(record => record.id === run.id ? run : record);
}
function selectRun(run: AutomationRun) {
    if (busy.value) return;
    selectedId.value = run.id;
    expandedDevices.value = new Set(run.devices.filter(device => device.steps?.length).map(device => device.device_id));
    error.value = ''; notice.value = '';
}
function toggleDevice(id: string) {
    const value = new Set(expandedDevices.value);
    if (value.has(id)) value.delete(id); else value.add(id);
    expandedDevices.value = value;
}
async function loadRuns(focusId?: string) {
    if (busy.value) return;
    const scope = scopeRevision;
    const request = ++requestSequence;
    const owner = props.ownerId;
    loading.value = true; error.value = '';
    try {
        const result = await api.listRuns(owner);
        if (disposed || scope !== scopeRevision || request !== requestSequence) return;
        const previous = new Map(runs.value.map(run => [run.id, run]));
        runs.value = result.items.map(run => {
            const cached = previous.get(run.id);
            return cached && (cached.executor_revision ?? -1) > (run.executor_revision ?? -1) ? cached : run;
        });
        const wanted = focusId ?? selectedId.value;
        if (wanted && !runs.value.some(run => run.id === wanted)) {
            const found = await api.getRun(owner, wanted);
            if (disposed || scope !== scopeRevision || request !== requestSequence) return;
            mergeRun(found);
        }
        const next = runs.value.find(run => run.id === wanted) ?? runs.value[0];
        if (next) selectRun(next); else selectedId.value = undefined;
    } catch (cause) {
        if (!disposed && scope === scopeRevision && request === requestSequence) error.value = automationErrorMessage(cause);
    } finally {
        if (!disposed && scope === scopeRevision && request === requestSequence) loading.value = false;
    }
}
async function refreshRun() {
    if (!selected.value || busy.value) return;
    const id = selected.value.id;
    const owner = props.ownerId;
    const scope = scopeRevision;
    ++requestSequence; loading.value = false;
    working.value = 'refresh'; error.value = ''; notice.value = '';
    try {
        const run = await api.refreshRun(owner, id);
        if (disposed || scope !== scopeRevision) return;
        mergeRun(run);
        notice.value = text('已查询执行器，本次收到的结果已保存。', 'Executor queried. The returned results have been saved.');
    } catch (cause) {
        if (!disposed && scope === scopeRevision) error.value = automationErrorMessage(cause) + text(' 保留上次记录，当前结果可能尚未更新。', ' The last saved results remain and may be out of date.');
    } finally {
        if (!disposed && scope === scopeRevision) working.value = null;
    }
}
function askCancel() {
    if (!selected.value || busy.value) return;
    cancelId.value = selected.value.id; cancelVisible.value = true;
}
async function cancelRun() {
    if (!cancelId.value || busy.value) return;
    const id = cancelId.value;
    const scope = scopeRevision;
    const owner = props.ownerId;
    ++requestSequence; loading.value = false;
    working.value = 'cancel'; error.value = ''; notice.value = '';
    try {
        const run = await api.cancelRun(owner, id);
        if (disposed || scope !== scopeRevision) return;
        mergeRun(run); cancelVisible.value = false;
        notice.value = text('取消请求已处理，请以每台设备的停止状态为准。', 'Cancellation processed. Check each device’s actual stop state.');
    } catch (cause) {
        if (!disposed && scope === scopeRevision) error.value = automationErrorMessage(cause) + text(' 停止尚未确认，请手动查询结果。', ' Stopping is unconfirmed. Query the result manually.');
    } finally {
        if (!disposed && scope === scopeRevision) working.value = null;
    }
}
function openRerun() {
    if (busy.value || !selected.value || !canRerun.value) return;
    if (rerunPending.value && rerunUncertain.value) { rerunVisible.value = true; return; }
    rerunSource.value = JSON.parse(JSON.stringify(selected.value)) as AutomationRun;
    rerunDeviceIds.value = retryableDevices.value.map(device => device.device_id);
    rerunParameters.value = Object.create(null);
    rerunConcurrency.value = Math.min(3, rerunDeviceIds.value.length);
    rerunPending.value = undefined; rerunUncertain.value = false; rerunError.value = '';
    rerunVisible.value = true;
}
async function submitRerun() {
    if (busy.value || !rerunSource.value) return;
    if (!rerunPending.value) {
        const allowed = new Set(retryableDevices.value.map(device => device.device_id));
        const ids = [...new Set(rerunDeviceIds.value)].filter(id => allowed.has(id));
        if (!ids.length) { rerunError.value = text('至少选择一台设备。', 'Select at least one device.'); return; }
        if (parameterNames.value.some(key => typeof rerunParameters.value[key] !== 'string' || new TextEncoder().encode(rerunParameters.value[key]).length > 2000)) {
            rerunError.value = text('请重新填写所有运行参数，每项最多 2000 字节。', 'Re-enter all run parameters, at most 2000 bytes each.'); return;
        }
        rerunPending.value = {
            request_id: uuid(), name: rerunSource.value.name,
            steps: rerunSource.value.script_snapshot.steps.map(step => ({ ...step, selector: step.selector ? { ...step.selector } : undefined })),
            device_ids: ids, parameters: { ...rerunParameters.value },
            concurrency: Math.max(1, Math.min(ids.length, Math.floor(rerunConcurrency.value ?? 1))),
        };
    }
    const scope = scopeRevision;
    const owner = props.ownerId;
    const payload = rerunPending.value;
    if (!payload) return;
    ++requestSequence; loading.value = false;
    working.value = 'rerun'; rerunError.value = '';
    try {
        const run = await api.createRun(owner, payload);
        if (disposed || scope !== scopeRevision) return;
        mergeRun(run); selectedId.value = run.id; expandedDevices.value = new Set();
        rerunVisible.value = false; rerunPending.value = undefined; rerunUncertain.value = false; rerunParameters.value = Object.create(null);
        notice.value = text('新运行已登记，旧结果保持不变。', 'New run registered. Previous results are unchanged.');
    } catch (cause) {
        if (disposed || scope !== scopeRevision) return;
        rerunError.value = automationErrorMessage(cause);
        rerunUncertain.value = !(cause instanceof AutomationApiError) || cause.status >= 500;
        if (!rerunUncertain.value) rerunPending.value = undefined;
        else rerunError.value += text(' 接收结果未确认；重试保持相同请求编号。', ' Acceptance is unconfirmed; retrying retains the same request ID.');
    } finally {
        if (!disposed && scope === scopeRevision) working.value = null;
    }
}
function stepTitle(step?: AutomationStep): string {
    if (!step) return text('历史步骤', 'Historical step');
    const labels: Record<string, [string, string]> = {
        key: ['导航按键', 'Navigation key'], tap: ['点击', 'Tap'], long_press: ['长按', 'Long press'],
        swipe: ['滑动', 'Swipe'], drag: ['拖拽', 'Drag'], input_text: ['输入文字', 'Enter text'],
        launch_app: ['打开应用', 'Launch app'], wait_for: ['等待控件', 'Wait for element'],
        wait_gone: ['等待消失', 'Wait until gone'], if_gone: ['条件判断（If）', 'If condition'],
        if_else: ['条件判断（If/Else）', 'If/Else condition'],
        assert: ['检查控件', 'Assert element'], assert_property: ['属性断言', 'Property assertion'],
        scroll_find: ['滚动查找', 'Scroll find'], screenshot: ['截图检查点', 'Screenshot checkpoint'],
    };
    const title = labels[step.type];
    return title ? text(title[0], title[1]) : step.type;
}
function branchLabel(value: 'then' | 'else' | 'skip'): string {
    const labels = {
        then: text('已选 Then（控件消失）', 'Then selected (gone)'),
        else: text('已选 Else（控件存在）', 'Else selected (present)'),
        skip: text('控件存在，If 条件不成立', 'If skipped (element present)'),
    };
    return labels[value];
}
function repairStep(device: AutomationRunDevice, step: AutomationRunDevice['steps'][number], destination: 'scripts'|'card' = 'scripts') {
    if (!selected.value || busy.value) return;
    const request=createAutomationRepairRequest(selected.value,device,step,props.ownerId,uuid());
    if (!request) {
        error.value=text('找不到原始失败步骤或其历史快照，无法安全定位编辑。', 'The historical failed step or snapshot is missing.');
        return;
    }
    if (destination === 'card') emit('repair-card',request);
    else emit('repair-step',request);
}
function diagnosticTarget(device: AutomationRunDevice): string {
    const target=device.diagnostic_target;
    if (!target) return '';
    return target.ip && target.adb_port ? target.ip + ':' + target.adb_port : '';
}
function snapshotStep(id: string): AutomationStep | undefined {
    return selected.value?.script_snapshot?.steps?.find(step => step.id === id);
}
function stepNumber(id: string): string {
    const index = selected.value?.script_snapshot?.steps?.findIndex(step => step.id === id) ?? -1;
    return index >= 0 ? String(index + 1) : '—';
}
function progress(device: AutomationRunDevice): string {
    const total = selected.value?.script_snapshot?.steps?.length ?? 0;
    const recorded = device.steps?.length ?? 0;
    return text(`已记录 ${recorded}/${total} 步`, `${recorded}/${total} step results recorded`);
}

function releasePreview() {
    previewRequest++;
    previewController?.abort(); previewController = undefined;
    if (previewUrl.value) URL.revokeObjectURL(previewUrl.value);
    previewUrl.value = ''; previewLoading.value = false;
}
async function previewArtifact(runId: string, artifactId: string, deviceName: string) {
    releasePreview();
    const request = previewRequest;
    const scope = scopeRevision;
    const owner = props.ownerId;
    const controller = new AbortController();
    previewController = controller;
    previewTitle.value = deviceName;
    previewError.value = ''; previewVisible.value = true; previewLoading.value = true;
    const current = () => !disposed && scope === scopeRevision && request === previewRequest && previewVisible.value;
    try {
        const url = new URL(`${getApiBase()}/api/v1/automation/runs/${encodeURIComponent(runId)}/artifacts/${encodeURIComponent(artifactId)}`, window.location.href);
        url.searchParams.set('owner_user_id', String(owner));
        const response = await fetch(url, { credentials: 'include', cache: 'no-store', signal: controller.signal });
        if (!response.ok) {
            const data = await response.json().catch(() => undefined);
            throw new Error(data?.message ?? `HTTP ${response.status}`);
        }
        const mime = response.headers.get('Content-Type')?.split(';')[0].trim().toLowerCase();
        if (mime !== 'image/png' && mime !== 'image/jpeg') throw new Error(text('返回的文件不是受支持的截图格式。', 'The returned file is not a supported screenshot.'));
        const sizeLimit = 16 * 1024 * 1024;
        if (Number(response.headers.get('Content-Length')) > sizeLimit) throw new Error(text('截图超过预览大小上限。', 'The screenshot exceeds the preview size limit.'));
        const blob = await response.blob();
        if (!current()) return;
        if (!blob.size || blob.size > sizeLimit) throw new Error(text('截图为空或超过预览大小上限。', 'The screenshot is empty or exceeds the preview size limit.'));
        previewUrl.value = URL.createObjectURL(blob);
    } catch (cause) {
        if (current() && !controller.signal.aborted) previewError.value = automationErrorMessage(cause);
    } finally {
        if (current()) previewLoading.value = false;
    }
}

watch([() => props.ownerId, () => props.focusRunId] as const, ([owner, focus], old) => {
    if (!old || owner !== old[0]) {
        scopeRevision++; requestSequence++; runs.value = []; selectedId.value = undefined;
        loading.value = false; working.value = null; error.value = ''; notice.value = '';
        progressFailures = 0; refreshCursor = 0; overviewMode.value = 'runs'; stopProgressRefresh();
        expandedDevices.value = new Set(); cancelVisible.value = false; rerunVisible.value = false;
        rerunPending.value = undefined; rerunUncertain.value = false; rerunParameters.value = Object.create(null); rerunSource.value = undefined;
        previewVisible.value = false; releasePreview();
    }
    if (owner > 0) void loadRuns(focus);
}, { immediate: true });
watch(previewVisible, visible => { if (!visible) releasePreview(); });
onBeforeUnmount(() => { disposed = true; scopeRevision++; requestSequence++; stopProgressRefresh(); rerunParameters.value = Object.create(null); rerunPending.value = undefined; releasePreview(); });
</script>

<template>
    <section class="run-history" :aria-label="text('执行记录', 'Execution history')">
        <div class="history-intro"><p class="muted">{{ text('正在排队或执行的任务每几秒自动向执行器同步状态；已结束的任务保留历史结果，可手动查询。', 'Queued and running tasks automatically refresh from the executor; completed records can still be queried manually.') }}</p><Button icon="pi pi-refresh" :label="text('刷新记录列表', 'Refresh records')" outlined size="small" :loading="loading" :disabled="busy" @click="loadRuns()" /></div>
        <p v-if="error" class="error-message" role="alert">{{ error }}</p>
        <p v-if="notice" class="notice-message" role="status">{{ notice }}</p>
        <section v-if="activeRuns.length" class="console-panel parallel-overview" :aria-label="text('并行执行总览', 'Parallel run overview')">
            <div class="parallel-heading">
                <h2>{{ text('并行执行总览', 'Parallel run overview') }}</h2>
                <div class="parallel-view-switch">
                    <Button :label="text('按任务', 'By run')" size="small" :outlined="overviewMode !== 'runs'" @click="overviewMode = 'runs'" />
                    <Button :label="text('按设备', 'By device')" size="small" :outlined="overviewMode !== 'devices'" @click="overviewMode = 'devices'" />
                </div>
            </div>
            <div class="parallel-metrics">
                <div><strong>{{ summary.runs }}</strong><span>{{ text('活跃任务', 'Active runs') }}</span></div>
                <div><strong>{{ summary.running }}</strong><span>{{ text('执行设备', 'Running devices') }}</span></div>
                <div><strong>{{ summary.queued }}</strong><span>{{ text('排队设备', 'Queued devices') }}</span></div>
                <div><strong>{{ summary.failed }}</strong><span>{{ text('异常设备', 'Problem devices') }}</span></div>
            </div>
            <div v-if="overviewMode === 'runs'" class="parallel-run-grid">
                <article v-for="run in activeRuns" :key="run.id" class="parallel-run-card">
                    <header>
                        <button type="button" class="parallel-title" @click="selectRun(run)">{{ run.name }}</button>
                        <Tag :value="statusLabel(run.state)" :severity="severity(run.state)" />
                    </header>
                    <small>{{ text(`${run.devices.length} 台设备 · 并发 ${run.concurrency ?? '—'}`, `${run.devices.length} devices · concurrency ${run.concurrency ?? '—'}`) }}</small>
                    <div class="parallel-device-list">
                        <button v-for="device in run.devices" :key="device.device_id" type="button" class="parallel-device-row" @click="focusDevice(run, device.device_id)">
                            <span class="parallel-device-name">{{ device.name }}</span>
                            <span class="parallel-meter"><span :style="{ width: devicePercent(run, device) + '%' }" /></span>
                            <span class="parallel-step-count">{{ deviceProgress(run, device) }}</span>
                            <Tag :value="statusLabel(device.state)" :severity="severity(device.state)" />
                        </button>
                    </div>
                </article>
            </div>
            <div v-else class="parallel-device-grid">
                <button v-for="item in activeDevices" :key="item.run.id + ':' + item.device.device_id" type="button" class="parallel-device-card" @click="focusDevice(item.run, item.device.device_id)">
                    <strong>{{ item.device.name }}</strong>
                    <small>{{ item.run.name }}</small>
                    <span class="parallel-meter"><span :style="{ width: devicePercent(item.run, item.device) + '%' }" /></span>
                    <span class="parallel-device-footer"><span>{{ deviceProgress(item.run, item.device) }}</span><Tag :value="statusLabel(item.device.state)" :severity="severity(item.device.state)" /></span>
                </button>
            </div>
        </section>
        <div v-if="!loading && !runs.length" class="console-panel console-empty-state"><i class="pi pi-history" aria-hidden="true" /><h2>{{ text('还没有执行记录', 'No runs yet') }}</h2><p>{{ text('在脚本库试跑或运行脚本后，会在这里保留逐台结果。', 'Trial and batch runs from the script library appear here with individual results.') }}</p></div>
        <div v-else class="history-columns">
            <aside class="console-panel run-list-panel">
                <div class="section-heading"><h2>{{ text('运行记录', 'Runs') }}</h2><span class="muted">{{ runs.length }}</span></div>
                <ul class="run-list"><li v-for="run in runs" :key="run.id" :class="{ active: run.id === selectedId }"><button type="button" :aria-pressed="run.id === selectedId" :disabled="busy" @click="selectRun(run)"><span class="run-list-title"><strong>{{ run.name }}</strong><Tag :value="statusLabel(run.state)" :severity="severity(run.state)" /></span>
                            <small>{{ businessLabel(run.business_verification) }}</small><small>{{ date(run.created_at) }}</small><small>{{ text(`${run.devices.length} 台设备`, `${run.devices.length} devices`) }}</small></button></li></ul>
            </aside>
            <div v-if="selected" class="console-panel result-panel">
                <header class="section-heading result-heading"><div><h2>{{ selected.name }}</h2><span class="run-id">{{ selected.id }}</span></div><Tag :value="statusLabel(selected.state)" :severity="severity(selected.state)" /></header>
                <div class="business-verification-header" role="status">
                    <Tag :value="businessLabel(selected.business_verification)" :severity="businessSeverity(selected.business_verification)" />
                    <span>{{ text('脚本完成只表示动作与检查流程结束；业务验证仅由用户明确标记的最后一步断言决定。',
                        'Script completion means the procedure ended; business success requires an explicitly marked final assertion.') }}</span>
                </div>
                <div class="result-meta"><span>{{ text('创建于', 'Created') }} {{ date(selected.created_at) }}</span><span>{{ text('记录更新于', 'Record updated') }} {{ date(selected.updated_at) }}</span><span>{{ text('执行快照', 'Run snapshot') }} · {{ selected.script_snapshot?.steps?.length ?? 0 }} {{ text('步', 'steps') }}</span></div>
                <div class="result-actions"><Button :label="text('查询执行器', 'Query executor')" icon="pi pi-refresh" size="small" outlined :loading="working === 'refresh'" :disabled="busy && working !== 'refresh'" @click="refreshRun" /><Button :label="text('取消运行 / 清理占用', 'Cancel run / release control')" icon="pi pi-stop" severity="danger" outlined size="small" :disabled="busy || !canCancel" @click="askCancel" /><Button :label="text('选择设备重新运行', 'Run selected devices again')" icon="pi pi-replay" severity="secondary" outlined size="small" :disabled="busy || !canRerun" @click="openRerun" /></div>
                <p class="result-explanation">{{ text('“动作已发送”只表示命令已送出；“条件检查通过”仅代表脚本声明的条件满足。未确认结果不会自动重放。', '“Action sent” confirms dispatch only. “Condition passed” applies only to the stated assertion. Unconfirmed actions are not replayed automatically.') }}</p>
                <div class="device-results">
                    <article v-for="device in selected.devices" :key="device.device_id" class="device-result">
                        <button type="button" class="device-toggle" :aria-expanded="expandedDevices.has(device.device_id)" :aria-controls="`run-steps-${selected.id}-${device.device_id}`" @click="toggleDevice(device.device_id)"><i class="pi" :class="expandedDevices.has(device.device_id) ? 'pi-chevron-down' : 'pi-chevron-right'" aria-hidden="true" /><span class="device-name"><strong>{{ device.name }}</strong><small>{{ progress(device) }}</small></span>
                            <Tag :value="businessLabel(device.business_verification)" :severity="businessSeverity(device.business_verification)" />
                            <Tag :value="statusLabel(device.state)" :severity="severity(device.state)" /></button>
                        <div v-if="cleanupNotice(device)" class="cleanup-notice" role="status"><Tag :value="device.cleanup_confirmed ? text('控制进程已清理', 'Control process cleaned up') : text('控制进程待清理', 'Control cleanup pending')" :severity="device.cleanup_confirmed ? 'secondary' : 'warn'" /><p>{{ cleanupNotice(device) }}</p></div>
                        <p v-if="message(device.error)" class="device-error">{{ message(device.error) }}</p>
                        <div v-if="expandedDevices.has(device.device_id)" :id="`run-steps-${selected.id}-${device.device_id}`" class="step-results">
                            <p v-if="!device.steps?.length" class="muted empty-steps">{{ text('暂无步骤结果；未开始或执行器尚未回报。', 'No step results yet. Execution may not have started or reported back.') }}</p>
                            <ol v-else><li v-for="step in device.steps" :key="step.step_id"><div class="result-step-title"><span class="step-number">{{ stepNumber(step.step_id) }}</span><strong>{{ stepTitle(snapshotStep(step.step_id)) }}</strong>
                                <Tag v-if="snapshotStep(step.step_id)?.business_verification" :value="text('业务验证步骤', 'Business check')" severity="info" />
                                <small v-if="nestedBranchPath(step.step_id)" class="run-branch-path">{{ nestedBranchPath(step.step_id) }}</small>
                                <Tag :value="statusLabel(step.state)" :severity="severity(step.state)" /></div><p v-if="step.branch_taken" class="branch-decision">
                                    <Tag :value="branchLabel(step.branch_taken)" severity="info" />
                                    <small v-if="snapshotStep(step.step_id)?.type === 'if_else'">
                                        Then {{ snapshotStep(step.step_id)?.then_count ?? 0 }} · Else {{ snapshotStep(step.step_id)?.else_count ?? 0 }}
                                    </small>
                                </p>
                                <p v-if="step.skipped_by_step_id" class="branch-skip-source">
                                    {{ text('由第', 'Skipped by step') }} {{ stepNumber(step.skipped_by_step_id) }} {{ text('步条件分支跳过', 'condition') }}
                                </p>
                                <p v-if="step.message">{{ step.message }}</p><p v-if="message(step.error)" class="device-error">{{ message(step.error) }}</p>
                                <small>{{ date(step.started_at) }} <template v-if="step.finished_at"> → {{ date(step.finished_at) }}</template></small>
                                <div v-if="['failed','interrupted','outcome_unknown'].includes(step.state)" class="failure-diagnostics">
                                    <strong>{{ text('失败诊断', 'Failure diagnostics') }}</strong>
                                    <span>{{ text('步骤 ID', 'Step ID') }} · {{ step.step_id }}</span>
                                    <span>{{ text('历史脚本', 'Historical script') }} · {{ selected.script_snapshot.script_id || text('内联试跑', 'Inline trial') }}
                                        <template v-if="selected.script_snapshot.revision"> / {{ text('修订', 'revision') }} {{ selected.script_snapshot.revision }}</template></span>
                                    <span>{{ text('设备', 'Device') }} · {{ device.name }} ({{ device.device_id }})</span>
                                    <span v-if="diagnosticTarget(device)">{{ text('运行时 ADB 目标', 'Run-time ADB target') }} · {{ diagnosticTarget(device) }}</span>
                                    <span v-if="device.diagnostic_target?.target_fingerprint">{{ text('设备绑定指纹', 'Device fingerprint') }} · {{ device.diagnostic_target.target_fingerprint.slice(0, 16) }}…</span>
                                    <span v-if="message(device.error)">{{ text('设备异常', 'Device error') }} · {{ message(device.error) }}</span>
                                    <span v-if="step.state === 'outcome_unknown'" class="device-error">{{ text('动作结果未知。先人工核对手机现状，修复不会自动重试或继续执行。',
                                        'Action outcome is unknown. Check the phone before editing; no automatic replay occurs.') }}</span>
                                    <div class="failure-diagnostics-actions">
                                        <Button icon="pi pi-pencil" size="small" outlined
                                            :label="text('定位并修复此步骤', 'Locate and repair step')" :disabled="busy"
                                            @click="repairStep(device, step)" />
                                        <Button icon="pi pi-mobile" size="small" outlined
                                            :label="text('回手机卡片修复', 'Repair on phone card')" :disabled="busy"
                                            @click="repairStep(device,step,'card')" />
                                    </div>
                                    <small>{{ text('历史记录只读；截图仅在该运行原本保存了证据时可查看。',
                                        'History remains read-only; screenshots are available only if originally captured.') }}</small>
                                </div>
                                <Button v-if="step.artifact_id" class="artifact-button" icon="pi pi-image" :label="text('查看截图证据', 'View screenshot')" text size="small" @click="previewArtifact(selected.id, step.artifact_id, device.name)" /></li></ol>
                        </div>
                    </article>
                </div>
            </div>
        </div>

        <Dialog v-model:visible="cancelVisible" modal :header="text('取消运行 / 清理占用', 'Cancel run / release control')" :style="{ width: '460px', maxWidth: 'calc(100vw - 24px)' }" :closable="!busy" :close-on-escape="!busy"><p>{{ text('停止该运行后续步骤并清理控制进程。已经发生的手机操作不会撤回；清理确认不代表未知动作已成功或失败。设备或执行器失联时，停止结果可能暂时无法确认。', 'Stop subsequent steps and clean up control processes. Actions already performed cannot be undone; cleanup does not prove that an unconfirmed action succeeded or failed. Stopping may remain unconfirmed if a device or executor is unreachable.') }}</p><p v-if="error" class="error-message" role="alert">{{ error }}</p><template #footer><Button :label="text('返回', 'Back')" text :disabled="busy" @click="cancelVisible = false" /><Button :label="text('确认取消', 'Confirm cancellation')" severity="danger" :loading="working === 'cancel'" @click="cancelRun" /></template></Dialog>

        <Dialog v-model:visible="rerunVisible" modal :header="text('确认重新运行', 'Confirm new run')" :style="{ width: '590px', maxWidth: 'calc(100vw - 24px)' }" :closable="!busy" :close-on-escape="!busy">
            <div class="rerun-form">
                <p><strong>{{ rerunSource?.name }}</strong></p>
                <p class="muted">{{ text('使用这次历史运行的步骤快照，从第一个步骤创建新运行。已经成功的设备不会默认选中；再次运行可能重复产生操作效果。', 'Create a new run from the first step using this historical snapshot. Successful devices are not selected. Repeating the run may repeat its effects.') }}</p>
                <p class="muted">{{ text('结果未确认或停止待确认的设备不在重跑清单内。未知结果即使已清理占用，也须先核对手机实际结果，再到脚本库另建任务。', 'Devices with unconfirmed outcomes or pending stops are excluded. Even after control cleanup, verify the actual phone outcome before creating a separate task in the script library.') }}</p>
                <label v-for="device in retryableDevices" :key="device.device_id" class="rerun-target" :for="`rerun-device-${device.device_id}`"><Checkbox :input-id="`rerun-device-${device.device_id}`" v-model="rerunDeviceIds" :value="device.device_id" :disabled="busy || !!rerunPending" /><span><strong>{{ device.name }}</strong><small>{{ device.device_id }}</small></span><Tag :value="statusLabel(device.state)" :severity="severity(device.state)" /></label>
                <label class="field" for="rerun-concurrency"><span>{{ text('同时执行设备数', 'Concurrent devices') }}</span><InputNumber input-id="rerun-concurrency" v-model="rerunConcurrency" :min="1"  :use-grouping="false" :disabled="busy || !!rerunPending" /></label>
                <label v-for="parameter in parameterNames" :key="parameter" class="field" :for="`rerun-parameter-${parameter}`"><span>{{ text('重新填写参数', 'Re-enter parameter') }} · {{ parameter }}</span><InputText :id="`rerun-parameter-${parameter}`" v-model="rerunParameters[parameter]" type="password" autocomplete="new-password" :maxlength="2000" :disabled="busy || !!rerunPending" /></label>
                <p v-if="rerunError" class="error-message" role="alert">{{ rerunError }}</p>
            </div>
            <template #footer><Button :label="text('关闭', 'Close')" text :disabled="busy" @click="rerunVisible = false" /><Button :label="rerunUncertain ? text('重试同一请求', 'Retry same request') : text('创建新运行', 'Create new run')" icon="pi pi-play" :loading="working === 'rerun'" @click="submitRerun" /></template>
        </Dialog>
        <Dialog v-model:visible="previewVisible" modal :header="text('截图证据', 'Screenshot evidence') + ' · ' + previewTitle" :style="{ width: '720px', maxWidth: 'calc(100vw - 24px)' }">
            <div class="evidence-preview"><p v-if="previewLoading" role="status"><i class="pi pi-spin pi-spinner" aria-hidden="true" /> {{ text('正在读取截图…', 'Loading screenshot…') }}</p><p v-if="previewError" class="error-message" role="alert">{{ previewError }}</p><img v-if="previewUrl && !previewError" :src="previewUrl" :alt="text('执行步骤保存的手机画面', 'Phone screenshot saved by an execution step')" @error="previewError = text('截图无法显示。', 'The screenshot could not be displayed.')" /><p class="muted">{{ text('截图反映采集时的画面，不单独证明应用操作成功。', 'This is evidence of the captured screen, not proof that the app operation succeeded.') }}</p></div>
        </Dialog>
    </section>
</template>

<style scoped>
.failure-diagnostics{display:grid;gap:6px;margin-top:8px;padding:10px 12px;border:1px solid var(--console-border);border-radius:8px;font-size:12px;overflow-wrap:anywhere}
.failure-diagnostics strong{font-size:13px}
.failure-diagnostics span,.failure-diagnostics small{color:var(--console-muted)}
.failure-diagnostics-actions{display:flex;gap:8px;flex-wrap:wrap}

.business-verification-header{display:flex;align-items:center;gap:9px;flex-wrap:wrap;padding:10px 0;font-size:12px;color:var(--console-muted);line-height:1.5}
.run-branch-path{font-size:10px;color:var(--console-muted);overflow-wrap:anywhere}
.branch-decision{display:flex;align-items:center;gap:8px;flex-wrap:wrap}
.branch-skip-source{color:var(--console-muted);font-size:12px}

.run-history { display: grid; gap: 16px; }
.history-intro { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; font-size: 13px; }
.parallel-overview{padding:16px;display:grid;gap:14px}
.parallel-heading{display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:12px}
.parallel-heading h2{font-size:15px;margin:0}
.parallel-view-switch{display:flex;gap:6px}
.parallel-metrics{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:8px}
.parallel-metrics>div{display:grid;gap:4px;padding:12px;background:var(--console-panel-raised);border-radius:8px}
.parallel-metrics strong{font-size:22px;line-height:1.1;font-variant-numeric:tabular-nums}
.parallel-metrics span{font-size:11px;color:var(--console-muted)}
.parallel-run-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,330px),1fr));gap:12px}
.parallel-run-card{display:grid;align-content:start;gap:10px;border:1px solid var(--console-border);border-radius:9px;padding:12px;min-width:0}
.parallel-run-card header{display:flex;justify-content:space-between;align-items:center;gap:8px}
.parallel-title{color:var(--p-text-color);font-size:13px;font-weight:600;text-align:left;overflow-wrap:anywhere}
.parallel-title:hover{text-decoration:underline}
.parallel-run-card>small,.parallel-device-card>small{color:var(--console-muted);font-size:11px;overflow-wrap:anywhere}
.parallel-device-list{display:grid;gap:4px}
.parallel-device-row{display:flex;align-items:center;gap:7px;padding:6px 0;border-top:1px solid var(--console-border);font-size:11px;text-align:left;min-width:0}
.parallel-device-row:hover,.parallel-device-card:hover{color:var(--p-primary-color)}
.parallel-device-name{min-width:65px;max-width:100px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
.parallel-meter{display:block;flex:1;height:7px;min-width:20px;background:var(--console-ground);border-radius:6px;overflow:hidden}
.parallel-meter>span{display:block;height:100%;background:var(--p-primary-color);border-radius:6px}
.parallel-step-count{min-width:48px;text-align:right;white-space:nowrap;color:var(--console-muted)}
.parallel-device-grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(min(100%,200px),1fr));gap:10px}
.parallel-device-card{display:grid;gap:8px;padding:12px;border:1px solid var(--console-border);border-radius:8px;text-align:left}
.parallel-device-card strong{font-size:13px;overflow-wrap:anywhere}
.parallel-device-footer{display:flex;justify-content:space-between;align-items:center;gap:6px;font-size:11px}
@media(max-width:560px){.parallel-metrics{grid-template-columns:repeat(2,minmax(0,1fr))}.parallel-device-row{flex-wrap:wrap}}

.history-columns { display: grid; grid-template-columns: minmax(230px, .75fr) minmax(0, 2fr); gap: 20px; align-items: start; }
.run-list { list-style: none; margin: 0; padding: 0; max-height: 700px; overflow: auto; }
.run-list li { border-bottom: 1px solid var(--console-border); }
.run-list li:last-child { border-bottom: 0; }
.run-list li.active { background: var(--console-accent-soft); box-shadow: inset 3px 0 var(--p-primary-color); }
.run-list button { display: grid; gap: 8px; text-align: left; width: 100%; padding: 14px 16px; }
.run-list-title { display: flex; align-items: center; justify-content: space-between; gap: 10px; font-size: 13px; }
.run-list-title strong { overflow-wrap: anywhere; }
.run-list small { color: var(--console-muted); font-size: 11px; }
.result-heading { flex-wrap: wrap; }
.run-id { display: block; color: var(--console-muted); font-size: 11px; margin-top: 5px; overflow-wrap: anywhere; }
.result-meta { padding: 14px 20px 0; display: flex; flex-wrap: wrap; gap: 6px 20px; color: var(--console-muted); font-size: 12px; }
.result-actions { display: flex; flex-wrap: wrap; gap: 8px; padding: 16px 20px; }
.result-explanation { padding: 0 20px 16px; font-size: 12px; color: var(--console-muted); line-height: 1.7; }
.device-result { border-top: 1px solid var(--console-border); }
.device-toggle { display: flex; align-items: center; gap: 12px; width: 100%; text-align: left; padding: 16px 20px; }
.device-toggle > .pi { color: var(--console-muted); font-size: 12px; }
.device-name { display: grid; gap: 5px; min-width: 0; flex: 1; font-size: 13px; }
.device-name strong { overflow-wrap: anywhere; }
.device-name small { color: var(--console-muted); font-size: 11px; }
.device-error { color: var(--p-red-600); overflow-wrap: anywhere; font-size: 12px; padding: 0 20px 12px; }
.cleanup-notice { display: grid; justify-items: start; gap: 8px; padding: 0 20px 14px; }
.cleanup-notice p { color: var(--console-muted); font-size: 12px; line-height: 1.7; overflow-wrap: anywhere; }
.step-results { background: var(--console-panel-raised); }
.step-results ol { list-style: none; padding: 0; margin: 0; }
.step-results li { padding: 14px 20px; border-top: 1px solid var(--console-border); }
.result-step-title { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; font-size: 12px; }
.result-step-title :deep(.p-tag) { margin-left: auto; }
.step-number { display: inline-grid; place-items: center; width: 24px; height: 24px; border-radius: 6px; background: var(--console-ground); color: var(--console-muted); }
.step-results p { font-size: 12px; line-height: 1.7; margin-top: 8px; overflow-wrap: anywhere; }
.step-results small { display: block; margin-top: 7px; font-size: 11px; color: var(--console-muted); }
.step-results .device-error { padding: 0; }
.artifact-button { margin-top: 8px; }
.evidence-preview { display: grid; gap: 14px; font-size: 13px; }
.evidence-preview img { display: block; width: 100%; max-height: 65vh; object-fit: contain; border-radius: 8px; background: var(--console-ground); }
.empty-steps { padding: 16px 20px; }
.error-message { padding: 12px 14px; color: var(--p-red-600); background: color-mix(in srgb, var(--p-red-500) 8%, transparent); border-radius: 8px; font-size: 13px; overflow-wrap: anywhere; }
.notice-message { color: var(--p-primary-color); font-size: 13px; }
.rerun-form { display: grid; gap: 16px; font-size: 13px; }
.rerun-target { display: flex; align-items: center; gap: 10px; padding: 12px; border: 1px solid var(--console-border); border-radius: 8px; }
.rerun-target > span { display: grid; gap: 4px; min-width: 0; flex: 1; }
.rerun-target small { font-size: 11px; color: var(--console-muted); overflow-wrap: anywhere; }
.field { display: grid; gap: 7px; min-width: 0; font-size: 13px; }
.field :deep(.p-inputnumber-input) { width: 100%; }
button:disabled { cursor: default; opacity: .6; }
@media (max-width: 1000px) { .history-columns { grid-template-columns: 1fr; }.run-list { max-height: 320px; }.run-list-panel { min-width: 0; } }
@media (max-width: 480px) { .result-actions, .result-meta, .device-toggle, .step-results li { padding-left: 14px; padding-right: 14px; }.rerun-target { flex-wrap: wrap; } }
</style>
