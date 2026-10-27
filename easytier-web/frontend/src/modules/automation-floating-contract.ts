/**
 * FINAL V3 / T20 — identity and UI-event contract for the ORIGINAL phone card.
 *
 * This is not a second video, draft store, execution controller or lock.
 * The center owns one AutomationDraftWorkspace; PhoneCard owns its one media
 * session; Rust/Executor remain the authorities for running real actions.
 */
import type { AutomationDevice } from '../types/automation';
import type { AutomationDraftWorkspace } from './automation-draft';
import type { DraftSyncState } from './automation-draft-sync';

/** One authorized phone on the original automation-center card list. */
export interface AutomationCardScope {
    readonly ownerId: number;
    readonly deviceId: string;
    readonly targetFingerprint: string | null;
    /** May be null while editing an offline phone. Never use it as a run lock. */
    readonly sessionId: string | null;
}

/**
 * Two independent revision spaces: a published script and a synchronized
 * working draft. Neither may be inferred from the name displayed in the UI.
 */
export interface AutomationScriptScope {
    readonly scriptId: string | null;
    readonly scriptRevision: number | null;
    readonly draftId: string | null;
    readonly draftRevision: number | null;
}

/** Snapshot for stale async responses and edits, not a persistent state store. */
export interface AutomationFloatingScope extends AutomationCardScope, AutomationScriptScope {
    /** Monotonic for one active panel operation; maintained by its caller. */
    readonly generation: number;
}

/**
 * Per-phone presentation ONLY; never store steps, parameters, streams,
 * executor status, permission tokens or formal script copies here.
 */
export type AutomationFloatingWindow =
    | 'main' | 'step-editor' | 'script-clone' | 'condition-delete'
    | 'inspector' | 'run-parameters' | 'run-progress';
export interface AutomationFloatingPoint {
    readonly x: number;
    readonly y: number;
}
export interface AutomationFloatingUiState {
    readonly deviceId: string;
    readonly open: boolean;
    readonly activeWindow: AutomationFloatingWindow;
    readonly position: AutomationFloatingPoint | null;
}

/**
 * Commands carry an immutable origin.  The parent must revalidate the scope
 * before acting; this contract alone never authorizes Android commands.
 */
export type AutomationFloatingCommand =
    | { readonly kind: 'panel.open' | 'panel.close'; readonly scope: AutomationFloatingScope }
    | { readonly kind: 'step.edit' | 'step.delete' | 'step.trial' | 'step.repick';
        readonly scope: AutomationFloatingScope; readonly stepId: string }
    | { readonly kind: 'script.open' | 'script.clone';
        readonly scope: AutomationFloatingScope; readonly scriptId: string }
    | { readonly kind: 'script.create';
        readonly scope: AutomationFloatingScope; readonly name: string }
    | { readonly kind: 'script.run';
        readonly scope: AutomationFloatingScope;
        /** The explicit WORKBENCH selection, never the floating panel's deviceId. */
        readonly deviceIds: readonly string[];
        readonly requestId: string };

/** Access to a script editor may be allowed while ADB/video is offline. */
export function canAccessAutomationCard(
    ownerId: number,
    deviceId: string,
    device: Pick<AutomationDevice, 'id' | 'owner_user_id'> | null | undefined,
    workspace: Pick<AutomationDraftWorkspace, 'ownerId'>,
): boolean {
    return ownerId > 0 && workspace.ownerId === ownerId &&
        !!device && device.id === deviceId && device.owner_user_id === ownerId;
}

export interface AutomationScopeSource {
    readonly ownerId: number;
    readonly device: AutomationDevice | null | undefined;
    readonly workspace: AutomationDraftWorkspace;
    readonly draftSync: Pick<DraftSyncState, 'id' | 'revision'>;
    readonly sessionId: string | null;
    readonly generation: number;
}

/** Capture one card's identity without mutating media/draft or starting a run. */
export function captureAutomationFloatingScope(source: AutomationScopeSource): AutomationFloatingScope | null {
    const device = source.device;
    if (!device || !canAccessAutomationCard(source.ownerId, device.id, device, source.workspace))
        return null;
    return {
        ownerId: source.ownerId,
        deviceId: device.id,
        targetFingerprint: device.target_fingerprint,
        sessionId: source.sessionId,
        scriptId: source.workspace.editingId ?? null,
        scriptRevision: source.workspace.revision ?? null,
        draftId: source.draftSync.id,
        draftRevision: source.draftSync.revision,
        generation: source.generation,
    };
}

/**
 * Use after async hierarchy/selector/editor operations.
 * Offline script management checks identity and revisions but not sessionId.
 * Actions involving pixels/video must explicitly request requireLiveMedia.
 */
export function automationFloatingScopeIsCurrent(
    expected: AutomationFloatingScope,
    source: AutomationScopeSource,
    options: { requireLiveMedia?: boolean } = {},
): boolean {
    const latest = captureAutomationFloatingScope(source);
    if (!latest || expected.generation !== latest.generation ||
        expected.ownerId !== latest.ownerId ||
        expected.deviceId !== latest.deviceId ||
        expected.targetFingerprint !== latest.targetFingerprint ||
        expected.scriptId !== latest.scriptId ||
        expected.scriptRevision !== latest.scriptRevision ||
        expected.draftId !== latest.draftId ||
        expected.draftRevision !== latest.draftRevision) return false;
    return !options.requireLiveMedia ||
        (!!expected.targetFingerprint && !!expected.sessionId &&
            expected.sessionId === latest.sessionId);
}
