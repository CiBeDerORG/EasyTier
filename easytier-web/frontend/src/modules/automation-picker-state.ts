/**
 * T01 — One transient picker mode per viewed phone.
 *
 * This state is intentionally NOT a second script store. All confirmed steps
 * stay in the account-scoped AutomationDraftWorkspace managed by the center.
 * Generations protect pending hierarchy / click results from stale sessions.
 */
import type { AutomationStepType } from '../types/automation';

export type PickerPhase = 'off' | 'preparing' | 'on' | 'error';
/** Physical video pixel coordinates, not CSS pixels or fullscreen margins. */
export interface PickerVideoPoint { x: number; y: number; width: number; height: number }
export type PickerMenuAction = 'inspect' | 'tap' | 'long_press' | 'input_text'
    | 'wait_for' | 'wait_gone' | 'assert' | 'assert_property' | 'refresh' | 'exit';
export type PickerIntent =
    | { kind: 'append' }
    | { kind: 'replace'; stepId: string; stepType: AutomationStepType };

export interface PickerBinding {
    ownerId: number;
    deviceId: string;
    targetFingerprint: string;
    sessionId: string;
    draftOwnerId: number;
    draftScriptId: string | null;
    draftRevision: number | null;
}
export interface PickerContext extends PickerBinding {
    phase: PickerPhase;
    intent: PickerIntent;
    /** Never reuse a generation, even after closing and opening the same card. */
    generation: number;
    /** Increment on every screen geometry or hierarchy-context change. */
    snapshotGeneration: number;
    error: string;
}

export function pickerMatches(context: PickerContext | undefined, binding: PickerBinding): boolean {
    return !!context &&
        context.ownerId === binding.ownerId &&
        context.deviceId === binding.deviceId &&
        context.targetFingerprint === binding.targetFingerprint &&
        context.sessionId === binding.sessionId &&
        context.draftOwnerId === binding.draftOwnerId &&
        context.draftScriptId === binding.draftScriptId &&
        context.draftRevision === binding.draftRevision;
}

export function beginPicker(binding: PickerBinding, generation: number, intent: PickerIntent = {kind:'append'}): PickerContext {
    return { ...binding, generation, snapshotGeneration: 0, phase: 'preparing', intent, error: '' };
}

export function readyPicker(context: PickerContext, expectedGeneration: number): PickerContext {
    return context.generation === expectedGeneration && context.phase === 'preparing'
        ? { ...context, phase: 'on', error: '' } : context;
}

export function errorPicker(context: PickerContext, expectedGeneration: number, message: string): PickerContext {
    return context.generation === expectedGeneration && context.phase === 'preparing'
        ? { ...context, phase: 'error', error: message } : context;
}

export function stopPicker(context: PickerContext, generation: number): PickerContext {
    return {
        ...context, phase: 'off', intent: {kind:'append'}, generation,
        snapshotGeneration: context.snapshotGeneration + 1, error: '',
    };
}

export function setPickerIntent(context: PickerContext, intent: PickerIntent, generation: number): PickerContext {
    if (context.phase !== 'on') return context;
    return {
        ...context, intent, generation,
        snapshotGeneration: context.snapshotGeneration + 1,
    };
}

export function invalidatePickerSnapshot(context: PickerContext): PickerContext {
    return { ...context, snapshotGeneration: context.snapshotGeneration + 1 };
}

/** Used before accepting an async picker result or writing a confirmed step. */
export function pickerCanApply(
    context: PickerContext | undefined, binding: PickerBinding,
    expectedGeneration: number, expectedSnapshotGeneration: number,
    intent: PickerIntent,
): boolean {
    if (!context || context.phase !== 'on' ||
        context.generation !== expectedGeneration ||
        context.snapshotGeneration !== expectedSnapshotGeneration ||
        !pickerMatches(context, binding) ||
        context.intent.kind !== intent.kind) return false;
    return intent.kind === 'append' || (
        context.intent.kind === 'replace' &&
        context.intent.stepId === intent.stepId &&
        context.intent.stepType === intent.stepType
    );
}
