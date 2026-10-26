import type { AutomationControl, AutomationStep } from '../types/automation';

export interface CapturedGesture {
    deviceId: string;
    x: number;
    y: number;
    width: number;
    height: number;
    startedAt: number;
    lastX: number;
    lastY: number;
    maxExcursion: number;
    pathDistance: number;
    valid: boolean;
}

export type GestureRecordingResult =
    | { kind: 'step'; step: AutomationStep }
    | { kind: 'confirm'; swipe: AutomationStep; drag: AutomationStep; durationMs: number }
    | { kind: 'invalid'; reason: 'incomplete' | 'too_long' | 'returned_to_origin' };

type GesturePoint = Pick<AutomationControl, 'x' | 'y' | 'width' | 'height'>;

function validPoint(point: GesturePoint): point is Required<GesturePoint> {
    return Number.isFinite(point.x) && Number.isFinite(point.y)
        && Number.isInteger(point.width) && Number.isInteger(point.height)
        && (point.width ?? 0) > 0 && (point.height ?? 0) > 0
        && point.x! >= 0 && point.y! >= 0
        && point.x! < point.width! && point.y! < point.height!;
}

export function beginCapturedGesture(deviceId: string, point: GesturePoint, now: number): CapturedGesture | null {
    if (!deviceId || !validPoint(point) || !Number.isFinite(now)) return null;
    return {
        deviceId, x: point.x, y: point.y, width: point.width, height: point.height,
        lastX: point.x, lastY: point.y, startedAt: now,
        maxExcursion: 0, pathDistance: 0, valid: true,
    };
}

/** O(1) memory: update the maximum excursion, even on very long move streams. */
export function observeCapturedGesture(capture: CapturedGesture, point: GesturePoint): boolean {
    if (!capture.valid || !validPoint(point) || point.width !== capture.width || point.height !== capture.height) {
        capture.valid = false;
        return false;
    }
    capture.pathDistance += Math.hypot(
        (point.x - capture.lastX) / capture.width,
        (point.y - capture.lastY) / capture.height,
    );
    capture.maxExcursion = Math.max(capture.maxExcursion, Math.hypot(
        (point.x - capture.x) / capture.width,
        (point.y - capture.y) / capture.height,
    ));
    capture.lastX = point.x;
    capture.lastY = point.y;
    return true;
}

/** Classify only a confirmed pointer-up; moving touch events are not actions. */
export function finishCapturedGesture(
    capture: CapturedGesture, point: GesturePoint, endedAt: number, stepId: string,
): GestureRecordingResult {
    if (!stepId || !Number.isFinite(endedAt) || !observeCapturedGesture(capture, point))
        return { kind: 'invalid', reason: 'incomplete' };
    const elapsed = Math.round(endedAt - capture.startedAt);
    if (elapsed < 0) return { kind: 'invalid', reason: 'incomplete' };
    if (elapsed > 3000) return { kind: 'invalid', reason: 'too_long' };
    const start = {
        id: stepId, x: capture.x / capture.width, y: capture.y / capture.height,
        width: capture.width, height: capture.height,
    };
    const travelToEnd = Math.hypot(
        (point.x! - capture.x) / capture.width,
        (point.y! - capture.y) / capture.height,
    );
    // Stationary gesture: distinguish a tap from a held press. A long hold
    // must never be mislabelled a swipe with equal start/end coordinates.
    if (capture.maxExcursion < 0.025) {
        return elapsed >= 500
            ? { kind: 'step', step: { ...start, type: 'long_press', duration_ms: Math.max(400, elapsed) } }
            : { kind: 'step', step: { ...start, type: 'tap' } };
    }
    // A finger may travel and return to the origin, but a flat swipe/drag
    // action cannot faithfully replay that path. Refuse a misleading script.
    if (travelToEnd < 0.025) return { kind: 'invalid', reason: 'returned_to_origin' };
    const end = { to_x: point.x! / capture.width, to_y: point.y! / capture.height };
    return {
        kind: 'confirm', durationMs: elapsed,
        swipe: { ...start, ...end, type: 'swipe', duration_ms: Math.max(100, elapsed) },
        drag: { ...start, ...end, type: 'drag', duration_ms: Math.max(300, elapsed) },
    };
}
