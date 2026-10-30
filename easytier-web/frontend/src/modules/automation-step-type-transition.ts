import type { AutomationStep } from '../types/automation';

/**
 * Tap and long press share the same target configuration. Only long press
 * supports duration_ms in the executor contract; do not leak it into tap.
 */
export function switchTapLongPressStep(
    current: AutomationStep, type: 'tap' | 'long_press', rememberedDuration?: number,
): AutomationStep {
    const { duration_ms, ...shared } = current;
    const next: AutomationStep = {
        ...shared,
        type,
        ...(current.selector ? { selector: { ...current.selector } } : {}),
    };
    if (type === 'long_press') next.duration_ms = rememberedDuration ?? duration_ms ?? 800;
    return next;
}
