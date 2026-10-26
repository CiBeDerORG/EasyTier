import type { AutomationRun, AutomationRunDevice, AutomationStep } from '../types/automation';

export type BusinessVerificationState = 'verified' | 'failed' | 'not_verified' | 'pending' | 'not_reached' | 'outcome_unknown' | 'partial';

const assertionTypes = new Set(['assert', 'assert_property']);
const completedStates = new Set(['completed', 'succeeded']);
const activeStates = new Set(['dispatch_pending', 'queued', 'accepted', 'starting', 'running']);

/** An explicit business check must be one unconditional, terminal assertion. */
export function businessCheckStepId(steps: AutomationStep[]): string | null {
    if (!steps.length) return null;
    const marked = steps.filter(step => step.business_verification === true);
    if (marked.length !== 1) return null;
    const last = steps[steps.length - 1];
    return last.id === marked[0].id && assertionTypes.has(last.type) ? last.id : null;
}

export function businessDeviceState(run: Pick<AutomationRun, 'script_snapshot'>,
                                    device: Pick<AutomationRunDevice, 'state' | 'steps'>): BusinessVerificationState {
    const id = businessCheckStepId(run.script_snapshot.steps);
    if (!id) return 'not_verified';
    const result = device.steps.find(step => step.step_id === id);
    if (completedStates.has(device.state) && result?.state === 'assertion_passed') return 'verified';
    if (result?.state === 'failed') return 'failed';
    if (device.state === 'outcome_unknown' || result?.state === 'outcome_unknown') return 'outcome_unknown';
    if (activeStates.has(device.state)) return 'pending';
    return 'not_reached';
}

export function businessRunState(run: Pick<AutomationRun, 'script_snapshot' | 'devices'>): BusinessVerificationState {
    if (!businessCheckStepId(run.script_snapshot.steps)) return 'not_verified';
    if (!run.devices.length) return 'not_reached';
    const states = run.devices.map(device => businessDeviceState(run, device));
    if (states.every(state => state === 'verified')) return 'verified';
    if (states.every(state => state === 'failed')) return 'failed';
    if (states.every(state => state === 'not_reached')) return 'not_reached';
    if (states.some(state => state === 'outcome_unknown')) return 'outcome_unknown';
    if (states.every(state => state === 'pending')) return 'pending';
    return 'partial';
}
