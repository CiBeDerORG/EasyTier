<script setup lang="ts">
import { computed } from 'vue';
import { Button, InputText, Tag } from 'primevue';
import type { AutomationHierarchyNode, AutomationSelector, AutomationStep } from '../../types/automation';
import type { LocatorCandidate } from '../../modules/automation-locator-candidates';
import type { FilterOption } from '../../modules/automation-cross-filter';
import InspectorCrossFilter from './InspectorCrossFilter.vue';
import { inspectorActionLabels, type InspectorAction } from '../../modules/automation-inspector-actions';

type LocatorMode = 'resource_id' | 'text' | 'description' | 'composite' | 'xpath' | 'coordinates';
type ModeOption = {value: LocatorMode; label: string; unique: boolean};
const props = defineProps<{
    selected: AutomationHierarchyNode | null;
    selectedId: number | null;
    visibleNodes: AutomationHierarchyNode[];
    nodeCount: number;
    snapshotLoaded: boolean;
    parentIds: Set<number>;
    collapsedIds: number[];
    search: string;
    loading: boolean;
    lineage: AutomationHierarchyNode[];
    clickableParent: AutomationHierarchyNode | null;
    locatorCandidates: LocatorCandidate[];
    selectedCandidate: string | null;
    filterOptions: FilterOption[];
    activeFilterIds: string[];
    filteredNodes: AutomationHierarchyNode[];
    allNodes: AutomationHierarchyNode[];
    filterSelector: AutomationSelector | null;
    filterError: string;
    filterVerified: boolean;
    anchorPicking: boolean;
    anchorName: string;
    verifying: boolean;
    mode: LocatorMode;
    activeMode: ModeOption | undefined;
    coordinateConfirmed: boolean;
    coordinatesAllowed: boolean;
    confirmedLocator: AutomationSelector | null;
    suggestedXpath: string;
    snapshotTruncated: boolean;
    canAdd: boolean;
    canReplace: boolean;
    scrollContainer: AutomationSelector | null;
    replaceStep?: Pick<AutomationStep,'id'|'type'> | null;
    preferredAction?: InspectorAction | null;
}>();
const emit = defineEmits<{
    'update-search': [value: string];
    'toggle-branch': [id: number];
    'select-node': [id: number];
    'choose-candidate': [id: string];
    'toggle-filter': [id: string,checked: boolean];
    'verify-filter': [];
    'preview-match': [id: number];
    'focus-parent': [id: number];
    'parent-filter': [nodeId: number,field:import('../../modules/automation-cross-filter').FilterField];
    'reset-preview': [];
    'start-anchor-pick': [];
    'clear-anchor': [];
    'change-mode': [mode: LocatorMode];
    'confirm-coordinates': [];
    'replace-target': [];
    'add-step': [action: InspectorAction];
}>();
const actionLabel = inspectorActionLabels;
const preferredEnabled = computed(() => {
    const action = props.preferredAction;
    if (!props.canAdd || !action) return false;
    if (props.mode === 'coordinates') return action === 'tap' || action === 'long_press';
    if (props.mode === 'xpath') return action !== 'input_text' && action !== 'assert_property';
    return action !== 'scroll_find' || !!props.scrollContainer;
});
function title(node: AutomationHierarchyNode) {
    if (node.password) return node.resource_id || node.class_name || '敏感输入框';
    return node.text || node.description || node.resource_id || node.class_name || '未命名控件';
}
</script>

