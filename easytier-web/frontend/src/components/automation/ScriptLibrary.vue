<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue';
import { Button, Dialog, InputNumber, InputText, Select, Tag, Textarea } from 'primevue';
import { useI18n } from 'vue-i18n';
import { v4 as uuid } from 'uuid';
import AutomationApi, { AutomationApiError, automationErrorMessage } from '../../modules/automation-api';
import type { AutomationRun, AutomationRunInput, AutomationScript, AutomationStep, AutomationStepType } from '../../types/automation';
import ScriptStepEditor from './ScriptStepEditor.vue';
import { analyzeAutomationFlow, canInsertBranchStep, insertBranchStep, type BranchLane } from '../../modules/automation-flow';

const props = defineProps<{
    ownerId: number;
    selectedDeviceIds: string[];
    recordedSteps: AutomationStep[];
    deviceNames?: Record<string, string>;
}>();
const emit = defineEmits<{ 'run-created': [run: AutomationRun]; 'recording-consumed': [] }>();
const api = new AutomationApi();
const { locale } = useI18n();
const cn = computed(() => locale.value === 'cn' || locale.value.startsWith('zh'));
const text = (zh: string, en: string) => cn.value ? zh : en;
const scripts = ref<AutomationScript[]>([]);
const loading = ref(false);
const saving = ref(false);
const deleting = ref(false);
const error = ref('');
const notice = ref('');
const editingId = ref<string>();
const revision = ref<number>();
const name = ref('');
const description = ref('');
const steps = ref<AutomationStep[]>([]);
const selectedStepId = ref<string>();
const flowView = ref(true);
const flowLayout = computed(() => analyzeAutomationFlow(steps.value));
const baseline = ref('');
const deleteVisible = ref(false);
const deleteTarget = ref<AutomationScript>();
const runVisible = ref(false);
const runSubmitting = ref(false);
const runError = ref('');
const runMode = ref<'saved' | 'draft'>('saved');
const runName = ref('');
const runScriptId = ref<string>();
const runRevision = ref<number>();
const runSteps = ref<AutomationStep[]>([]);
const runDevices = ref<string[]>([]);
const runTrialDevice = ref<string>();
const runConcurrency = ref<number | null>(1);
const runParameters = ref<Record<string, string>>(Object.create(null));
const pendingRun = ref<AutomationRunInput>();
const uncertainRun = ref(false);
let scopeRevision = 0;
let listRequest = 0;
let disposed = false;

const busy = computed(() => saving.value || deleting.value || runSubmitting.value);
const selectedStep = computed(() => steps.value.find(step => step.id === selectedStepId.value));
const draftJson = () => JSON.stringify({ name: name.value, description: description.value, steps: steps.value });
const dirty = computed(() => draftJson() !== baseline.value);
const parameterNames = computed(() => [...new Set(runSteps.value
    .filter(step => step.type === 'input_text' && step.parameter !== undefined)
    .map(step => step.parameter!.trim()))]);
const runDeviceOptions = computed(() => runDevices.value.map(id => ({ value: id, label: props.deviceNames?.[id] || id })));
const shownRunDevices = computed(() => runMode.value === 'draft'
    ? runDevices.value.filter(id => id === runTrialDevice.value) : runDevices.value);
const stepTitles = computed<Record<AutomationStepType, string>>(() => ({
    key: text('导航按键', 'Navigation key'), tap: text('点击', 'Tap'),
    long_press: text('长按', 'Long press'), swipe: text('滑动', 'Swipe'), drag: text('拖拽', 'Drag'),
    input_text: text('输入文字', 'Enter text'), launch_app: text('打开应用', 'Launch app'),
    wait_for: text('等待出现', 'Wait for element'), wait_gone: text('等待消失', 'Wait until gone'),
    if_gone: text('消失则执行', 'If gone, run following steps'),
    if_else: text('If / Else 条件分支', 'If / Else branch'),
    assert: text('检查控件', 'Assert element'), assert_property: text('属性断言', 'Assert property'),
    scroll_find: text('滚动查找', 'Scroll find'), screenshot: text('截图检查点', 'Screenshot checkpoint'),
}));

