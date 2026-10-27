import type {
    AutomationRun, AutomationRunDevice, AutomationScript, AutomationStep, AutomationStepResult,
} from '../types/automation';

export interface AutomationRepairRequest {
    requestId: string;
    ownerId: number;
    runId: string;
    deviceId: string;
    stepId: string;
    resultState: string;
    scriptId?: string;
    revision?: number;
    scriptName: string;
    snapshotSteps: AutomationStep[];
}

export type AutomationRepairPlan =
    | { kind: 'saved'; script: AutomationScript; stepId: string; sourceRunId: string; sourceDeviceId: string }
    | { kind: 'copy'; name: string; steps: AutomationStep[]; stepId: string; sourceRunId: string;
        sourceDeviceId: string; sourceRevision?: number; reason: 'updated' | 'deleted' | 'inline' };

const repairableStates = new Set(['failed', 'interrupted', 'outcome_unknown']);

/** A known failed step in the authenticated, persisted run, not an inferred/queued step. */
export function createAutomationRepairRequest(
    run: AutomationRun, device: AutomationRunDevice, result: AutomationStepResult,
    ownerId: number, requestId: string,
): AutomationRepairRequest | null {
    if (!Number.isSafeInteger(ownerId) || ownerId <= 0 || !requestId
        || !run.id || !device.device_id || !repairableStates.has(result.state)) return null;
    const snapshot = run.script_snapshot;
    if (!snapshot?.steps?.length || snapshot.steps.length > 100
        || snapshot.steps.some(step => !step.id)
        || snapshot.steps.filter(step => step.id === result.step_id).length !== 1
        || !device.steps.some(step => step.step_id === result.step_id && step.state === result.state)) return null;
    return {
        requestId, ownerId, runId: run.id, deviceId: device.device_id,
        stepId: result.step_id, resultState: result.state,
        scriptId: snapshot.script_id, revision: snapshot.revision,
        scriptName: snapshot.name || run.name, snapshotSteps: cloneRepairSteps(snapshot.steps),
    };
}

export function cloneRepairSteps(steps: AutomationStep[]): AutomationStep[] {
    return steps.map(step => ({
        ...step,
        ...(step.selector ? { selector: { ...step.selector } } : {}),
        ...(step.scroll_container ? { scroll_container: { ...step.scroll_container } } : {}),
    }));
}

/** Never silently edit a newer script revision or mutate a historical snapshot. */
export function planAutomationRepair(
    request: AutomationRepairRequest, latest: AutomationScript[],
): AutomationRepairPlan | null {
    if (!request.snapshotSteps.length
        || request.snapshotSteps.filter(step => step.id === request.stepId).length !== 1) return null;
    const original = request.scriptId ? latest.find(script => script.id === request.scriptId) : undefined;
    if (original && request.revision === original.revision
        && JSON.stringify(original.steps) === JSON.stringify(request.snapshotSteps)) {
        return {
            kind: 'saved', script: original, stepId: request.stepId,
            sourceRunId: request.runId, sourceDeviceId: request.deviceId,
        };
    }
    return {
        kind: 'copy', name: request.scriptName || '执行记录修复草稿',
        steps: cloneRepairSteps(request.snapshotSteps), stepId: request.stepId,
        sourceRunId: request.runId, sourceDeviceId: request.deviceId,
        sourceRevision: request.revision,
        reason: !request.scriptId ? 'inline' : original ? 'updated' : 'deleted',
    };
}


/** T15: ephemeral link back from immutable run evidence to the shared card editor.
 * No run snapshot is mutated and no "retry" request is created here. */
export interface CardRepairContext {
    ownerId: number;
    runId: string;
    deviceId: string;
    stepId: string;
    resultState: string;
    sourceStepFingerprint: string;
    sourceRevision?: number;
    sourceKind: AutomationRepairPlan['kind'] | 'current';
    sourceReason?: 'updated' | 'deleted' | 'inline';
}

export function createCardRepairContext(
    request: AutomationRepairRequest, plan: AutomationRepairPlan,
): CardRepairContext | null {
    const source = request.snapshotSteps.filter(step => step.id === request.stepId);
    if (source.length !== 1 || plan.stepId !== request.stepId ||
        plan.sourceRunId !== request.runId || plan.sourceDeviceId !== request.deviceId ||
        !repairableStates.has(request.resultState) ||
        !planSteps(plan).some(step => step.id === request.stepId)) return null;
    return {
        ownerId: request.ownerId, runId: request.runId, deviceId: request.deviceId,
        stepId: request.stepId, resultState: request.resultState,
        sourceStepFingerprint: JSON.stringify(source[0]),
        sourceRevision: request.revision,
        sourceKind: plan.kind,
        ...(plan.kind === 'copy' ? {sourceReason: plan.reason} : {}),
    };
}
function planSteps(plan: AutomationRepairPlan): AutomationStep[] {
    return plan.kind === 'saved' ? plan.script.steps : plan.steps;
}

/** Workspaces created for history repair must be new server DRAFTS.
 * A safe copy is never allowed to inherit the newer published script's CAS id.
 * Caller must call draftSync.fork() before applying the result. */
export function cardRepairWorkspace(
    ownerId: number, plan: AutomationRepairPlan,
): Pick<import('./automation-draft').AutomationDraftWorkspace,
    'ownerId' | 'editingId' | 'revision' | 'name' | 'description' |
    'steps' | 'selectedStepId' | 'baseline'> {
    const steps = cloneRepairSteps(planSteps(plan));
    const name = plan.kind === 'saved' ? plan.script.name : plan.name;
    const description = plan.kind === 'saved' ? plan.script.description : '';
    return {
        ownerId,
        editingId: plan.kind === 'saved' ? plan.script.id : undefined,
        revision: plan.kind === 'saved' ? plan.script.revision : undefined,
        name, description, steps, selectedStepId: plan.stepId,
        baseline: plan.kind === 'saved'
            ? JSON.stringify({name,description,steps})
            : JSON.stringify({name:'',description:'',steps:[]}),
    };
}
