<script setup lang="ts">
import {computed,ref,watch} from 'vue';
import type {AutomationHierarchyNode,AutomationSelector} from '../../types/automation';
import {boundsText,type FilterOption,type FilterField} from '../../modules/automation-cross-filter';

const props=defineProps<{
  selectedId:number|null; options:FilterOption[]; activeIds:string[];
  matches:AutomationHierarchyNode[]; nodes:AutomationHierarchyNode[];
  selector:AutomationSelector|null; verified:boolean; verifying:boolean; filterError:string;
  truncated:boolean;
}>();
const emit=defineEmits<{
  toggle:[id:string,checked:boolean];verify:[];preview:[id:number];'focus-parent':[id:number];
  'parent-filter':[nodeId:number,field:FilterField];'reset-preview':[];
}>();
const expanded=ref(false),page=ref(0),parentDetailId=ref<number|null>(null);
const size=50;
const previewId=ref<number|null>(null);
const pages=computed(()=>Math.max(1,Math.ceil(props.matches.length/size)));
const rows=computed(()=>props.matches.slice(page.value*size,(page.value+1)*size));
const selfOptions=computed(()=>props.options.filter(x=>x.scope==='self'));
const ancestors=computed(()=>{
  const groups=new Map<number,{distance:number;nodeId:number;items:FilterOption[]}>();
  for(const option of props.options.filter(x=>x.scope==='ancestor')){
    let group=groups.get(option.distance);
    if(!group){group={distance:option.distance,nodeId:option.nodeId,items:[]};groups.set(option.distance,group);}
    group.items.push(option);
  }
  return [...groups.values()].sort((a,b)=>a.distance-b.distance);
});
const nodeMap=computed(()=>new Map(props.nodes.map(x=>[x.id,x])));
const parentDetail=computed(()=>parentDetailId.value===null?null:nodeMap.value.get(parentDetailId.value)??null);
watch(()=>props.activeIds.join('|'),()=>{page.value=0;});
watch(()=>props.matches.length,()=>{page.value=Math.min(page.value,pages.value-1);});
watch(()=>props.selectedId,()=>{expanded.value=false;page.value=0;parentDetailId.value=null;previewId.value=null;});
const fields=[
  {key:'id',label:'节点编号'},{key:'parent_id',label:'父级编号'},
  {key:'depth',label:'层级深度'},
  {key:'resource_id',label:'Resource ID'},{key:'description',label:'无障碍描述'},
  {key:'text',label:'文字'},{key:'class_name',label:'类名'},{key:'package',label:'应用包名'},
  {key:'clickable',label:'可点击'},{key:'enabled',label:'已启用'},
  {key:'long_clickable',label:'支持长按'},{key:'focusable',label:'可聚焦'},
  {key:'checkable',label:'可勾选'},{key:'scrollable',label:'可滚动'},
  {key:'selected',label:'已选中'},{key:'checked',label:'已勾选'},
  {key:'password',label:'密码控件'},{key:'bounds',label:'范围'},
  {key:'parent_class',label:'父级类名'},{key:'parent_resource',label:'父级 Resource ID'},
  {key:'parent_description',label:'父级描述'},{key:'parent_text',label:'父级文字'},
  {key:'parent_clickable',label:'父级可点击'},
] as const;
function val(n:AutomationHierarchyNode,key:string):string{
  const parent=n.parent_id===null?null:nodeMap.value.get(n.parent_id);
  let value:unknown;
  switch(key){
    case 'bounds':return boundsText(n.bounds);
    case 'parent_class':value=parent?.class_name;break;
    case 'parent_resource':value=parent?.resource_id;break;
    case 'parent_description':value=parent?.password?'（敏感）':parent?.description;break;
    case 'parent_text':value=parent?.password?'（敏感）':parent?.text;break;
    case 'parent_clickable':value=parent?.clickable;break;
    case 'text':case 'description':
      value=n.password?'（敏感）':n[key];break;
    default:value=(n as unknown as Record<string,unknown>)[key];
  }
  if(value===null||value===undefined||value==='')return '—';
  return typeof value==='boolean'?(value?'true':'false'):String(value);
}
function label(n:AutomationHierarchyNode){
  return n.password?'敏感控件':n.text||n.description||n.resource_id||n.class_name||'节点';
}
const parentFields:Partial<Record<string,FilterField>>={
  parent_class:'class_name',parent_resource:'resource_id',parent_description:'description',
  parent_text:'text',parent_clickable:'clickable',
};
function isParentField(key:string):boolean{return key in parentFields;}
function canFilterParent(node:AutomationHierarchyNode,key:string):boolean {
  const parent=node.parent_id===null?null:nodeMap.value.get(node.parent_id);
  const field=parentFields[key];
  if(!parent||!field||parent.password)return false;
  const value=parent[field];
  return typeof value==='boolean' || (typeof value==='string'&&!!value.trim());
}
function inspectParent(id:number|null){
  parentDetailId.value=id;
  if(id!==null)emit('focus-parent',id);
}
function previewNode(id:number){
  previewId.value=id;emit('preview',id);
}
function restoreOriginal(){
  previewId.value=null;emit('reset-preview');
}
</script>

