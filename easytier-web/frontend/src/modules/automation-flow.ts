import type { AutomationStep } from '../types/automation';

export type BranchLane = 'then' | 'else';
export interface FlowAncestor {
    conditionIndex: number;
    lane: BranchLane;
}
export interface FlowEntry {
    index: number;
    ancestors: FlowAncestor[];
}
export interface FlowProblem {
    index: number;
    reason: 'invalid_span' | 'crossing' | 'too_deep';
}
export interface FlowLayout {
    entries: FlowEntry[];
    problem?: FlowProblem;
}

/** Mirror the Python/Rust forward-only condition rules before saving a script. */
export function analyzeAutomationFlow(steps: AutomationStep[]): FlowLayout {
    const entries: FlowEntry[] = [];
    const regions: { index: number; thenEnd: number; end: number }[] = [];
    for (let index = 0; index < steps.length; index++) {
        while (regions.length && index > regions[regions.length - 1].end) regions.pop();
        const ancestors = regions.map(region => ({
            conditionIndex: region.index,
            lane: (index <= region.thenEnd ? 'then' : 'else') as BranchLane,
        }));
        entries.push({ index, ancestors });
        const step = steps[index];
        if (step.type !== 'if_gone' && step.type !== 'if_else') continue;
        const thenCount = step.type === 'if_gone' ? step.skip_count : step.then_count;
        const elseCount = step.type === 'if_else' ? step.else_count : 0;
        if (!Number.isInteger(thenCount) || !Number.isInteger(elseCount)
            || (thenCount ?? 0) < 1 || (thenCount ?? 0) > 20
            || (step.type === 'if_else' && ((elseCount ?? 0) < 1 || (elseCount ?? 0) > 20))) {
            return { entries, problem: { index, reason: 'invalid_span' } };
        }
        const thenEnd = index + thenCount!;
        const end = thenEnd + (elseCount ?? 0);
        if (end >= steps.length) return { entries, problem: { index, reason: 'invalid_span' } };
        if (regions.length) {
            const parent = regions[regions.length - 1];
            const ceiling = index <= parent.thenEnd ? parent.thenEnd : parent.end;
            if (end > ceiling) return { entries, problem: { index, reason: 'crossing' } };
        }
        if (regions.length >= 3) return { entries, problem: { index, reason: 'too_deep' } };
        regions.push({ index, thenEnd, end });
    }
    return { entries };
}

export function canInsertBranchStep(steps: AutomationStep[], layout: FlowLayout, index: number, lane: BranchLane): boolean {
    if (layout.problem || steps.length >= 100 || !layout.entries[index]) return false;
    const step = steps[index];
    if (step.type !== 'if_else' && step.type !== 'if_gone') return false;
    if (lane === 'else' && step.type !== 'if_else') return false;
    const count = lane === 'then' ? (step.type === 'if_gone' ? step.skip_count : step.then_count) : step.else_count;
    if (!count || count >= 20) return false;
    return layout.entries[index].ancestors.every(ancestor => {
        const parent = steps[ancestor.conditionIndex];
        const parentCount = ancestor.lane === 'then'
            ? (parent.type === 'if_gone' ? parent.skip_count : parent.then_count) : parent.else_count;
        return parentCount !== undefined && parentCount < 20;
    });
}

/** Returns a fresh flat script, preserving per-branch membership and ancestry. */
export function insertBranchStep(steps: AutomationStep[], layout: FlowLayout, index: number, lane: BranchLane, added: AutomationStep): AutomationStep[] | null {
    if (!canInsertBranchStep(steps, layout, index, lane)) return null;
    const result = steps.map(step => ({ ...step }));
    const conditional = result[index];
    const then = conditional.type === 'if_gone' ? conditional.skip_count! : conditional.then_count!;
    const otherwise = conditional.type === 'if_else' ? conditional.else_count! : 0;
    const position = index + then + (lane === 'else' ? otherwise : 0) + 1;
    for (const ancestor of [...layout.entries[index].ancestors, { conditionIndex: index, lane }]) {
        const step = result[ancestor.conditionIndex];
        if (ancestor.lane === 'then') {
            if (step.type === 'if_gone') step.skip_count! += 1;
            else step.then_count! += 1;
        } else step.else_count! += 1;
    }
    result.splice(position, 0, added);
    return analyzeAutomationFlow(result).problem ? null : result;
}
