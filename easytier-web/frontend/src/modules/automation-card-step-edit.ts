import type { AutomationStep } from '../types/automation';
import type { AutomationDraftWorkspace } from './automation-draft';
import { analyzeAutomationFlow } from './automation-flow.ts';

/** Replace exactly one existing step in the account's shared draft (CAS). */
export function updateCardDraftStep(
    draft: AutomationDraftWorkspace, ownerId:number, stepId:string,
    expected:AutomationStep, edited:AutomationStep,
): boolean {
    if(draft.ownerId!==ownerId || !stepId || expected.id!==stepId || edited.id!==stepId
        || draft.steps.filter(s=>s.id===stepId).length!==1) return false;
    const index=draft.steps.findIndex(step=>step.id===stepId);
    if(JSON.stringify(draft.steps[index])!==JSON.stringify(expected)) return false;
    // Conditional blocks are governed by the flow editor, never resized here.
    if(edited.then_count!==expected.then_count || edited.else_count!==expected.else_count
        || edited.skip_count!==expected.skip_count) return false;
    if(edited.type==='input_text' && typeof edited.text==='string' && edited.text.length>2000) return false;
    const next=[...draft.steps];
    next[index]={...edited,selector:edited.selector?{...edited.selector}:undefined,
        scroll_container:edited.scroll_container?{...edited.scroll_container}:undefined};
    if(analyzeAutomationFlow(next).problem) return false;
    draft.steps=next;
    draft.selectedStepId=stepId;
    return true;
}