<template>
<section class="cross-filter" aria-label="交叉过滤定位条件">
  <div class="filter-heading">
    <strong>定位条件 · 多选交叉过滤</strong>
    <small>勾选后立即筛选；所有条件同时成立（AND）</small>
  </div>
  <div class="filter-section">
    <strong>当前目标的属性</strong>
    <div class="filter-options">
      <label v-for="option in selfOptions" :key="option.id" class="filter-option"
             :title="option.caution||''">
        <input type="checkbox" :checked="activeIds.includes(option.id)"
               :disabled="truncated" @change="emit('toggle',option.id,($event.target as HTMLInputElement).checked)" />
        <span class="filter-name">{{option.label}}</span>
        <code>{{option.displayValue}}</code>
        <small v-if="option.caution" class="caution">!</small>
      </label>
    </div>
  </div>
  <div class="filter-section">
    <strong>父级 / 祖先节点条件</strong>
    <small>可继续多选，与上方目标条件同时过滤；父级编号仅用于查看，不写入脚本。</small>
    <div v-if="!ancestors.length" class="muted">当前未发现可作为过滤条件的父级属性。</div>
    <details v-for="group in ancestors" :key="group.distance" class="ancestor-group"
             :open="group.distance===1 ? true : undefined">
      <summary>父级 #{{group.nodeId}} · 上溯 {{group.distance}} 层
        <span>可选 {{group.items.length}} 项</span>
      </summary>
      <div class="filter-options">
        <label v-for="option in group.items" :key="option.id" class="filter-option"
               :title="option.caution||''">
          <input type="checkbox" :checked="activeIds.includes(option.id)"
                 :disabled="truncated" @change="emit('toggle',option.id,($event.target as HTMLInputElement).checked)" />
          <span class="filter-name">{{option.label}}</span>
          <code>{{option.displayValue}}</code>
          <small v-if="option.caution" class="caution">!</small>
        </label>
      </div>
    </details>
  </div>
  <div class="filter-result">
    <button type="button" class="match-toggle" :aria-expanded="expanded" @click="expanded=!expanded">
      <span>当前快照预计命中 <strong>{{matches.length}}</strong> 个目标</span>
      <span>{{expanded?'收起列表':'查看目标列表与父级信息'}}</span>
    </button>
    <div v-if="expanded" class="matches">
      <div class="muted">下面是当前控件树的预计匹配结果，复杂层级条件以执行器重新采集的 XML 验证为准；横向滚动查看。
        节点编号、深度只是快照元数据，不作为永久执行条件。</div>
      <div class="preview-controls">
        <span>原操作目标：#{{selectedId ?? '—'}}；预览目标：#{{previewId ?? selectedId ?? '—'}}</span>
        <button type="button" @click="restoreOriginal">恢复高亮原目标</button>
      </div>
      <div v-if="parentDetail" class="parent-detail">
        <strong>父级 #{{parentDetail.id}}</strong>
        <span>{{label(parentDetail)}}</span>
        <span>{{parentDetail.class_name}} · {{boundsText(parentDetail.bounds)}}</span>
        <small>可以直接点击下方表格父级属性的“加入过滤”，继续缩小目标集合。</small>
      </div>
      <div class="table-scroll">
        <table class="match-table">
          <thead><tr><th>目标</th><th v-for="f in fields" :key="f.key">{{f.label}}</th></tr></thead>
          <tbody>
            <tr v-for="node in rows" :key="node.id" :class="{chosen:node.id===selectedId}">
              <td>
                <button type="button" class="row-action" @click="previewNode(node.id)">高亮</button>
                <span v-if="node.id===selectedId">原目标</span>
              </td>
              <td v-for="f in fields" :key="f.key" :title="val(node,f.key)">
                <button v-if="f.key==='parent_id' && node.parent_id!==null"
                        type="button" class="parent-link" @click="inspectParent(node.parent_id)">
                  #{{node.parent_id}} · 查看父级
                </button>
                <button v-else-if="isParentField(f.key) && canFilterParent(node,f.key)"
                        type="button" class="parent-link"
                        :title="'加入父级过滤：'+f.label+' = '+val(node,f.key)"
                        @click="emit('parent-filter',node.id,parentFields[f.key]!)">
                  {{val(node,f.key)}} · 加入过滤
                </button>
                <span v-else>{{val(node,f.key)}}</span>
              </td>
            </tr>
            <tr v-if="!rows.length"><td :colspan="fields.length+1">没有匹配的控件。</td></tr>
          </tbody>
        </table>
      </div>
      <div v-if="pages>1" class="pagination">
        <button type="button" :disabled="page===0" @click="page--">上一页</button>
        <span>{{page+1}} / {{pages}}（每页 {{size}} 条）</span>
        <button type="button" :disabled="page>=pages-1" @click="page++">下一页</button>
      </div>
    </div>
    <p v-if="filterError" class="warning">{{filterError}}</p>
    <p v-else-if="activeIds.length===0" class="muted">请至少选择一项定位属性。</p>
    <p v-else-if="matches.length===0" class="warning">当前快照未匹配控件；请调整条件至唯一命中后再录入。</p>
    <p v-else class="muted">当前快照匹配 {{matches.length}} 个控件；点击确认录入时，如果超过 1 个，将弹出属性对比列表继续过滤，唯一命中后才能录入。</p>
    <small class="muted">“范围”及 selected/checked 属于易变条件；请优先用描述、ID、类名和稳定父级属性。</small>
  </div>