<template>
    <div class="inspector-properties-panel">
                <section class="tree-panel" aria-label="控件列表">
                    <div class="pane-heading"><strong>控件树</strong><small>{{ nodeCount }} 个节点</small></div>
                    <InputText :model-value="search" @update:model-value="value => emit('update-search', String(value ?? ''))" placeholder="搜索文字 / ID / 类名" size="small" :disabled="!snapshotLoaded" />
                    <div class="node-list">
                        <div v-for="node in visibleNodes" :key="node.id" class="tree-item"
                            :style="{ paddingLeft: Math.min(100, node.depth * 10) + 'px' }">
                            <button v-if="parentIds.has(node.id)" type="button" class="branch-toggle"
                                :disabled="!!search.trim()" :aria-label="collapsedIds.includes(node.id) ? '展开子控件' : '折叠子控件'"
                                :aria-expanded="!collapsedIds.includes(node.id)" @click="emit('toggle-branch', node.id)">
                                <i :class="collapsedIds.includes(node.id) && !search.trim() ? 'pi pi-chevron-right' : 'pi pi-chevron-down'" aria-hidden="true" />
                            </button>
                            <span v-else class="branch-spacer" aria-hidden="true" />
                            <button type="button" class="node-row" :class="{ active: selectedId === node.id }"
                                :aria-pressed="selectedId === node.id" @click="emit('select-node', node.id)">
                                <i :class="node.clickable ? 'pi pi-circle-fill' : 'pi pi-circle'" aria-hidden="true" />
                                <span><strong :title="title(node)">{{ title(node) }}</strong><small>{{ node.class_name.split('.').pop() }}</small></span>
                            </button>
                        </div>
                        <div v-if="!snapshotLoaded" class="empty-tree">{{ loading ? '正在读取手机控件树…' : '暂无控件信息' }}</div>
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
                                    :title="title(node)" @click="emit('select-node', node.id)">{{ node.class_name.split('.').pop() || 'Node' }} #{{ node.id }}</button>
                            </div>
                        </div>
                        <div v-if="!selected.clickable && clickableParent" class="parent-recommendation">
                            <span>当前控件不可点击，建议使用最近的可点击父节点。</span>
                            <Button label="选中可点击父节点" text size="small" @click="emit('select-node', clickableParent.id)" />
                        </div>
                        <div class="property"><span>类名</span><code>{{ selected.class_name || '—' }}</code></div>
                        <div class="property"><span>Resource ID</span><code>{{ selected.resource_id || '—' }}</code></div>
                        <div class="property"><span>文字</span><code>{{ selected.password ? '敏感输入框（已隐藏）' : selected.text || '—' }}</code></div>
                        <div class="property"><span>描述</span><code>{{ selected.password ? '敏感输入框（已隐藏）' : selected.description || '—' }}</code></div>
                        <div class="property"><span>状态</span><code>已启用：{{ selected.enabled ? "是" : "否" }} / 可滚动：{{ selected.scrollable ? "是" : "否" }}</code></div>
                        <div class="property"><span>扩展状态</span><code>支持长按：{{ selected.long_clickable == null ? "未知" : selected.long_clickable ? "是" : "否" }} / 可聚焦：{{ selected.focusable == null ? "未知" : selected.focusable ? "是" : "否" }} / 可勾选：{{ selected.checkable == null ? "未知" : selected.checkable ? "是" : "否" }}</code></div>
                        <div class="property"><span>范围</span><code>{{ selected.bounds.left }}, {{ selected.bounds.top }} → {{ selected.bounds.right }}, {{ selected.bounds.bottom }}</code></div>
                        <InspectorCrossFilter
                            :selected-id="selectedId" :options="filterOptions" :active-ids="activeFilterIds"
                            :matches="filteredNodes" :nodes="allNodes" :selector="filterSelector"
                            :filter-error="filterError" :verified="filterVerified" :verifying="verifying"
                            :truncated="snapshotTruncated"
                            @toggle="(id,checked) => emit('toggle-filter',id,checked)"
                            @verify="emit('verify-filter')"
                            @preview="id => emit('preview-match',id)"
                            @focus-parent="id => emit('focus-parent',id)"
                            @parent-filter="(nodeId,field) => emit('parent-filter',nodeId,field)"
                            @reset-preview="emit('reset-preview')" />
                        <details class="legacy-candidates">
                            <summary>高级：自动关联锚点与快捷定位建议（可选）</summary>
                            <div class="parent-recommendation">
                                <span>关联锚点：{{ anchorName || '自动查找' }}。手动选取时不改变操作目标。</span>
                                <Button :label="anchorPicking ? '请在画面或树中选择锚点' : '手动指定关联控件'"
                                    text size="small" :disabled="loading || snapshotTruncated || anchorPicking"
                                    @click="emit('start-anchor-pick')" />
                                <Button v-if="anchorName || anchorPicking" label="清除关联锚点" text size="small"
                                    @click="emit('clear-anchor')" />
                            </div>
                            <div class="locator-recommendations" aria-label="候选控件定位">
                                <strong>候选定位条件（需人工确认）</strong>
                                <small v-if="snapshotTruncated">控件树已截断，无法验证唯一性，请刷新。</small>
                                <small v-else-if="!locatorCandidates.length">没有安全候选，可改选其他控件。</small>
                                <button v-for="item in locatorCandidates.slice(0, 8)" :key="item.id" type="button"
                                    class="candidate-row" :class="{ chosen: selectedCandidate === item.id }"
                                    :disabled="verifying" @click="emit('choose-candidate', item.id)">
                                    <span>{{ item.label }} · {{ item.unique ? '唯一' : '匹配 ' + item.matches + ' 个' }}</span>
                                    <small>{{ selectedCandidate === item.id ? '已选为过滤条件' : '可选择，运行时查看实际匹配' }}</small>
                                </button>
                                <small>可先选择匹配多个控件的候选；确认录入时通过属性对比缩小至唯一目标。</small>
                            </div>

                        </details>
                        <div class="locator-choice">
                            <strong>当前执行定位：{{ mode === 'coordinates' ? '屏幕坐标（兜底）'
                                : confirmedLocator?.xpath ? '已配置的层级条件 XPath'
                                : confirmedLocator ? '当前交叉过滤属性组合'
                                : '待配置定位条件' }}</strong>
                            <small>以上多选条件是正常定位入口；原来的 Resource ID / 文字 / 描述单选菜单已移除，避免保存错条件。</small>
                            <Button v-if="mode !== 'coordinates'" label="改用坐标兜底（不保存过滤条件）"
                                outlined size="small" :disabled="!coordinatesAllowed"
                                @click="emit('change-mode','coordinates')" />
                            <Button v-else label="恢复属性交叉过滤" outlined size="small"
                                @click="emit('change-mode','composite')" />
                        </div>
                        <div v-if="mode === 'coordinates'" class="coordinate-confirm">
                            <small class="hint">坐标定位依赖当前画面和方向；必须主动确认，不会自动回退。</small>
                            <Button :label="coordinateConfirmed ? '已确认坐标' : '确认使用坐标'"
                                outlined size="small" :disabled="!coordinatesAllowed || coordinateConfirmed" @click="emit('confirm-coordinates')" />
                        </div>
                        <p v-else-if="mode === 'composite'" class="hint">已配置的交叉过滤条件：{{ JSON.stringify(confirmedLocator ?? {}) }}</p>
                        <p v-else-if="mode === 'xpath'" class="hint"><code>{{ confirmedLocator?.xpath || suggestedXpath }}</code></p>
                        <div v-if="replaceStep" class="step-actions">
                            <Button label="更新当前步骤控件" icon="pi pi-check" size="small"
                                :disabled="!canReplace" @click="emit('replace-target')" />
                            <small v-if="activeMode && !activeMode.unique" class="hint">当前条件尚未唯一命中；确认时将打开属性对比帮助继续过滤。</small>
                            <small v-if="mode === 'xpath' && ['input_text', 'assert_property'].includes(replaceStep.type)" class="hint">此动作只支持组合控件定位，请选择 Resource ID、文字或组合条件。</small>
                            <small v-if="mode === 'coordinates' && !['tap', 'long_press'].includes(replaceStep.type)" class="hint">当前步骤必须使用唯一控件定位，不能使用画面坐标。</small>
                        </div>
                        <div v-else class="step-actions">
                            <div v-if="preferredAction" class="preferred-action">
                                <strong>当前右键操作 · {{ actionLabel[preferredAction] }}</strong>
                                <small>先确认控件定位条件，再录入脚本；不会直接操作手机。</small>
                                <Button :label="'确认录入 · ' + actionLabel[preferredAction]" icon="pi pi-check" size="small"
                                    :disabled="!preferredEnabled" @click="emit('add-step', preferredAction)" />
                                <small v-if="!preferredEnabled">先选择有效条件；确认录入时如果命中多个，将弹出对比列表直至唯一。</small>
                            </div>
                            <div class="other-step-actions">
                                <small v-if="preferredAction">或选择其他操作</small>
                            <Button label="添加点击" icon="pi pi-plus" size="small" :disabled="!canAdd" @click="emit('add-step', 'tap')" />
                            <Button label="添加长按" outlined size="small" :disabled="!canAdd" @click="emit('add-step', 'long_press')" />
                            <Button label="等待出现" outlined size="small" :disabled="!canAdd || mode === 'coordinates'" @click="emit('add-step', 'wait_for')" />
                            <Button label="等待消失" outlined size="small" :disabled="!canAdd || mode === 'coordinates'" @click="emit('add-step', 'wait_gone')" />
                            <Button label="断言存在" outlined size="small" :disabled="!canAdd || mode === 'coordinates'" @click="emit('add-step', 'assert')" />
                            <Button label="属性断言" outlined size="small" :disabled="!canAdd || mode === 'coordinates' || mode === 'xpath'" @click="emit('add-step', 'assert_property')" />
                            <Button label="滚动查找" outlined size="small" :disabled="!canAdd || mode === 'coordinates' || !scrollContainer"
                                :title="scrollContainer ? '自动使用唯一滚动父容器' : '请先在脚本编辑器中指定滚动容器'" @click="emit('add-step', 'scroll_find')" />
                            <Button label="输入文字" outlined size="small" :disabled="!canAdd || mode === 'coordinates' || mode === 'xpath'" @click="emit('add-step', 'input_text')" />
                            </div>
                        </div>
                    </template>
                    <div v-else class="empty-detail">从左侧控件树中选择元素，或点击手机画面开始拾取。</div>
                </section>
    </div>
