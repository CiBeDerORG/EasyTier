export type AutomationKey = 'HOME' | 'BACK' | 'RECENT' | 'WAKE';
export interface AutomationSelector { resource_id?: string; description?: string; text?: string; class_name?: string; package?: string; xpath?: string }
export interface AutomationHierarchyNode {
    id: number; parent_id: number | null; depth: number;
    class_name: string; package: string; resource_id: string; text: string; description: string;
    clickable: boolean; enabled: boolean; scrollable: boolean; selected: boolean; checked: boolean; password: boolean;
    bounds: { left: number; top: number; right: number; bottom: number };
}
export interface AutomationHierarchy {
    device_id: string; target_fingerprint: string; captured_at: string;
    snapshot: { width: number; height: number; truncated: boolean; nodes: AutomationHierarchyNode[] };
}
export type AutomationStepType = 'key' | 'tap' | 'long_press' | 'swipe' | 'drag' | 'input_text' | 'launch_app' | 'wait_for' | 'wait_gone' | 'if_gone' | 'if_else' | 'assert' | 'assert_property' | 'scroll_find' | 'screenshot';
export interface AutomationStep {
    id: string;
    type: AutomationStepType;
    key?: AutomationKey;
    selector?: AutomationSelector;
    x?: number; y?: number; to_x?: number; to_y?: number;
    width?: number; height?: number; duration_ms?: number;
    text?: string; parameter?: string; package?: string; timeout_ms?: number;
    property?: 'enabled' | 'checked' | 'selected' | 'clickable' | 'scrollable';
    expected?: boolean;
    /** User explicitly designated this final assertion as the business outcome check. */
    business_verification?: true;
    scroll_container?: AutomationSelector;
    direction?: 'up' | 'down'; max_swipes?: number; skip_count?: number; then_count?: number; else_count?: number;
}
export interface AutomationContext {
    available: boolean; runner_configured: boolean; can_manage_others: boolean;
    can_manage_settings: boolean;
    actor: { id: number; username: string }; protocol_version: number;
}
export interface AutomationRunnerSettings {
    endpoint: string; token_configured: boolean; configured: boolean;
    source: 'database' | 'environment' | 'none'; revision: number; updated_at: string | null;
}
export interface AutomationRunnerSettingsInput {
    endpoint: string; token_action: 'preserve' | 'replace'; token?: string; revision: number;
}
export interface AutomationExecutorCapacity {
    executor_id: string; revision: number; source: 'database' | 'environment';
    max_running: number; max_pending: number;
    max_media_sessions: number; max_media_starting: number;
    recommended: { max_running: number; max_pending: number;
        max_media_sessions: number; max_media_starting: number };
}
export type AutomationExecutorCapacityInput = Pick<AutomationExecutorCapacity,
    'revision' | 'max_running' | 'max_pending' | 'max_media_sessions' | 'max_media_starting'> & {
        expected_executor_id: string;
    };
