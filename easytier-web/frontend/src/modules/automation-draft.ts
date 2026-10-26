import type { AutomationStep } from '../types/automation';
import { analyzeAutomationFlow } from './automation-flow.ts';

export interface AutomationDraftWorkspace {
    ownerId: number;
    editingId?: string;
    revision?: number;
    name: string;
    description: string;
    steps: AutomationStep[];
    selectedStepId?: string;
    baseline: string;
}

/** Session-only draft: never writes scripts, text parameters or selectors to browser storage. */
export function emptyAutomationDraft(ownerId: number): AutomationDraftWorkspace {
    return {
        ownerId, name: '', description: '', steps: [], selectedStepId: undefined,
        baseline: JSON.stringify({ name: '', description: '', steps: [] }),
    };
}

/** Both recording and inspector append into one per-owner workspace. */
export function appendAutomationDraftSteps(
    draft: AutomationDraftWorkspace, ownerId: number, incoming: AutomationStep[],
    newId: () => string,
): boolean {
    if (draft.ownerId !== ownerId || !incoming.length
        || draft.steps.length + incoming.length > 100 || analyzeAutomationFlow(draft.steps).problem) return false;
    const ids = new Set(draft.steps.map(step => step.id));
    const added = incoming.map(step => {
        let id = step.id;
        if (!id || ids.has(id)) {
            // Use distinct step IDs so an execution result can always refer to one step.
            do { id = newId(); } while (!id || ids.has(id));
        }
        ids.add(id);
        return { ...step, id, selector: step.selector ? { ...step.selector } : undefined,
            scroll_container: step.scroll_container ? { ...step.scroll_container } : undefined };
    });
    const next = [...draft.steps, ...added];
    if (analyzeAutomationFlow(next).problem) return false;
    draft.steps = next;
    draft.selectedStepId = added[0].id;
    if (!draft.name.trim()) draft.name = '自动化草稿';
    return true;
}
