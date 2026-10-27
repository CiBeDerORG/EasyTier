/**
 * T16: read-only, account/device/fingerprint-scoped inspection before a
 * shared draft is tried on another phone.
 *
 * Evidence is deliberately EPHEMERAL. A saved script contains selectors and
 * coordinate reference dimensions, not a trusted origin-device certificate.
 * Never infer that a restored draft "belongs" to whichever phone is open.
 */
import type { AutomationDevice, AutomationHierarchy, AutomationStep } from '../types/automation';
import { locateReadOnlyStep } from './automation-locator-demo.ts';

export type CompatibilitySeverity = 'block' | 'confirm' | 'info';
export interface CompatibilityIssue {
    stepId: string;
    stepNumber: number;
    severity: CompatibilitySeverity;
    message: string;
}
export interface DeviceCompatibilityReview {
    ownerId: number;
    deviceId: string;
    targetFingerprint: string;
    capturedAt: string;
    viewport: { width: number; height: number };
    status: 'blocked' | 'confirm' | 'compatible';
    issues: CompatibilityIssue[];
    stepFingerprint: string;
}
export function reuseStepFingerprint(steps: AutomationStep[]): string {
    return JSON.stringify(steps);
}
function validSize(n: number) {return Number.isInteger(n) && n > 0 && n <= 16384;}
function absoluteGeometry(step: AutomationStep): boolean {
    return step.type === 'swipe' || step.type === 'drag'
        || ((step.type === 'tap' || step.type === 'long_press') &&
            step.x !== undefined && step.y !== undefined);
}
/** Native keys and app launches need no heavy UiAutomator2 helper. */
export function requiresReuseHierarchy(steps:AutomationStep[]):boolean {
    return steps.some(step=>absoluteGeometry(step) || !!step.selector || !!step.scroll_container);
}
export function statelessCompatibility(
    ownerId:number, device:AutomationDevice, steps:AutomationStep[],
):DeviceCompatibilityReview {
    if(ownerId<=0 || device.owner_user_id!==ownerId ||
        !device.connection.enabled || !!device.availability_reason || !device.target_fingerprint)
        throw new Error('目标手机没有有效的授权或 ADB 绑定。');
    if(requiresReuseHierarchy(steps))
        throw new Error('包含定位步骤，必须先读取本机最新控件快照。');
    return {ownerId,deviceId:device.id,targetFingerprint:device.target_fingerprint,
        capturedAt:new Date().toISOString(),viewport:{width:0,height:0},
        status:'compatible',issues:[],stepFingerprint:reuseStepFingerprint(steps)};
}
function issue(step: AutomationStep, i: number, severity: CompatibilitySeverity, message: string): CompatibilityIssue {
    return {stepId:step.id,stepNumber:i+1,severity,message};
}
export function reviewAutomationDeviceCompatibility(
    ownerId: number,
    device: AutomationDevice,
    hierarchy: AutomationHierarchy,
    steps: AutomationStep[],
    nowMs=Date.now(),
    options: { requireUniqueSelectors?: boolean } = {},
): DeviceCompatibilityReview {
    if(ownerId<=0 || !steps.length || device.owner_user_id!==ownerId ||
       !device.connection.enabled || device.availability_reason ||
       !device.target_fingerprint || hierarchy.device_id!==device.id ||
       hierarchy.target_fingerprint!==device.target_fingerprint)
        throw new Error('目标手机授权、ADB 服务或绑定指纹与快照不一致，已阻止复用。');

    const {width,height,truncated,nodes}=hierarchy.snapshot;
    const captured=Date.parse(hierarchy.captured_at);
    if(!validSize(width)||!validSize(height)||!Number.isFinite(captured) ||
       captured>nowMs+5000 || nowMs-captured>20000 || !Array.isArray(nodes))
        throw new Error('只读控件快照已过期或尺寸无法确认，禁止按历史坐标试跑。');

    const issues:CompatibilityIssue[]=[];
    for(const [i,step] of steps.entries()) {
        if(absoluteGeometry(step)) {
            if(!validSize(step.width ?? 0) || !validSize(step.height ?? 0) ||
               step.width!==width || step.height!==height) {
                issues.push(issue(step,i,'block',
                    '坐标参考画面 '+(step.width??'?')+'×'+(step.height??'?')+
                    ' 与目标手机 '+width+'×'+height+' 不匹配；必须重新拾取或重新录制，不会自动缩放坐标。'));
            } else {
                issues.push(issue(step,i,'confirm',
                    '当前手机尺寸与坐标参考画面一致，但设备不同或页面布局变化仍可能点错；需人工核实位置。'));
            }
        } else if(step.type==='tap' || step.type==='long_press') {
            // Selector-backed actions may still carry reference coordinates.
            // They are never assumed portable just because the size matches.
            issues.push(issue(step,i,'confirm',
                '此操作包含控件定位。跨手机需确认当前页面的目标唯一且正确。'));
        }
        if(step.selector) {
            if(step.selector.xpath) {
                issues.push(issue(step,i,options.requireUniqueSelectors?'block':'confirm',
                    'XPath 需要执行器 XML 解析；现有只读树无法证明跨手机定位唯一，建议重新拾取。'));
            } else if(truncated) {
                issues.push(issue(step,i,options.requireUniqueSelectors?'block':'confirm',
                    '控件树存在截断，无法证明当前手机定位唯一；请刷新控件树或重新拾取。'));
            } else {
                const demo=locateReadOnlyStep(hierarchy,step,{
                    deviceId:device.id,targetFingerprint:device.target_fingerprint!,
                    videoWidth:width,videoHeight:height,nowMs,
                });
                if(demo.status!=='one' && !(step.type==='wait_gone' && demo.status==='none')) {
                    issues.push(issue(step,i,options.requireUniqueSelectors?'block':'confirm',
                        '只读定位核验：'+demo.message+'；执行前应确认手机处于预期页面。'));
                }
            }
        }
        if(step.scroll_container) {
            issues.push(issue(step,i,'confirm',
                '滚动容器可能因手机尺寸、布局或应用版本不同而变化；需人工确认容器。'));
        }
    }
    const status=issues.some(item=>item.severity==='block')?'blocked'
        :issues.some(item=>item.severity==='confirm')?'confirm':'compatible';
    return {ownerId,deviceId:device.id,targetFingerprint:device.target_fingerprint!,
        capturedAt:hierarchy.captured_at,viewport:{width,height},status,issues,
        stepFingerprint:reuseStepFingerprint(steps)};
}
/** The same device/steps/target evidence is required at final confirmation. */
export function compatibilityStillCurrent(
    review:DeviceCompatibilityReview | null,
    owner:number, device:AutomationDevice, steps:AutomationStep[], nowMs=Date.now(),
):boolean {
    if(!review) return false;
    const seen=Date.parse(review.capturedAt);
    return review.ownerId===owner && review.deviceId===device.id &&
        device.owner_user_id===owner && device.target_fingerprint===review.targetFingerprint &&
        review.stepFingerprint===reuseStepFingerprint(steps) &&
        Number.isFinite(seen) && nowMs-seen<=20000 && seen<=nowMs+5000 &&
        review.status!=='blocked';
}
