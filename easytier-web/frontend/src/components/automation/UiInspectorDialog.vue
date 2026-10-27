<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { Button, Dialog, Message, Tag } from 'primevue';
import { v4 as uuid } from 'uuid';
import AutomationApi, { automationErrorMessage } from '../../modules/automation-api';
import type { AutomationDevice, AutomationHierarchy, AutomationHierarchyNode, AutomationSelector, AutomationStep } from '../../types/automation';
import type { StepTarget } from '../../modules/automation-step-target';
import { locatorIsUnique, rankLocatorCandidates } from '../../modules/automation-locator-candidates';
import InspectorPropertiesPanel from './InspectorPropertiesPanel.vue';
import type { InspectorAction } from '../../modules/automation-inspector-actions';
import { matchPickerPoint } from '../../modules/automation-pick-engine';
import type { PickerVideoPoint } from '../../modules/automation-picker-state';

const props = defineProps<{
    visible: boolean; device: AutomationDevice; ownerId: number;
    stream: MediaStream | null; queuedCount: number;
    replaceStep?: Pick<AutomationStep, 'id' | 'type'> | null;
    /** Original card's right-click position. Null keeps the legacy manual picker. */
    initialPoint?: PickerVideoPoint | null;
    preferredAction?: InspectorAction | null;
}>();
const emit = defineEmits<{
    close: []; 'add-step': [step: AutomationStep]; edit: [];
    'replace-target': [stepId: string, target: StepTarget];
}>();
const api = new AutomationApi();
const video = ref<HTMLVideoElement | null>(null);
const surface = ref<HTMLDivElement | null>(null);
const response = ref<AutomationHierarchy | null>(null);
const selectedId = ref<number | null>(null);
const collapsedIds = ref<number[]>([]);
const search = ref('');
type LocatorMode = 'resource_id' | 'text' | 'description' | 'composite' | 'xpath' | 'coordinates';
const mode = ref<LocatorMode>('resource_id');
const loading = ref(false);
const error = ref('');
const feedback = ref('');
const videoSize = ref({ width: 0, height: 0 });
type Point = { x: number; y: number };
const dragPickMode = ref(false);
const dragStart = ref<Point | null>(null);
const dragEnd = ref<Point | null>(null);
const dragReady = computed(() => !!snapshot.value && compatibleVideo.value && !!dragStart.value && !!dragEnd.value
    && Math.hypot((dragEnd.value.x - dragStart.value.x) * snapshot.value.width,
        (dragEnd.value.y - dragStart.value.y) * snapshot.value.height) >= 16);
const dragStyle = (point: Point) => ({ left: point.x * 100 + '%', top: point.y * 100 + '%' });
let requestGeneration = 0;
let initialPointUsed = false;

const snapshot = computed(() => response.value?.snapshot);
const nodeIndex = computed(() => new Map((snapshot.value?.nodes ?? []).map(node => [node.id, node] as const)));
const parentIds = computed(() => new Set((snapshot.value?.nodes ?? [])
    .map(node => node.parent_id).filter((value): value is number => value !== null)));
const selected = computed(() => selectedId.value === null ? null : nodeIndex.value.get(selectedId.value) ?? null);
const locatorCandidates = computed(() => snapshot.value && selectedId.value !== null ? rankLocatorCandidates(snapshot.value, selectedId.value) : []);
const selectedCandidate = ref<string | null>(null);
// Coordinates are a deliberate, separately confirmed fallback, never a default.
const coordinateConfirmed = ref(false);
const confirmedLocator = computed(() => {
    const candidate = locatorCandidates.value.find(item => item.id === selectedCandidate.value && item.unique);
    return candidate && snapshot.value && selectedId.value !== null
        && locatorIsUnique(snapshot.value, selectedId.value, candidate.selector) ? candidate.selector : null;
});
const lineage = computed(() => {
    const chain: AutomationHierarchyNode[] = [];
    const visited = new Set<number>();
    let node: AutomationHierarchyNode | null | undefined = selected.value;
    while (node && !visited.has(node.id)) {
        visited.add(node.id);
        chain.unshift(node);
        node = node.parent_id === null ? undefined : nodeIndex.value.get(node.parent_id);
    }
    return chain;
});
const clickableParent = computed(() => [...lineage.value].slice(0, -1).reverse()
    .find(node => node.clickable && node.enabled) ?? null);
