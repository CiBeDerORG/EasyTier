/**
 * LOC-08: bounded semantic relation locator suggestions.
 * Only the selected hierarchy node is the ACTION target. Anchors are read-only
 * evidence; a candidate never falls back to a positional instance selector.
 * Backend must independently evaluate XPath on a freshly captured raw XML.
 */
import type { AutomationHierarchyNode, AutomationSelector } from '../types/automation';
import type { LocatorCandidate } from './automation-locator-candidates';

type Snapshot = { nodes: AutomationHierarchyNode[]; truncated: boolean };
type StringField = 'resource_id' | 'description' | 'text' | 'class_name';
type Attr = 'resource-id' | 'content-desc' | 'text' | 'class';
const attr: Record<StringField, Attr> = {
    resource_id:'resource-id', description:'content-desc', text:'text', class_name:'class',
};
const label: Record<StringField,string> = {
    resource_id:'Resource ID', description:'描述', text:'文字', class_name:'类名',
};
const utf8 = (v: string) => new TextEncoder().encode(v).length;
const valid = (s: unknown): s is string => typeof s === 'string' && !!s.trim() && utf8(s) <= 256;

/** XPath 1.0 has no escaping for apostrophes; use concat only when necessary. */
export function xpathString(value: string): string {
    if (!value.includes("'")) return "'" + value + "'";
    if (!value.includes('"')) return '"' + value + '"';
    return "concat(" + value.split("'").map((part, i) =>
        (i ? ',"\'",' : '') + "'" + part + "'").join('') + ")";
}
function condition(field: StringField, value: string): string {
    return '@' + attr[field] + '=' + xpathString(value);
}
type Evidence = { field: StringField; value: string };
function evidence(node: AutomationHierarchyNode, forAnchor: boolean): Evidence[] {
    if (node.password) return [];
    const fields: StringField[] = forAnchor
        ? ['resource_id','description','text']
        : ['resource_id','description','text','class_name'];
    return fields.filter(key => valid(node[key])).map(field => ({field,value:node[field]}));
}
function predicates(target: AutomationHierarchyNode): Evidence[][] {
    const e = evidence(target,false);
    const singles=e.map(x=>[x]);
    const pairs: Evidence[][]=[];
    for(let i=0;i<e.length;i++) for(let j=i+1;j<e.length;j++) pairs.push([e[i],e[j]]);
    // Prefer semantic attributes, retain class for a fallback if the target
    // is an anonymous FrameLayout (does not invent index/instance).
    return [...singles,...pairs].sort((a,b)=>
        Number(a.some(x=>x.field==='class_name'))-Number(b.some(x=>x.field==='class_name'))
        || a.length-b.length);
}
function buildPredicate(e:Evidence[]):string {return e.map(x=>condition(x.field,x.value)).join(' and ');}
const selectorFrom = (xpath: string): AutomationSelector => ({xpath});
type Context = { byId: Map<number,AutomationHierarchyNode>; nodes: AutomationHierarchyNode[] };
function ancestorChain(ctx:Context,id:number):AutomationHierarchyNode[] {
    const result:AutomationHierarchyNode[]=[];const visited=new Set<number>();
    let node=ctx.byId.get(id);
    while(node&&!visited.has(node.id)&&result.length<32){
        result.push(node);visited.add(node.id);
        node=node.parent_id===null?undefined:ctx.byId.get(node.parent_id);
    }
    return result;
}
function inside(ctx:Context,node:AutomationHierarchyNode,ancestorId:number):boolean {
    if(node.id===ancestorId)return true;
    const visited=new Set<number>();let id=node.parent_id;
    while(id!==null&&!visited.has(id)){
        if(id===ancestorId)return true;
        visited.add(id);id=ctx.byId.get(id)?.parent_id??null;
    }
    return false;
}
function matches(node:AutomationHierarchyNode,pattern:Evidence[]):boolean {
    return !node.password && pattern.every(e=>node[e.field]===e.value);
}
function freshCandidate(id:string,labelText:string,xpath:string,score:number):LocatorCandidate {
    return {id,label:labelText,selector:selectorFrom(xpath),matches:1,unique:true,score};
}
/**
 * Compute only candidates that match the selected node exactly ON THIS SNAPSHOT.
 * XPath candidates must also be checked against fresh raw XML by the executor.
 * `manualAnchorId` is optional and only restricts the anchor search.
 */
