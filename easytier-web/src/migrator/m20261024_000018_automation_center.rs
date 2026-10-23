use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
CREATE TABLE automation_connections (
    device_id TEXT PRIMARY KEY NOT NULL REFERENCES asset_devices(id) ON DELETE CASCADE,
    owner_user_id INTEGER NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0,1)),
    adb_port INTEGER NOT NULL DEFAULT 5555 CHECK(adb_port BETWEEN 1 AND 65535),
    connection_mode TEXT NOT NULL DEFAULT 'tcp' CHECK(connection_mode IN ('tcp','wireless')),
    updated_at TEXT NOT NULL
);
CREATE TABLE automation_scripts (
    id TEXT PRIMARY KEY NOT NULL,
    owner_user_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    revision INTEGER NOT NULL DEFAULT 1,
    steps_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX idx_automation_scripts_owner ON automation_scripts(owner_user_id,updated_at DESC,id);
CREATE TABLE automation_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    actor_user_id INTEGER NOT NULL,
    owner_user_id INTEGER NOT NULL,
    device_id TEXT NOT NULL,
    target_key TEXT NOT NULL,
    target_fingerprint TEXT NOT NULL,
    target_json TEXT NOT NULL,
    quality TEXT NOT NULL,
    state TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);
CREATE INDEX idx_automation_sessions_owner ON automation_sessions(owner_user_id,created_at DESC);
CREATE UNIQUE INDEX idx_automation_session_viewer_device ON automation_sessions(device_id)
    WHERE state NOT IN ('closed','failed');
CREATE UNIQUE INDEX idx_automation_session_viewer_target ON automation_sessions(target_key)
    WHERE state NOT IN ('closed','failed');
CREATE TABLE automation_runs (
    id TEXT PRIMARY KEY NOT NULL,
    actor_user_id INTEGER NOT NULL,
    owner_user_id INTEGER NOT NULL,
    request_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    name TEXT NOT NULL,
    state TEXT NOT NULL,
    cancel_requested INTEGER NOT NULL DEFAULT 0 CHECK(cancel_requested IN(0,1)),
    runner_identity TEXT,
    never_started_confirmed_at TEXT,
    result_revision INTEGER NOT NULL DEFAULT -1,
    result_hash TEXT,
    snapshot_json TEXT NOT NULL,
    parameters_json TEXT NOT NULL,
    concurrency INTEGER NOT NULL CHECK(concurrency BETWEEN 1 AND 3),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(actor_user_id,owner_user_id,request_id)
);
CREATE INDEX idx_automation_runs_owner ON automation_runs(owner_user_id,created_at DESC,id);
CREATE TABLE automation_run_devices (
    run_id TEXT NOT NULL REFERENCES automation_runs(id) ON DELETE RESTRICT,
    device_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    name TEXT NOT NULL,
    target_key TEXT NOT NULL,
    target_fingerprint TEXT NOT NULL,
    target_json TEXT NOT NULL,
    state TEXT NOT NULL,
    cleanup_confirmed INTEGER NOT NULL DEFAULT 0 CHECK(cleanup_confirmed IN(0,1)),
    current_step TEXT,
    error_json TEXT,
    steps_json TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL,
    PRIMARY KEY(run_id,device_id)
);
CREATE UNIQUE INDEX idx_automation_running_device ON automation_run_devices(device_id)
    WHERE cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending');
CREATE UNIQUE INDEX idx_automation_running_target ON automation_run_devices(target_key)
    WHERE cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending');
CREATE TABLE automation_result_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- These records intentionally have no asset/account cascading foreign key.
-- Asset removal must retain history, revocation intent and unresolved occupancy.
CREATE TRIGGER automation_account_revoked AFTER UPDATE OF deleted_at ON users
WHEN NEW.deleted_at IS NOT NULL AND OLD.deleted_at IS NULL
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE (actor_user_id=NEW.id OR owner_user_id=NEW.id) AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE run_id IN (SELECT id FROM automation_runs WHERE actor_user_id=NEW.id OR owner_user_id=NEW.id)
      AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
    UPDATE automation_runs SET cancel_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE (actor_user_id=NEW.id OR owner_user_id=NEW.id)
      AND state NOT IN ('completed','succeeded','failed','cancelled','interrupted','partial_failed')
      AND EXISTS(SELECT 1 FROM automation_run_devices d WHERE d.run_id=automation_runs.id AND d.cleanup_confirmed=0 AND d.state='cancel_requested');
END;
CREATE TRIGGER automation_admin_removed AFTER DELETE ON users_groups
WHEN EXISTS(SELECT 1 FROM groups WHERE id=OLD.group_id AND name='admins')
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE actor_user_id=OLD.user_id AND owner_user_id<>OLD.user_id AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE run_id IN (SELECT id FROM automation_runs WHERE actor_user_id=OLD.user_id AND owner_user_id<>OLD.user_id)
      AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
    UPDATE automation_runs SET cancel_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE actor_user_id=OLD.user_id AND owner_user_id<>OLD.user_id
      AND state NOT IN ('completed','succeeded','failed','cancelled','interrupted','partial_failed')
      AND EXISTS(SELECT 1 FROM automation_run_devices d WHERE d.run_id=automation_runs.id AND d.cleanup_confirmed=0 AND d.state='cancel_requested');