</template>

<style scoped>
.inspector-properties-panel{display:contents}
.inspector-properties-panel>section{min-width:0;border:1px solid var(--console-border);border-radius:10px;padding:12px;background:var(--console-panel);display:flex;flex-direction:column;gap:11px}
.pane-heading{display:flex;align-items:center;justify-content:space-between;min-height:26px;gap:10px;font-size:13px}
.pane-heading small,.hint{font-size:11px;color:var(--console-muted);line-height:1.6}
.tree-panel :deep(input){width:100%}
.node-list{max-height:480px;min-height:265px;overflow:auto;border-radius:8px;border:1px solid var(--console-border);background:var(--console-panel-raised)}
.tree-item{display:flex;align-items:center;border-bottom:1px solid var(--console-border);min-width:0}
.branch-toggle,.branch-spacer{width:22px;min-width:22px;height:28px;display:grid;place-items:center;border:0;background:transparent;color:var(--console-muted)}
.branch-toggle{cursor:pointer}.branch-toggle:hover:not(:disabled){color:var(--p-primary-color)}.branch-toggle:disabled{opacity:.5;cursor:not-allowed}
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
.locator-choice{display:grid;gap:6px;padding:8px;border:1px solid var(--console-border);border-radius:7px;font-size:11px}
.locator-choice small{color:var(--console-muted)}
.locator-choice :deep(.p-button){justify-self:start}
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
.step-actions{display:grid;gap:9px;margin-top:5px}
.preferred-action{display:flex;flex-direction:column;align-items:flex-start;gap:6px;padding:10px;border:1px solid var(--p-primary-color);border-radius:8px;background:var(--console-accent-soft)}
.preferred-action strong{font-size:12px}
.preferred-action small{font-size:11px;color:var(--console-muted)}
.preferred-action :deep(.p-button){width:100%;justify-content:center}
.other-step-actions{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px}
.other-step-actions>small{grid-column:1/-1;font-size:11px;color:var(--console-muted)}
.other-step-actions :deep(.p-button){justify-content:center}
.step-actions>.hint{grid-column:1/-1}
@media(max-width:950px){.details-panel{grid-column:1/-1}}
@media(max-width:580px){.node-list{max-height:240px;min-height:130px}}
</style>