const scrollContainer = computed<AutomationSelector | null>(() => {
    const shot = snapshot.value;
    if (!shot || shot.truncated) return null;
    for (const ancestor of [...lineage.value].reverse()) {
        if (!ancestor.scrollable || !ancestor.resource_id) continue;
        const count = shot.nodes.filter(node => node.resource_id === ancestor.resource_id).length;
        if (count === 1) return { resource_id: ancestor.resource_id };
    }
    return null;
});
const capturedAt = computed(() => {
    if (!response.value?.captured_at) return '';
    const date = new Date(response.value.captured_at);
    return Number.isNaN(date.getTime()) ? '' : date.toLocaleString('zh-CN', { hour12: false });
});
const visibleNodes = computed(() => {
    const nodes = snapshot.value?.nodes ?? [];
    const term = search.value.trim().toLocaleLowerCase();
    if (term) {
        const included = new Set<number>();
        for (const node of nodes) {
            if (![node.password ? '' : node.text, node.password ? '' : node.description, node.resource_id, node.class_name]
                .some(value => value.toLocaleLowerCase().includes(term))) continue;
            let parent: AutomationHierarchyNode | undefined = node;
            const visited = new Set<number>();
            while (parent && !visited.has(parent.id)) {
                included.add(parent.id);
                visited.add(parent.id);
                parent = parent.parent_id === null ? undefined : nodeIndex.value.get(parent.parent_id);
            }
        }
        return nodes.filter(node => included.has(node.id)).slice(0, 250);
    }
    const collapsed = new Set(collapsedIds.value);
    return nodes.filter(node => {
        let parent = node.parent_id;
        const visited = new Set<number>();
        while (parent !== null && !visited.has(parent)) {
            if (collapsed.has(parent)) return false;
            visited.add(parent);
            parent = nodeIndex.value.get(parent)?.parent_id ?? null;
        }
        return true;
    }).slice(0, 250);
});
const compatibleVideo = computed(() => {
    if (!snapshot.value || !videoSize.value.width || !videoSize.value.height) return false;
    const expected = snapshot.value.width / snapshot.value.height;
    const displayed = videoSize.value.width / videoSize.value.height;
    return Math.abs(expected / displayed - 1) < .035;
});
const coordinateBoundsValid = computed(() => {
    const b = selected.value?.bounds, shot = snapshot.value;
    return !!b && !!shot && !shot.truncated && b.left >= 0 && b.top >= 0
        && b.right > b.left && b.bottom > b.top
        && b.right <= shot.width && b.bottom <= shot.height;
});
const coordinatesAllowed = computed(() => compatibleVideo.value && coordinateBoundsValid.value
    && response.value?.target_fingerprint === props.device.target_fingerprint);
