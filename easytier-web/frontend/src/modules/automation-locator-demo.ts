/**
 * T09 - read-only locator preview against the current UiAutomator2 snapshot.
 * No touch, app command or script execution is permitted in this module.
 * The executor evaluates XPath using its original XML, which is not provided
 * by this API. Never fake an XPath match by approximating that XML.
 */
import type { AutomationHierarchy, AutomationHierarchyNode, AutomationSelector, AutomationStep } from '../types/automation';

export type LocatorDemoStatus = 'one' | 'none' | 'many' | 'unsupported' | 'stale';
export interface LocatorDemoResult {
    status: LocatorDemoStatus;
    count: number;
    message: string;
    capturedAt: string | null;
    sourceWidth: number;
    sourceHeight: number;
    bounds: AutomationHierarchyNode['bounds'] | null;
}
export interface LocatorDemoOptions {
    deviceId: string;
    targetFingerprint: string;
    videoWidth: number;
    videoHeight: number;
    nowMs?: number;
    maxAgeMs?: number;
}
const fields = ['resource_id','text','description','class_name','package'] as const;
type NativeField = typeof fields[number];
const fail = (status: Exclude<LocatorDemoStatus,'one'|'many'>, message:string): LocatorDemoResult =>
    ({status,count:0,message,capturedAt:null,sourceWidth:0,sourceHeight:0,bounds:null});
const finite = (n:number) => Number.isFinite(n);
function validBounds(b:AutomationHierarchyNode['bounds'],w:number,h:number) {
    return [b.left,b.top,b.right,b.bottom].every(finite)
        && b.left>=0 && b.top>=0 && b.right>b.left && b.bottom>b.top
        && b.right<=w && b.bottom<=h;
}
function nativeSelector(selector:AutomationSelector): [NativeField,string][] | null {
    const entries=Object.entries(selector);
    if (!entries.length || entries.some(([key,value]) =>
        !fields.includes(key as NativeField) || typeof value!=='string' ||
        !value.trim() || new TextEncoder().encode(value).length>256)) return null;
    return entries as [NativeField,string][];
}
function stepSelector(step:AutomationStep): AutomationSelector | null {
    return step.selector ?? null;
}
export function locateReadOnlyStep(
    hierarchy:AutomationHierarchy, step:AutomationStep, options:LocatorDemoOptions,
):LocatorDemoResult {
    const selector=stepSelector(step);
    if (!selector) return fail('unsupported',
        '此步骤没有控件定位条件。坐标或非控件操作无法验证控件匹配；可在真实单步试跑中检查。');
    if (selector.xpath!==undefined) return fail('unsupported',
        'XPath 由执行器基于原始 XML 解析，目前只读接口不返回原始 XML，无法可靠高亮。请使用组合控件条件。');
    const fieldsToMatch=nativeSelector(selector);
    if (!fieldsToMatch) return fail('unsupported',
        '定位条件为空、格式不合法或包含不支持的字段，请先编辑步骤。');
    if (hierarchy.device_id!==options.deviceId ||
        !options.targetFingerprint || hierarchy.target_fingerprint!==options.targetFingerprint)
        return fail('stale','当前设备绑定已变化；请重新发起定位演示。');
    const shot=hierarchy.snapshot, captured=Date.parse(hierarchy.captured_at);
    const now=options.nowMs??Date.now();
    if (!finite(captured)||!finite(now)||now-captured>(options.maxAgeMs??20000)
        ||captured-now>5000)
        return fail('stale','控件树快照已过期或采集时间无效，请重新定位。');
    if (!shot || !Number.isInteger(shot.width)||!Number.isInteger(shot.height)||
        shot.width<=0||shot.height<=0||!Array.isArray(shot.nodes)||
        !finite(options.videoWidth)||!finite(options.videoHeight)||
        options.videoWidth<=0||options.videoHeight<=0)
        return fail('stale','手机画面或控件快照尺寸无效。');
    if (Math.abs((shot.width/shot.height)/(options.videoWidth/options.videoHeight)-1)>=0.035)
        return fail('stale','控件树与手机画面方向或比例已变化，请旋转完成后重试。');
    if (shot.truncated) return fail('stale','控件树已截断，无法证明定位唯一；请精简页面后重新获取。');
    const uniqueIds=new Set<number>();
    for(const node of shot.nodes) {
        if(!Number.isInteger(node.id)||uniqueIds.has(node.id))
            return fail('stale','控件树节点编号异常，请重新获取。');
        uniqueIds.add(node.id);
    }
    const matches=shot.nodes.filter(node=>
        fieldsToMatch.every(([key,value])=>
            // Android password fields must never be matched using displayed text.
            !(node.password&&(key==='text'||key==='description')) && node[key]===value));
    if (matches.length===0) return {
        status:'none',count:0,capturedAt:hierarchy.captured_at,
        sourceWidth:shot.width,sourceHeight:shot.height,bounds:null,
        message:step.type==='wait_gone'||step.type==='if_gone'
            ? '未匹配到控件（对“等待消失”类步骤可能符合预期）；未执行脚本。'
            : '当前页面没有匹配到该控件，请检查页面状态或重新拾取。',
    };
    if (matches.length>1) return {
        status:'many',count:matches.length,capturedAt:hierarchy.captured_at,
        sourceWidth:shot.width,sourceHeight:shot.height,bounds:null,
        message:'当前快照匹配 '+matches.length+' 个控件，定位条件不唯一；请补充约束后重试。',
    };
    const node=matches[0];
    if(!validBounds(node.bounds,shot.width,shot.height))
        return fail('stale','命中的控件没有可靠的屏幕矩形，无法安全高亮。');
    return {status:'one',count:1,message:'定位到 1 个控件：仅高亮画面，不执行点击或输入。',
        capturedAt:hierarchy.captured_at,sourceWidth:shot.width,sourceHeight:shot.height,
        bounds:{...node.bounds}};
}
