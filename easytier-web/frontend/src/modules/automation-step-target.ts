import type { AutomationSelector, AutomationStep } from '../types/automation';

export type StepTarget =
    | { selector: AutomationSelector; coordinates?: never }
    | { selector?: never; coordinates: { x: number; y: number; width: number; height: number } };

export const TARGETABLE_STEPS = [
    'tap', 'long_press', 'input_text', 'wait_for', 'wait_gone',
    'if_gone', 'if_else', 'assert', 'assert_property', 'scroll_find',
] as const;

export function stepCanPickTarget(step: AutomationStep): boolean {
    return TARGETABLE_STEPS.some(type => step.type === type);
}

function validSelector(raw: AutomationSelector, type: AutomationStep['type']): boolean {
    const entries = Object.entries(raw);
    if (!entries.length) return false;
    const keys = new Set(['resource_id', 'text', 'description', 'class_name', 'package',
        'clickable', 'enabled', 'long_clickable', 'focusable', 'checkable', 'scrollable']);
    const booleanKeys = new Set(['clickable', 'enabled', 'long_clickable', 'focusable', 'checkable', 'scrollable']);
    const xpath = raw.xpath;
    if (xpath !== undefined) {
        return entries.length === 1 && type !== 'input_text' && type !== 'assert_property'
            && typeof xpath === 'string' && !!xpath.trim()
            && new TextEncoder().encode(xpath).length <= 512;
    }
    return entries.every(([key, value]) => keys.has(key) && (booleanKeys.has(key)
        ? typeof value === 'boolean'
        : typeof value === 'string' && !!value.trim()
            && new TextEncoder().encode(value).length <= 256));
}

/**
 * Replace the locator on exactly one existing step, preserving the ID, action,
 * text/parameters, conditional spans, and timing. Never append a new action.
 * The picker has checked uniqueness in a fresh, non-truncated hierarchy; the
 * executor will independently check that uniqueness again at run time.
 */
export function replaceAutomationStepTarget(
    steps: AutomationStep[], stepId: string, expectedType: AutomationStep['type'], target: StepTarget,
): AutomationStep[] | null {
    const index = steps.findIndex(step => step.id === stepId);
    if (index < 0 || !stepId || steps.filter(step => step.id === stepId).length !== 1) return null;
    const original = steps[index];
    if (original.type !== expectedType || !stepCanPickTarget(original)) return null;
    const updated: AutomationStep = { ...original };
    if (target.selector) {
        if (!validSelector(target.selector, original.type)) return null;
        updated.selector = { ...target.selector };
        if (original.type === 'tap' || original.type === 'long_press') {
            delete updated.x; delete updated.y;
            delete updated.width; delete updated.height;
        }
    } else {
        if (original.type !== 'tap' && original.type !== 'long_press') return null;
        const point = target.coordinates;
        if (!point || !Number.isFinite(point.x) || !Number.isFinite(point.y)
            || point.x < 0 || point.x > 1 || point.y < 0 || point.y > 1
            || !Number.isInteger(point.width) || !Number.isInteger(point.height)
            || point.width <= 0 || point.height <= 0) return null;
        delete updated.selector;
        updated.x = point.x; updated.y = point.y;
        updated.width = point.width; updated.height = point.height;
    }
    const next = [...steps];
    next[index] = updated;
    return next;
}
