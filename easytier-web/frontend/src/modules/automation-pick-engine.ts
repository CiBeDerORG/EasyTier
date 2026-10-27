/**
 * T04: read-only and deterministic UIAutomator2 point -> UI node matching.
 * Never assumes that a match is a confirmed selector or executes a touch.
 */
import type { AutomationHierarchy, AutomationHierarchyNode } from '../types/automation';
import type { PickerVideoPoint } from './automation-picker-state';
import { rankLocatorCandidates, type LocatorCandidate } from './automation-locator-candidates.ts';

export type PickResultStatus = 'matched' | 'outside' | 'stale' | 'invalid'
    | 'truncated' | 'not_found';
export interface PickMatch {
    status: PickResultStatus;
    message: string;
    nodeId: number | null;
    clickableParentId: number | null;
    /** Candidate IDs are only suggestions; a user must explicitly confirm. */
    locatorCandidates: LocatorCandidate[];
    /** Other intersecting node IDs in deterministic hit order. */
    overlappingNodeIds: number[];
    snapshotCapturedAt: string;
}

export interface PickOptions {
    targetFingerprint: string;
    nowMs?: number;
    maxAgeMs?: number;
}
type Bounds = AutomationHierarchyNode['bounds'];
const finite = (value: number) => Number.isFinite(value);
const area = (bounds: Bounds) => (bounds.right - bounds.left) * (bounds.bottom - bounds.top);
const validBounds = (bounds: Bounds, width: number, height: number) =>
    [bounds.left, bounds.top, bounds.right, bounds.bottom].every(finite)
    && bounds.left >= 0 && bounds.top >= 0 && bounds.right > bounds.left
    && bounds.bottom > bounds.top && bounds.right <= width && bounds.bottom <= height;

const empty = (status: PickResultStatus, message: string, capturedAt = ''): PickMatch => ({
    status, message, nodeId: null, clickableParentId: null,
    locatorCandidates: [], overlappingNodeIds: [], snapshotCapturedAt: capturedAt,
});

/**
 * Coordinates describe the EXACT decoded video frame at right click.
 * Scaling between otherwise compatible video/snapshot dimensions is valid,
 * but rotation, a future/stale timestamp and mismatched device never are.
 * Truncated snapshots can be inspected in the old tree, not trusted for a hit.
 */
export function matchPickerPoint(
    hierarchy: AutomationHierarchy,
    point: PickerVideoPoint,
    options: PickOptions,
): PickMatch {
    const shot = hierarchy.snapshot, timestamp = hierarchy.captured_at;
    if (hierarchy.target_fingerprint !== options.targetFingerprint)
        return empty('stale', '手机绑定已变化，请重新拾取。', timestamp);
    const now = options.nowMs ?? Date.now(), maxAge = options.maxAgeMs ?? 20000;
    const captured = Date.parse(timestamp);
    if (!finite(captured) || !finite(now) || now - captured > maxAge || captured - now > 5000)
        return empty('stale', '控件树采集时间已过期或不可信，请刷新后重新拾取。', timestamp);
    if (!shot || !Number.isInteger(shot.width) || !Number.isInteger(shot.height)
        || shot.width < 1 || shot.height < 1 || !Array.isArray(shot.nodes)
        || !finite(point.x) || !finite(point.y) || !finite(point.width) || !finite(point.height)
        || point.width <= 0 || point.height <= 0)
        return empty('invalid', '控件树或视频尺寸无效，无法定位。', timestamp);
    if (point.x < 0 || point.y < 0 || point.x >= point.width || point.y >= point.height)
        return empty('outside', '右键位置不在有效手机画面内。', timestamp);
    const videoRatio = point.width / point.height, shotRatio = shot.width / shot.height;
    if (!finite(videoRatio) || !finite(shotRatio) || Math.abs(shotRatio / videoRatio - 1) >= .035)
        return empty('stale', '控件树与手机画面方向或比例不一致，请刷新后重新拾取。', timestamp);
    if (shot.truncated)
        return empty('truncated', '控件树已截断，不能根据不完整快照自动匹配。', timestamp);

    // Apply the scale from the current encoded video to the UIA capture.
    const x = point.x / point.width * shot.width;
    const y = point.y / point.height * shot.height;
    const seen = new Set<number>();
    const nodes: AutomationHierarchyNode[] = [];
    for (const node of shot.nodes) {
        if (!Number.isInteger(node.id) || seen.has(node.id))
            return empty('invalid', '控件树存在重复或无效节点 ID，请刷新。', timestamp);
        seen.add(node.id);
        if (!validBounds(node.bounds, shot.width, shot.height)) continue;
        if (x >= node.bounds.left && x < node.bounds.right
            && y >= node.bounds.top && y < node.bounds.bottom) nodes.push(node);
    }
    if (!nodes.length) return empty('not_found', '该位置未找到可用控件，可刷新控件树或在树中手动选择。', timestamp);
    nodes.sort((a,b) => area(a.bounds) - area(b.bounds)
        || Number(b.clickable && b.enabled) - Number(a.clickable && a.enabled)
        || b.depth - a.depth || a.id - b.id);
    const target = nodes[0];
    const byId = new Map(shot.nodes.map(node => [node.id, node]));
    let clickableParentId: number | null = null;
    let parent = target.parent_id;
    const walked = new Set<number>([target.id]);
    while (parent !== null && !walked.has(parent)) {
        walked.add(parent);
        const node = byId.get(parent);
        if (!node) break;
        if (node.clickable && node.enabled) { clickableParentId = node.id; break; }
        parent = node.parent_id;
    }
    const locatorCandidates = rankLocatorCandidates(shot, target.id);
    return {
        status:'matched', nodeId:target.id, clickableParentId,
        overlappingNodeIds:nodes.map(node => node.id), locatorCandidates,
        snapshotCapturedAt:timestamp,
        message: target.password
            ? '已匹配敏感输入框；文字与描述保持隐藏，需手动确认安全的定位方式。'
            : locatorCandidates.some(candidate => candidate.unique)
                ? '已找到候选控件，请在属性面板确认定位条件。'
                : '已找到控件，但没有可确认的唯一定位条件；请调整选择。',
    };
}
