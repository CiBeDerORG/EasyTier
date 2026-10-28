<script setup lang="ts">
import {computed,ref,watch} from 'vue';
import type {TrialLocatorDiagnosis} from '../../modules/automation-trial-diagnostics';
const props=defineProps<{diagnosis:TrialLocatorDiagnosis}>();
const page=ref(0);
const size=20;
const pages=computed(()=>Math.max(1,Math.ceil(props.diagnosis.matches.length/size)));
const visibleRows=computed(()=>props.diagnosis.matches.slice(page.value*size,(page.value+1)*size));
const attrs=computed(()=>[...new Set(props.diagnosis.matches.flatMap(x=>Object.keys(x.attributes)))]);
const parents=computed(()=>[...new Set(props.diagnosis.matches.flatMap(x=>Object.keys(x.parent)))]);
watch(()=>props.diagnosis,()=>{page.value=0;});
const nice=(x:string)=>({
  'content-desc':'无障碍描述','resource-id':'Resource ID','class':'类名',
  'package':'应用包名','text':'文字','bounds':'范围','clickable':'可点击',
  'enabled':'已启用','long-clickable':'可长按','focusable':'可聚焦',
  'scrollable':'可滚动','selected':'已选中','checked':'已勾选',
  'password':'密码控件','index':'XML 序号',
} as Record<string,string>)[x]??x;
</script>
<template>
  <section class="compare-root" aria-label="单步定位匹配控件对比">
    <p class="compare-message">
      当前手机 XML 匹配到 <strong>{{diagnosis.count}}</strong> 个控件。
      <template v-if="diagnosis.count===0">请检查定位条件和手机页面。</template>
      <template v-else>以下是只读对比结果，不会操作手机。</template>
    </p>
    <small v-if="diagnosis.limited" class="compare-warning">
      匹配数超过 80，仅展示前 80 项；完整数量为 {{diagnosis.count}}。
    </small>
    <details class="compare-selector">
      <summary>查看本步骤定位条件</summary>
      <code>{{JSON.stringify(diagnosis.selector)}}</code>
    </details>
    <div v-if="diagnosis.matches.length" class="compare-scroller">
      <table>
        <thead>
          <tr>
            <th>匹配序号</th><th>XML 深度</th>
            <th v-for="a in attrs" :key="'a'+a">{{nice(a)}}</th>
            <th v-for="a in parents" :key="'p'+a">父级 · {{nice(a)}}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(item,index) in visibleRows" :key="page*size+index">
            <th>{{page*size+index+1}}</th><td>{{item.depth}}</td>
            <td v-for="a in attrs" :key="'a'+a" :title="item.attributes[a]||'—'">
              {{item.attributes[a]??'—'}}
            </td>
            <td v-for="a in parents" :key="'p'+a" :title="item.parent[a]||'—'">
              {{item.parent[a]??'—'}}
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-if="pages>1" class="compare-pages">
      <button type="button" :disabled="page===0" @click="page--">上一页</button>
      <span>{{page+1}} / {{pages}}</span>
      <button type="button" :disabled="page>=pages-1" @click="page++">下一页</button>
    </div>
    <small class="compare-footer">
      节点序号、范围和 XML 层级可能随页面刷新变化；对比时优先查找稳定的
      Resource ID、描述、类名及父级属性。密码控件的敏感内容不会展示。
    </small>
  </section>
</template>
<style scoped>
.compare-root{display:grid;gap:12px;min-width:0;max-width:min(80vw,960px)}
.compare-message{font-size:13px;margin:0;line-height:1.6}
.compare-message strong{color:var(--p-primary-color)}
.compare-warning{color:var(--p-orange-500)}
.compare-selector{font-size:11px;color:var(--console-muted);min-width:0}
.compare-selector code{display:block;margin-top:7px;white-space:pre-wrap;overflow-wrap:anywhere}
.compare-scroller{overflow:auto;max-height:min(55vh,520px);max-width:100%;border:1px solid var(--console-border);border-radius:7px}
.compare-scroller table{width:max-content;min-width:100%;border-collapse:collapse;font-size:11px;white-space:nowrap}
.compare-scroller th,.compare-scroller td{text-align:left;border-bottom:1px solid var(--console-border);padding:8px 10px;max-width:330px;overflow:hidden;text-overflow:ellipsis}
.compare-scroller thead th{position:sticky;top:0;background:var(--console-panel-raised);z-index:1}
.compare-scroller tbody tr:nth-child(even){background:var(--console-accent-soft)}
.compare-pages{display:flex;justify-content:center;align-items:center;gap:12px;font-size:11px}
.compare-pages button{border:1px solid var(--console-border);border-radius:5px;padding:5px 8px;color:var(--p-primary-color);background:transparent;cursor:pointer}
.compare-pages button:disabled{opacity:.4}
.compare-footer{color:var(--console-muted);font-size:11px;line-height:1.6}
</style>
