<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { Button, Dialog, InputText, Message, Tag } from 'primevue';
import { v4 as uuid } from 'uuid';
import AutomationApi, { automationErrorMessage } from '../../modules/automation-api';
import type { AutomationDevice, AutomationHierarchy, AutomationHierarchyNode, AutomationSelector, AutomationStep } from '../../types/automation';

const props = defineProps<{
    visible: boolean; device: AutomationDevice; ownerId: number;
    stream: MediaStream | null; queuedCount: number;
}>();
const emit = defineEmits<{
    close: []; 'add-step': [step: AutomationStep]; edit: [];
}>();
const api = new AutomationApi();
const video = ref<HTMLVideoElement | null>(null);
const surface = ref<HTMLDivElement | null>(null);
const response = ref<AutomationHierarchy | null>(null);
const selectedId = ref<number | null>(null);
const collapsedIds = ref<number[]>([]);
const search = ref('');
const mode = ref<'resource_id' | 'text' | 'description' | 'coordinates'>('resource_id');
const loading = ref(false);
const error = ref('');
const feedback = ref('');
const videoSize = ref({ width: 0, height: 0 });
let requestGeneration = 0;

const snapshot = computed(() => response.value?.snapshot);
const nodeIndex = computed(() => new Map((snapshot.value?.nodes ?? []).map(node => [node.id, node] as const)));
const parentIds = computed(() => new Set((snapshot.value?.nodes ?? [])
    .map(node => node.parent_id).filter((value): value is number => value !== null)));
