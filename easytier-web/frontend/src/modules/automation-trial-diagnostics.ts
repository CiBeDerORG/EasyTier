/** Read-only, bounded UIAutomator selector diagnostics for one phone. */
import type { AutomationSelector,AutomationHierarchyNode } from '../types/automation';
import { xpathString } from './automation-relational-locator.ts';
export interface TrialLocatorRow {
  attributes:Record<string,string>;
  parent:Record<string,string>;
  depth:number;
}
export interface TrialLocatorDiagnosis {
  status:'none'|'many'|'one';
  count:number; limited:boolean; matches:TrialLocatorRow[];
  selector:AutomationSelector; capturedAt:string;
}
const XML_KEYS:Record<string,string>={
  resource_id:'resource-id',description:'content-desc',text:'text',
  class_name:'class',package:'package',clickable:'clickable',enabled:'enabled',
  long_clickable:'long-clickable',focusable:'focusable',checkable:'checkable',scrollable:'scrollable',
};
/** Compiles only allowlisted native selector fields; never interpolates XPath from arbitrary keys. */
export function diagnosticXPath(selector:AutomationSelector):string|null {
  if(selector.xpath)return selector.xpath;
  const entries=Object.entries(selector);
  if(!entries.length || entries.some(([key,value])=>!(key in XML_KEYS) ||
     (typeof value!=='string' && typeof value!=='boolean') ||
     (typeof value==='string' && !value.trim())))return null;
  const conditions=entries.map(([key,value])=>
    '@'+XML_KEYS[key]+'='+xpathString(String(value)));
  const xpath='//*['+conditions.join(' and ')+']';
  return new TextEncoder().encode(xpath).length<=512?xpath:null;
}
export function normalizeTrialDiagnosis(
   selector:AutomationSelector,
   report:{count?:number;limited?:boolean;matches?:TrialLocatorRow[]},
   capturedAt:string,
):TrialLocatorDiagnosis|null{
   if(!Number.isInteger(report.count) || (report.count??-1)<0 ||
      !Array.isArray(report.matches) || report.matches.length>80)return null;
   const count=report.count!;
   return {selector:{...selector},capturedAt,count,limited:!!report.limited,
     status:count===0?'none':count===1?'one':'many',matches:report.matches};
}
export function summarizeTrialDiagnosis(d:TrialLocatorDiagnosis):string{
  if(d.count===0)return '定位条件未找到控件（0 个）。请确认当前手机页面。条件：'+(d.selector.xpath||JSON.stringify(d.selector));
  if(d.count>1)return '定位条件匹配 '+d.count+' 个控件，已打开属性对比列表；请增加区分条件。';
  return '当前定位条件唯一命中 1 个控件。';
}
export function nativeSnapshotMatches(
 nodes:AutomationHierarchyNode[],selector:AutomationSelector,
):AutomationHierarchyNode[]{
 if(selector.xpath)return [];
 return nodes.filter(node=>Object.entries(selector).every(([key,v])=>
    (node as unknown as Record<string,unknown>)[key]===v));
}