</section>
</template>

<style scoped>
.cross-filter{display:grid;gap:12px;border:1px solid var(--console-border);border-radius:10px;padding:12px;min-width:0}
.filter-heading,.filter-section{display:grid;gap:7px}
.filter-heading strong,.filter-section strong{font-size:12px}
.filter-heading small,.filter-section small,.muted{font-size:11px;color:var(--console-muted);line-height:1.6}
.filter-options{display:grid;grid-template-columns:repeat(auto-fit,minmax(195px,1fr));gap:7px}
.filter-option{display:flex;align-items:center;gap:7px;border:1px solid var(--console-border);border-radius:7px;padding:7px;background:var(--console-panel-raised);font-size:11px;min-width:0;cursor:pointer}
.filter-option:has(input:checked){border-color:var(--p-primary-color);background:var(--console-accent-soft)}
.filter-option input{flex:none;accent-color:var(--p-primary-color)}
.filter-name{flex:none}.filter-option code{font-size:10px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;min-width:0}
.filter-option .caution{color:var(--p-orange-500);font-weight:bold}
.ancestor-group{border:1px solid var(--console-border);border-radius:6px;padding:8px}
.ancestor-group summary{cursor:pointer;font-size:11px;font-weight:600;display:flex;justify-content:space-between;gap:8px}
.ancestor-group .filter-options{margin-top:9px}
.filter-result{display:grid;gap:8px;min-width:0}
.match-toggle{display:flex;justify-content:space-between;gap:8px;width:100%;text-align:left;cursor:pointer;border-radius:7px;border:1px solid var(--console-border);background:var(--console-panel-raised);color:var(--p-text-color);padding:10px;font-size:12px}
.match-toggle strong{color:var(--p-primary-color);font-size:15px}
.matches{display:grid;gap:9px;min-width:0}
.table-scroll{max-width:100%;overflow:auto;border:1px solid var(--console-border);border-radius:7px;max-height:340px}
.match-table{border-collapse:collapse;white-space:nowrap;font-size:10px;width:max-content;min-width:100%}
.match-table th,.match-table td{border-bottom:1px solid var(--console-border);padding:7px;text-align:left;max-width:280px;overflow:hidden;text-overflow:ellipsis}
.match-table thead{position:sticky;top:0;background:var(--console-panel-raised);z-index:1}
.match-table tr.chosen{background:var(--console-accent-soft)}
.row-action,.parent-link,.pagination button{background:transparent;border:0;color:var(--p-primary-color);cursor:pointer;font-size:11px}
.match-table td:first-child{display:flex;gap:6px;align-items:center}
.parent-detail,.preview-controls{display:flex;flex-wrap:wrap;gap:6px;padding:8px;border:1px solid var(--console-border);font-size:11px;border-radius:7px}
.preview-controls{justify-content:space-between;align-items:center}
.preview-controls button{color:var(--p-primary-color);border:0;background:transparent;cursor:pointer}
.parent-detail small{width:100%;color:var(--console-muted)}
.pagination{display:flex;justify-content:center;gap:12px;align-items:center;font-size:11px}
.pagination button:disabled{opacity:.35;cursor:not-allowed}
.warning{font-size:11px;color:var(--p-orange-500);margin:0;line-height:1.6}
.verified{font-size:11px;color:var(--p-green-500)}
</style>