export interface AutomationRunnerCheck {
    reachable: boolean; executor_id: string; protocol_version: number; checked_at: string;
}
export interface AutomationConnection { enabled: boolean; adb_port: number; connection_mode: 'tcp' | 'wireless' }
export interface AutomationDevice {
    endpoint_event?: import('../modules/discovery-api').EndpointEvent;
    id: string; name: string; model: string | null; updated_at?: string; owner_user_id: number;
    machine_id: string; node_name: string; gateway_group_id: string | null;
    gateway_name: string | null; device_type_id: string | null; type_name: string | null;
    overlay_ip: string; lan_ip: string | null; binding_state: string;
    connection: AutomationConnection; target_fingerprint: string | null; availability_reason: string | null;
    active_session?: { id: string; state: string; actor_user_id: number; expires_at: string } | null;
    run_occupied?: boolean;
}
export interface AutomationPreparation {
    id: string; request_id: string; device_id: string; owner_user_id: number;
    target_fingerprint: string; state: string; stage: string; code: string | null; message: string | null;
    executor_generation: string; created_at: string; updated_at: string;
}
export type AutomationAdbConnectionState = 'connected' | 'offline' | 'unauthorized' | 'disconnected' | 'unavailable';
export type AutomationPhoneConnectionState = AutomationAdbConnectionState | 'connecting' | 'checking' | 'failed' | 'query_failed';
export interface AutomationAdbStatus {
    device_id: string; state: AutomationAdbConnectionState; code?: string;
}
export interface AutomationComponentStatus {
    device_id: string; state: 'ready' | 'not_prepared' | 'preparing' | 'adb_unavailable' | 'unavailable' | 'checking' | 'query_failed';
    executor_id: string; preparation_id?: string; stage?: string;
}
export interface AutomationPreparationLog {
    seq: number; at: string; level: string; stage: string; message: string;
}
export interface AutomationPrepareResult {
    state: string; message: string; metadata?: Record<string, unknown>;
    capabilities?: Record<string, unknown> | string[];
}
/** Maximum encoded long edge, target bitrate, FPS and optional device power-off. */
export interface AutomationMediaSettings {
    max_size: number;
    video_bit_rate: number;
    max_fps: number;
    low_latency: boolean;
    screen_off: boolean;
}
export interface AutomationSession {
    id: string; device_id: string; state: string; ice_servers: RTCIceServer[]; expires_at?: string;
}
export interface AutomationScriptInput { name: string; description: string; steps: AutomationStep[]; revision?: number }
export interface AutomationScript extends AutomationScriptInput {
    id: string; revision: number; created_at: string; updated_at: string;
}
export interface AutomationRunInput {
    request_id: string; script_id?: string; revision?: number; name?: string;
    steps?: AutomationStep[]; device_ids: string[]; parameters: Record<string, string>;
    concurrency: number;
}
export interface AutomationStepResult {
    step_id: string; state: string; message?: string; started_at?: string; finished_at?: string;
    artifact_id?: string; error?: string | { code?: string; message?: string };
    branch_taken?: 'then' | 'else' | 'skip'; skipped_by_step_id?: string;
}
export type BusinessVerificationStatus = 'verified' | 'failed' | 'not_verified' | 'pending' | 'not_reached' | 'outcome_unknown' | 'partial';
export interface AutomationRunDevice {
    device_id: string; name: string; state: string; current_step: number | string | null;
    business_verification?: BusinessVerificationStatus;
    cleanup_confirmed?: boolean;
    error?: string | { code?: string; message?: string }; steps: AutomationStepResult[];
    diagnostic_target?: { ip?: string; adb_port?: number; target_fingerprint?: string };
}
export interface AutomationRun {
    id: string; name: string; state: string; created_at: string; updated_at: string;
    executor_revision?: number; concurrency?: number;
    business_verification?: BusinessVerificationStatus;
    script_snapshot: { name?: string; script_id?: string; revision?: number; steps: AutomationStep[] };
    devices: AutomationRunDevice[];
}
export interface AutomationControl {
    type: 'touch' | 'touch.release' | 'key' | 'input_text' | 'stop';
    action?: 'down' | 'move' | 'up'; x?: number; y?: number; width?: number; height?: number;
    key?: AutomationKey; text?: string;
}
export interface AutomationControlReply {
    type: 'ready' | 'ack' | 'error'; command_id?: string; state?: string;
    code?: string; message?: string; session_id?: string; device_id?: string;
}

/** Unpublished, account-scoped server copy; separate from AutomationScript. */
export interface AutomationDraftRecord {
    id: string;
    owner_user_id: number;
    name: string;
    description: string;
    steps: AutomationStep[];
    revision: number;
    created_at: string;
    updated_at: string;
}
export interface AutomationDraftReceipt {
    id: string;
    revision: number;
    deleted: boolean;
    request_replayed: true;
}
export type AutomationDraftWrite = AutomationDraftRecord | AutomationDraftReceipt;
export interface AutomationDraftInput {
    request_id: string;
    name: string;
    description: string;
    steps: AutomationStep[];
    revision?: number;
}