export function rankRelationalCandidates(
    shot:Snapshot,selectedId:number,manualAnchorId:number|null=null,
):LocatorCandidate[] {
    if(shot.truncated||shot.nodes.length>1500||shot.nodes.length===0)return [];
    const byId=new Map<number,AutomationHierarchyNode>();
    for(const node of shot.nodes) {
        if(!Number.isInteger(node.id)||byId.has(node.id))return [];
        byId.set(node.id,node);
    }
    const target=byId.get(selectedId);
    if(!target||target.password||!evidence(target,false).length)return [];
    const ctx={byId,nodes:shot.nodes};const chain=ancestorChain(ctx,selectedId);
    const patterns=predicates(target);
    const candidates:LocatorCandidate[]=[];const seen=new Set<string>();
    const add=(id:string,title:string,path:string,score:number)=>{
        if(utf8(path)>512||seen.has(path))return;
        seen.add(path);candidates.push(freshCandidate(id,title,path,score));
    };
    // A stable, globally unique ancestor is a narrow search scope.
    {
        for(const ancestor of chain.slice(1,8)){
            if(manualAnchorId!==null && ancestor.id!==manualAnchorId)continue;
            if(ancestor.depth<2)continue;
            for(const key of evidence(ancestor,true)){
                if(shot.nodes.filter(n=>n[key.field]===key.value).length!==1)continue;
                for(const p of patterns) {
                    if(shot.nodes.filter(n=>inside(ctx,n,ancestor.id)&&n.id!==ancestor.id&&matches(n,p)).length!==1)continue;
                    add('rel:parent:'+ancestor.id+':'+p.map(x=>x.field).join('+'),
                        '唯一祖先 '+label[key.field]+' → '+p.map(x=>label[x.field]).join('+'),
                        '//*['+condition(key.field,key.value)+']//*['+buildPredicate(p)+']',
                        970-(p.length-1)*12-(target.depth-ancestor.depth)*3);
                }
            }
        }
    }
    // Semantically distinct anchor in the SAME bounded task container. The
    // anchor must itself be globally unique; shared page/root scope is denied.
    const anchors=manualAnchorId===null?shot.nodes:shot.nodes.filter(n=>n.id===manualAnchorId);
    for(const anchor of anchors) {
        if(anchor.id===target.id||anchor.password||inside(ctx,anchor,target.id)||inside(ctx,target,anchor.id))continue;
        const anchorChain=ancestorChain(ctx,anchor.id);
        const ancestor=chain.slice(1,9).find(a=>a.depth>=2&&anchorChain.some(b=>b.id===a.id));
        if(!ancestor||target.depth-ancestor.depth>7||anchor.depth-ancestor.depth>7)continue;
        const scope=shot.nodes.filter(n=>inside(ctx,n,ancestor.id));
        if(scope.length>120)continue; // protect against page-wide/global anchors
        for(const key of evidence(anchor,true)){
            if(shot.nodes.filter(n=>n[key.field]===key.value).length!==1)continue;
            const up=anchor.depth-ancestor.depth;
            if(up<1||up>7)continue;
            for(const p of patterns){
                const found=scope.filter(n=>n.id!==ancestor.id&&matches(n,p));
                if(found.length!==1||found[0].id!==selectedId)continue;
                // A distinctive anchor anchored at a fixed semantic container
                // level, not at a target instance/index.
                add('rel:anchor:'+anchor.id+':'+p.map(x=>x.field).join('+'),
                    (manualAnchorId===null?'同容器锚点 ':'手动锚点 ')+label[key.field]+' → '+p.map(x=>label[x.field]).join('+'),
                    '//*['+condition(key.field,key.value)+']/ancestor::*['+up+']//*['+buildPredicate(p)+']',
                    880-(p.length-1)*12-up*3);
            }
        }
    }
    return candidates.sort((a,b)=>b.score-a.score||a.id.localeCompare(b.id)).slice(0,16);
}
