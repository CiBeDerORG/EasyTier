<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { Button } from 'primevue';
import AutomationFloatingDialog from './AutomationFloatingDialog.vue';
import { FILTER_FIELDS, boundsText, buildFilterOptions, type FilterField } from '../../modules/automation-cross-filter';
import type { AutomationHierarchyNode } from '../../types/automation';

const props = defineProps<{
    visible: boolean; portalTarget: HTMLElement | null; fullscreen: boolean;
    matches: AutomationHierarchyNode[] | null;
    nodes: AutomationHierarchyNode[]; truncated: boolean;
    selectedId: number | null; filterCount: number;
}>();
const emit = defineEmits<{
    close: []; confirm: []; 'add-filter': [nodeId: number, field: FilterField, parent: boolean];
    preview: [nodeId: number];
}>();
const page = ref(0);
const perPage = 20;
const candidates = computed(() => props.matches ?? props.nodes.filter(node => !node.password));
const pages = computed(() => Math.max(1, Math.ceil(candidates.value.length / perPage)));
const rows = computed(() => candidates.value.slice(page.value * perPage, (page.value + 1) * perPage));
watch(() => [props.matches, props.nodes], () => { page.value = 0; });
const parents = computed(() => new Map(props.nodes.map(node => [node.id, node] as const)));
const names: Record<FilterField, string> = {
    resource_id: 'Resource ID', description: '无障碍描述', text: '文字',
    class_name: '类名', package: '包名', clickable: '可点击', enabled: '已启用',
    long_clickable: '长按', focusable: '可聚焦', checkable: '可勾选',
    scrollable: '可滚动', selected: '已选中', checked: '已勾选',
    password: '密码控件', bounds: '控件区域',
};
type Column = { key: FilterField; parent: boolean; label: string };
const columns: Column[] = [
    ...FILTER_FIELDS.map(key => ({ key, parent: false, label: names[key] })),
    ...(['resource_id','description','text','class_name','package','clickable','enabled'] as FilterField[])
        .map(key => ({ key, parent: true, label: '父级·' + names[key] })),
];
function subject(node: AutomationHierarchyNode, column: Column) {
    return column.parent && node.parent_id !== null
        ? parents.value.get(node.parent_id) ?? null : column.parent ? null : node;
}
function value(node: AutomationHierarchyNode, column: Column): string {
    const target = subject(node, column);
    if (!target) return '—';
    if (target.password && (column.key === 'text' || column.key === 'description')) return '（敏感）';
    const raw = column.key === 'bounds'
        ? target.raw_bounds || boundsText(target.bounds)
        : target[column.key];
    if (raw === null || raw === undefined || raw === '') return '—';
    return typeof raw === 'boolean' ? (raw ? 'true' : 'false') : String(raw);
}
const different = computed(() => new Set(columns
    .filter(col => candidates.value.length > 1
        && new Set(candidates.value.map(row => value(row, col))).size > 1)
    .map(col => (col.parent ? 'parent:' : 'self:') + col.key)));
const changes = computed(() => columns.filter(col =>
    different.value.has((col.parent ? 'parent:' : 'self:') + col.key)).length);
// Build selectable fields only once per displayed row. Re-running the full
// hierarchy traversal for every table cell makes large phone trees laggy.
const availableCells = computed(() => {
    const available = new Set<string>();
    const shot = { nodes: props.nodes, truncated: props.truncated };
    if (props.truncated) return available;
    for (const row of rows.value) {
        const options = buildFilterOptions(shot, row.id);
        const parent = row.parent_id === null ? null : parents.value.get(row.parent_id);
        const distance = parent ? row.depth - parent.depth : 0;
        for (const col of columns) {
            if (options.some(option => option.field === col.key
                && (col.parent ? option.scope === 'ancestor' && distance > 0
                    && option.distance === distance : option.scope === 'self'))) {
                available.add(row.id + ':' + (col.parent ? 'parent:' : 'self:') + col.key);
            }
        }
    }
    return available;
});
function canAdd(row: AutomationHierarchyNode, column: Column): boolean {
    return availableCells.value.has(row.id + ':' + (column.parent ? 'parent:' : 'self:') + column.key);
}
function add(row: AutomationHierarchyNode, column: Column) {
    if (!canAdd(row, column)) return;
    emit('add-filter', row.id, column.key, column.parent);
}
</script>

