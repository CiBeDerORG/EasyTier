<script setup lang="ts">
import { computed, onBeforeUnmount, ref, toRef, watch } from 'vue';
import { Button, Checkbox, Dialog, InputText, Select, Tag, Textarea } from 'primevue';
import { useI18n } from 'vue-i18n';
import { v4 as uuid } from 'uuid';
import AutomationApi, { AutomationApiError, automationErrorMessage } from '../../modules/automation-api';
import type { AutomationDevice, AutomationRun, AutomationScript, AutomationStep, AutomationStepType } from '../../types/automation';
import { readTrialReceipt, clearTrialReceipt, findTrialRun,
    type TrialSubmissionReceipt } from '../../modules/automation-trial-receipt';
import ScriptStepEditor from './ScriptStepEditor.vue';
import { analyzeAutomationFlow, canInsertBranchStep, type BranchLane } from '../../modules/automation-flow';
import { addFlowBranchStep, createFlowCondition, removeFlowBlock } from '../../modules/automation-flow-tree';
import { emptyAutomationDraft, type AutomationDraftWorkspace } from '../../modules/automation-draft';
import { stepCanPickTarget } from '../../modules/automation-step-target';
import { businessCheckStepId } from '../../modules/automation-business-verification';
import {
    planAutomationRepair, cloneRepairSteps,
    type AutomationRepairRequest, type AutomationRepairPlan,
} from '../../modules/automation-run-repair';
const props = defineProps<{
    ownerId: number;
    selectedDeviceIds: string[];
    draft: AutomationDraftWorkspace;
    draftSyncLabel?: string;
    draftSyncMessage?: string;
    canReplaceWorkspace?: () => boolean;
    deviceNames?: Record<string, string>;
    pickerDevices: { id: string; name: string }[];
    runDeviceSnapshots: AutomationDevice[];
    repairRequest?: AutomationRepairRequest | null;
}>();
const emit = defineEmits<{
    'run-created': [run: AutomationRun];
    'pick-step-target': [stepId: string, stepType: AutomationStepType, deviceId: string];
    'repair-consumed': [requestId: string];
}>();
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
// The parent owns this workspace; changing tabs never destroys unsaved edits.
const editingId = toRef(props.draft, 'editingId');
const revision = toRef(props.draft, 'revision');
const name = toRef(props.draft, 'name');
const description = toRef(props.draft, 'description');
const steps = toRef(props.draft, 'steps');
const selectedStepId = toRef(props.draft, 'selectedStepId');
const flowView = ref(true);
const flowLayout = computed(() => analyzeAutomationFlow(steps.value));
const baseline = toRef(props.draft, 'baseline');
const replaceDraftVisible = ref(false);
const pendingOpenScript = ref<AutomationScript | null>(null);
const pendingRepairPlan = ref<AutomationRepairPlan | null>(null);
const repairOrigin = ref<{ runId: string; deviceId: string; revision?: number; isCopy: boolean } | null>(null);
let repairGeneration = 0;
const deleteVisible = ref(false);
const deleteTarget = ref<AutomationScript>();
const blockedRunReceipts=ref<TrialSubmissionReceipt[]>([]);
const checkingOldRun=ref(false);
function checkOldRunReceipts(ids:string[]):boolean {
    try{
        blockedRunReceipts.value=ids.map(id=>readTrialReceipt(window.sessionStorage,props.ownerId,id))
            .filter((item):item is TrialSubmissionReceipt=>item!==null);
        if(!blockedRunReceipts.value.length)return true;
        error.value='这些设备存在接收结果未知的运行请求：'+
            blockedRunReceipts.value.map(item=>item.requestId).join('、')+
            '。请先只读核对旧请求，不能另建运行。';
        return false;
    }catch(cause){
        error.value='无法确认历史请求保护：'+automationErrorMessage(cause)+
            '。禁止发送手机动作。';return false;
    }
}
async function recoverOldRunReceipts(){
    if(checkingOldRun.value || !blockedRunReceipts.value.length)return;
    checkingOldRun.value=true;
    const owner=props.ownerId,scope=scopeRevision;
    try {
        const response=await api.listRuns(owner);
        if(disposed || scope!==scopeRevision)return;
        const matched=blockedRunReceipts.value.map(receipt=>({
            receipt,run:findTrialRun(response.items,receipt),
        }));
        const missing=matched.find(item=>!item.run ||
            !item.run.script_snapshot.steps.some(step=>step.id===item.receipt.stepId));
        if(missing){
            error.value='近期执行历史没有找到原请求 '+missing.receipt.requestId+
                '；不能认定动作未发生，保护继续生效。请查看服务器日志与手机现场。';return;
        }
        for(const item of matched)clearTrialReceipt(window.sessionStorage,item.receipt);
        blockedRunReceipts.value=[];
        error.value='';notice.value='已从只读历史找回原任务，没有重新发送手机动作。';
        if(matched[0]?.run)emit('run-created',matched[0].run);
    }catch(cause){
        if(!disposed && scope===scopeRevision)error.value='历史恢复失败：'+automationErrorMessage(cause);
    }finally{checkingOldRun.value=false;}
}

