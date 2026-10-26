import type { AutomationRunInput, AutomationStep } from '../types/automation';

export type StepTrialSafety = 'read_only' | 'confirm_required' | 'unsupported';

const SAFE_CHECKS = new Set(['wait_for', 'wait_gone', 'assert', 'assert_property']);
const UNSUPPORTED = new Set(['if_gone', 'if_else']);

export function stepTrialSafety(step: AutomationStep): StepTrialSafety {
    if (UNSUPPORTED.has(step.type)) return 'unsupported';
    if (SAFE_CHECKS.has(step.type)) return 'read_only';
    // Screenshot does not mutate the phone, but creates a retained artifact
    // that might contain secrets, so it requires explicit acknowledgment.
    return 'confirm_required';
}

/** Explicitly opt out of the full-script business marker on an isolated trial. */
export function isolateAutomationStep(step: AutomationStep): AutomationStep | null {
    if (!step.id || stepTrialSafety(step) === 'unsupported') return null;
    const { business_verification: _ignored, ...clean } = step;
    return {
        ...clean,
        ...(clean.selector ? { selector: { ...clean.selector } } : {}),
        ...(clean.scroll_container ? { scroll_container: { ...clean.scroll_container } } : {}),
    };
}

export function makeSingleStepRun(
    step: AutomationStep, deviceId: string, requestId: string,
    parameters: Record<string, string>, confirmed: boolean,
): AutomationRunInput | null {
    if (!deviceId || !requestId
        || (stepTrialSafety(step) === 'confirm_required' && !confirmed)) return null;
    const isolated = isolateAutomationStep(step);
    if (!isolated) return null;
    return {
        request_id: requestId, name: '单步骤诊断',
        device_ids: [deviceId], concurrency: 1,
        parameters: { ...parameters }, steps: [isolated],
    };
}