<template>
    <AutomationFloatingDialog :visible="visible" :portal-target="portalTarget"
        :fullscreen="fullscreen" title="确认录入 · 匹配控件属性对比" wide @close="emit('close')">
        <div class="match-review" aria-label="确认录入匹配控件对比">
            <div class="review-summary">
                <strong>当前筛选命中 {{matches?.length ?? '未知'}} 个控件</strong>
                <span v-if="matches && matches.length > 1">有 {{changes}} 列属性存在差异；请选择能区分目标的值，继续缩小范围。</span>
                <span v-else-if="matches?.length === 1">已唯一命中 1 个控件，可最终确认录入。</span>
                <span v-else-if="matches?.length === 0">没有匹配控件；请调整筛选条件，不能录入。</span>
                <span v-else>当前层级 XPath 无法在快照中可靠枚举；先选择表格属性生成可比较的交叉过滤条件。</span>
                <small>右键属性值直接加入交叉过滤；手机和平板可点按属性值。相同属性为普通背景，不同属性高亮。不会立即点击手机。</small>
            </div>
            <div class="review-scroll">
                <table class="comparison">
                    <thead>
                        <tr>
                            <th class="sticky-first">候选</th>
                            <th v-for="col in columns" :key="(col.parent?'p':'s')+col.key"
                                :class="{distinct:different.has((col.parent?'parent:':'self:')+col.key)}">
                                {{col.label}}
                            </th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr v-for="node in rows" :key="node.id">
                            <th class="sticky-first">
                                <button type="button" class="preview" @click="emit('preview',node.id)">
                                    #{{node.id}} · 高亮
                                </button>
                                <small v-if="node.id===selectedId">原拾取</small>
                            </th>
                            <td v-for="col in columns" :key="(col.parent?'p':'s')+col.key"
                                :class="{distinct:different.has((col.parent?'parent:':'self:')+col.key)}">
                                <button type="button" class="value" :disabled="!canAdd(node,col)"
                                    :title="canAdd(node,col)?'右键或点击：加入交叉过滤':'该值不能用于过滤'"
                                    @contextmenu.prevent.stop="add(node,col)" @click.stop="add(node,col)">
                                    {{value(node,col)}}
                                </button>
                            </td>
                        </tr>
                        <tr v-if="!rows.length"><td :colspan="columns.length+1">暂无可比较控件。</td></tr>
                    </tbody>
                </table>
            </div>
            <div v-if="pages>1" class="pagination">
                <Button label="上一页" text size="small" :disabled="page===0" @click="page--"/>
                <span>{{page+1}} / {{pages}}（每页 {{perPage}} 条）</span>
                <Button label="下一页" text size="small" :disabled="page>=pages-1" @click="page++"/>
            </div>
            <div class="review-bottom">
                <small>当前选择 {{filterCount}} 项条件。只有最终恰好命中 1 个控件，才能完成录入；列表采用当前已读取的控件树，不校验原位置。</small>
                <div class="actions">
                    <Button label="返回调整" outlined size="small" @click="emit('close')"/>
                    <Button label="最终确认录入（1 个）" icon="pi pi-check" size="small"
                        :disabled="!matches || matches.length!==1 || truncated" @click="emit('confirm')"/>
                </div>
            </div>
        </div>
    </AutomationFloatingDialog>
</template>

<style scoped>
.match-review{display:grid;gap:12px;min-width:0;font-size:12px}
.review-summary{display:grid;gap:5px;line-height:1.5}
.review-summary strong{font-size:14px}
.review-summary span,.review-summary small,.review-bottom small{color:var(--console-muted)}
.review-scroll{max-width:100%;max-height:min(55vh,450px);overflow:auto;border:1px solid var(--console-border);border-radius:8px;overscroll-behavior:contain}
.comparison{width:max-content;min-width:100%;border-collapse:collapse;white-space:nowrap;font-size:11px}
.comparison th,.comparison td{border-bottom:1px solid var(--console-border);padding:5px 7px;text-align:left;max-width:260px}
.comparison thead th{position:sticky;top:0;z-index:2;background:var(--console-panel-raised)}
.comparison th.sticky-first{position:sticky;left:0;z-index:1;background:var(--console-panel-raised)}
.comparison thead th.sticky-first{z-index:3}
.comparison .distinct{background:var(--console-accent-soft)}
.comparison thead .distinct{background:var(--console-accent-soft)}
.value{border:0;border-radius:5px;padding:7px 5px;background:transparent;color:inherit;text-align:left;cursor:context-menu;max-width:240px;overflow:hidden;text-overflow:ellipsis}
.value:hover:not(:disabled),.value:focus-visible{outline:2px solid var(--p-primary-color);outline-offset:-2px}
.value:disabled{opacity:.55;cursor:default}
.preview{border:0;background:transparent;color:var(--p-primary-color);cursor:pointer;padding:5px}
.sticky-first small{display:block;color:var(--console-muted)}
.pagination,.review-bottom,.actions{display:flex;gap:8px;align-items:center;flex-wrap:wrap}
.pagination{justify-content:center}
.review-bottom{justify-content:space-between}
.review-bottom small{flex:1;min-width:170px;line-height:1.5}
@media (pointer:coarse){.value{min-height:42px;cursor:pointer}}
</style>