const selected = computed(() => selectedId.value === null ? null : nodeIndex.value.get(selectedId.value) ?? null);
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
            if (![node.text, node.description, node.resource_id, node.class_name]
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
const modeOptions = computed(() => {
    if (!selected.value || !snapshot.value) return [];
    const node = selected.value;
    const choices: {value: typeof mode.value; label: string; unique: boolean}[] = [
        { value: 'resource_id', label: 'Resource ID', unique: false },
        { value: 'text', label: '控件文字', unique: false },
        { value: 'description', label: '无障碍描述', unique: false },
        { value: 'coordinates', label: '画面坐标', unique: compatibleVideo.value },
    ];
    for (const choice of choices) {
        if (choice.value === 'coordinates') continue;
        const key = choice.value as 'resource_id' | 'text' | 'description';
        const value = node[key];
        choice.unique = !!value && snapshot.value.nodes.filter(item => item[key] === value).length === 1
            && !snapshot.value.truncated;
    }
    return choices;
});
const activeMode = computed(() => modeOptions.value.find(item => item.value === mode.value));
const canAdd = computed(() => !!selected.value && !!activeMode.value?.unique && props.queuedCount < 100);
const nodeHighlight = computed(() => {
    const node = selected.value, shot = snapshot.value;
    if (!node || !shot || !compatibleVideo.value) return null;
    const { left, top, right, bottom } = node.bounds;
    return { left: left / shot.width * 100 + '%', top: top / shot.height * 100 + '%',
        width: (right - left) / shot.width * 100 + '%', height: (bottom - top) / shot.height * 100 + '%' };
});
function title(node: AutomationHierarchyNode) {
    return node.text || node.description || node.resource_id || node.class_name || '未命名控件';
}
function selectNode(id: number) {
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
    response.value = null; selectedId.value = null; collapsedIds.value = [];
    error.value = ''; feedback.value = ''; loading.value = true;
    try {
        const result = await api.hierarchy(props.ownerId, props.device.id);
        if (generation !== requestGeneration || !props.visible) return;
        if (result.device_id !== props.device.id || result.target_fingerprint !== props.device.target_fingerprint) {
            error.value = '手机连接目标已经变化，请关闭后重新打开拾取窗口。';
            return;
        }
        response.value = result;
    } catch (cause) {
        if (generation === requestGeneration && props.visible) error.value = automationErrorMessage(cause);
    } finally {
        if (generation === requestGeneration) loading.value = false;
    }
}
function pick(event: MouseEvent) {
    if (!surface.value || !snapshot.value || !compatibleVideo.value) return;
    const rect = surface.value.getBoundingClientRect();
    const x = (event.clientX - rect.left) / rect.width * snapshot.value.width;
    const y = (event.clientY - rect.top) / rect.height * snapshot.value.height;
    // Prefer a small clickable element; parent containers would otherwise cover everything.
    const candidates = snapshot.value.nodes.filter(node => {
        const b = node.bounds;
        return x >= b.left && x < b.right && y >= b.top && y < b.bottom;
    });
    candidates.sort((a, b) => {
        const area = (node: AutomationHierarchyNode) => (node.bounds.right - node.bounds.left) * (node.bounds.bottom - node.bounds.top);
        return area(a) - area(b) || Number(b.clickable) - Number(a.clickable) || b.depth - a.depth;
    });
    if (candidates.length) selectNode(candidates[0].id);
    else { selectedId.value = null; feedback.value = '点击位置未找到有效控件，请刷新控件树。'; }
}
watch(selected, node => {
    if (!node) return;
    mode.value = (['resource_id', 'text', 'description'] as const)
        .find(key => modeOptions.value.some(option => option.value === key && option.unique)) ?? 'coordinates';
});
function locator(): AutomationSelector | null {
    if (!selected.value || !activeMode.value?.unique) return null;
    if (mode.value === 'resource_id') return { resource_id: selected.value.resource_id };
    if (mode.value === 'text') return { text: selected.value.text };
    if (mode.value === 'description') return { description: selected.value.description };
    return null;
}
function add(type: 'tap' | 'wait_for' | 'assert' | 'input_text') {
    if (!selected.value || !snapshot.value || !canAdd.value) return;
    const selector = locator();
    if (!selector && type !== 'tap') return;
    const step: AutomationStep = { id: uuid(), type };
    if (selector) step.selector = selector;
    else {
        const bounds = selected.value.bounds, { width, height } = snapshot.value;
        step.x = (bounds.left + bounds.right) / 2 / width;
        step.y = (bounds.top + bounds.bottom) / 2 / height;
        step.width = width; step.height = height;
    }
    if (type === 'wait_for' || type === 'assert') step.timeout_ms = 10000;
    if (type === 'input_text') step.parameter = 'picked_' + uuid().slice(0, 8);
    emit('add-step', step);
    feedback.value = '已添加到脚本草稿：' + ({ tap: '点击', wait_for: '等待控件', assert: '检查控件', input_text: '输入文字' }[type]);
}
watch([() => props.visible, () => props.device.id, () => props.ownerId], async () => {
    ++requestGeneration;
    response.value = null; selectedId.value = null; collapsedIds.value = []; loading.value = false;
    error.value = ''; feedback.value = ''; videoSize.value = { width: 0, height: 0 };
    await nextTick();
    setVideo();
    if (props.visible) void refresh();
}, { immediate: true });
watch(() => props.stream, async () => { await nextTick(); setVideo(); });
</script>

<template>
    <Dialog :visible="visible" modal append-to="body" :header="device.name + ' · UiAutomator2 控件拾取'"
        :style="{ width: '1160px', maxWidth: 'calc(100vw - 24px)' }" :content-style="{ padding: '0' }"
        @show="setVideo"
        @update:visible="value => { if (!value) emit('close'); }">
        <div class="ui-inspector">
            <div class="inspector-toolbar">
                <div class="status">
                    <Tag :value="loading ? '正在读取' : snapshot ? '控件树已加载' : '未加载'" :severity="snapshot ? 'success' : 'secondary'" />
                    <span v-if="capturedAt">层级采集时间：{{ capturedAt }}</span>
                    <span>视频是实时流，控件树是独立快照；切换页面后请重新读取。</span>
                </div>
                <Button label="刷新控件树" icon="pi pi-refresh" outlined size="small" :loading="loading" @click="refresh" />
            </div>
            <Message v-if="error" severity="error" :closable="false">{{ error }}</Message>
            <Message v-if="snapshot?.truncated" severity="warn" :closable="false">当前页面控件过多，已截断。为防止错误定位，精确控件步骤暂不可添加。</Message>
            <div class="inspector-columns">
                <section class="preview-panel" aria-label="手机画面拾取">
                    <div class="preview-heading"><strong>手机实时画面</strong><small>{{ snapshot ? snapshot.width + ' × ' + snapshot.height : '等待控件树' }}</small></div>
                    <div class="video-well">
                        <div ref="surface" class="video-surface" :style="{ aspectRatio: snapshot ? snapshot.width + ' / ' + snapshot.height : '9 / 19', width: snapshot ? 'min(100%, ' + (snapshot.width / snapshot.height * 58) + 'vh)' : 'min(100%, 27.5vh)' }">
                            <video ref="video" autoplay muted playsinline @loadedmetadata="videoMetadata" @resize="videoMetadata" />
                            <button v-if="snapshot && compatibleVideo" type="button" class="pick-layer" title="点击选取控件（不会操作手机）" aria-label="点击画面选择控件" @click="pick" />
                            <div v-if="nodeHighlight" class="node-highlight" :style="nodeHighlight" />
                        </div>
                    </div>
                    <small v-if="snapshot && !compatibleVideo" class="hint">画面尺寸或方向与控件树不一致，请确认投屏已加载或旋转完毕，再刷新控件树。</small>
                    <small v-else class="hint">在画面上点选控件，右侧自动读取属性；如果界面发生变化，请刷新。</small>
                </section>
                <section class="tree-panel" aria-label="控件列表">
                    <div class="pane-heading"><strong>控件树</strong><small>{{ snapshot?.nodes.length ?? 0 }} 个节点</small></div>
                    <InputText v-model="search" placeholder="搜索文字 / ID / 类名" size="small" :disabled="!snapshot" />
                    <div class="node-list">
                        <div v-for="node in visibleNodes" :key="node.id" class="tree-item"
                            :style="{ paddingLeft: Math.min(100, node.depth * 10) + 'px' }">
                            <button v-if="parentIds.has(node.id)" type="button" class="branch-toggle"
                                :disabled="!!search.trim()" :aria-label="collapsedIds.includes(node.id) ? '展开子控件' : '折叠子控件'"
                                :aria-expanded="!collapsedIds.includes(node.id)" @click="toggleBranch(node.id)">
                                <i :class="collapsedIds.includes(node.id) && !search.trim() ? 'pi pi-chevron-right' : 'pi pi-chevron-down'" aria-hidden="true" />
                            </button>
                            <span v-else class="branch-spacer" aria-hidden="true" />
                            <button type="button" class="node-row" :class="{ active: selectedId === node.id }"
                                :aria-pressed="selectedId === node.id" @click="selectNode(node.id)">
                                <i :class="node.clickable ? 'pi pi-circle-fill' : 'pi pi-circle'" aria-hidden="true" />
                                <span><strong :title="title(node)">{{ title(node) }}</strong><small>{{ node.class_name.split('.').pop() }}</small></span>
                            </button>
                        </div>
                        <div v-if="!snapshot" class="empty-tree">{{ loading ? '正在读取手机控件树…' : '暂无控件信息' }}</div>
                        <div v-else-if="!visibleNodes.length" class="empty-tree">没有匹配的控件</div>
                    </div>
                    <small class="hint">按层级展开或折叠；搜索时会显示匹配节点及其祖先。最多展示 250 条。</small>
                </section>
                <section class="details-panel" aria-label="控件属性及生成步骤">
                    <div class="pane-heading"><strong>控件属性</strong><Tag v-if="selected" :value="selected.clickable ? '可点击' : '非可点击'" severity="secondary" /></div>
                    <template v-if="selected">
                        <div class="ancestry" aria-label="控件父子层级">
                            <span>控件路径</span>
                            <div class="ancestry-nodes">
                                <button v-for="node in lineage" :key="node.id" type="button" :class="{ current: node.id === selectedId }"
                                    :title="title(node)" @click="selectNode(node.id)">{{ node.class_name.split('.').pop() || 'Node' }} #{{ node.id }}</button>
                            </div>
                        </div>
                        <div v-if="!selected.clickable && clickableParent" class="parent-recommendation">
                            <span>当前控件不可点击，建议使用最近的可点击父节点。</span>
                            <Button label="选中可点击父节点" text size="small" @click="selectNode(clickableParent.id)" />
                        </div>
                        <div class="property"><span>类名</span><code>{{ selected.class_name || '—' }}</code></div>
                        <div class="property"><span>Resource ID</span><code>{{ selected.resource_id || '—' }}</code></div>
                        <div class="property"><span>文字</span><code>{{ selected.password ? '敏感输入框（已隐藏）' : selected.text || '—' }}</code></div>
                        <div class="property"><span>描述</span><code>{{ selected.description || '—' }}</code></div>
                        <div class="property"><span>范围</span><code>{{ selected.bounds.left }}, {{ selected.bounds.top }} → {{ selected.bounds.right }}, {{ selected.bounds.bottom }}</code></div>
                        <label class="locator-label" for="ui-inspector-locator">定位方式</label>
                        <select id="ui-inspector-locator" v-model="mode" class="locator-select">
                            <option v-for="option in modeOptions" :key="option.value" :value="option.value" :disabled="!option.unique">{{ option.label }}{{ option.unique ? '' : ' · 不可用或不唯一' }}</option>
                        </select>
                        <p v-if="mode === 'coordinates'" class="hint">坐标仅适用于当前画面比例；优先选择唯一 Resource ID 或文字定位。</p>
                        <div class="step-actions">
                            <Button label="添加点击" icon="pi pi-plus" size="small" :disabled="!canAdd" @click="add('tap')" />
                            <Button label="等待出现" outlined size="small" :disabled="!canAdd || mode === 'coordinates'" @click="add('wait_for')" />
                            <Button label="断言存在" outlined size="small" :disabled="!canAdd || mode === 'coordinates'" @click="add('assert')" />
                            <Button label="输入文字" outlined size="small" :disabled="!canAdd || mode === 'coordinates'" @click="add('input_text')" />
                        </div>
                    </template>
                    <div v-else class="empty-detail">从左侧控件树中选择元素，或点击手机画面开始拾取。</div>
                </section>
            </div>
            <div class="inspector-footer">
                <span role="status">{{ feedback || '脚本步骤只会加入草稿，不会立即执行。' }}</span>
                <div class="footer-actions">
                    <strong>草稿 {{ queuedCount }}/100 步</strong>
                    <Button label="关闭" text size="small" @click="emit('close')" />
                    <Button label="去编辑脚本" icon="pi pi-arrow-right" icon-pos="right" size="small" :disabled="queuedCount === 0" @click="emit('edit')" />
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
.inspector-columns>section{min-width:0;border:1px solid var(--console-border);border-radius:10px;padding:12px;background:var(--console-panel);display:flex;flex-direction:column;gap:11px}
.pane-heading,.preview-heading{min-height:26px;font-size:13px}
.pane-heading small,.preview-heading small,.hint{font-size:11px;color:var(--console-muted);line-height:1.6}
.video-well{display:flex;align-items:center;justify-content:center;min-height:365px;background:#0b1018;border-radius:9px;padding:8px}
.video-surface{width:100%;max-width:340px;max-height:58vh;position:relative;overflow:hidden;background:#05070b;border-radius:7px;box-shadow:0 3px 18px #0007}
.video-surface video{position:absolute;inset:0;width:100%;height:100%;object-fit:contain;pointer-events:none}
.pick-layer{position:absolute;inset:0;z-index:2;background:transparent;border:0;cursor:crosshair}
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
.parent-recommendation{padding:9px;border:1px solid var(--console-border);border-radius:8px;background:var(--console-accent-soft);display:grid;gap:4px;font-size:11px;line-height:1.6}
.parent-recommendation :deep(.p-button){justify-self:start}
.property{display:grid;gap:4px;padding:5px 0;border-bottom:1px solid var(--console-border)}
.property span,.locator-label{font-size:11px;color:var(--console-muted)}
.property code{font-family:ui-monospace,monospace;font-size:11px;overflow-wrap:anywhere;white-space:normal}
.locator-select{width:100%;border:1px solid var(--console-border-strong);background:var(--console-panel-raised);color:var(--p-text-color);padding:9px;border-radius:8px}
.step-actions{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px;margin-top:5px}
.step-actions :deep(.p-button){justify-content:center}
.inspector-footer{border-top:1px solid var(--console-border);padding-top:12px;font-size:12px;color:var(--console-muted);flex-wrap:wrap}
.footer-actions{display:flex;align-items:center;gap:9px}
@media(max-width:950px){.inspector-columns{grid-template-columns:minmax(220px,1fr) minmax(250px,1fr)}.details-panel{grid-column:1/-1}}
@media(max-width:580px){.inspector-columns{display:flex;flex-direction:column}.video-well{min-height:300px}.node-list{max-height:240px;min-height:130px}.inspector-footer,.footer-actions{align-items:flex-start;flex-wrap:wrap}}
</style>
