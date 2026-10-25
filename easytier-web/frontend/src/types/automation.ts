export type AutomationKey = 'HOME' | 'BACK' | 'RECENT' | 'WAKE';
export interface AutomationSelector { resource_id?: string; description?: string; text?: string }
export type AutomationStepType = 'key' | 'tap' | 'swipe' | 'input_text' | 'launch_app' | 'wait_for' | 'assert' | 'screenshot';
export interface AutomationStep {
    id: string;
    type: AutomationStepType;
    key?: AutomationKey;
    selector?: AutomationSelector;
    x?: number; y?: number; to_x?: number; to_y?: number;
    width?: number; height?: number; duration_ms?: number;
    text?: string; parameter?: string; package?: string; timeout_ms?: number;
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
}
export interface AutomationRunDevice {
    device_id: string; name: string; state: string; current_step: number | string | null;
    cleanup_confirmed?: boolean;
    error?: string | { code?: string; message?: string }; steps: AutomationStepResult[];
}
export interface AutomationRun {
    id: string; name: string; state: string; created_at: string; updated_at: string;
    script_snapshot: { name?: string; revision?: number; steps: AutomationStep[] };
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