function confirmCoordinates() {
    if (mode.value !== 'coordinates' || !coordinatesAllowed.value) return;
    coordinateConfirmed.value = true;
    feedback.value = '已手动确认使用坐标定位；画面旋转、控件变更或刷新控件树后需要重新确认。';
}
function onLocatorModeChange() {
    selectedCandidate.value = null;
    coordinateConfirmed.value = false;
}
function changeLocatorMode(value: LocatorMode) {
    mode.value = value;
    onLocatorModeChange();
}
const nativeFields = ['resource_id', 'text', 'description', 'class_name', 'package'] as const;
type NativeField = typeof nativeFields[number];
const bestComposite = computed<AutomationSelector | null>(() => {
    const shot = snapshot.value, node = selected.value;
    if (!shot || !node || shot.truncated) return null;
    const available = nativeFields.filter(key => !!node[key]);
    for (let size = 2; size <= available.length; size++) {
        const search = (start: number, group: NativeField[]): AutomationSelector | null => {
            if (group.length === size) {
                const terms = Object.fromEntries(group.map(key => [key, node[key]])) as AutomationSelector;
                const matches = shot.nodes.filter(item => group.every(key => item[key] === node[key]));
                return matches.length === 1 ? terms : null;
            }
            for (let i = start; i < available.length; i++) {
                const result = search(i + 1, [...group, available[i]]);
                if (result) return result;
            }
            return null;
        };
        const result = search(0, []);
        if (result) return result;
    }
    return null;
});
function xpathLiteral(value: string): string {
    if (!value.includes("'")) return "'" + value + "'";
    if (!value.includes('"')) return '"' + value + '"';
    return "concat(" + value.split("'").map(chunk => "'" + chunk + "'").join(", \"'\", ") + ")";
}
const suggestedXPath = computed(() => {
    const node = selected.value;
    if (!node) return '';
    const chosen = bestComposite.value ?? (['resource_id', 'text', 'description'] as const)
        .reduce<AutomationSelector | null>((found, key) => {
            if (found || !node[key] || !snapshot.value || snapshot.value.truncated) return found;
            return snapshot.value.nodes.filter(item => item[key] === node[key]).length === 1 ? { [key]: node[key] } : null;
        }, null);
    if (!chosen) return '';
    const attrs: Record<string, string> = {
        resource_id: 'resource-id', text: 'text', description: 'content-desc',
        class_name: 'class', package: 'package',
    };
    const expr = '//*[' + Object.entries(chosen).map(([key, value]) =>
        '@' + attrs[key] + '=' + xpathLiteral(value!)).join(' and ') + ']';
    return new TextEncoder().encode(expr).length <= 512 ? expr : '';
});
const modeOptions = computed(() => {
    if (!selected.value || !snapshot.value) return [];
    const node = selected.value;
    const choices: {value: typeof mode.value; label: string; unique: boolean}[] = [
        { value: 'resource_id', label: 'Resource ID', unique: false },
        { value: 'text', label: '控件文字', unique: false },
        { value: 'description', label: '无障碍描述', unique: false },
        { value: 'composite', label: '组合条件（候选推荐）', unique: locatorCandidates.value.some(item => item.id.includes('+') && item.unique) },
        { value: 'xpath', label: 'XPath 1.0（请在脚本编辑器手动配置）', unique: false },
        { value: 'coordinates', label: '画面坐标（手动兜底）', unique: coordinatesAllowed.value },
    ];
    for (const choice of choices) {
        if (choice.value === 'coordinates' || choice.value === 'composite' || choice.value === 'xpath') continue;
        const key = choice.value as 'resource_id' | 'text' | 'description';
        const value = node[key];
        choice.unique = !!value && snapshot.value.nodes.filter(item => item[key] === value).length === 1
            && !snapshot.value.truncated;
    }
    return choices;
});
const activeMode = computed(() => modeOptions.value.find(item => item.value === mode.value));
const canAdd = computed(() => !props.replaceStep && !!selected.value
    && !!snapshot.value && !snapshot.value.truncated
    && response.value?.target_fingerprint === props.device.target_fingerprint
    && (mode.value === 'coordinates' ? coordinatesAllowed.value && coordinateConfirmed.value : !!confirmedLocator.value)
    && props.queuedCount < 100);
