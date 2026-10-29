/**
 * Interactive AND filtering for a manually selected UiAutomator hierarchy node.
 * Node IDs and parent IDs are ephemeral DISPLAY metadata, not executable keys.
 * Native selectors are used when possible; structural filters compile to XPath 1.0.
 */
import type { AutomationHierarchyNode, AutomationSelector } from '../types/automation';
import { xpathString } from './automation-relational-locator.ts';

export const FILTER_FIELDS = [
  'resource_id','description','text','class_name','package',
  'clickable','enabled','long_clickable','focusable','checkable','scrollable',
  'selected','checked','password','bounds',
] as const;
export type FilterField = typeof FILTER_FIELDS[number];
export type FilterOption = {
  id:string; field:FilterField; scope:'self'|'ancestor'; distance:number;
  nodeId:number; label:string; value:string|boolean; displayValue:string;
  caution?:string;
};
export type FilterSnapshot = {nodes:AutomationHierarchyNode[];truncated:boolean};
const BOOLEAN_FIELDS:ReadonlySet<string> = new Set([
  'clickable','enabled','long_clickable','focusable','checkable','scrollable',
  'selected','checked','password',
]);
const NATIVE_FIELDS:ReadonlySet<string> = new Set([
  'resource_id','description','class_name','package',
  'clickable','enabled','long_clickable','focusable','checkable','scrollable',
]);
const XML_FIELDS:Record<FilterField,string> = {
  resource_id:'resource-id', description:'content-desc',text:'text',
  class_name:'class',package:'package',clickable:'clickable',enabled:'enabled',
  long_clickable:'long-clickable',focusable:'focusable',checkable:'checkable',
  scrollable:'scrollable',selected:'selected',checked:'checked',
  password:'password',bounds:'bounds',
};
const LABELS:Record<FilterField,string> = {
  resource_id:'Resource ID',description:'无障碍描述',text:'文本（分段模糊）',class_name:'类名',
  package:'应用包名',clickable:'可点击',enabled:'已启用',
  long_clickable:'支持长按',focusable:'可聚焦',checkable:'可勾选',
  scrollable:'可滚动',selected:'已选中',checked:'已勾选',password:'密码控件',
  bounds:'控件区域',
};
const size=(v:string)=>new TextEncoder().encode(v).length;

/**
 * Text locator values are split on whitespace into independent fragments.
 * All non-numeric fragments must occur in the SAME node's text; standalone
 * numbers are ignored because values such as counters change between frames.
 */
export function textMatchSegments(value:string):string[] {
  const source=value.trim().split(/\s+/u).filter(Boolean);
  const unique=[...new Set(source)].filter(part=>!/^\d+(?:[.,]\d+)*$/.test(part));
  return unique.length?unique:[value.trim()];
}
export function textMatches(actual:string,expected:string):boolean {
  return textMatchSegments(expected).every(part=>actual.includes(part));
}