let scopeRevision = 0;
let listRequest = 0;
let disposed = false;

const busy = computed(() => saving.value || deleting.value);
const selectedStep = computed(() => steps.value.find(step => step.id === selectedStepId.value));
const businessCheckId = computed(() => businessCheckStepId(steps.value));
const selectedIsFinalAssertion = computed(() => {
    const selected = selectedStep.value;
    const last = steps.value[steps.value.length - 1];
    return !!selected && selected.id === last?.id && ['assert', 'assert_property'].includes(selected.type)
        && !flowLayout.value.problem && !flowLayout.value.entries[flowLayout.value.entries.length - 1]?.ancestors.length;
});
function setBusinessCheck(enabled: boolean) {
    if (busy.value || !selectedStep.value
        || (enabled && !selectedIsFinalAssertion.value)
        || (!enabled && selectedStep.value.business_verification !== true)) return;
    const id = selectedStep.value.id;
    steps.value = steps.value.map(step => {
        const { business_verification: _old, ...rest } = step;
        return step.id === id && enabled ? { ...rest, business_verification: true as const } : rest;
    });
    error.value = '';
}
const pickerDeviceId = ref<string>();
const pickerOptions = computed(() => props.pickerDevices.map(device => ({
    label: device.name, value: device.id,
})));
const defaultPickerDevice = computed(() => {
    const available = new Set(props.pickerDevices.map(device => device.id));
    if (pickerDeviceId.value && available.has(pickerDeviceId.value)) return pickerDeviceId.value;
    return props.selectedDeviceIds.find(id => available.has(id)) ?? props.pickerDevices[0]?.id;
});
function pickExistingStep() {
    if (busy.value || !selectedStep.value || !stepCanPickTarget(selectedStep.value)
        || !defaultPickerDevice.value) return;
    emit('pick-step-target', selectedStep.value.id, selectedStep.value.type, defaultPickerDevice.value);
}
const draftJson = () => JSON.stringify({ name: name.value, description: description.value, steps: steps.value });
const dirty = computed(() => draftJson() !== baseline.value);
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
    const newStep = blankStep();
    const proposed = addFlowBranchStep(steps.value, steps.value[index].id, lane, newStep);
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
    repairOrigin.value = null;
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
function blankStep(): AutomationStep {
    // An unconfigured key cannot pass validation or send an accidental HOME.
    return { id: uuid(), type: 'key' };
}
function addStep() {
    if (busy.value || steps.value.length >= 100) return;
    const step = blankStep();
    steps.value = [...steps.value, step];
    selectedStepId.value = step.id;
}
function updateStep(step: AutomationStep) {
    if (busy.value) return;
    const index = steps.value.findIndex(item => item.id === step.id);
    if (index < 0) return;
    const previous = steps.value[index];
    const wasCondition = previous.type === 'if_gone' || previous.type === 'if_else';
    const nowCondition = step.type === 'if_gone' || step.type === 'if_else';
    if (!wasCondition && nowCondition) {
        const proposed = createFlowCondition(steps.value, step.id, step, blankStep);
        if (!proposed) {
            error.value = text('无法创建此条件：请检查父分支的 20 步上限或三级嵌套限制。',
                'Cannot create this condition: check the enclosing branch size and maximum nesting depth.');
            return;
        }
        steps.value = proposed;
        error.value = '';
        return;
    }
    if (wasCondition && previous.type !== step.type) {
        // Deleting/converting a marker alone would silently reassign its entire
        // body to a different branch. Remove the complete block instead.
        error.value = text('不能直接修改条件类型，请删除整个条件块后重新添加。',
            'A conditional type cannot be changed in place. Remove its whole block first.');
        return;
    }
    if (wasCondition) {
        // Branch sizes are derived from the editor tree, never editable metadata.
        step = { ...step, skip_count: previous.skip_count,
            then_count: previous.then_count, else_count: previous.else_count };
    }
    steps.value = steps.value.map(current => current.id === step.id ? step : current);
    error.value = '';
}
function removeStep(index: number) {
    if (busy.value) return;
    const removed = steps.value[index];
    if (!removed) return;
    const proposed = removeFlowBlock(steps.value, removed.id);
    if (!proposed) {
        error.value = text('无法单独删除分支最后一个步骤。请修改该步骤，或删除整个父条件块。',
            'The final step in a branch cannot be removed. Edit it or remove the parent condition block.');
        return;
    }
    steps.value = proposed;
    if (removed.id === selectedStepId.value)
        selectedStepId.value = steps.value[Math.min(index, steps.value.length - 1)]?.id;
    error.value = '';
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
function requestOpenScript(script?: AutomationScript) {
    if (busy.value || props.canReplaceWorkspace?.() === false) return;
    repairGeneration++;
    pendingRepairPlan.value = null;
    if (dirty.value) {
        pendingOpenScript.value = script ?? null;
        replaceDraftVisible.value = true;
        return;
    }
    resetEditor(script);
}
function applyRepairPlan(plan: AutomationRepairPlan) {
    if (plan.kind === 'saved') {
        resetEditor(plan.script);
    } else {
        // Historical execution snapshots are immutable. Changed/deleted
        // revisions are never saved back under the original script ID.
        editingId.value = undefined;
        revision.value = undefined;
        name.value = plan.name;
        description.value = '';
        steps.value = cloneRepairSteps(plan.steps);
        baseline.value = JSON.stringify({name:'',description:'',steps:[]});
    }
    selectedStepId.value = plan.stepId;
    repairOrigin.value = {
        runId: plan.sourceRunId, deviceId: plan.sourceDeviceId,
        revision: plan.kind === 'saved' ? plan.script.revision : plan.sourceRevision,
        isCopy: plan.kind === 'copy',
    };
    flowView.value = true;
    replaceDraftVisible.value = false;
    pendingRepairPlan.value = null;
    pendingOpenScript.value = null;
    notice.value = plan.kind === 'saved'
        ? text('已打开原脚本的相同修订并定位失败步骤。修改并保存将生成新的脚本修订，历史记录不变。',
            'Opened the exact saved revision at the failed step. Saving creates a new revision; history is unchanged.')
        : text('原脚本已修改、被删除或来自单机试跑。已从历史快照建立独立修复草稿，保存时会创建新脚本。',
            'The original is newer, deleted or inline. Created a separate draft from the historical snapshot; saving creates a new script.');
    error.value = '';
}
function confirmOpenScript() {
    if (busy.value || props.canReplaceWorkspace?.() === false) return;
    const repair = pendingRepairPlan.value;
    if (repair) {
        applyRepairPlan(repair);
        return;
    }
    resetEditor(pendingOpenScript.value ?? undefined);
    pendingOpenScript.value = null;
    replaceDraftVisible.value = false;
}
async function receiveRepairRequest(request: AutomationRepairRequest) {
    if (disposed || request.ownerId !== props.ownerId || props.draft.ownerId !== props.ownerId) return;
    const generation = ++repairGeneration;
    const owner = props.ownerId, scope = scopeRevision;
    try {
        // Query the current saved script state instead of relying on an old
        // copy cached when the ScriptLibrary tab was last opened.
        const result = await api.listScripts(owner);
        if (disposed || scope !== scopeRevision || owner !== props.ownerId
            || generation !== repairGeneration || props.repairRequest?.requestId !== request.requestId) return;
        scripts.value = result.items;
        const plan = planAutomationRepair(request, result.items);
        if (!plan) {
            error.value = text('无法验证历史步骤与执行快照，修复入口已取消。',
                'Historical step or run snapshot could not be verified. Repair was cancelled.');
            emit('repair-consumed',request.requestId);
            return;
        }
        // Consume the navigation request once. Returning to this tab must
        // not unexpectedly re-open a cancelled repair dialog.
        emit('repair-consumed',request.requestId);
        if (dirty.value) {
            pendingOpenScript.value = null;
            pendingRepairPlan.value = plan;
            replaceDraftVisible.value = true;
        } else {
            applyRepairPlan(plan);
        }
    } catch (cause) {
        if (!disposed && scope === scopeRevision && generation === repairGeneration) {
            // Cannot prove that the saved script revision still matches.
            // Do not silently fall back to editing a possibly newer script.
            error.value = automationErrorMessage(cause);
            emit('repair-consumed',request.requestId);
        }
    }
}
function validationError(candidate?: AutomationStep[], singleStep = false, allowEmpty = false): string | null {
    if (!singleStep) {
        if (!name.value.trim()) return text('请填写脚本名称。', 'Enter a script name.');
        if (new TextEncoder().encode(name.value.trim()).length > 128) return text('脚本名称不能超过 128 字节。', 'The script name must be at most 128 bytes.');
        if (new TextEncoder().encode(description.value.trim()).length > 2000) return text('脚本说明不能超过 2000 字节。', 'The description must be at most 2000 bytes.');
    }
    const normalized = candidate ?? normalizeSteps();
    if (normalized.length > 100 || (!allowEmpty && !normalized.length)) return text('执行需 1–100 个步骤；空脚本可以保存后继续编辑。', 'Runs need 1–100 steps; an empty script can be saved for editing.');
    const marked = normalized.filter(step => step.business_verification !== undefined);
    if (marked.length && (!businessCheckStepId(normalized)
        || analyzeAutomationFlow(normalized).entries[normalized.length - 1]?.ancestors.length)) {
        return text('业务验证只能标记最后一个、不属于条件分支的控件断言或属性断言。',
            'Only a final, unconditional element or property assertion may verify business success.');
    }
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
    // T25: persisted scripts may be empty; trial/batch validation stays strict.
    const invalid = validationError(undefined, false, true);
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
watch(() => props.ownerId, () => {
    scopeRevision++; listRequest++; repairGeneration++;
    pendingRepairPlan.value = null; repairOrigin.value = null;
    scripts.value = []; loading.value = false; saving.value = false; deleting.value = false;
    deleteVisible.value = false;
    blockedRunReceipts.value=[];checkingOldRun.value=false;
    deleteTarget.value = undefined;
    if (props.draft.ownerId !== props.ownerId) {
        Object.assign(props.draft, emptyAutomationDraft(props.ownerId));
    }
    if (props.ownerId > 0) {
        void loadScripts();
        // T46: retain recovery for legacy unknown requests after removing the
        // redundant modal. Discovery is read-only; never resubmit.
        checkOldRunReceipts(props.runDeviceSnapshots.map(device=>device.id));
    }
}, { immediate: true });
watch(()=>props.runDeviceSnapshots.map(device=>device.id).join(','),()=>{
    if(props.ownerId>0)checkOldRunReceipts(props.runDeviceSnapshots.map(device=>device.id));
});
onBeforeUnmount(() => {
    disposed = true; scopeRevision++; listRequest++; repairGeneration++;
});
watch(() => props.repairRequest, request => {
    if (request) void receiveRepairRequest(request);
}, { immediate: true });

</script>

<template>
    <section class="script-library" :aria-label="text('脚本库', 'Script library')">
        <p v-if="error" class="error-message" role="alert">{{ error }}</p>
        <Button v-if="blockedRunReceipts.length" label="只读查询未确认的执行请求"
            icon="pi pi-history" outlined size="small" :loading="checkingOldRun"
            :disabled="checkingOldRun" @click="recoverOldRunReceipts"/>
        <p v-if="notice" class="notice-message" role="status">{{ notice }}</p>
        <div class="library-columns">
            <aside class="console-panel script-sidebar">
                <div class="section-heading">
                    <h2>{{ text('已保存脚本', 'Saved scripts') }} <span class="muted">{{ scripts.length }}</span></h2>
                    <Button icon="pi pi-refresh" text rounded size="small" :loading="loading" :disabled="busy" :aria-label="text('刷新脚本列表', 'Refresh scripts')" @click="loadScripts" />
                </div>
                <div class="new-script"><Button :label="text('新建脚本', 'New script')" icon="pi pi-plus" outlined size="small" :disabled="busy" @click="requestOpenScript()" /></div>
                <p v-if="!loading && !scripts.length" class="sidebar-empty">{{ text('还没有保存的脚本。新增步骤，或导入一次录制。', 'No saved scripts yet. Add steps or import a recording.') }}</p>
                <ul class="script-list">
                    <li v-for="script in scripts" :key="script.id" :class="{ active: script.id === editingId }">
                        <button type="button" class="script-select" :disabled="busy" :aria-pressed="script.id === editingId" @click="requestOpenScript(script)">
                            <i class="pi pi-file" aria-hidden="true" />
                            <span><strong>{{ script.name }}</strong><small>{{ text(`${script.steps.length} 步 · 修订 ${script.revision}`, `${script.steps.length} steps · revision ${script.revision}`) }}</small></span>
                        </button>
                        <Button icon="pi pi-trash" text rounded size="small" severity="secondary" :disabled="busy" :aria-label="text(`删除脚本 ${script.name}`, `Delete ${script.name}`)" @click="askDelete(script)" />
                    </li>
                </ul>
            </aside>

            <div class="console-panel draft-panel">
                <div v-if="repairOrigin" class="repair-origin-banner" role="status">
                    <Tag :value="repairOrigin.isCopy ? text('历史快照修复副本', 'Historical repair copy') : text('原脚本同修订', 'Matching saved revision')"
                        :severity="repairOrigin.isCopy ? 'warn' : 'info'" />
                    <span>{{ text('来源运行', 'Source run') }} {{ repairOrigin.runId }}
                        · {{ text('设备', 'Device') }} {{ repairOrigin.deviceId }}
                        <template v-if="repairOrigin.revision">· {{ text('原修订', 'Original revision') }} {{ repairOrigin.revision }}</template>
                    </span>
                    <small>{{ text('当前只在编辑草稿；历史执行记录不会修改，手机也不会自动重新操作。',
                        'Editing only the current draft; history remains unchanged and no phone action is replayed.') }}</small>
                </div>
                <div class="section-heading draft-heading">
                    <div><h2>{{ text('步骤编辑器', 'Step editor') }}</h2>
                        <span class="muted draft-state">{{ dirty || !editingId ? text('正式脚本未保存', 'Not published as a script') : text(`正式脚本 · 修订 ${revision}`, `Published script · revision ${revision}`) }}</span>
                        <span v-if="draftSyncLabel" class="muted draft-state">{{ draftSyncLabel }}</span>
                        <small v-if="draftSyncMessage" class="draft-sync-error">{{ draftSyncMessage }}</small>
                    </div>
                    <div class="button-row">
                        <Button :label="text('保存正式脚本', 'Publish script')" icon="pi pi-save" size="small" :loading="saving" :disabled="deleting" @click="save" />
                    </div>
                </div>
                <div class="draft-meta">
                    <div class="business-check-summary" role="status">
                        <Tag :value="businessCheckId ? text('已配置业务成功验证', 'Business check configured') : text('未配置业务验证', 'No business check')" :severity="businessCheckId ? 'success' : 'secondary'" />
                        <small>{{ text('只有明确标记的最后一步断言通过，并且该手机的脚本完整执行，才认定业务成功。否则仅显示脚本执行完成。',
                            'Business success requires an explicitly marked terminal assertion to pass and the device run to complete. Otherwise only script completion is reported.') }}</small>
                    </div>
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
                                    <span class="flow-node-name">{{ step.type === 'key' && !step.key ? text('待配置步骤', 'Unconfigured step') : stepTitles[step.type] }}</span>
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
                                <button type="button" class="step-select" :disabled="busy" :aria-pressed="step.id === selectedStepId" @click="selectedStepId = step.id"><span class="step-number">{{ index + 1 }}</span><span class="step-description">{{ step.type === 'key' && !step.key ? text('待配置步骤', 'Unconfigured step') : stepTitles[step.type] }}<small v-if="branchScope(index)" class="branch-scope">{{ branchScope(index) }}</small></span></button>
                                <div class="step-actions">
                                    <Button icon="pi pi-arrow-up" text rounded size="small" :disabled="!canMoveStep(index, -1)" :aria-label="text('上移步骤', 'Move step up')" @click="moveStep(index, -1)" />
                                    <Button icon="pi pi-arrow-down" text rounded size="small" :disabled="!canMoveStep(index, 1)" :aria-label="text('下移步骤', 'Move step down')" @click="moveStep(index, 1)" />
                                    <Button icon="pi pi-times" text rounded size="small" :disabled="busy" :aria-label="['if_gone','if_else'].includes(step.type) ? text('删除整个条件块', 'Remove entire condition block') : text('删除步骤', 'Remove step')" @click="removeStep(index)" />
                                </div>
                            </li>
                        </ol>
                        <Button :label="text('添加步骤', 'Add step')" icon="pi pi-plus" text size="small" :disabled="busy || steps.length >= 100" @click="addStep" />
                    </div>
                    <div class="step-detail">
                        <div v-if="selectedStep && stepCanPickTarget(selectedStep)" class="step-pick-controls">
                            <strong>{{ text('重新拾取当前步骤的控件', 'Re-pick the current step target') }}</strong>
                            <div class="button-row">
                                <Select :model-value="defaultPickerDevice" @update:model-value="pickerDeviceId = $event" :options="pickerOptions" option-label="label" option-value="value"
                                    :placeholder="pickerOptions.length ? text('选择手机', 'Select phone') : text('无可用手机', 'No available phone')"
                                    :disabled="busy || !pickerOptions.length"
                                    :aria-label="text('用于控件拾取的手机', 'Phone for element inspection')" />
                                <Button :label="text('重新拾取', 'Re-pick target')" icon="pi pi-search" size="small" outlined
                                    :disabled="busy || !defaultPickerDevice" @click="pickExistingStep" />
                            </div>
                            <small class="hint">{{ text('不新增步骤；保留步骤 ID、动作、参数和条件范围。未初始化 UiAutomator2 时请先到工作台准备手机。',
                                'Keeps the current step ID and action; prepare UiAutomator2 on the workbench first if needed.') }}</small>
                        </div>
                        <div v-if="selectedStep" class="business-check-editor">
                            <label v-if="selectedIsFinalAssertion || selectedStep.business_verification === true" class="business-check-label">
                                <Checkbox :model-value="selectedStep.business_verification === true" binary
                                    :disabled="busy" @update:model-value="setBusinessCheck(!!$event)" />
                                <strong>{{ text('将这一步作为业务成功验证', 'Verify business success with this assertion') }}</strong>
                            </label>
                            <small v-if="selectedStep.business_verification === true && !selectedIsFinalAssertion" class="error-message">
                                {{ text('该断言已不在脚本末尾，请取消勾选或将它移回最后一步，才能保存或运行。',
                                    'This business check is no longer terminal. Uncheck it or move it to the last step before saving.') }}
                            </small>
                            <small v-else-if="!selectedIsFinalAssertion">
                                {{ text('业务成功条件必须放在脚本最后一步、位于所有 If / Else 分支之外，类型为“检查控件”或“属性断言”。',
                                    'Place an element or property assertion at the very end, outside any If / Else branch, to enable business verification.') }}
                            </small>
                        </div>
                        <ScriptStepEditor v-if="selectedStep" :model-value="selectedStep" :disabled="busy" @update:model-value="updateStep" />
                        <p v-if="selectedStep" class="muted" role="status">
                            {{ text('单步 ▶ 请在工作台原手机卡片的脚本浮窗中执行；不会另外打开视频或切换页面。',
                                'Use the original phone card script window for a one-step trial. No secondary video is opened.') }}
                        </p>
                        <div v-else class="console-empty-state"><i class="pi pi-list" aria-hidden="true" /><h2>{{ text('从第一个步骤开始', 'Add the first step') }}</h2><p>{{ text('选择有限操作，填写目标与条件。运行前可先在一台手机试跑。', 'Choose an action and its target conditions. Try the draft on one phone before a batch run.') }}</p></div>
                    </div>
                </div>
                <div class="draft-footer"><span class="muted">{{ text(`工作台已选 ${selectedDeviceIds.length} 台设备`, `${selectedDeviceIds.length} devices selected in the workspace`) }}</span><span class="muted">{{ text('完整脚本执行已迁移到工作台手机原画面的脚本浮窗；此页仅编辑、保存及管理历史脚本。', 'Run full scripts in the phone card floating window; this page manages saved scripts.') }}</span></div>
            </div>
        </div>

        <Dialog v-model:visible="replaceDraftVisible" modal :header="text('覆盖未保存草稿？', 'Discard unsaved draft?')" :style="{ width: '440px', maxWidth: 'calc(100vw - 24px)' }">
            <p v-if="pendingRepairPlan">
                {{ text('当前草稿有未保存更改。从历史执行记录打开失败步骤会替换当前工作区草稿，但不会修改历史记录或后台已保存脚本。确定丢弃当前未保存内容？',
                    'The current draft has unsaved changes. Opening a failed step replaces the workspace draft, not the historical record or saved script. Discard your current edits?') }}
            </p>
            <p v-else>{{ text('当前草稿存在未保存的录制、拾取或编辑步骤。切换脚本会丢失这些更改。', 'Your current draft contains unsaved recording, inspection or editing changes. Switching scripts discards them.') }}</p>
            <template #footer>
                <Button :label="text('继续编辑', 'Keep editing')" text @click="replaceDraftVisible = false; pendingRepairPlan = null" />
                <Button :label="text('丢弃并切换', 'Discard and switch')" severity="danger" @click="confirmOpenScript" />
            </template>
        </Dialog>
        <Dialog v-model:visible="deleteVisible" modal :header="text('删除脚本', 'Delete script')" :style="{ width: '430px', maxWidth: 'calc(100vw - 24px)' }" :closable="!deleting" :close-on-escape="!deleting">
            <p>{{ text(`永久删除“${deleteTarget?.name ?? ''}”？已有执行记录和运行快照会保留。`, `Permanently delete “${deleteTarget?.name ?? ''}”? Existing run snapshots and results are retained.`) }}</p>
            <p v-if="error" class="error-message" role="alert">{{ error }}</p>
            <template #footer><Button :label="text('取消', 'Cancel')" text :disabled="deleting" @click="deleteVisible = false" /><Button :label="text('删除', 'Delete')" severity="danger" :loading="deleting" @click="deleteScript" /></template>
        </Dialog>

    </section>
</template>

<style scoped>
.script-library { display: grid; gap: 16px; }
.repair-origin-banner{display:flex;align-items:center;gap:8px;flex-wrap:wrap;padding:12px 18px;border-bottom:1px solid var(--console-border);font-size:12px;overflow-wrap:anywhere}
.repair-origin-banner span{flex:1;min-width:200px}
.repair-origin-banner small{width:100%;color:var(--console-muted)}

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
.step-pick-controls{display:grid;gap:8px;padding:12px;border-bottom:1px solid var(--console-border)}
.step-pick-controls strong{font-size:13px}
.step-pick-controls .button-row{align-items:center}
.step-pick-controls .hint{font-size:12px;color:var(--console-muted)}
.step-pick-controls :deep(.p-select){min-width:165px;max-width:100%}
.draft-state { display: block; margin-top: 5px; font-size: 12px; }
.draft-sync-error{display:block;margin-top:4px;color:var(--p-orange-500);font-size:11px;max-width:480px}
.button-row { display: flex; flex-wrap: wrap; gap: 8px; }
.draft-meta { padding: 18px 20px; display: grid; gap: 14px; border-bottom: 1px solid var(--console-border); }
.business-check-summary{display:flex;align-items:center;gap:10px;flex-wrap:wrap}
.business-check-summary small{font-size:12px;line-height:1.6;color:var(--console-muted);flex:1;min-width:200px}
.business-check-editor{padding:10px 0;display:grid;gap:8px}
.business-check-label{display:flex;align-items:center;gap:8px;font-size:13px}
.business-check-editor small{font-size:11px;line-height:1.6;color:var(--console-muted)}
.field { display: grid; gap: 7px; min-width: 0; font-size: 13px; }
.field > span { font-weight: 550; }
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
button:disabled { cursor: default; opacity: .6; }
@media (max-width: 1100px) { .library-columns { grid-template-columns: 1fr; }.script-list { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); } }
@media (max-width: 640px) { .step-columns { grid-template-columns: 1fr; }.steps-navigation { border-right: 0; border-bottom: 1px solid var(--console-border); }.script-list { grid-template-columns: 1fr; }.step-detail { padding: 16px; } }
</style>