const canReplace = computed(() => {
    if (!props.replaceStep || !selected.value || !snapshot.value || snapshot.value.truncated
        || (mode.value === 'coordinates' ? !coordinatesAllowed.value || !coordinateConfirmed.value : !confirmedLocator.value)
        || response.value?.target_fingerprint !== props.device.target_fingerprint) return false;
    const type = props.replaceStep.type;
    if (mode.value === 'coordinates') return (type === 'tap' || type === 'long_press') && compatibleVideo.value;
    if (mode.value === 'xpath' && (type === 'input_text' || type === 'assert_property')) return false;
    return true;
});
const nodeHighlight = computed(() => {
    const node = selected.value, shot = snapshot.value;
    if (!node || !shot || !compatibleVideo.value || dragPickMode.value) return null;
    const { left, top, right, bottom } = node.bounds;
    return { left: left / shot.width * 100 + '%', top: top / shot.height * 100 + '%',
        width: (right - left) / shot.width * 100 + '%', height: (bottom - top) / shot.height * 100 + '%' };
});
function selectNode(id: number) {
    selectedId.value = id; selectedCandidate.value = null; coordinateConfirmed.value = false; feedback.value = '';
    const expanded = new Set(collapsedIds.value);
    const visited = new Set<number>();
    let parent = nodeIndex.value.get(id)?.parent_id ?? null;
    while (parent !== null && !visited.has(parent)) {
        expanded.delete(parent);
        visited.add(parent);
        parent = nodeIndex.value.get(parent)?.parent_id ?? null;
    }
    collapsedIds.value = [...expanded];
}
function toggleBranch(id: number) {
    const next = new Set(collapsedIds.value);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    collapsedIds.value = [...next];
}
function setVideo() {
    if (!video.value) return;
    video.value.srcObject = props.visible ? props.stream : null;
    if (props.visible && props.stream) void video.value.play().catch(() => undefined);
}
function videoMetadata() {
    videoSize.value = { width: video.value?.videoWidth ?? 0, height: video.value?.videoHeight ?? 0 };
}
async function refresh() {
    if (!props.visible || loading.value) return;
    const generation = ++requestGeneration;
    const pointHint = !initialPointUsed ? props.initialPoint : null;
    initialPointUsed = true;
    response.value = null; selectedId.value = null; selectedCandidate.value = null; coordinateConfirmed.value = false; collapsedIds.value = [];
    dragPickMode.value = false; dragStart.value = null; dragEnd.value = null;
    error.value = ''; feedback.value = ''; loading.value = true;
    try {
        const result = await api.hierarchy(props.ownerId, props.device.id);
        if (generation !== requestGeneration || !props.visible) return;
        if (result.device_id !== props.device.id || result.target_fingerprint !== props.device.target_fingerprint) {
            error.value = '手机连接目标已经变化，请关闭后重新打开拾取窗口。';
            return;
        }
        response.value = result;
        if (pointHint) {
            const match = matchPickerPoint(result, pointHint, {
                targetFingerprint: props.device.target_fingerprint ?? '',
            });
            if (match.status === 'matched' && match.nodeId !== null) {
                selectNode(match.nodeId);
                feedback.value = match.message;
            } else {
                // No silent fallback to an unrelated node or screen coordinate.
                feedback.value = match.message + ' 可在控件树中手动选择或再次在卡片右键拾取。';
            }
        }
    } catch (cause) {
        if (generation === requestGeneration && props.visible) error.value = automationErrorMessage(cause);
    } finally {
        if (generation === requestGeneration) loading.value = false;
    }
}
function startDragPick() {
    if (!snapshot.value || !compatibleVideo.value) return;
    dragPickMode.value = true; dragStart.value = null; dragEnd.value = null;
    selectedId.value = null; feedback.value = '请先点击画面选择拖拽起点，再点击画面选择终点（不会操作手机）。';
}
function cancelDragPick() {
    dragPickMode.value = false; dragStart.value = null; dragEnd.value = null;
    feedback.value = '';
}
function saveDrag() {
    if (!dragReady.value || !snapshot.value || !dragStart.value || !dragEnd.value || props.queuedCount >= 100) return;
    emit('add-step', {
        id: uuid(), type: 'drag',
        x: dragStart.value.x, y: dragStart.value.y, to_x: dragEnd.value.x, to_y: dragEnd.value.y,
        width: snapshot.value.width, height: snapshot.value.height, duration_ms: 700,
    });
    cancelDragPick();
    feedback.value = '已添加拖拽步骤到草稿；可在脚本库调整持续时间，执行前先单机试跑。';
}
function pick(event: MouseEvent) {
    if (!surface.value || !snapshot.value || !compatibleVideo.value) return;
    const rect = surface.value.getBoundingClientRect();
    const normX = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width));
    const normY = Math.max(0, Math.min(1, (event.clientY - rect.top) / rect.height));
    if (dragPickMode.value) {
        if (!dragStart.value || dragEnd.value) { dragStart.value = { x: normX, y: normY }; dragEnd.value = null; }
        else dragEnd.value = { x: normX, y: normY };
        feedback.value = dragEnd.value ? '已选定起终点；确认后添加拖拽，或再次点画面重新选择起点。' : '已选择起点，请点击终点。';
        return;
    }
    const match = matchPickerPoint(response.value!, {
        x: normX * snapshot.value.width, y: normY * snapshot.value.height,
        width: snapshot.value.width, height: snapshot.value.height,
    }, { targetFingerprint: props.device.target_fingerprint ?? '' });
    if (match.status === 'matched' && match.nodeId !== null) {
        selectNode(match.nodeId);
        feedback.value = match.message;
    } else { selectedId.value = null; feedback.value = match.message; }
}
watch(selected, node => {
    if (!node) return;
    coordinateConfirmed.value = false;
    selectedCandidate.value = null;
    // Never automatically fall back to screen coordinates when all selectors are ambiguous.
    mode.value = (['resource_id', 'description', 'text', 'composite'] as const)
        .find(key => modeOptions.value.some(option => option.value === key && option.unique)) ?? 'resource_id';
});
function chooseCandidate(id: string) {
    const candidate = locatorCandidates.value.find(item => item.id === id && item.unique);
    if (!candidate || !snapshot.value || selectedId.value === null
        || !locatorIsUnique(snapshot.value, selectedId.value, candidate.selector)) return;
    const keys = Object.keys(candidate.selector);
    coordinateConfirmed.value = false;
    mode.value = keys.length === 1 && ['resource_id', 'text', 'description'].includes(keys[0])
        ? keys[0] as LocatorMode : 'composite';
    selectedCandidate.value = id;
}
function locator(): AutomationSelector | null {
    // AUT-10: native locators are only provided by the explicitly confirmed,
    // currently unique candidate. Coordinates are a separate manual fallback.
    return mode.value === 'coordinates' ? null : confirmedLocator.value;
}
function replaceTarget() {
    if (!canReplace.value || !selected.value || !snapshot.value || !props.replaceStep) return;
    const selector = locator();
    let target: StepTarget;
    if (selector) {
        target = { selector: { ...selector } };
    } else {
        if (mode.value !== 'coordinates' || !compatibleVideo.value) return;
        const bounds = selected.value.bounds;
        const { width, height } = snapshot.value;
        target = {
            coordinates: {
                x: (bounds.left + bounds.right) / (2 * width),
                y: (bounds.top + bounds.bottom) / (2 * height),
                width, height,
            },
        };
    }
    // The controller checks the owner, script identity and step type again.
    emit('replace-target', props.replaceStep.id, target);
}
function add(type: InspectorAction) {
    if (!selected.value || !snapshot.value || !canAdd.value) return;
    const selector = locator();
    if (!selector && type !== 'tap' && type !== 'long_press') return;
    if (type === 'input_text' && selector?.xpath) return;
    if (type === 'assert_property' && selector?.xpath) return;
    if (type === 'scroll_find' && (!selector || !scrollContainer.value)) return;
    const step: AutomationStep = { id: uuid(), type };
    if (selector) step.selector = selector;
    else {
        const bounds = selected.value.bounds, { width, height } = snapshot.value;
        step.x = (bounds.left + bounds.right) / 2 / width;
        step.y = (bounds.top + bounds.bottom) / 2 / height;
        step.width = width; step.height = height;
    }
    if (type === 'long_press') step.duration_ms = 800;
    if (['wait_for', 'wait_gone', 'assert'].includes(type)) step.timeout_ms = 10000;
    if (type === 'assert_property') { step.property = 'enabled'; step.expected = selected.value.enabled; }
    if (type === 'scroll_find') {
        step.scroll_container = scrollContainer.value ?? undefined;
        step.direction = 'down'; step.max_swipes = 3; step.duration_ms = 350;
        step.timeout_ms = 20000;
    }
    if (type === 'input_text') step.parameter = 'picked_' + uuid().slice(0, 8);
    emit('add-step', step);
    feedback.value = '已添加到脚本草稿：' + ({ tap: '点击', long_press: '长按', wait_for: '等待控件', wait_gone: '等待消失', assert: '检查控件',
        assert_property: '断言属性', scroll_find: '滚动查找', input_text: '输入文字' }[type]);
}
watch([() => props.visible, () => props.device.id, () => props.device.target_fingerprint, () => props.ownerId], async () => {
    ++requestGeneration;
    initialPointUsed = false;
    response.value = null; selectedId.value = null; selectedCandidate.value = null;
    coordinateConfirmed.value = false; collapsedIds.value = []; loading.value = false;
    dragPickMode.value = false; dragStart.value = null; dragEnd.value = null;
    error.value = ''; feedback.value = ''; videoSize.value = { width: 0, height: 0 };
    await nextTick();
    setVideo();
    if (props.visible) void refresh();
}, { immediate: true });
watch(() => props.stream, async () => {
    coordinateConfirmed.value = false;
    videoSize.value = { width: 0, height: 0 };
    await nextTick(); setVideo();
});
watch([() => props.replaceStep?.id, () => props.replaceStep?.type], () => {
    selectedCandidate.value = null;
    coordinateConfirmed.value = false;
});
</script>

