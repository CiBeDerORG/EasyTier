<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { Button, Message, Tag } from 'primevue';
import AutomationFloatingDialog from './AutomationFloatingDialog.vue';
import type { InspectorCardHighlight, InspectorCardDragGuide, InspectorCardScreenPoint } from '../../modules/automation-inspector-overlay';
import { v4 as uuid } from 'uuid';
import AutomationApi, { automationErrorMessage } from '../../modules/automation-api';
import type { AutomationDevice, AutomationHierarchy, AutomationHierarchyNode, AutomationSelector, AutomationStep } from '../../types/automation';
import type { StepTarget } from '../../modules/automation-step-target';
import { locatorIsUnique, rankLocatorCandidates } from '../../modules/automation-locator-candidates';
import { rankRelationalCandidates } from '../../modules/automation-relational-locator';
import { buildFilterOptions, filterHierarchy, buildCrossFilterSelector, type FilterOption, type FilterField } from '../../modules/automation-cross-filter';
import InspectorPropertiesPanel from './InspectorPropertiesPanel.vue';
import type { InspectorAction } from '../../modules/automation-inspector-actions';
import { matchPickerPoint } from '../../modules/automation-pick-engine';
import { revalidatePickedNativeTarget } from '../../modules/automation-inspector-revalidation';
import type { PickerVideoPoint } from '../../modules/automation-picker-state';

const props = defineProps<{
    visible: boolean; device: AutomationDevice; ownerId: number;
    queuedCount: number;
    portalTarget: HTMLElement | null; fullscreen: boolean;
    /** Original PhoneCard primary video native pixels; no secondary video is mounted. */
    screenWidth: number; screenHeight: number;
    screenPoint?: InspectorCardScreenPoint | null;
    replaceStep?: Pick<AutomationStep, 'id' | 'type'> | null;
    /** Original card's right-click position. Null keeps the legacy manual picker. */
    initialPoint?: PickerVideoPoint | null;
    preferredAction?: InspectorAction | null;
    /** Unique token for the dialog opening; prevents stale/double commits. */
    commitToken: number;
}>();
const emit = defineEmits<{
    close: []; 'add-step': [step: AutomationStep, token: number, acknowledge: (accepted: boolean) => void]; edit: [];
    'replace-target': [stepId: string, target: StepTarget];
    highlight: [rect: InspectorCardHighlight | null];
    'drag-mode': [active: boolean];
    'drag-guide': [guide: InspectorCardDragGuide | null];
}>();
const api = new AutomationApi();
const response = ref<AutomationHierarchy | null>(null);
const selectedId = ref<number | null>(null);
const collapsedIds = ref<number[]>([]);
const search = ref('');
type LocatorMode = 'resource_id' | 'text' | 'description' | 'composite' | 'xpath' | 'coordinates';
const mode = ref<LocatorMode>('composite');
const loading = ref(false);
const committing = ref(false);
const validatingSubmission = ref(false);
function submitStep(step: AutomationStep) {
    if (committing.value) return;
    committing.value = true;
    emit('add-step', step, props.commitToken, accepted => {
        if (!accepted) {
            committing.value = false;
            feedback.value = '步骤未录入：请查看错误信息，确认后可以重试。';
        }
    });
}
const error = ref('');
const feedback = ref('');
const videoSize = computed(() => ({ width: props.screenWidth, height: props.screenHeight }));
type Point = { x: number; y: number };
const dragPickMode = ref(false);
const dragStart = ref<Point | null>(null);
const dragEnd = ref<Point | null>(null);
const dragReady = computed(() => !!snapshot.value && compatibleVideo.value && !!dragStart.value && !!dragEnd.value
    && Math.hypot((dragEnd.value.x - dragStart.value.x) * snapshot.value.width,
        (dragEnd.value.y - dragStart.value.y) * snapshot.value.height) >= 16);
let requestGeneration = 0;
let initialPointUsed = false;

const snapshot = computed(() => response.value?.snapshot);
const nodeIndex = computed(() => new Map((snapshot.value?.nodes ?? []).map(node => [node.id, node] as const)));
const parentIds = computed(() => new Set((snapshot.value?.nodes ?? [])
    .map(node => node.parent_id).filter((value): value is number => value !== null)));
const selected = computed(() => selectedId.value === null ? null : nodeIndex.value.get(selectedId.value) ?? null);
// These are the actual user-chosen AND conditions, not a pre-generated 1/2/3-field candidate.
const activeFilterIds = ref<string[]>([]);
const extraFilterOptions = ref<FilterOption[]>([]);
const filterOptions = computed(() => snapshot.value && selectedId.value !== null
    ? [...buildFilterOptions(snapshot.value,selectedId.value),...extraFilterOptions.value] : []);
