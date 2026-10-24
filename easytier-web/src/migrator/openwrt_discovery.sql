CREATE UNIQUE INDEX idx_asset_devices_lan_group ON asset_devices(owner_user_id,machine_id,gateway_group_id,lan_ip) WHERE gateway_group_id IS NOT NULL AND lan_ip IS NOT NULL;
CREATE UNIQUE INDEX idx_asset_devices_lan_ungrouped ON asset_devices(owner_user_id,machine_id,lan_ip) WHERE gateway_group_id IS NULL AND lan_ip IS NOT NULL;
ALTER TABLE asset_device_types ADD COLUMN semantic_kind TEXT CHECK(semantic_kind IS NULL OR semantic_kind IN ('openwrt','phone','other'));
ALTER TABLE asset_devices ADD COLUMN discovery_json TEXT;
ALTER TABLE asset_devices ADD COLUMN display_name_manual INTEGER NOT NULL DEFAULT 1 CHECK(display_name_manual IN (0,1));
ALTER TABLE asset_devices ADD COLUMN endpoint_revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE asset_services ADD COLUMN service_type TEXT NOT NULL DEFAULT 'generic' CHECK(service_type IN ('generic','adb'));
ALTER TABLE asset_services ADD COLUMN connection_mode TEXT NOT NULL DEFAULT 'tcp' CHECK(connection_mode IN ('tcp','wireless'));
ALTER TABLE asset_services ADD COLUMN endpoint_revision INTEGER NOT NULL DEFAULT 1;
CREATE UNIQUE INDEX idx_asset_services_single_adb ON asset_services(device_id) WHERE service_type='adb';
ALTER TABLE automation_sessions ADD COLUMN executor_identity TEXT;
ALTER TABLE automation_run_devices ADD COLUMN withdraw_requested INTEGER NOT NULL DEFAULT 0 CHECK(withdraw_requested IN(0,1));
ALTER TABLE automation_run_devices ADD COLUMN withdraw_event_id TEXT;
CREATE TABLE discovery_gateway_access (
    gateway_device_id TEXT PRIMARY KEY REFERENCES asset_devices(id) ON DELETE CASCADE,
    owner_user_id INTEGER NOT NULL,
    endpoint_url TEXT NOT NULL,
    ca_pem TEXT NOT NULL DEFAULT '',
    secret_cipher TEXT,
    config_revision INTEGER NOT NULL,
    last_result_json TEXT,
    updated_at TEXT NOT NULL
);
CREATE TABLE discovery_operations (
    id TEXT PRIMARY KEY,
    actor_user_id INTEGER NOT NULL,
    owner_user_id INTEGER NOT NULL,
    gateway_device_id TEXT NOT NULL,
    machine_id TEXT NOT NULL,
    group_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    request_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL,
    context_json TEXT NOT NULL,
    snapshot_json TEXT,
    result_json TEXT NOT NULL DEFAULT '{}',
    originating_operation_id TEXT,
    supplement_used INTEGER NOT NULL DEFAULT 0,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(actor_user_id,owner_user_id,gateway_device_id,kind,request_id)
);
CREATE TABLE discovery_preview_rows (
    operation_id TEXT NOT NULL REFERENCES discovery_operations(id) ON DELETE CASCADE,
    row_id TEXT NOT NULL,
    input_json TEXT NOT NULL,
    preview_json TEXT NOT NULL,
    applied_json TEXT,
    PRIMARY KEY(operation_id,row_id)
);
CREATE TABLE discovery_requests (
    actor_user_id INTEGER NOT NULL,
    owner_user_id INTEGER NOT NULL,
    scope TEXT NOT NULL,
    request_id TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    response_json TEXT NOT NULL,
    PRIMARY KEY(actor_user_id,owner_user_id,scope,request_id)
);
CREATE TABLE discovery_endpoint_events (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    operation_id TEXT,
    owner_user_id INTEGER NOT NULL,
    machine_id TEXT NOT NULL,
    group_id TEXT,
    device_id TEXT NOT NULL,
    adb_service_id TEXT,
    endpoint_revision INTEGER NOT NULL,
    old_target_json TEXT,
    predecessor_device_id TEXT,
    reason TEXT NOT NULL,
    publication_json TEXT NOT NULL DEFAULT '{"state":"pending"}',
    cleanup_json TEXT NOT NULL DEFAULT '{"state":"pending"}',
    connection_json TEXT NOT NULL DEFAULT '{"state":"pending"}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_discovery_events_device ON discovery_endpoint_events(owner_user_id,device_id,seq);
CREATE INDEX idx_discovery_events_operation ON discovery_endpoint_events(operation_id,seq);
CREATE TABLE discovery_targets (
    device_id TEXT NOT NULL,
    owner_user_id INTEGER NOT NULL,
    executor_id TEXT NOT NULL,
    target_fingerprint TEXT NOT NULL,
    target_json TEXT NOT NULL,
    retired INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY(device_id, executor_id, target_fingerprint)
);
CREATE TABLE discovery_publication_plans (
 id TEXT PRIMARY KEY, owner_user_id INTEGER NOT NULL, plan_json TEXT NOT NULL,
 desired_fence_json TEXT NOT NULL, report_json TEXT, created_at TEXT NOT NULL
);