<template>
    <Dialog :visible="visible" modal append-to="body" :header="device.name + (replaceStep ? ' · 更新已有步骤控件' : ' · UiAutomator2 控件拾取')"
        :style="{ width: '1160px', maxWidth: 'calc(100vw - 24px)' }" :content-style="{ padding: '0' }"
        @show="setVideo"
        @update:visible="value => { if (!value) emit('close'); }">
        <div class="ui-inspector">
            <div class="inspector-toolbar">
                <div class="status">
                    <Tag :value="loading ? '正在读取' : snapshot ? '控件树已加载' : '未加载'" :severity="snapshot ? 'success' : 'secondary'" />
                    <span v-if="capturedAt">层级采集时间：{{ capturedAt }}</span>
                    <span v-if="stream">视频是实时流，控件树是独立快照；切换页面后请重新读取。</span>
                    <span v-else>当前使用独立控件树检查模式；不需要打开实时投屏。</span>
                </div>
                <Button label="刷新控件树" icon="pi pi-refresh" outlined size="small" :loading="loading" @click="refresh" />
            </div>
            <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
            <Message v-if="snapshot?.truncated" severity="warn" :closable="false">当前页面控件过多，已截断。为防止错误定位，精确控件步骤暂不可添加。</Message>
            <Message v-if="replaceStep" severity="info" :closable="false">
                正在更新已存在的 {{ replaceStep.type }} 步骤：只替换控件定位，不修改动作类型、步骤编号、按住时长、输入参数或条件分支。
                需选择当前控件树中唯一匹配的定位条件；执行时仍会再次检查唯一性。
            </Message>
            <div class="inspector-columns" :class="{ 'tree-only': !stream }">
                <section v-if="stream" class="preview-panel" aria-label="手机画面拾取">
                    <div class="preview-heading"><strong>手机实时画面</strong><small>{{ snapshot ? snapshot.width + ' × ' + snapshot.height : '等待控件树' }}</small></div>
                    <div class="video-well">
                        <div ref="surface" class="video-surface" :style="{ aspectRatio: snapshot ? snapshot.width + ' / ' + snapshot.height : '9 / 19', width: snapshot ? 'min(100%, ' + (snapshot.width / snapshot.height * 58) + 'vh)' : 'min(100%, 27.5vh)' }">
                            <video ref="video" autoplay muted playsinline @loadedmetadata="videoMetadata" @resize="videoMetadata" />
                            <button v-if="snapshot && compatibleVideo" type="button" class="pick-layer"
                                :title="dragPickMode ? '点击选择拖拽起点或终点（不会操作手机）' : '点击选取控件（不会操作手机）'"
                                :aria-label="dragPickMode ? '选择拖拽起点或终点' : '点击画面选择控件'" @click="pick" />
                            <div v-if="nodeHighlight" class="node-highlight" :style="nodeHighlight" />
                            <svg v-if="dragPickMode && dragStart && dragEnd" class="drag-guide" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
                                <line :x1="dragStart.x * 100" :y1="dragStart.y * 100" :x2="dragEnd.x * 100" :y2="dragEnd.y * 100" />
                            </svg>
                            <span v-if="dragPickMode && dragStart" class="drag-marker" :style="dragStyle(dragStart)">A</span>
                            <span v-if="dragPickMode && dragEnd" class="drag-marker drag-end" :style="dragStyle(dragEnd)">B</span>
                        </div>
                    </div>
                    <div class="drag-pick-controls">
                        <Button :label="dragPickMode ? '重选起点 / 终点' : '在画面上拾取拖拽'" icon="pi pi-arrows-h"
                            outlined size="small" :disabled="!snapshot || !compatibleVideo || queuedCount >= 100" @click="startDragPick" />
                        <Button v-if="dragPickMode" label="添加拖拽步骤" size="small" :disabled="!dragReady || queuedCount >= 100" @click="saveDrag" />
                        <Button v-if="dragPickMode" label="取消" text size="small" @click="cancelDragPick" />
                    </div>
                    <small v-if="dragPickMode" class="hint">A 为起点，B 为终点；至少相距 16 像素。仅添加草稿，不会立即拖动手机。</small>
                    <small v-if="snapshot && !                <InspectorPropertiesPanel
                    :selected="selected" :selected-id="selectedId" :visible-nodes="visibleNodes"
                    :node-count="snapshot?.nodes.length ?? 0" :snapshot-loaded="!!snapshot" :parent-ids="parentIds"
                    :collapsed-ids="collapsedIds" :search="search" :loading="loading"
                    :lineage="lineage" :clickable-parent="clickableParent"
                    :locator-candidates="locatorCandidates" :selected-candidate="selectedCandidate"
                    :mode="mode" :mode-options="modeOptions" :active-mode="activeMode"
                    :coordinate-confirmed="coordinateConfirmed" :coordinates-allowed="coordinatesAllowed"
                    :confirmed-locator="confirmedLocator" :suggested-xpath="suggestedXPath"
                    :snapshot-truncated="!!snapshot?.truncated" :can-add="canAdd" :can-replace="canReplace"
                    :scroll-container="scrollContainer" :replace-step="replaceStep"
                    :preferred-action="preferredAction"
                    @update-search="search = $event" @toggle-branch="toggleBranch"
                    @select-node="selectNode" @choose-candidate="chooseCandidate"
                    @change-mode="changeLocatorMode" @confirm-coordinates="confirmCoordinates"
                    @replace-target="replaceTarget" @add-step="add"
                />
            </div>
            <div class="inspector-footer">
                <span role="status">{{ feedback || (replaceStep ? '重新拾取只修改已有步骤，不会操作手机或新增步骤。' : '脚本步骤只会加入草稿，不会立即执行。') }}</span>
                <div class="footer-actions">
                    <strong v-if="!replaceStep">草稿 {{ queuedCount }}/100 步</strong>
                    <strong v-else>更新步骤 {{ replaceStep.id.slice(0, 8) }}</strong>
                    <Button label="关闭" text size="small" @click="emit('close')" />
                    <Button v-if="!replaceStep" label="去编辑脚本" icon="pi pi-arrow-right" icon-pos="right" size="small" :disabled="queuedCount === 0" @click="emit('edit')" />
                </div>
            </div>
        </div>
    </Dialog>