export function boundsText(b:AutomationHierarchyNode['bounds']):string {
  return `[${b.left},${b.top}][${b.right},${b.bottom}]`;
}
function getValue(node:AutomationHierarchyNode,key:FilterField):string|boolean|null {
  if(key==='bounds') {
    // The executor compares @bounds against original XML, never clamped pixels.
    return typeof node.raw_bounds === 'string' && /^\[-?\d+,-?\d+\]\[-?\d+,-?\d+\]$/.test(node.raw_bounds)
      ? node.raw_bounds : null;
  }
  if((key==='text'||key==='description')&&node.password)return null;
  const value=node[key];
  if(BOOLEAN_FIELDS.has(key))return typeof value==='boolean'?value:null;
  return typeof value==='string'&&value.trim()&&size(value)<=256?value:null;
}
export function buildFilterOptions(shot:FilterSnapshot,selectedId:number):FilterOption[] {
  if(shot.truncated || shot.nodes.length>2500)return [];
  const byId=new Map(shot.nodes.map(node=>[node.id,node]));
  if(byId.size!==shot.nodes.length)return [];
  const target=byId.get(selectedId);
  if(!target||target.password)return [];
  const options:FilterOption[]=[];
  let node:AutomationHierarchyNode|undefined=target;
  const seen=new Set<number>();
  for(let level=0;node&&level<=8&&!seen.has(node.id);level++){
    seen.add(node.id);
    const distance=target.depth-node.depth;
    if(distance<0||distance>32)break;
    for(const field of FILTER_FIELDS){
      const value=getValue(node,field);
      if(value===null)continue;
      const scope=level===0?'self':'ancestor';
      if(scope==='ancestor' && node.password) continue;
      const caution=field==='text'
        ? '按空白拆分多个文字片段，所有片段必须出现在同一控件；纯数字片段自动忽略以适配动态数值。'
        : field==='bounds'
          ? '区域随布局和分辨率变化，建议只作为最后的限定条件。'
          : (field==='selected'||field==='checked')
            ? '这是动态状态，操作后可能变化。' : undefined;
      options.push({
        id:level===0?`self:${field}`:`ancestor:${distance}:${field}`,
        field,scope,distance,nodeId:node.id,label:LABELS[field],
        value,displayValue:typeof value==='boolean'?(value?'是 (true)':'否 (false)'):value,caution,
      });
    }
    node=node.parent_id===null?undefined:byId.get(node.parent_id);
  }
  return options;
}
function parentAt(node:AutomationHierarchyNode,depth:number,byId:Map<number,AutomationHierarchyNode>) {
  const visited=new Set<number>();let current:AutomationHierarchyNode|undefined=node;
  // The browser hierarchy can skip zero-area XML elements. Depth difference,
  // not the number of retained parents, determines the executor XPath distance.
  while(current && node.depth-current.depth<depth && !visited.has(current.id)){
    visited.add(current.id);current=current.parent_id===null?undefined:byId.get(current.parent_id);
  }
  return current && node.depth-current.depth===depth?current:undefined;
}
export function filterHierarchy(shot:FilterSnapshot,allOptions:FilterOption[],ids:readonly string[]):AutomationHierarchyNode[] {
  if(shot.truncated||shot.nodes.length>2500)return [];
  if(!ids.length) return shot.nodes;
  const selected=new Map(allOptions.filter(x=>ids.includes(x.id)).map(x=>[x.id,x]));
  if(selected.size!==new Set(ids).size)return [];
  const byId=new Map(shot.nodes.map(n=>[n.id,n]));
  if(byId.size!==shot.nodes.length)return [];
  return shot.nodes.filter(node=>[...selected.values()].every(x=>{
    const subject=x.scope==='self'?node:parentAt(node,x.distance,byId);
    if(subject===undefined)return false;
    const actual=getValue(subject,x.field);
    if(x.field==='text'&&typeof x.value==='string')
      return typeof actual==='string'&&textMatches(actual,x.value);
    return actual===x.value;
  }));
}
function xpathCondition(x:FilterOption):string {
  if(x.field==='text'&&typeof x.value==='string')
    return textMatchSegments(x.value).map(part=>'contains(@text,'+xpathString(part)+')').join(' and ');
  const literal=x.value===true?'true':x.value===false?'false':x.value;
  return '@'+XML_FIELDS[x.field]+'='+xpathString(literal);
}
export function buildCrossFilterSelector(
  options:FilterOption[],ids:readonly string[],
):{selector:AutomationSelector|null;error?:string;requiresVerification:boolean}{
  const chosen=options.filter(x=>ids.includes(x.id));
  if(!chosen.length || chosen.length!==new Set(ids).size)
    return {selector:null,error:'至少勾选一项有效属性。',requiresVerification:false};
  const native=chosen.every(x=>x.scope==='self'&&NATIVE_FIELDS.has(x.field));
  if(native){
    const selector=Object.fromEntries(chosen.map(x=>[x.field,x.value])) as AutomationSelector;
    return {selector,requiresVerification:false};
  }
  const constraints=chosen.map(x=>x.scope==='self'?xpathCondition(x)
    :`ancestor::*[${x.distance}][${xpathCondition(x)}]`);
  const xpath='//*['+constraints.join(' and ')+']';
  if(size(xpath)>512)return {selector:null,error:'组合 XPath 超过 512 字节，请减少条件或去掉较长的文字。',requiresVerification:true};
  return {selector:{xpath},requiresVerification:true};
}
