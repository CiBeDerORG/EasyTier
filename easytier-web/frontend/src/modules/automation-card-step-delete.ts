/**
 * T30/T31 — deletion planner over the ONE shared draft.
 * Never splice a forward-conditional flat array: use the existing tree
 * compiler and validate the entire candidate before exposing a mutation.
 */
import type { AutomationStep } from '../types/automation';
import { analyzeAutomationFlow } from './automation-flow';
import { removeFlowBlock } from './automation-flow-tree';

export interface AutomationStepDeletion {
    requestedId: string;
    targetId: string;
    affectedIds: string[];
    nextSteps: AutomationStep[];
    fingerprint: string;
    requiresConfirmation: boolean;
    reason: 'ordinary' | 'condition' | 'parent-condition';
}

export function planAutomationStepDeletion(
    steps: AutomationStep[], stepId: string,
): AutomationStepDeletion | null {
    const at = steps.findIndex(step => step.id === stepId);
    if (at < 0) return null;
    const layout = analyzeAutomationFlow(steps);
    if (layout.problem) return null;
    const conditional = steps[at].type === 'if_else' || steps[at].type === 'if_gone';
    let targetId = stepId;
    let nextSteps = removeFlowBlock(steps, targetId);
    let reason: AutomationStepDeletion['reason'] = conditional ? 'condition' : 'ordinary';
    // A branch must keep at least one child. Find the nearest ancestor block
    // that can be removed without leaving a broken Then/Else lane.
    if (!nextSteps) {
        const parents = [...(layout.entries[at]?.ancestors ?? [])].reverse();
        for (const parent of parents) {
            targetId = steps[parent.conditionIndex].id;
            nextSteps = removeFlowBlock(steps, targetId);
            if (nextSteps) { reason = 'parent-condition'; break; }
        }
    }
    if (!nextSteps) return null;
    const remaining = new Set(nextSteps.map(step => step.id));
    const affectedIds = steps.filter(step => !remaining.has(step.id)).map(step => step.id);
    if (!affectedIds.includes(stepId)) return null;
    return {
        requestedId: stepId, targetId, nextSteps,
        affectedIds, fingerprint: JSON.stringify(steps),
        reason, requiresConfirmation: reason !== 'ordinary' || affectedIds.length > 1,
    };
}
