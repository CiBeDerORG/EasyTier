/**
 * T40 — frozen single-step ▶ intent.
 *
 * A click makes one immutable device/step/request identity. A network-unknown
 * POST is never retried with a fresh UUID; the durable trial receipt decides
 * read-only reconciliation. This module never sends Android actions.
 */
import type { AutomationStep } from '../types/automation';
import type { DeviceCompatibilityReview } from './automation-device-compatibility';
import type { StepTrialSafety } from './automation-single-step';

export interface InlineStepRunIntent {
    readonly ownerId: number;
    readonly deviceId: string;
    readonly targetFingerprint: string;
    readonly stepId: string;
    readonly stepJson: string;
    readonly requestId: string;
    readonly step: AutomationStep;
}

export function freezeInlineStepRunIntent(
    ownerId: number, deviceId: string, fingerprint: string,
    step: AutomationStep | null, requestId: string,
): InlineStepRunIntent | null {
    if (!ownerId || !deviceId || !fingerprint || !step?.id || !requestId) return null;
    const stepJson = JSON.stringify(step);
    return Object.freeze({
        ownerId, deviceId, targetFingerprint: fingerprint, stepId: step.id,
        stepJson, requestId, step: JSON.parse(stepJson) as AutomationStep,
    });
}

export function inlineStepIntentStillCurrent(
    intent: InlineStepRunIntent | null,
    ownerId: number, deviceId: string, fingerprint: string | null,
    step: AutomationStep | null,
): boolean {
    return !!intent && intent.ownerId === ownerId && intent.deviceId === deviceId
        && intent.targetFingerprint === fingerprint && intent.stepId === step?.id
        && intent.stepJson === JSON.stringify(step);
}

/**
 * Read-only assertions and waits with a confirmed compatible target need no
 * second click after ▶. Real input, screenshot retention, coordinate/selector
 * uncertainty or parameters require explicit operator action in the
 * same original-video floating window.
 */
export function inlineStepMayAutoSubmit(
    safety: StepTrialSafety, review: DeviceCompatibilityReview | null,
    parameter: string | undefined,
): boolean {
    return safety === 'read_only' && review?.status === 'compatible' && !parameter;
}

/** T41 display-only projection; never causes a second POST or retries. */
export interface InlineStepRunProgress {
    ownerId: number;
    deviceId: string;
    stepId: string;
    requestId: string;
    runId: string | null;
    phase: 'preparing' | 'ready' | 'submitting' | 'running' |
        'action_sent' | 'succeeded' | 'failed' | 'cancelled' | 'unknown';
    label: string;
    message?: string;
    /** From the server's immutable step timestamps, not a guessed local timer. */
    elapsedMs?: number;
}
