/**
 * LOC-08: validate a manually picked target using a NEW hierarchy capture.
 * A stale timestamp is not a reason to discard the user's AND filters.
 * Only the same visible node under the same screen geometry may be recorded.
 */
import type { AutomationHierarchy, AutomationHierarchyNode, AutomationSelector } from '../types/automation';

type Bounds = AutomationHierarchyNode['bounds'];
const edges: (keyof Bounds)[] = ['left','top','right','bottom'];
export function samePickedBounds(a: Bounds,b: Bounds,tolerance=3):boolean {
    return edges.every(k=>Number.isFinite(a[k])&&Number.isFinite(b[k])
        && Math.abs(a[k]-b[k])<=tolerance);
}
function sameSemanticTarget(a:AutomationHierarchyNode,b:AutomationHierarchyNode):boolean {
    if(a.password||b.password||a.class_name!==b.class_name||a.package!==b.package)return false;
    // A repeated name alone is not proof. Require unchanged visible geometry
    // and any populated identity evidence, even when it wasn't in the selector.
    return (['resource_id','description','text'] as const).every(k=>
        !a[k] || a[k]===b[k]);
}
export function revalidatePickedNativeTarget(
    source:AutomationHierarchyNode,
    originalWidth:number,originalHeight:number,
    fresh:AutomationHierarchy,
    selector:AutomationSelector|null,
):boolean {
    const tree=fresh.snapshot;
    if(tree.truncated||tree.width!==originalWidth||tree.height!==originalHeight
        || tree.nodes.length===0 || source.password)return false;
    const visible=tree.nodes.filter(node=>samePickedBounds(source.bounds,node.bounds)
        && sameSemanticTarget(source,node));
    if(!selector)return visible.length===1;
    if(selector.xpath)return false; // must be checked on raw XML via executor
    const entries=Object.entries(selector);
    if(!entries.length || entries.some(([key,value])=>![
        'resource_id','description','text','class_name','package',
        'clickable','enabled','long_clickable','focusable','checkable','scrollable',
    ].includes(key) || (
        typeof value!=='boolean' && (typeof value!=='string'||!value.trim())
    )))return false;
    const candidates=tree.nodes.filter(node=>entries.every(([key,value])=>
        node[key as keyof AutomationHierarchyNode]===value));
    return candidates.length===1&&visible.length===1&&candidates[0].id===visible[0].id;
}
