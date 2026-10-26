import type { AutomationStep } from '../types/automation';
import { analyzeAutomationFlow, type BranchLane } from './automation-flow.ts';

/**
 * Editor-only tree. The persisted/executor wire format remains the existing
 * forward-only flat step array; no new action type is sent to the phone.
 */
export type FlowNode =
    | { kind: 'action'; step: AutomationStep }
    | { kind: 'condition'; step: AutomationStep; thenNodes: FlowNode[]; elseNodes: FlowNode[] };

const condition = (step: AutomationStep) => step.type === 'if_gone' || step.type === 'if_else';

const copy = (step: AutomationStep): AutomationStep => ({
    ...step,
    selector: step.selector ? { ...step.selector } : undefined,
    scroll_container: step.scroll_container ? { ...step.scroll_container } : undefined,
});

/** Parse only a fully valid script; a malformed input never gets silently repaired. */
export function parseFlowTree(flat: AutomationStep[]): FlowNode[] | null {
    if (analyzeAutomationFlow(flat).problem) return null;
    function range(first: number, last: number): FlowNode[] {
        const nodes: FlowNode[] = [];
        for (let index = first; index < last;) {
            const step = flat[index];
            if (!condition(step)) {
                nodes.push({ kind: 'action', step: copy(step) });
                index++;
                continue;
            }
            const count = step.type === 'if_gone' ? step.skip_count! : step.then_count!;
            const elseCount = step.type === 'if_else' ? step.else_count! : 0;
            const mid = index + 1 + count;
            const end = mid + elseCount;
            // analyzeAutomationFlow already checked that neither branch escapes its parent.
            if (end > last) throw new Error('Invalid nested condition boundary');
            nodes.push({
                kind: 'condition', step: copy(step),
                thenNodes: range(index + 1, mid), elseNodes: range(mid, end),
            });
            index = end;
        }
        return nodes;
    }
    return range(0, flat.length);
}

/** Compile the editor tree into the unchanged wire format, with recalculated spans. */
export function compileFlowTree(tree: FlowNode[]): AutomationStep[] | null {
    const result: AutomationStep[] = [];
    const ids = new Set<string>();
    function flatten(nodes: FlowNode[], depth: number): boolean {
        for (const node of nodes) {
            if (!node.step.id || ids.has(node.step.id) || result.length >= 100) return false;
            ids.add(node.step.id);
            if (node.kind === 'action') {
                if (condition(node.step)) return false;
                result.push(copy(node.step));
                continue;
            }
            if (!condition(node.step) || depth >= 3) return false;
            if (!node.thenNodes.length || node.thenNodes.length > 20
                || (node.step.type === 'if_else' && (!node.elseNodes.length || node.elseNodes.length > 20))
                || (node.step.type === 'if_gone' && node.elseNodes.length > 0)) return false;
            const at = result.length;
            result.push(copy(node.step));
            const start = result.length;
            if (!flatten(node.thenNodes, depth + 1)) return false;
            const thenCount = result.length - start;
            const mid = result.length;
            if (!flatten(node.elseNodes, depth + 1)) return false;
            const elseCount = result.length - mid;
            if (thenCount > 20 || (node.step.type === 'if_else' && elseCount > 20)) return false;
            if (node.step.type === 'if_gone') {
                result[at].skip_count = thenCount;
                delete result[at].then_count;
                delete result[at].else_count;
            } else {
                result[at].then_count = thenCount;
                result[at].else_count = elseCount;
                delete result[at].skip_count;
            }
        }
        return true;
    }
    if (!flatten(tree, 0) || analyzeAutomationFlow(result).problem) return null;
    return result;
}

function search(nodes: FlowNode[], id: string): FlowNode | undefined {
    for (const node of nodes) {
        if (node.step.id === id) return node;
        if (node.kind === 'condition') {
            const inner = search(node.thenNodes, id) ?? search(node.elseNodes, id);
            if (inner) return inner;
        }
    }
    return undefined;
}

/**
 * Remove an entire conditional block, never just its marker.
 * Reject deletion of the only child of a branch (a branch may not be empty).
 */
export function removeFlowBlock(flat: AutomationStep[], id: string): AutomationStep[] | null {
    const tree = parseFlowTree(flat);
    if (!tree) return null;
    function remove(nodes: FlowNode[], inBranch: boolean): boolean {
        const at = nodes.findIndex(node => node.step.id === id);
        if (at !== -1) {
            if (inBranch && nodes.length === 1) return false;
            nodes.splice(at, 1);
            return true;
        }
        for (const node of nodes) {
            if (node.kind === 'condition' &&
                (remove(node.thenNodes, true) || remove(node.elseNodes, node.step.type === 'if_else'))) return true;
        }
        return false;
    }
    return remove(tree, false) ? compileFlowTree(tree) : null;
}

/** Insert into a specific lane, letting the compiler adjust all enclosing counts. */
export function addFlowBranchStep(flat: AutomationStep[], conditionId: string, lane: BranchLane, step: AutomationStep): AutomationStep[] | null {
    const tree = parseFlowTree(flat);
    if (!tree || search(tree, step.id)) return null;
    const parent = search(tree, conditionId);
    if (!parent || parent.kind !== 'condition' || (lane === 'else' && parent.step.type !== 'if_else')) return null;
    (lane === 'then' ? parent.thenNodes : parent.elseNodes).push({ kind: 'action', step: copy(step) });
    return compileFlowTree(tree);
}

/** Converting a normal step to a conditional creates non-executable, unconfigured editor entries. */
export function createFlowCondition(
    flat: AutomationStep[], id: string, replacement: AutomationStep, placeholder: () => AutomationStep,
): AutomationStep[] | null {
    if (!condition(replacement)) return null;
    const tree = parseFlowTree(flat);
    if (!tree) return null;
    const target = search(tree, id);
    if (!target || target.kind !== 'action') return null;
    const thenStep = placeholder();
    const elseStep = replacement.type === 'if_else' ? placeholder() : undefined;
    if (condition(thenStep) || (elseStep && condition(elseStep))) return null;
    // The caller can create a blank key step, which validation refuses to execute
    // until the user explicitly selects a key or changes the action.
    Object.assign(target, {
        kind: 'condition', step: copy(replacement),
        thenNodes: [{ kind: 'action', step: copy(thenStep) }],
        elseNodes: elseStep ? [{ kind: 'action', step: copy(elseStep) }] : [],
    });
    return compileFlowTree(tree);
}