</template>

<style scoped>
.ui-inspector{display:flex;flex-direction:column;gap:12px;padding:14px;color:var(--p-text-color)}
.inspector-toolbar,.inspector-footer,.pane-heading,.preview-heading{display:flex;align-items:center;justify-content:space-between;gap:10px}
.inspector-toolbar .status{display:flex;align-items:center;gap:10px;flex-wrap:wrap;font-size:12px;color:var(--console-muted)}
.inspector-columns{display:grid;grid-template-columns:minmax(210px, .9fr) minmax(220px,1fr) minmax(235px,1fr);gap:12px;min-width:0}
.inspector-columns.tree-only{grid-template-columns:minmax(240px,1fr) minmax(260px,1fr)}
.inspector-columns>section{min-width:0;border:1px solid var(--console-border);border-radius:10px;padding:12px;background:var(--console-panel);display:flex;flex-direction:column;gap:11px}
.pane-heading,.preview-heading{min-height:26px;font-size:13px}
.pane-heading small,.preview-heading small,.hint{font-size:11px;color:var(--console-muted);line-height:1.6}
.video-well{display:flex;align-items:center;justify-content:center;min-height:365px;background:#0b1018;border-radius:9px;padding:8px}
.video-surface{width:100%;max-width:340px;max-height:58vh;position:relative;overflow:hidden;background:#05070b;border-radius:7px;box-shadow:0 3px 18px #0007}
.video-surface video{position:absolute;inset:0;width:100%;height:100%;object-fit:contain;pointer-events:none}
.pick-layer{position:absolute;inset:0;z-index:2;background:transparent;border:0;cursor:crosshair}
.drag-pick-controls{display:flex;align-items:center;gap:6px;flex-wrap:wrap}
.drag-guide{position:absolute;inset:0;width:100%;height:100%;z-index:3;pointer-events:none;overflow:visible}
.drag-guide line{stroke:#f59e0b;stroke-width:1.1;stroke-dasharray:2.5 1}
.drag-marker{position:absolute;z-index:4;transform:translate(-50%,-50%);height:20px;width:20px;display:grid;place-items:center;border:2px solid white;border-radius:50%;background:#2563eb;color:white;font-size:11px;font-weight:700;pointer-events:none;box-shadow:0 2px 8px #0008}
.drag-marker.drag-end{background:#c2410c}
.node-highlight{position:absolute;z-index:3;border:2px solid #31c48d;background:#31c48d26;pointer-events:none;box-shadow:0 0 0 1px #000a}
.tree-panel :deep(input){width:100%}
.node-list{max-height:480px;min-height:265px;overflow:auto;border-radius:8px;border:1px solid var(--console-border);background:var(--console-panel-raised)}
.tree-item{display:flex;align-items:center;border-bottom:1px solid var(--console-border);min-width:0}
.branch-toggle,.branch-spacer{width:22px;min-width:22px;height:28px;display:grid;place-items:center;border:0;background:transparent;color:var(--console-muted)}
.branch-toggle{cursor:pointer}.branch-toggle:hover:not(:disabled){color:var(--p-primary-color)}.branch-toggle:disabled{opacity:.5;cursor:default}
.branch-toggle i{font-size:10px}
.node-row{flex:1;min-width:0;display:flex;align-items:center;gap:9px;text-align:left;background:transparent;border:0;padding:8px;color:var(--p-text-color);cursor:pointer}
.node-row:hover,.node-row.active{background:var(--console-accent-soft)}.node-row.active{box-shadow:inset 3px 0 var(--p-primary-color)}
.node-row i{font-size:7px;color:var(--console-muted)}.node-row span{min-width:0;display:grid;gap:3px}
.node-row strong{font-size:12px;font-weight:550;white-space:nowrap;text-overflow:ellipsis;overflow:hidden}
.node-row small{font-size:10px;color:var(--console-muted)}
.empty-tree,.empty-detail{padding:25px 10px;color:var(--console-muted);font-size:12px;line-height:1.7}
.ancestry{display:grid;gap:7px}.ancestry>span{font-size:11px;color:var(--console-muted)}.ancestry-nodes{display:flex;gap:4px;flex-wrap:wrap}
.ancestry-nodes button{max-width:100%;overflow-wrap:anywhere;border:1px solid var(--console-border);border-radius:6px;padding:4px 6px;font-size:10px;background:var(--console-panel-raised);color:var(--p-text-color);cursor:pointer}
.ancestry-nodes button.current{border-color:var(--p-primary-color);background:var(--console-accent-soft)}
.coordinate-confirm{display:flex;gap:8px;align-items:center;flex-wrap:wrap}
.parent-recommendation{padding:9px;border:1px solid var(--console-border);border-radius:8px;background:var(--console-accent-soft);display:grid;gap:4px;font-size:11px;line-height:1.6}
.parent-recommendation :deep(.p-button){justify-self:start}
.property{display:grid;gap:4px;padding:5px 0;border-bottom:1px solid var(--console-border)}
.property span,.locator-label{font-size:11px;color:var(--console-muted)}
.property code{font-family:ui-monospace,monospace;font-size:11px;overflow-wrap:anywhere;white-space:normal}
.locator-select{width:100%;border:1px solid var(--console-border-strong);background:var(--console-panel-raised);color:var(--p-text-color);padding:9px;border-radius:8px}
.locator-recommendations{display:grid;gap:6px;border:1px solid var(--console-border);border-radius:8px;padding:8px}
.locator-recommendations strong{font-size:12px}
.locator-recommendations small{font-size:11px;color:var(--console-muted)}
.candidate-row{display:flex;justify-content:space-between;gap:8px;text-align:left;border:1px solid var(--console-border);border-radius:6px;padding:7px;background:var(--console-panel-raised);color:var(--p-text-color);cursor:pointer;font-size:11px}
.candidate-row.chosen{border-color:var(--p-primary-color);background:var(--console-accent-soft)}
.candidate-row:disabled{opacity:.45;cursor:not-allowed}
.step-actions{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px;margin-top:5px}
.step-actions :deep(.p-button){justify-content:center}
.inspector-footer{border-top:1px solid var(--console-border);padding-top:12px;font-size:12px;color:var(--console-muted);flex-wrap:wrap}
.footer-actions{display:flex;align-items:center;gap:9px}
@media(max-width:950px){.inspector-columns{grid-template-columns:minmax(220px,1fr) minmax(250px,1fr)}.details-panel{grid-column:1/-1}}
@media(max-width:580px){.inspector-columns{display:flex;flex-direction:column}.video-well{min-height:300px}.node-list{max-height:240px;min-height:130px}.inspector-footer,.footer-actions{align-items:flex-start;flex-wrap:wrap}}
</style>