function cloneSteps(value: AutomationStep[]): AutomationStep[] {
    return value.map(step => ({ ...step,
        selector: step.selector ? { ...step.selector } : undefined,
        scroll_container: step.scroll_container ? { ...step.scroll_container } : undefined,
    }));
}
function normalizeSteps(): AutomationStep[] {
    return steps.value.map(step => {
        const result = { ...step };
        if (step.selector) result.selector = Object.fromEntries(Object.entries(step.selector)
            .filter(([, value]) => typeof value === 'string' && value.trim().length)
            .map(([key, value]) => [key, value!.trim()]));
        if (step.scroll_container) result.scroll_container = Object.fromEntries(Object.entries(step.scroll_container)
            .filter(([, value]) => typeof value === 'string' && value.trim().length)
            .map(([key, value]) => [key, value!.trim()]));
        if (result.parameter !== undefined) result.parameter = result.parameter.trim();
        if (result.package !== undefined) result.package = result.package.trim();
        return result;
    });
}
function branchScope(index: number): string {
    const entry = flowLayout.value.entries[index];
    if (!entry) return '';
    return entry.ancestors.map(ancestor => {
        const prefix = text('条件', 'If') + ' ' + (ancestor.conditionIndex + 1);
        return prefix + ': ' + (ancestor.lane === 'then' ? 'Then' : 'Else');
    }).join(' / ');
}
function branchInsertEnabled(index: number, lane: BranchLane): boolean {
    return !busy.value && canInsertBranchStep(steps.value, flowLayout.value, index, lane);
}
function addBranchStep(index: number, lane: BranchLane) {
    if (busy.value) return;
    const newStep: AutomationStep = { id: uuid(), type: 'key', key: 'HOME' };
    const proposed = insertBranchStep(steps.value, flowLayout.value, index, lane, newStep);
    if (!proposed) {
        error.value = text('无法在这个分支插入步骤：请检查嵌套层级、分支跨度或 20 步上限。',
            'Cannot insert into this branch. Check nesting, branch spans and the 20-step limit.');
        return;
    }
    steps.value = proposed;
    selectedStepId.value = newStep.id;
    error.value = '';
}
function resetEditor(script?: AutomationScript) {
    editingId.value = script?.id;
    revision.value = script?.revision;
    name.value = script?.name ?? '';
    description.value = script?.description ?? '';
    steps.value = cloneSteps(script?.steps ?? []);
    selectedStepId.value = steps.value[0]?.id;
    baseline.value = draftJson();
    error.value = ''; notice.value = '';
}
async function loadScripts() {
    const scope = scopeRevision;
    const request = ++listRequest;
    const owner = props.ownerId;
    loading.value = true; error.value = '';
    try {
        const result = await api.listScripts(owner);
        if (disposed || scope !== scopeRevision || request !== listRequest) return;
        scripts.value = result.items;
    } catch (cause) {
        if (!disposed && scope === scopeRevision && request === listRequest) error.value = automationErrorMessage(cause);
    } finally {
        if (!disposed && scope === scopeRevision && request === listRequest) loading.value = false;
    }
}
function addStep() {
    if (busy.value || steps.value.length >= 100) return;
    const step: AutomationStep = { id: uuid(), type: 'key', key: 'HOME' };
    steps.value = [...steps.value, step];
    selectedStepId.value = step.id;
}
function updateStep(step: AutomationStep) {
    if (!busy.value) steps.value = steps.value.map(current => current.id === step.id ? step : current);
}
function removeStep(index: number) {
    if (busy.value) return;
    const removed = steps.value[index];
    steps.value = steps.value.filter((_, position) => position !== index);
    if (removed?.id === selectedStepId.value) selectedStepId.value = steps.value[Math.min(index, steps.value.length - 1)]?.id;
}
function canMoveStep(index: number, delta: number): boolean {
    const next = index + delta;
    if (busy.value || flowLayout.value.problem || next < 0 || next >= steps.value.length) return false;
    // Moving a conditional marker alone would detach its owned branch. Only
    // swap ordinary steps inside the exact same nested lane.
    if (['if_else', 'if_gone'].includes(steps.value[index].type) ||
        ['if_else', 'if_gone'].includes(steps.value[next].type)) return false;
    return branchScope(index) === branchScope(next);
}
function moveStep(index: number, delta: number) {
    const next = index + delta;
    if (!canMoveStep(index, delta)) return;
    const reordered = [...steps.value];
    [reordered[index], reordered[next]] = [reordered[next], reordered[index]];
    steps.value = reordered;
}
function importRecording() {
    if (busy.value || !props.recordedSteps.length) return;
    if (props.recordedSteps.length > 100) {
        error.value = text('录制超过 100 步，请先缩短录制后再导入。', 'The recording exceeds 100 steps. Shorten it before importing.');
        return;
    }
    resetEditor();
    name.value = text('录制草稿', 'Recorded draft');
    const used = new Set<string>();
    steps.value = cloneSteps(props.recordedSteps).map(step => {
        const id = step.id && !used.has(step.id) ? step.id : uuid();
        used.add(id); return { ...step, id };
    });
    selectedStepId.value = steps.value[0]?.id;
    emit('recording-consumed');
    notice.value = text('录制已转成未保存草稿。请检查定位和参数，再试跑。', 'Recording imported as an unsaved draft. Review targets and parameters before trying it.');
}
function validationError(): string | null {
    if (!name.value.trim()) return text('请填写脚本名称。', 'Enter a script name.');
    if (new TextEncoder().encode(name.value.trim()).length > 128) return text('脚本名称不能超过 128 字节。', 'The script name must be at most 128 bytes.');
    if (new TextEncoder().encode(description.value.trim()).length > 2000) return text('脚本说明不能超过 2000 字节。', 'The description must be at most 2000 bytes.');
    if (!steps.value.length || steps.value.length > 100) return text('脚本需要 1–100 个步骤。', 'A script needs 1–100 steps.');
    const normalized = normalizeSteps();
    for (let index = 0; index < normalized.length; index++) {
        const step = normalized[index];
        const prefix = text(`第 ${index + 1} 步：`, `Step ${index + 1}: `);
        const validNumber = (value: unknown, min: number, max: number, integer = false) =>
            typeof value === 'number' && Number.isFinite(value) && value >= min && value <= max && (!integer || Number.isInteger(value));
        if (['wait_for', 'wait_gone', 'if_gone', 'if_else', 'assert', 'assert_property', 'scroll_find'].includes(step.type) && !step.selector) return prefix + text('请选择控件。', 'Choose an element.');
        if (step.selector) {
            const attrs = Object.entries(step.selector);
            if (!attrs.length || !attrs.some(([, value]) => value?.trim())) return prefix + text('至少填写一个控件条件。', 'Specify at least one element condition.');
            if (attrs.some(([key, value]) => !['resource_id', 'text', 'description', 'class_name', 'package', 'xpath'].includes(key)
                || !value?.trim() || new TextEncoder().encode(value).length > 512)) {
                return prefix + text('定位条件不能为空且不能超过 512 字节。', 'Each selector must be nonempty and at most 512 bytes.');
            }
            if (step.selector.xpath !== undefined) {
                if (attrs.length !== 1 || !/^[/(]/.test(step.selector.xpath)
                    || ['input_text', 'assert_property'].includes(step.type)) {
                    return prefix + text('XPath 必须独立使用，文字输入不支持 XPath。', 'XPath must be standalone and is not supported for text input.');
                }
            }
        }
        if (step.type === 'key' && !['HOME', 'BACK', 'RECENT', 'WAKE'].includes(step.key ?? '')) return prefix + text('请选择导航按键。', 'Choose a navigation key.');
        if ((['tap', 'long_press'].includes(step.type) && !step.selector) || ['swipe', 'drag'].includes(step.type)) {
            const points = ['swipe', 'drag'].includes(step.type) ? [step.x, step.y, step.to_x, step.to_y] : [step.x, step.y];
            if (points.some(value => !validNumber(value, 0, 1))) return prefix + text('坐标必须位于 0–1。', 'Coordinates must be between 0 and 1.');
            if (!validNumber(step.width, 1, 16384, true) || !validNumber(step.height, 1, 16384, true)) return prefix + text('请填写参考画面宽高。', 'Enter the reference screen dimensions.');
        }
        if (step.type === 'swipe' && !validNumber(step.duration_ms, 100, 3000, true)) return prefix + text('滑动时长为 100–3000 毫秒。', 'Swipe duration must be 100–3000 ms.');
        if (step.type === 'long_press' && !validNumber(step.duration_ms, 400, 3000, true)) return prefix + text('长按时长为 400–3000 毫秒。', 'Long-press duration must be 400–3000 ms.');
        if (step.type === 'drag' && !validNumber(step.duration_ms, 300, 3000, true)) return prefix + text('拖拽时长为 300–3000 毫秒。', 'Drag duration must be 300–3000 ms.');
        if (step.type === 'if_gone') {
            if (!validNumber(step.skip_count, 1, 20, true) || step.skip_count! > normalized.length - index - 1) {
                return prefix + text('条件控制需要 1–20 个有效的后续步骤，不能超出脚本结尾。', 'Conditional range must contain 1–20 following steps within the script.');
            }
        }
        if (step.type === 'if_else') {
            if (!validNumber(step.then_count, 1, 20, true) || !validNumber(step.else_count, 1, 20, true)
                || step.then_count! + step.else_count! > normalized.length - index - 1) {
                return prefix + text('Then/Else 必须各有 1–20 步，且两段不能超过脚本结尾。', 'Then/Else must each contain 1–20 steps within the script.');
            }

        }
        if (['wait_for', 'wait_gone', 'assert', 'scroll_find'].includes(step.type) && !validNumber(step.timeout_ms, 100, 30000, true)) return prefix + text('等待上限为 100–30000 毫秒。', 'The timeout must be 100–30000 ms.');
        if (step.type === 'assert_property') {
            if (!['enabled', 'checked', 'selected', 'clickable', 'scrollable'].includes(step.property ?? '')
                || typeof step.expected !== 'boolean') {
                return prefix + text('请选择布尔属性及明确的预期状态。', 'Choose a boolean property and expected state.');
            }
        }
        if (step.type === 'scroll_find') {
            const container = Object.entries(step.scroll_container ?? {});
            if (!container.length || container.some(([key, value]) =>
                !['resource_id', 'description', 'text', 'class_name', 'package'].includes(key)
                || !value?.trim() || new TextEncoder().encode(value).length > 512)) {
                return prefix + text('滚动容器需要有效的唯一原生条件。', 'Provide a unique native scroll-container selector.');
            }
            if (!['up', 'down'].includes(step.direction ?? '') || !validNumber(step.max_swipes, 1, 5, true)
                || !validNumber(step.duration_ms, 200, 1000, true)) {
                return prefix + text('滚动方向、次数或时长不合法。', 'Scroll direction, count or duration is invalid.');
            }
        }
        if (step.type === 'launch_app' && !/^[A-Za-z][A-Za-z0-9_]*(\.[A-Za-z][A-Za-z0-9_]*)+$/.test(step.package ?? '')) return prefix + text('请填写有效应用包名。', 'Enter a valid Android package name.');
        if (step.type === 'input_text') {
            if (step.parameter !== undefined && !/^[A-Za-z_][A-Za-z0-9_]{0,63}$/.test(step.parameter)) return prefix + text('参数名使用英文字母、数字和下划线，且不能以数字开头。', 'Parameter names use letters, digits and underscores, starting with a letter or underscore.');
            if (step.parameter === undefined && (typeof step.text !== 'string' || new TextEncoder().encode(step.text).length > 2000)) return prefix + text('输入文字不能超过 2000 字节。', 'Text must be at most 2000 bytes.');
        }
    }
    const layout = analyzeAutomationFlow(normalized);
    if (layout.problem) {
        const reason = layout.problem.reason === 'too_deep'
            ? text('条件最多嵌套 3 层。', 'Maximum branch depth is 3.')
            : layout.problem.reason === 'crossing'
                ? text('子条件跨越了父条件的 Then/Else 边界。', 'Child branch crosses a parent Then/Else boundary.')
                : text('条件区域长度无效。', 'Branch span is invalid.');
        return text(`第 ${layout.problem.index + 1} 步：`, `Step ${layout.problem.index + 1}: `) + reason;
    }
    return null;
}
async function save() {
    if (busy.value) return;
    const invalid = validationError();
    if (invalid) { error.value = invalid; return; }
    const scope = scopeRevision;
    const owner = props.ownerId;
    ++listRequest; loading.value = false;
    saving.value = true; error.value = ''; notice.value = '';
    try {
        const saved = await api.saveScript(owner, {
            name: name.value.trim(), description: description.value.trim(), steps: normalizeSteps(),
            ...(editingId.value ? { revision: revision.value } : {}),
        }, editingId.value);
        if (disposed || scope !== scopeRevision) return;
        scripts.value = [saved, ...scripts.value.filter(script => script.id !== saved.id)];
        resetEditor(saved);
        notice.value = text('草稿已保存。', 'Draft saved.');
    } catch (cause) {
        if (disposed || scope !== scopeRevision) return;
        error.value = automationErrorMessage(cause);
        if (cause instanceof AutomationApiError && cause.status === 409) {
            error.value += text(' 当前草稿已保留；刷新列表后重新打开服务端版本再处理。', ' Your draft is retained. Refresh the list and reopen the server version to resolve the conflict.');
        }
    } finally {
        if (!disposed && scope === scopeRevision) saving.value = false;
    }
}
function askDelete(script: AutomationScript) {
    if (busy.value) return;
    error.value = '';
    deleteTarget.value = script; deleteVisible.value = true;
}
async function deleteScript() {
    if (busy.value || !deleteTarget.value) return;
    const scope = scopeRevision;
    const owner = props.ownerId;
    const id = deleteTarget.value.id;
    ++listRequest; loading.value = false;
    deleting.value = true; error.value = '';
    try {
        await api.deleteScript(owner, id);
        if (disposed || scope !== scopeRevision) return;
        scripts.value = scripts.value.filter(script => script.id !== id);
        if (editingId.value === id) resetEditor();
        deleteVisible.value = false;
    } catch (cause) {
        if (!disposed && scope === scopeRevision) error.value = automationErrorMessage(cause);
    } finally {
        if (!disposed && scope === scopeRevision) deleting.value = false;
    }
}
function prepareRun(mode: 'saved' | 'draft') {
    if (busy.value) return;
    if (pendingRun.value && uncertainRun.value) { runVisible.value = true; return; }
    const invalid = validationError();
    if (invalid) { error.value = invalid; return; }
    if (mode === 'saved' && (!editingId.value || dirty.value)) {
        error.value = text('请先保存当前草稿，再批量运行。', 'Save the current draft before a batch run.'); return;
    }
    const ids = [...new Set(props.selectedDeviceIds)];
    if (!ids.length) { error.value = text('请先在设备工作台选择目标手机。', 'Select target phones in the device workspace first.'); return; }
    runMode.value = mode;
    runName.value = name.value.trim();
    runSteps.value = normalizeSteps();
    runDevices.value = ids;
    runTrialDevice.value = ids[0];
    runScriptId.value = mode === 'saved' ? editingId.value : undefined;
    runRevision.value = mode === 'saved' ? revision.value : undefined;
    runConcurrency.value = Math.min(3, ids.length);
    runParameters.value = Object.create(null);
    pendingRun.value = undefined; uncertainRun.value = false;
    runError.value = ''; runVisible.value = true;
}
async function submitRun() {
    if (runSubmitting.value) return;
    if (!pendingRun.value) {
        const ids = shownRunDevices.value;
        if (!ids.length) { runError.value = text('请选择试跑设备。', 'Choose a trial device.'); return; }
        if (parameterNames.value.some(key => typeof runParameters.value[key] !== 'string' || new TextEncoder().encode(runParameters.value[key]).length > 2000)) {
            runError.value = text('请填写全部运行参数，每项最多 2000 字节。', 'Fill every run parameter, at most 2000 bytes each.'); return;
        }
        pendingRun.value = {
            request_id: uuid(), device_ids: [...ids], parameters: { ...runParameters.value },
            concurrency: Math.max(1, Math.min(ids.length, Math.floor(runConcurrency.value ?? 1))),
            ...(runMode.value === 'saved'
                ? { script_id: runScriptId.value, revision: runRevision.value }
                : { name: runName.value, steps: cloneSteps(runSteps.value) }),
        };
    }
    const scope = scopeRevision;
    const owner = props.ownerId;
    const payload = pendingRun.value;
    if (!payload) return;
    runSubmitting.value = true; runError.value = '';
    try {
        const run = await api.createRun(owner, payload);
        if (disposed || scope !== scopeRevision) return;
        runVisible.value = false; pendingRun.value = undefined; uncertainRun.value = false;
        runParameters.value = Object.create(null);
        notice.value = text('任务已登记，请在执行记录查看逐台结果。', 'Run registered. See individual results in execution history.');
        emit('run-created', run);
    } catch (cause) {
        if (disposed || scope !== scopeRevision) return;
        runError.value = automationErrorMessage(cause);
        uncertainRun.value = !(cause instanceof AutomationApiError) || cause.status >= 500;
        if (!uncertainRun.value) pendingRun.value = undefined;
        else runError.value += text(' 接收结果未确认；再次提交会使用相同请求编号，不另建任务。', ' Acceptance is unconfirmed. Retrying uses the same request ID.');
    } finally {
        if (!disposed && scope === scopeRevision) runSubmitting.value = false;
    }
}

watch(() => props.ownerId, () => {
    scopeRevision++; listRequest++;
    scripts.value = []; loading.value = false; saving.value = false; deleting.value = false;
    runVisible.value = false; deleteVisible.value = false; runSubmitting.value = false;
    runParameters.value = Object.create(null); pendingRun.value = undefined; uncertainRun.value = false;
    runSteps.value = []; runDevices.value = []; runName.value = ''; deleteTarget.value = undefined;
    resetEditor();
    if (props.ownerId > 0) void loadScripts();
}, { immediate: true });
onBeforeUnmount(() => { disposed = true; scopeRevision++; listRequest++; runParameters.value = Object.create(null); pendingRun.value = undefined; });
</script>

<template>
    <section class="script-library" :aria-label="text('脚本库', 'Script library')">
        <div v-if="recordedSteps.length" class="recording-notice" role="status">
            <span><i class="pi pi-circle-fill" aria-hidden="true" /> {{ text(`有 ${recordedSteps.length} 个录制步骤等待整理`, `${recordedSteps.length} recorded steps ready to edit`) }}</span>
            <Button :label="text('新建录制草稿', 'Create recorded draft')" icon="pi pi-file-edit" size="small" :disabled="busy" @click="importRecording" />
        </div>
        <p v-if="error" class="error-message" role="alert">{{ error }}</p>
        <p v-if="notice" class="notice-message" role="status">{{ notice }}</p>
        <div class="library-columns">
            <aside class="console-panel script-sidebar">
                <div class="section-heading">
                    <h2>{{ text('已保存脚本', 'Saved scripts') }} <span class="muted">{{ scripts.length }}</span></h2>
                    <Button icon="pi pi-refresh" text rounded size="small" :loading="loading" :disabled="busy" :aria-label="text('刷新脚本列表', 'Refresh scripts')" @click="loadScripts" />
                </div>
                <div class="new-script"><Button :label="text('新建脚本', 'New script')" icon="pi pi-plus" outlined size="small" :disabled="busy" @click="resetEditor()" /></div>
                <p v-if="!loading && !scripts.length" class="sidebar-empty">{{ text('还没有保存的脚本。新增步骤，或导入一次录制。', 'No saved scripts yet. Add steps or import a recording.') }}</p>
                <ul class="script-list">
                    <li v-for="script in scripts" :key="script.id" :class="{ active: script.id === editingId }">
                        <button type="button" class="script-select" :disabled="busy" :aria-pressed="script.id === editingId" @click="resetEditor(script)">
                            <i class="pi pi-file" aria-hidden="true" />
                            <span><strong>{{ script.name }}</strong><small>{{ text(`${script.steps.length} 步 · 修订 ${script.revision}`, `${script.steps.length} steps · revision ${script.revision}`) }}</small></span>
                        </button>
                        <Button icon="pi pi-trash" text rounded size="small" severity="secondary" :disabled="busy" :aria-label="text(`删除脚本 ${script.name}`, `Delete ${script.name}`)" @click="askDelete(script)" />
                    </li>
                </ul>
            </aside>

            <div class="console-panel draft-panel">
                <div class="section-heading draft-heading">
                    <div><h2>{{ text('步骤编辑器', 'Step editor') }}</h2><span class="muted draft-state">{{ dirty || !editingId ? text('草稿 · 未保存', 'Draft · unsaved') : text(`草稿 · 已保存（修订 ${revision}）`, `Draft · saved (revision ${revision})`) }}</span></div>
                    <div class="button-row">
                        <Button :label="text('单机试跑', 'Trial run')" icon="pi pi-play" severity="secondary" outlined size="small" :disabled="busy || !steps.length" @click="prepareRun('draft')" />
                        <Button :label="text('保存草稿', 'Save draft')" icon="pi pi-save" size="small" :loading="saving" :disabled="deleting || runSubmitting" @click="save" />
                    </div>
                </div>
                <div class="draft-meta">
                    <label class="field" for="automation-script-name"><span>{{ text('脚本名称', 'Script name') }}</span><InputText id="automation-script-name" v-model="name" :maxlength="120" :disabled="busy" /></label>
                    <label class="field" for="automation-script-description"><span>{{ text('说明（选填）', 'Description (optional)') }}</span><Textarea id="automation-script-description" v-model="description" :rows="2" :maxlength="2000" :disabled="busy" /></label>
                </div>
                <div class="step-columns">
                    <div class="steps-navigation">
                        <div class="steps-title">
                            <strong>{{ text('步骤', 'Steps') }} {{ steps.length }}/100</strong>
                            <div class="flow-view-controls">
                                <Button :icon="flowView ? 'pi pi-list' : 'pi pi-sitemap'" text rounded size="small"
                                    :aria-label="flowView ? text('切换步骤列表', 'Show step list') : text('切换流程图', 'Show flowchart')"
                                    :title="flowView ? text('步骤列表', 'Step list') : text('流程图', 'Flowchart')"
                                    @click="flowView = !flowView" />
                                <Button icon="pi pi-plus" text rounded size="small" :disabled="busy || steps.length >= 100"
                                    :aria-label="text('添加步骤', 'Add step')" @click="addStep" />
                            </div>
                        </div>
                        <p v-if="flowLayout.problem" class="flow-error" role="status">
                            {{ text('条件层级或范围有误，请检查第', 'Invalid branch scope. Review step') }}
                            {{ flowLayout.problem.index + 1 }} {{ text('步。', '.') }}
                        </p>
                        <ol v-if="flowView" class="flow-list" :aria-label="text('嵌套流程编辑图', 'Nested workflow editor')">
                            <li v-for="(step, index) in steps" :key="step.id" class="flow-entry"
                                :class="{ selected: step.id === selectedStepId, conditional: ['if_gone', 'if_else'].includes(step.type) }"
                                :style="{ marginLeft: Math.min(3, flowLayout.entries[index]?.ancestors.length ?? 0) * 12 + 'px' }">
                                <div v-if="branchScope(index)" class="flow-lane-label">{{ branchScope(index) }}</div>
                                <button type="button" class="flow-node" :disabled="busy"
                                    :aria-pressed="step.id === selectedStepId" @click="selectedStepId = step.id">
                                    <span class="step-number">{{ index + 1 }}</span>
                                    <span class="flow-node-name">{{ stepTitles[step.type] }}</span>
                                    <i v-if="['if_gone', 'if_else'].includes(step.type)" class="pi pi-sitemap" aria-hidden="true" />
                                </button>
                                <div v-if="step.type === 'if_gone' || step.type === 'if_else'" class="flow-branches">
                                    <div class="flow-branch-actions">
                                        <span class="flow-then">Then · {{ step.type === 'if_gone' ? step.skip_count : step.then_count }}</span>
                                        <Button icon="pi pi-plus" text size="small" :disabled="!branchInsertEnabled(index, 'then')"
                                            :aria-label="text('向 Then 分支添加步骤', 'Add step to Then branch')"
                                            @click="addBranchStep(index, 'then')" />
                                    </div>
                                    <div v-if="step.type === 'if_else'" class="flow-branch-actions">
                                        <span class="flow-else">Else · {{ step.else_count }}</span>
                                        <Button icon="pi pi-plus" text size="small" :disabled="!branchInsertEnabled(index, 'else')"
                                            :aria-label="text('向 Else 分支添加步骤', 'Add step to Else branch')"
                                            @click="addBranchStep(index, 'else')" />
                                    </div>
                                </div>
                            </li>
                        </ol>
                        <ol v-else class="step-list">
                            <li v-for="(step, index) in steps" :key="step.id" :class="{ selected: step.id === selectedStepId }">
                                <button type="button" class="step-select" :disabled="busy" :aria-pressed="step.id === selectedStepId" @click="selectedStepId = step.id"><span class="step-number">{{ index + 1 }}</span><span class="step-description">{{ stepTitles[step.type] }}<small v-if="branchScope(index)" class="branch-scope">{{ branchScope(index) }}</small></span></button>
                                <div class="step-actions">
                                    <Button icon="pi pi-arrow-up" text rounded size="small" :disabled="!canMoveStep(index, -1)" :aria-label="text('上移步骤', 'Move step up')" @click="moveStep(index, -1)" />
                                    <Button icon="pi pi-arrow-down" text rounded size="small" :disabled="!canMoveStep(index, 1)" :aria-label="text('下移步骤', 'Move step down')" @click="moveStep(index, 1)" />
                                    <Button icon="pi pi-times" text rounded size="small" :disabled="busy" :aria-label="text('删除步骤', 'Remove step')" @click="removeStep(index)" />
                                </div>
                            </li>
                        </ol>
                        <Button :label="text('添加步骤', 'Add step')" icon="pi pi-plus" text size="small" :disabled="busy || steps.length >= 100" @click="addStep" />
                    </div>
                    <div class="step-detail">
                        <ScriptStepEditor v-if="selectedStep" :model-value="selectedStep" :disabled="busy" @update:model-value="updateStep" />
                        <div v-else class="console-empty-state"><i class="pi pi-list" aria-hidden="true" /><h2>{{ text('从第一个步骤开始', 'Add the first step') }}</h2><p>{{ text('选择有限操作，填写目标与条件。运行前可先在一台手机试跑。', 'Choose an action and its target conditions. Try the draft on one phone before a batch run.') }}</p></div>
                    </div>
                </div>
                <div class="draft-footer"><span class="muted">{{ text(`工作台已选 ${selectedDeviceIds.length} 台设备`, `${selectedDeviceIds.length} devices selected in the workspace`) }}</span><Button :label="text('运行已保存脚本', 'Run saved script')" icon="pi pi-play-circle" :disabled="busy || !editingId || dirty" @click="prepareRun('saved')" /></div>
            </div>
        </div>

        <Dialog v-model:visible="deleteVisible" modal :header="text('删除脚本', 'Delete script')" :style="{ width: '430px', maxWidth: 'calc(100vw - 24px)' }" :closable="!deleting" :close-on-escape="!deleting">
            <p>{{ text(`永久删除“${deleteTarget?.name ?? ''}”？已有执行记录和运行快照会保留。`, `Permanently delete “${deleteTarget?.name ?? ''}”? Existing run snapshots and results are retained.`) }}</p>
            <p v-if="error" class="error-message" role="alert">{{ error }}</p>
            <template #footer><Button :label="text('取消', 'Cancel')" text :disabled="deleting" @click="deleteVisible = false" /><Button :label="text('删除', 'Delete')" severity="danger" :loading="deleting" @click="deleteScript" /></template>
        </Dialog>

        <Dialog v-model:visible="runVisible" modal :header="runMode === 'draft' ? text('确认单机试跑', 'Confirm trial run') : text('确认批量运行', 'Confirm batch run')" :style="{ width: '620px', maxWidth: 'calc(100vw - 24px)' }" :closable="!runSubmitting" :close-on-escape="!runSubmitting">
            <div class="run-form">
                <p><strong>{{ runName }}</strong> · {{ text(`${runSteps.length} 步`, `${runSteps.length} steps`) }} <Tag :value="runMode === 'draft' ? text('当前草稿快照', 'Current draft snapshot') : text(`修订 ${runRevision}`, `Revision ${runRevision}`)" severity="info" /></p>
                <p class="muted">{{ text('本次目标和步骤已固定。后续修改草稿或工作台选择，不会改变本次运行。某台失败只停止该台，其余继续。', 'Targets and steps are fixed for this run. Later edits or selections do not change it. A device failure stops that device; the others continue.') }}</p>
                <label v-if="runMode === 'draft'" class="field" for="automation-trial-device"><span>{{ text('试跑设备', 'Trial device') }}</span><Select input-id="automation-trial-device" v-model="runTrialDevice" :options="runDeviceOptions" option-label="label" option-value="value" :disabled="runSubmitting || !!pendingRun" /></label>
                <div class="run-targets"><span class="muted">{{ text(`目标设备（${shownRunDevices.length}）`, `Targets (${shownRunDevices.length})`) }}</span><ul><li v-for="id in shownRunDevices" :key="id"><strong>{{ deviceNames?.[id] || id }}</strong><small v-if="deviceNames?.[id]">{{ id }}</small></li></ul></div>
                <label v-if="runMode === 'saved'" class="field" for="automation-run-concurrency"><span>{{ text('同时执行设备数', 'Concurrent devices') }}</span><InputNumber input-id="automation-run-concurrency" v-model="runConcurrency" :min="1"  :use-grouping="false" :disabled="runSubmitting || !!pendingRun" /></label>
                <label v-for="parameter in parameterNames" :key="parameter" class="field" :for="`automation-param-${parameter}`"><span>{{ text('参数', 'Parameter') }} · {{ parameter }}</span><InputText :id="`automation-param-${parameter}`" v-model="runParameters[parameter]" type="password" autocomplete="new-password" :maxlength="2000" :disabled="runSubmitting || !!pendingRun" /></label>
                <p v-if="parameterNames.length" class="muted">{{ text('参数仅提交给本次运行；不会写回脚本草稿。', 'Parameters apply to this run and are not written back to the script draft.') }}</p>
                <p class="muted">{{ text('关闭页面不会取消已提交的任务。需要停止时，请在执行记录中取消；已经发生的手机操作无法撤回。', 'Closing this page does not cancel a submitted run. Cancel in execution history to stop subsequent steps; completed phone actions cannot be undone.') }}</p>
                <p v-if="runError" class="error-message" role="alert">{{ runError }}</p>
            </div>
            <template #footer><Button :label="text('关闭', 'Close')" text :disabled="runSubmitting" @click="runVisible = false" /><Button :label="uncertainRun ? text('重试同一请求', 'Retry same request') : text('确认运行', 'Start run')" icon="pi pi-play" :loading="runSubmitting" @click="submitRun" /></template>
        </Dialog>
    </section>
</template>

<style scoped>
.script-library { display: grid; gap: 16px; }
.library-columns { display: grid; grid-template-columns: minmax(210px, .65fr) minmax(0, 2fr); gap: 20px; align-items: start; }
.recording-notice { display: flex; justify-content: space-between; gap: 12px; align-items: center; flex-wrap: wrap; padding: 14px 18px; border: 1px solid var(--console-border); border-radius: 12px; background: var(--console-accent-soft); }
.recording-notice .pi { color: var(--p-red-500); font-size: 10px; margin-right: 8px; }
.error-message { padding: 12px 14px; border-radius: 8px; color: var(--p-red-600); background: color-mix(in srgb, var(--p-red-500) 8%, transparent); overflow-wrap: anywhere; }
.notice-message { color: var(--p-primary-color); font-size: 13px; }
.new-script { padding: 14px; }
.script-list, .step-list { padding: 0; margin: 0; list-style: none; }
.script-list li { display: flex; align-items: center; gap: 3px; border-top: 1px solid var(--console-border); padding: 5px 7px; }
.script-list li.active { background: var(--console-accent-soft); }
.script-select { display: flex; gap: 10px; align-items: center; padding: 10px 6px; min-width: 0; flex: 1; text-align: left; }
.script-select > span { display: grid; gap: 5px; min-width: 0; }
.script-select strong { overflow-wrap: anywhere; font-size: 13px; }
.script-select small, .sidebar-empty { color: var(--console-muted); font-size: 12px; }
.sidebar-empty { padding: 6px 18px 24px; line-height: 1.7; }
.draft-heading { flex-wrap: wrap; }
.draft-state { display: block; margin-top: 5px; font-size: 12px; }
.button-row { display: flex; flex-wrap: wrap; gap: 8px; }
.draft-meta { padding: 18px 20px; display: grid; gap: 14px; border-bottom: 1px solid var(--console-border); }
.field { display: grid; gap: 7px; min-width: 0; font-size: 13px; }
.field > span { font-weight: 550; }
.field :deep(.p-inputnumber-input) { width: 100%; }
.step-columns { display: grid; grid-template-columns: minmax(175px, .8fr) minmax(0, 1.4fr); min-height: 340px; }
.steps-navigation { padding: 12px 10px; border-right: 1px solid var(--console-border); background: var(--console-panel-raised); }
.steps-title { display: flex; align-items: center; justify-content: space-between; font-size: 12px; padding: 0 6px 10px; }
.step-description{display:grid;gap:2px;min-width:0}.branch-scope{font-size:10px;line-height:1.4;color:var(--console-muted)}
.flow-view-controls{display:flex;align-items:center;gap:2px}
.flow-list{list-style:none;margin:0;padding:0;display:grid;gap:7px}
.flow-entry{position:relative;border-left:3px solid var(--console-border-strong);padding:0 0 4px 8px}
.flow-entry.conditional{border-left-color:var(--p-primary-color)}
.flow-entry.selected .flow-node{border-color:var(--p-primary-color);background:var(--console-accent-soft)}
.flow-lane-label{font-size:10px;color:var(--console-muted);padding:4px 3px;overflow-wrap:anywhere}
.flow-node{width:100%;display:flex;align-items:center;gap:7px;min-width:0;padding:7px 5px;text-align:left;background:var(--console-panel);border:1px solid var(--console-border);border-radius:8px;font-size:12px;color:var(--p-text-color);cursor:pointer}
.flow-node-name{flex:1;min-width:0;overflow-wrap:anywhere}
.flow-node i{color:var(--p-primary-color);font-size:12px}
.flow-branches{display:grid;gap:2px;padding:3px 5px 3px 25px}
.flow-branch-actions{display:flex;align-items:center;justify-content:space-between;gap:5px;font-size:11px}
.flow-branch-actions :deep(.p-button){width:24px;height:24px}
.flow-then{color:var(--p-primary-color)}.flow-else{color:var(--console-muted)}
.flow-error{font-size:11px;color:var(--p-red-500);padding:5px;line-height:1.5}
.step-list li { border: 1px solid transparent; border-radius: 9px; margin-bottom: 7px; }
.step-list li.selected { border-color: var(--p-primary-color); background: var(--console-accent-soft); }
.step-select { display: flex; gap: 8px; align-items: center; width: 100%; text-align: left; padding: 10px; font-size: 12px; }
.step-number { display: inline-grid; place-items: center; width: 23px; height: 23px; border-radius: 7px; background: var(--console-ground); color: var(--console-muted); }
.step-actions { display: flex; justify-content: flex-end; padding: 0 5px 4px; }
.step-actions :deep(.p-button) { width: 26px; height: 26px; }
.step-detail { padding: 20px; min-width: 0; }
.draft-footer { padding: 16px 20px; display: flex; justify-content: space-between; align-items: center; gap: 12px; flex-wrap: wrap; border-top: 1px solid var(--console-border); font-size: 12px; }
.run-form { display: grid; gap: 16px; font-size: 13px; }
.run-targets ul { max-height: 180px; overflow: auto; margin-top: 8px; border: 1px solid var(--console-border); border-radius: 8px; }
.run-targets li { display: grid; gap: 3px; padding: 8px 12px; border-bottom: 1px solid var(--console-border); overflow-wrap: anywhere; }
.run-targets li:last-child { border: 0; }
.run-targets small { color: var(--console-muted); }
button:disabled { cursor: default; opacity: .6; }
@media (max-width: 1100px) { .library-columns { grid-template-columns: 1fr; }.script-list { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); } }
@media (max-width: 640px) { .step-columns { grid-template-columns: 1fr; }.steps-navigation { border-right: 0; border-bottom: 1px solid var(--console-border); }.script-list { grid-template-columns: 1fr; }.step-detail { padding: 16px; } }
</style>