const filteredNodes = computed(() => snapshot.value
    ? filterHierarchy(snapshot.value,filterOptions.value,activeFilterIds.value) : []);
const crossFilterBuild = computed(() => buildCrossFilterSelector(filterOptions.value,activeFilterIds.value));
const crossFilterUnique = computed(() => selectedId.value !== null && !snapshot.value?.truncated
    && filteredNodes.value.length === 1 && filteredNodes.value[0]?.id === selectedId.value);
const previewNodeId = ref<number | null>(null);
const manualAnchorId = ref<number | null>(null);
const anchorPicking = ref(false);
const verifiedXPath = ref<string | null>(null);
const verifying = ref(false);
let verificationGeneration = 0;
const manualAnchorName = computed(() => {
    const node = manualAnchorId.value === null ? null : nodeIndex.value.get(manualAnchorId.value);
    return node ? (node.password ? '' : node.text || node.description || node.resource_id || node.class_name) : '';
});
const locatorCandidates = computed(() => {
    if (!snapshot.value || selectedId.value === null) return [];
    const native = rankLocatorCandidates(snapshot.value, selectedId.value);
    const relational = rankRelationalCandidates(snapshot.value, selectedId.value, manualAnchorId.value);
    return [...native, ...relational].sort((a,b) =>
        Number(b.unique)-Number(a.unique) || b.score-a.score || a.id.localeCompare(b.id));
});
const selectedCandidate = ref<string | null>(null);
// Coordinates are a deliberate, separately confirmed fallback, never a default.
const coordinateConfirmed = ref(false);
const confirmedLocator = computed<AutomationSelector | null>(() => {
    if (activeFilterIds.value.length) {
        if (!crossFilterUnique.value || !crossFilterBuild.value.selector
            || mode.value === 'coordinates') return null;
        const filtered = crossFilterBuild.value.selector;
        if (filtered.xpath) {
            return verifiedXPath.value === filtered.xpath && mode.value === 'xpath'
                && !verifying.value ? filtered : null;
        }
        return snapshot.value && selectedId.value !== null
            && locatorIsUnique(snapshot.value, selectedId.value, filtered) ? filtered : null;
    }
    const candidate = locatorCandidates.value.find(item => item.id === selectedCandidate.value && item.unique);
    if (!candidate || !snapshot.value || selectedId.value === null) return null;
    if (candidate.selector.xpath) {
        return verifiedXPath.value === candidate.selector.xpath && mode.value === 'xpath'
            && !verifying.value ? candidate.selector : null;
    }
    return locatorIsUnique(snapshot.value, selectedId.value, candidate.selector) ? candidate.selector : null;
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
// WebRTC's encoded dimensions can be scaled (e.g. 432×960) while the
// UiAutomator2 hierarchy uses Android's native 1080×2400 screen. Match the
// displayed aspect/orientation, then store all coordinates in snapshot space.
const coordinatesAllowed = computed(() => compatibleVideo.value && coordinateBoundsValid.value
    && response.value?.target_fingerprint === props.device.target_fingerprint);
function confirmCoordinates() {
    if (mode.value !== 'coordinates' || !coordinatesAllowed.value || !snapshotFresh()) {
        feedback.value='原控件树已过期，请刷新后重新确认坐标定位。';
        return;
    }
    coordinateConfirmed.value = true;
    feedback.value = '已手动确认使用坐标定位；画面旋转、控件变更或刷新控件树后需要重新确认。';
}
function onLocatorModeChange() {
    ++verificationGeneration; verifiedXPath.value = null; verifying.value = false;
    selectedCandidate.value = null;
    coordinateConfirmed.value = false;
}
function changeLocatorMode(value: LocatorMode) {
    // Only the explicitly selected coordinate fallback may bypass the AND
    // filters. No single-property mode is exposed to conflict with the UI.
    if (value !== 'coordinates' && value !== 'composite') return;
    mode.value = value === 'composite' && crossFilterBuild.value.selector?.xpath ? 'xpath' : value;
    onLocatorModeChange();
}
function addParentFilter(nodeId:number,field:FilterField) {
    const node=nodeIndex.value.get(nodeId);
    if (!snapshot.value || !selected.value || !node || node.parent_id===null || !snapshotFresh()) return;
    const parent=nodeIndex.value.get(node.parent_id);
    if(!parent || parent.password) {
        feedback.value='父级不可用或含敏感内容，不能作为定位筛选条件。';return;
    }
    const distance=node.depth-parent.depth;
    if(distance<1 || distance>8)return;
    const value=field==='bounds' ? parent.raw_bounds : parent[field];
    if ((typeof value !== 'boolean' && (typeof value!=='string' || !value.trim()
        || new TextEncoder().encode(value).length>256)) || value===null) return;
    const old=filterOptions.value.find(opt=>opt.scope==='ancestor'
        && opt.distance===distance && opt.field===field && opt.value===value);
    const id=old?.id ?? 'parent:'+distance+':'+field+':'+String(value);
    if(!old && !extraFilterOptions.value.some(opt=>opt.id===id))
        extraFilterOptions.value=[...extraFilterOptions.value,{
            id,field,scope:'ancestor',distance,nodeId:parent.id,
            label:'父级'+field,value,displayValue:String(value),
        }];
    toggleFilter(id,true);
    if(!crossFilterUnique.value)
        feedback.value='已加入父级条件；当前仍匹配 '+filteredNodes.value.length+' 个，只有原目标唯一时才能录入。';
}
function toggleFilter(id:string, checked:boolean) {
    if (!filterOptions.value.some(x=>x.id===id) || !selected.value) return;
    const next=new Set(activeFilterIds.value);
    if (checked) next.add(id); else next.delete(id);
    ++verificationGeneration;verifiedXPath.value=null;verifying.value=false;
    selectedCandidate.value=null;coordinateConfirmed.value=false;previewNodeId.value=null;
    activeFilterIds.value=[...next];
    mode.value=crossFilterBuild.value.selector?.xpath?'xpath':'composite';
    feedback.value = next.size
        ? '已更新交叉过滤：当前匹配 '+filteredNodes.value.length+' 个目标。'
        : '已清除筛选条件，请勾选需要组合的属性。';
}
async function verifyFilter() {
    const shot=snapshot.value,node=selected.value,selector=crossFilterBuild.value.selector;
    if (!shot || !node || !selector?.xpath || !crossFilterUnique.value) {
        feedback.value='请先将过滤结果缩小至当前选中的唯一控件。';
        return;
    }
    ++verificationGeneration;const generation=verificationGeneration;
    verifiedXPath.value=null;verifying.value=true;mode.value='xpath';
    feedback.value='正在从手机重新采集 XML 并验证交叉过滤条件（只读）。';
    try {
        const result=await api.verifyLocator(props.ownerId,props.device.id,{
            xpath:selector.xpath,expected_bounds:node.bounds,width:shot.width,height:shot.height,
        });
        if(generation!==verificationGeneration || !props.visible || selectedId.value!==node.id
            || crossFilterBuild.value.selector?.xpath!==selector.xpath) return;
        if(!result.verified || result.device_id!==props.device.id
            || result.target_fingerprint!==props.device.target_fingerprint) {
            feedback.value='验证未命中原目标或页面发生变化；请重新采集后调整过滤条件。';
            return;
        }
        verifiedXPath.value=selector.xpath;
        feedback.value='交叉过滤已通过执行器验证：唯一匹配原目标，可以配置操作。';
    } catch(cause) {
        if(generation===verificationGeneration) feedback.value='过滤条件验证失败：'+automationErrorMessage(cause);
    } finally {
        if(generation===verificationGeneration) verifying.value=false;
    }
}
function restoreOriginalHighlight() { previewNodeId.value=null; }
function previewMatch(id:number) {
    if(!snapshot.value || !nodeIndex.value.has(id) || snapshot.value.truncated)return;
    previewNodeId.value=id;
}
function focusParent(id:number) {
    // Inspect/highlight parent without replacing the original action target.
    previewMatch(id);
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
const activeMode = computed(() => ({
    value: mode.value,
    label: mode.value==='coordinates' ? '画面坐标' : mode.value==='xpath' ? '层级 XPath' : '交叉过滤',
    unique: mode.value==='coordinates'
        ? coordinatesAllowed.value && coordinateConfirmed.value
        : !!confirmedLocator.value,
}));
const canAdd = computed(() => !props.replaceStep && !committing.value && !validatingSubmission.value && !!selected.value
    && !!snapshot.value && !snapshot.value.truncated
    && response.value?.target_fingerprint === props.device.target_fingerprint
    && (mode.value === 'coordinates' ? coordinatesAllowed.value && coordinateConfirmed.value : !!confirmedLocator.value)
    && props.queuedCount < 100);
const canReplace = computed(() => {
    if (!props.replaceStep || validatingSubmission.value || !selected.value || !snapshot.value || snapshot.value.truncated
        || (mode.value === 'coordinates' ? !coordinatesAllowed.value || !coordinateConfirmed.value : !confirmedLocator.value)
        || response.value?.target_fingerprint !== props.device.target_fingerprint) return false;
    const type = props.replaceStep.type;
    if (mode.value === 'coordinates') return (type === 'tap' || type === 'long_press') && compatibleVideo.value;
    if (mode.value === 'xpath' && (type === 'input_text' || type === 'assert_property')) return false;
    return true;
});
const nodeHighlight = computed<InspectorCardHighlight | null>(() => {
    const node = previewNodeId.value === null ? selected.value : nodeIndex.value.get(previewNodeId.value), shot = snapshot.value;
    if (!node || !shot || !compatibleVideo.value || dragPickMode.value || shot.truncated
        || response.value?.target_fingerprint !== props.device.target_fingerprint) return null;
    return { ...node.bounds, sourceWidth: shot.width, sourceHeight: shot.height };
});
const dragGuide = computed<InspectorCardDragGuide | null>(() => dragPickMode.value && compatibleVideo.value
    ? { start: dragStart.value, end: dragEnd.value } : null);
watch(nodeHighlight, value => emit('highlight', value), { immediate: true });
watch(dragPickMode, value => emit('drag-mode', value), { immediate: true });
watch(dragGuide, value => emit('drag-guide', value), { immediate: true });
function selectNode(id: number) {
    if (anchorPicking.value) {
        if (id === selectedId.value) { feedback.value = '关联锚点必须与操作目标不同。'; return; }
        if (!nodeIndex.value.has(id) || nodeIndex.value.get(id)?.password) {
            feedback.value = '不能使用无效节点或敏感输入框作为关联锚点。'; return;
        }
        manualAnchorId.value = id; anchorPicking.value = false;
        onLocatorModeChange();
        feedback.value = '已指定关联锚点，操作目标保持不变。请选择自动生成的关联定位候选。';
        return;
    }
    manualAnchorId.value = null;
    activeFilterIds.value=[];extraFilterOptions.value=[];previewNodeId.value=null;
    onLocatorModeChange();
    selectedId.value = id; feedback.value = '';
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
async function refresh() {
    if (!props.visible || loading.value) return;
    const generation = ++requestGeneration;
    const pointHint = !initialPointUsed ? props.initialPoint : null;
    initialPointUsed = true;
    ++verificationGeneration; verifiedXPath.value = null; verifying.value = false;
    manualAnchorId.value = null; anchorPicking.value = false;
    response.value = null; selectedId.value = null; selectedCandidate.value = null; coordinateConfirmed.value = false; collapsedIds.value = [];
    activeFilterIds.value=[];previewNodeId.value=null;
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
/** UIA snapshots expire; never record an unverified gesture from stale pixels. */
function snapshotFresh():boolean {
    if(!response.value?.captured_at || !snapshot.value || snapshot.value.truncated ||
        response.value.target_fingerprint!==props.device.target_fingerprint)return false;
    const captured=Date.parse(response.value.captured_at), age=Date.now()-captured;
    return Number.isFinite(captured) && age>=-5000 && age<=20000;
}
function startDragPick() {
    if (props.replaceStep || !snapshotFresh()) {
        feedback.value='控件树已过期或设备绑定变化，请先刷新控件树。';
        return;
    }
    if (!snapshot.value || !compatibleVideo.value) {
        feedback.value='原手机画面与控件树方向/宽高比不一致，请刷新后再进行只读拖拽拾取。';
        return;
    }
    dragPickMode.value = true; dragStart.value = null; dragEnd.value = null;
    selectedId.value = null; feedback.value = '请先点击画面选择拖拽起点，再点击画面选择终点（不会操作手机）。';
}
function cancelDragPick() {
    dragPickMode.value = false; dragStart.value = null; dragEnd.value = null;
    feedback.value = '';
}
function saveDrag() {
    if (props.replaceStep || committing.value || !dragReady.value || !snapshot.value || !dragStart.value || !dragEnd.value ||
        !compatibleVideo.value || props.queuedCount>=100 || !snapshotFresh()) {
        feedback.value='原控件树已过期或屏幕方向变化，请重新刷新控件树和拖拽起终点。';
        return;
    }
    submitStep({
        id: uuid(), type: 'drag',
        x: dragStart.value.x, y: dragStart.value.y, to_x: dragEnd.value.x, to_y: dragEnd.value.y,
        width: snapshot.value.width, height: snapshot.value.height, duration_ms: 700,
    });
    // The parent closes this dialog only after a confirmed append. If
    // rejected, keep both drag points for a retry instead of claiming success.
}
/** Coordinates are sourced exclusively from the original PhoneCard primary video. */
function pickFromOriginal(point: InspectorCardScreenPoint) {
    const shot=snapshot.value;
    if (!props.visible || !shot || !compatibleVideo.value || shot.truncated ||
        point.width!==props.screenWidth || point.height!==props.screenHeight ||
        response.value?.target_fingerprint!==props.device.target_fingerprint ||
        point.x<0 || point.y<0 || point.x>=point.width || point.y>=point.height) {
        feedback.value='原画面的方向/宽高比或控件树快照已变化，请刷新控件树后重新选择。';
        return;
    }
    // Keep original encoded dimensions for matchPickerPoint: it already
    // performs safe aspect-ratio scaling into native UIA pixel coordinates.
    const normX=point.x/point.width, normY=point.y/point.height;
    if (dragPickMode.value) {
        if(!snapshotFresh()){
            feedback.value='控件树已过期，拖拽拾取无效，请刷新后重新选择。';
            dragStart.value=null;dragEnd.value=null;return;
        }
        if (!dragStart.value || dragEnd.value) {dragStart.value={x:normX,y:normY};dragEnd.value=null;}
        else dragEnd.value={x:normX,y:normY};
        feedback.value=dragEnd.value ? '拖拽起终点已记录在原画面，确认后添加草稿步骤。'
            : '拖拽起点已记录，请继续在原画面点击终点。';
        return;
    }
    const match=matchPickerPoint(response.value!,{
        x:point.x,y:point.y,width:point.width,height:point.height,
    },{targetFingerprint:props.device.target_fingerprint??''});
    if(match.status==='matched' && match.nodeId!==null) {
        selectNode(match.nodeId);
        if (!anchorPicking.value && manualAnchorId.value === null) feedback.value=match.message;
    } else {
        if (!anchorPicking.value) selectedId.value=null;
        feedback.value=match.message;
    }
}
watch(()=>props.screenPoint, point=>{if(point && props.visible)pickFromOriginal(point);});
watch(selected, node => {
    if (!node) return;
    coordinateConfirmed.value = false;
    verifiedXPath.value = null;
    selectedCandidate.value = null;
    activeFilterIds.value=[];extraFilterOptions.value=[];previewNodeId.value=null;
    // Never automatically fall back to screen coordinates when all selectors are ambiguous.
    mode.value = 'composite';
});
function startAnchorPick() {
    if (!selected.value || !snapshotFresh()) {
        feedback.value = '请先选择操作目标，并刷新过期的控件树。'; return;
    }
    anchorPicking.value = true;
    feedback.value = '请在原实时画面或控件树中选择一个业务标题/标签作为关联锚点。原操作目标不会改变。';
}
function clearAnchor() {
    anchorPicking.value = false; manualAnchorId.value = null;
    onLocatorModeChange();
    feedback.value = '已清除手动锚点，恢复自动寻找关联定位。';
}
async function chooseCandidate(id: string) {
    const candidate = locatorCandidates.value.find(item => item.id === id && item.unique);
    if (!candidate || !snapshot.value || selectedId.value === null) return;
    activeFilterIds.value=[];extraFilterOptions.value=[];previewNodeId.value=null;
    const shot = snapshot.value, node = selected.value;
    if (!node) return;
    ++verificationGeneration; verifiedXPath.value = null; verifying.value = false;
    const generation = verificationGeneration;
    coordinateConfirmed.value = false;
    selectedCandidate.value = id;
    if (!candidate.selector.xpath) {
        if (!locatorIsUnique(shot, node.id, candidate.selector)) {selectedCandidate.value=null;return;}
        const keys = Object.keys(candidate.selector);
        mode.value = keys.length === 1 && ['resource_id', 'text', 'description'].includes(keys[0])
            ? keys[0] as LocatorMode : 'composite';
        return;
    }
    // Verify the generated relation on fresh RAW XML, not merely on the
    // simplified frontend tree. No device action is sent.
    mode.value = 'xpath'; verifying.value = true;
    feedback.value = '正在只读复核关联定位，不会操作手机……';
    try {
        const result = await api.verifyLocator(props.ownerId, props.device.id, {
            xpath: candidate.selector.xpath, expected_bounds: node.bounds,
            width: shot.width, height: shot.height,
        });
        if (generation !== verificationGeneration || !props.visible
            || selectedId.value !== node.id || selectedCandidate.value !== id) return;
        if (!result.verified || result.device_id !== props.device.id
            || result.target_fingerprint !== props.device.target_fingerprint) {
            feedback.value = '关联定位没有再次匹配选中目标，或页面已变化；请刷新控件树后重新选择。';
            return;
        }
        verifiedXPath.value = candidate.selector.xpath;
        feedback.value = '关联 XPath 已通过执行器只读验证：仅命中原选中的控件，可以录入。';
    } catch (cause) {
        if (generation === verificationGeneration)
            feedback.value = '关联定位验证失败：' + automationErrorMessage(cause);
    } finally {
        if (generation === verificationGeneration) verifying.value = false;
    }
}
function locator(): AutomationSelector | null {
    // AUT-10: native locators are only provided by the explicitly confirmed,
    // currently unique candidate. Coordinates are a separate manual fallback.
    return mode.value === 'coordinates' ? null : confirmedLocator.value;
}
/**
 * Perform a read-only, fresh device validation at submit time. The 20-second
 * snapshot TTL is for gestures only; spending time configuring filters must
 * not erase a previously selected business target.
 */
async function verifyBeforeCommit(
    original:AutomationHierarchy,originalNode:AutomationHierarchyNode,
    selector:AutomationSelector|null,
):Promise<boolean>{
    const deviceId=props.device.id,fingerprint=props.device.target_fingerprint;
    const owner=props.ownerId,commitToken=props.commitToken;
    const request=requestGeneration,verification=verificationGeneration;
    const originalMode=mode.value,originalFilters=activeFilterIds.value.join('|');
    const sameScope=()=>props.visible && props.device.id===deviceId
        && props.device.target_fingerprint===fingerprint && props.ownerId===owner
        && props.commitToken===commitToken && requestGeneration===request
        && verificationGeneration===verification && mode.value===originalMode
        && activeFilterIds.value.join('|')===originalFilters
        && selectedId.value===originalNode.id && response.value===original;
    try{
        let valid=false;
        if(selector?.xpath){
            const result=await api.verifyLocator(owner,deviceId,{
                xpath:selector.xpath,expected_bounds:originalNode.bounds,
                width:original.snapshot.width,height:original.snapshot.height,
            });
            valid=result.verified && result.device_id===deviceId
                && result.target_fingerprint===fingerprint;
        }else{
            const latest=await api.hierarchy(owner,deviceId);
            valid=latest.device_id===deviceId && latest.target_fingerprint===fingerprint
                && revalidatePickedNativeTarget(originalNode,original.snapshot.width,
                    original.snapshot.height,latest,selector);
        }
        if(!sameScope()) {
            feedback.value='验证期间设备或筛选条件发生变化，请再次确认录入。';
            return false;
        }
        if(!valid){
            feedback.value='重新读取控件树后无法确认原目标仍然唯一且位置一致。已保留筛选条件，请重新右键拾取目标。';
            return false;
        }
        return true;
    }catch(cause){
        if(sameScope()) feedback.value='重新验证控件失败，未录入步骤：'+automationErrorMessage(cause);
        return false;
    }
}
async function replaceTarget() {
    if (!canReplace.value || !selected.value || !response.value || !props.replaceStep
        || validatingSubmission.value)return;
    const original=response.value,node=selected.value,stepId=props.replaceStep.id;
    const selector=locator();
    const modeAtStart=mode.value;
    if(modeAtStart!=='coordinates'&&!selector)return;
    validatingSubmission.value=true;
    feedback.value='正在重新读取手机控件树，核对原目标后更新步骤……';
    try{
        if(!await verifyBeforeCommit(original,node,selector))return;
        if(!props.replaceStep || props.replaceStep.id!==stepId
            || mode.value!==modeAtStart || !props.visible)return;
        let target: StepTarget;
        if(selector){
            target={selector:{...selector}};
        }else{
            if(modeAtStart!=='coordinates'||!compatibleVideo.value)return;
            const bounds=node.bounds,{width,height}=original.snapshot;
            target={coordinates:{
                x:(bounds.left+bounds.right)/(2*width),
                y:(bounds.top+bounds.bottom)/(2*height),width,height,
            }};
        }
        emit('replace-target',stepId,target);
    }finally{validatingSubmission.value=false;}
}
async function add(type: InspectorAction) {
    if(validatingSubmission.value || committing.value || !selected.value
        || !response.value || !canAdd.value)return;
    const original=response.value,node=selected.value,selector=locator(),modeAtStart=mode.value;
    if(!selector && type!=='tap' && type!=='long_press')return;
    if(selector?.xpath && (type==='input_text'||type==='assert_property'))return;
    if(type==='scroll_find'&&(!selector||!scrollContainer.value))return;
    const scroller=scrollContainer.value;
    validatingSubmission.value=true;
    feedback.value='正在重新读取手机控件树，核对原目标后录入步骤……';
    try{
        if(!await verifyBeforeCommit(original,node,selector))return;
        if(mode.value!==modeAtStart || !props.visible)return;
        const step:AutomationStep={id:uuid(),type};
        if(selector)step.selector={...selector};
        else{
            const bounds=node.bounds,{width,height}=original.snapshot;
            step.x=(bounds.left+bounds.right)/(2*width);
            step.y=(bounds.top+bounds.bottom)/(2*height);
            step.width=width;step.height=height;
        }
        if(type==='long_press')step.duration_ms=800;
        if(['wait_for','wait_gone','assert'].includes(type))step.timeout_ms=10000;
        if(type==='assert_property'){step.property='enabled';step.expected=node.enabled;}
        if(type==='scroll_find'){
            if(!scroller)return;
            step.scroll_container=scroller;
            step.direction='down';step.max_swipes=3;step.duration_ms=350;
            step.timeout_ms=20000;
        }
        if(type==='input_text')step.parameter='picked_'+uuid().slice(0,8);
        submitStep(step);
    }finally{validatingSubmission.value=false;}
}
watch([() => props.visible, () => props.device.id, () => props.device.target_fingerprint, () => props.ownerId], async () => {
    ++requestGeneration;
    committing.value = false;
    validatingSubmission.value=false;
    initialPointUsed = false;
    ++verificationGeneration; verifiedXPath.value = null; verifying.value = false;
    anchorPicking.value = false; manualAnchorId.value = null;
    response.value = null; selectedId.value = null; selectedCandidate.value = null;
    activeFilterIds.value=[];extraFilterOptions.value=[];previewNodeId.value=null;
    coordinateConfirmed.value = false; collapsedIds.value = []; loading.value = false;
    dragPickMode.value = false; dragStart.value = null; dragEnd.value = null;
    error.value = ''; feedback.value = '';
    if (props.visible) void refresh();
}, { immediate: true });
watch([() => props.screenWidth, () => props.screenHeight], () => {
    coordinateConfirmed.value=false;
    ++verificationGeneration; verifiedXPath.value=null; verifying.value=false;
    anchorPicking.value=false; manualAnchorId.value=null;
    dragPickMode.value=false;dragStart.value=null;dragEnd.value=null;
    selectedId.value=null;selectedCandidate.value=null;
    activeFilterIds.value=[];extraFilterOptions.value=[];previewNodeId.value=null;
    response.value=null; ++requestGeneration; loading.value=false;
    if (props.visible) {
        feedback.value='原画面分辨率已变化，旧树快照作废，正在重新读取控件树。';
        if(props.screenWidth>0 && props.screenHeight>0)void refresh();
    }
});
watch([() => props.replaceStep?.id, () => props.replaceStep?.type], () => {
    onLocatorModeChange();
});
</script>

<template>
    <AutomationFloatingDialog :visible="visible" :portal-target="portalTarget"
        :fullscreen="fullscreen" :title="device.name + (replaceStep ? ' · 重新拾取控件' : ' · 控件树与属性')"
        @close="emit('close')">
        <div class="ui-inspector">
            <div v-if="!dragPickMode" class="inspector-toolbar">
                <div class="status">
                    <Tag :value="loading ? '正在读取' : snapshot ? '控件树已加载' : '未加载'" :severity="snapshot ? 'success' : 'secondary'" />
                    <span v-if="capturedAt">层级采集时间：{{ capturedAt }}</span>
                    <span>唯一实时画面在原手机卡片；当前窗口只显示 UiAutomator2 控件树快照。</span>
                </div>
                <Button label="刷新控件树" icon="pi pi-refresh" outlined size="small" :loading="loading" @click="refresh" />
            </div>
            <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
            <Message v-if="!dragPickMode && snapshot?.truncated" severity="warn" :closable="false">当前页面控件过多，已截断。为防止错误定位，精确控件步骤暂不可添加。</Message>
            <Message v-if="!dragPickMode && replaceStep" severity="info" :closable="false">
                正在更新已存在的 {{ replaceStep.type }} 步骤：只替换控件定位，不修改动作类型、步骤编号、按住时长、输入参数或条件分支。
                需选择当前控件树中唯一匹配的定位条件；执行时仍会再次检查唯一性。
            </Message>
            <div class="inspector-columns">
                <div class="original-screen-tools">
                    <p class="hint">{{ dragPickMode ? '拖拽只读拾取已开启。浮窗已缩小以露出原画面；如挡住目标，可拖动标题栏。' : '右键点击原手机画面选控件；树节点高亮也直接显示在原画面上，不另开预览。' }}</p>
                    <div class="drag-pick-controls">
                        <Button :label="dragPickMode ? '重新选择 A / B' : '原画面拖拽拾取'"
                            icon="pi pi-arrows-h" outlined size="small"
                            :disabled="!!replaceStep || !snapshot || !compatibleVideo || queuedCount>=100"
                            @click="startDragPick" />
                        <Button v-if="dragPickMode" label="添加拖拽步骤" size="small"
                            :disabled="!dragReady || queuedCount>=100" @click="saveDrag" />
                        <Button v-if="dragPickMode" label="取消拖拽拾取" text size="small" @click="cancelDragPick" />
                    </div>
                    <small v-if="dragPickMode" class="hint">在原视频上点 A 起点和 B 终点，至少相距 16 像素；不会发真实手机动作。</small>
                    <small v-if="snapshot && !compatibleVideo" class="hint">原视频方向或宽高比与控件树不匹配，请刷新或等待旋转完成；压缩编码尺寸不同属正常情况。</small>
                </div>
                <InspectorPropertiesPanel v-if="!dragPickMode"
                    :selected="selected" :selected-id="selectedId" :visible-nodes="visibleNodes"
                    :node-count="snapshot?.nodes.length ?? 0" :snapshot-loaded="!!snapshot" :parent-ids="parentIds"
                    :collapsed-ids="collapsedIds" :search="search" :loading="loading"
                    :lineage="lineage" :clickable-parent="clickableParent"
                    :locator-candidates="locatorCandidates" :selected-candidate="selectedCandidate"
                    :filter-options="filterOptions" :active-filter-ids="activeFilterIds"
                    :filtered-nodes="filteredNodes" :all-nodes="snapshot?.nodes ?? []"
                    :filter-selector="crossFilterBuild.selector" :filter-error="crossFilterBuild.error ?? ''"
                    :filter-verified="!!crossFilterBuild.selector?.xpath && verifiedXPath === crossFilterBuild.selector.xpath"
                    :anchor-picking="anchorPicking" :anchor-name="manualAnchorName" :verifying="verifying"
                    :mode="mode" :active-mode="activeMode"
                    :coordinate-confirmed="coordinateConfirmed" :coordinates-allowed="coordinatesAllowed"
                    :confirmed-locator="confirmedLocator" :suggested-xpath="suggestedXPath"
                    :snapshot-truncated="!!snapshot?.truncated" :can-add="canAdd" :can-replace="canReplace"
                    :scroll-container="scrollContainer" :replace-step="replaceStep"
                    :preferred-action="preferredAction"
                    @update-search="search = $event" @toggle-branch="toggleBranch"
                    @select-node="selectNode" @choose-candidate="chooseCandidate"
                    @toggle-filter="toggleFilter" @verify-filter="verifyFilter"
                    @preview-match="previewMatch" @focus-parent="focusParent"
                    @parent-filter="addParentFilter" @reset-preview="restoreOriginalHighlight"
                    @start-anchor-pick="startAnchorPick" @clear-anchor="clearAnchor"
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
                    <Button v-if="!replaceStep" label="查看脚本浮窗" icon="pi pi-list" size="small" @click="emit('edit')" />
                </div>
            </div>
        </div>
    </AutomationFloatingDialog>
</template>

<style scoped>
.ui-inspector{display:flex;flex-direction:column;gap:9px;color:var(--p-text-color);min-width:0}
.inspector-toolbar,.inspector-footer{display:flex;align-items:center;justify-content:space-between;gap:7px;flex-wrap:wrap}
.inspector-toolbar .status{display:flex;align-items:center;gap:6px;flex-wrap:wrap;font-size:11px;color:var(--console-muted)}
.inspector-columns{display:flex;flex-direction:column;min-width:0;gap:9px}
.inspector-columns :deep(.inspector-properties){min-width:0}
.original-screen-tools{display:flex;flex-direction:column;gap:5px;padding:7px;border:1px solid var(--console-border);border-radius:7px}
.drag-pick-controls{display:flex;align-items:center;flex-wrap:wrap;gap:5px}
.hint{color:var(--console-muted);font-size:11px;line-height:1.5;margin:0}
.inspector-footer{border-top:1px solid var(--console-border);padding-top:8px;font-size:11px;color:var(--console-muted)}
.footer-actions{display:flex;align-items:center;gap:5px;flex-wrap:wrap}
</style>