END;
CREATE TRIGGER automation_asset_deleted BEFORE DELETE ON asset_devices
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id=OLD.id AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id=OLD.id AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
    UPDATE automation_runs SET cancel_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE id IN (SELECT run_id FROM automation_run_devices WHERE device_id=OLD.id AND state='cancel_requested');
END;
CREATE TRIGGER automation_asset_target_changed AFTER UPDATE OF overlay_ip,network_instance_id,binding_snapshot_json,rule_key,host_offset,gateway_group_id,address_mode ON asset_devices
WHEN OLD.overlay_ip IS NOT NEW.overlay_ip OR OLD.network_instance_id IS NOT NEW.network_instance_id
  OR OLD.binding_snapshot_json IS NOT NEW.binding_snapshot_json OR OLD.rule_key IS NOT NEW.rule_key
  OR OLD.host_offset IS NOT NEW.host_offset OR OLD.gateway_group_id IS NOT NEW.gateway_group_id OR OLD.address_mode IS NOT NEW.address_mode
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id=NEW.id AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id=NEW.id AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
    UPDATE automation_runs SET cancel_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE id IN (SELECT run_id FROM automation_run_devices WHERE device_id=NEW.id AND state='cancel_requested');
END;
CREATE TRIGGER automation_connection_changed AFTER UPDATE OF enabled,adb_port,connection_mode ON automation_connections
WHEN OLD.enabled IS NOT NEW.enabled OR OLD.adb_port IS NOT NEW.adb_port OR OLD.connection_mode IS NOT NEW.connection_mode
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id=NEW.device_id AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id=NEW.device_id AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
    UPDATE automation_runs SET cancel_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE id IN (SELECT run_id FROM automation_run_devices WHERE device_id=NEW.device_id AND state='cancel_requested');
END;
CREATE TRIGGER automation_gateway_binding_changed AFTER UPDATE OF binding_snapshot_json ON asset_gateway_groups
WHEN OLD.binding_snapshot_json IS NOT NEW.binding_snapshot_json
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id IN (SELECT id FROM asset_devices WHERE gateway_group_id=NEW.id) AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id IN (SELECT id FROM asset_devices WHERE gateway_group_id=NEW.id)
      AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
    UPDATE automation_runs SET cancel_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE id IN (SELECT d.run_id FROM automation_run_devices d JOIN asset_devices a ON a.id=d.device_id WHERE a.gateway_group_id=NEW.id AND d.state='cancel_requested');
END;
CREATE TRIGGER automation_instance_deleted BEFORE DELETE ON user_running_network_configs
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id IN (SELECT id FROM asset_devices WHERE owner_user_id=OLD.user_id AND machine_id=OLD.device_id AND network_instance_id=OLD.network_instance_id)
      AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id IN (SELECT id FROM asset_devices WHERE owner_user_id=OLD.user_id AND machine_id=OLD.device_id AND network_instance_id=OLD.network_instance_id)
      AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
    UPDATE automation_runs SET cancel_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE id IN (SELECT d.run_id FROM automation_run_devices d JOIN asset_devices a ON a.id=d.device_id
        WHERE a.owner_user_id=OLD.user_id AND a.machine_id=OLD.device_id AND a.network_instance_id=OLD.network_instance_id AND d.state='cancel_requested');
END;
CREATE TRIGGER automation_instance_route_changed AFTER UPDATE OF network_config,disabled ON user_running_network_configs
WHEN OLD.disabled IS NOT NEW.disabled
  OR (CASE WHEN json_valid(OLD.network_config) THEN json_extract(OLD.network_config,'$.virtual_ipv4','$.dhcp','$.dhcp_ipv4_cidr','$.network_length','$.proxy_cidrs','$.network_identity') ELSE OLD.network_config END)
    IS NOT (CASE WHEN json_valid(NEW.network_config) THEN json_extract(NEW.network_config,'$.virtual_ipv4','$.dhcp','$.dhcp_ipv4_cidr','$.network_length','$.proxy_cidrs','$.network_identity') ELSE NEW.network_config END)
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id IN (SELECT id FROM asset_devices WHERE owner_user_id=NEW.user_id AND machine_id=NEW.device_id AND network_instance_id=NEW.network_instance_id)
      AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id IN (SELECT id FROM asset_devices WHERE owner_user_id=NEW.user_id AND machine_id=NEW.device_id AND network_instance_id=NEW.network_instance_id)
      AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
    UPDATE automation_runs SET cancel_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE id IN (SELECT d.run_id FROM automation_run_devices d JOIN asset_devices a ON a.id=d.device_id
        WHERE a.owner_user_id=NEW.user_id AND a.machine_id=NEW.device_id AND a.network_instance_id=NEW.network_instance_id AND d.state='cancel_requested');
END;
"#).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(r#"
DROP TRIGGER automation_connection_changed;
DROP TRIGGER automation_instance_route_changed;
DROP TRIGGER automation_instance_deleted;
DROP TRIGGER automation_gateway_binding_changed;
DROP TRIGGER automation_asset_target_changed;
DROP TRIGGER automation_asset_deleted;
DROP TRIGGER automation_admin_removed;
DROP TRIGGER automation_account_revoked;
DROP TABLE automation_result_events;
DROP TABLE automation_run_devices;
DROP TABLE automation_runs;
DROP TABLE automation_sessions;
DROP TABLE automation_scripts;
DROP TABLE automation_connections;
"#).await?;
        Ok(())
    }
}
