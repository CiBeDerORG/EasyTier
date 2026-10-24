DROP TRIGGER automation_asset_deleted;

CREATE TRIGGER automation_asset_deleted BEFORE DELETE ON asset_devices
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id=OLD.id AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id=OLD.id AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
END;

DROP TRIGGER automation_asset_target_changed;

CREATE TRIGGER automation_asset_target_changed AFTER UPDATE OF overlay_ip,network_instance_id,binding_snapshot_json,rule_key,host_offset,gateway_group_id,address_mode ON asset_devices
WHEN OLD.overlay_ip IS NOT NEW.overlay_ip OR OLD.network_instance_id IS NOT NEW.network_instance_id
  OR OLD.binding_snapshot_json IS NOT NEW.binding_snapshot_json OR OLD.rule_key IS NOT NEW.rule_key
  OR OLD.host_offset IS NOT NEW.host_offset OR OLD.gateway_group_id IS NOT NEW.gateway_group_id OR OLD.address_mode IS NOT NEW.address_mode
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id=NEW.id AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id=NEW.id AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
END;

DROP TRIGGER automation_connection_changed;

DROP TRIGGER automation_gateway_binding_changed;

CREATE TRIGGER automation_gateway_binding_changed AFTER UPDATE OF binding_snapshot_json ON asset_gateway_groups
WHEN OLD.binding_snapshot_json IS NOT NEW.binding_snapshot_json
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id IN (SELECT id FROM asset_devices WHERE gateway_group_id=NEW.id) AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id IN (SELECT id FROM asset_devices WHERE gateway_group_id=NEW.id)
      AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
END;

DROP TRIGGER automation_instance_deleted;

CREATE TRIGGER automation_instance_deleted BEFORE DELETE ON user_running_network_configs
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id IN (SELECT id FROM asset_devices WHERE owner_user_id=OLD.user_id AND machine_id=OLD.device_id AND network_instance_id=OLD.network_instance_id)
      AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id IN (SELECT id FROM asset_devices WHERE owner_user_id=OLD.user_id AND machine_id=OLD.device_id AND network_instance_id=OLD.network_instance_id)
      AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
END;

DROP TRIGGER automation_instance_route_changed;

CREATE TRIGGER automation_instance_route_changed AFTER UPDATE OF network_config,disabled ON user_running_network_configs
WHEN OLD.disabled IS NOT NEW.disabled
  OR (CASE WHEN json_valid(OLD.network_config) THEN json_extract(OLD.network_config,'$.virtual_ipv4','$.dhcp','$.dhcp_ipv4_cidr','$.network_length','$.proxy_cidrs','$.network_identity') ELSE OLD.network_config END)
    IS NOT (CASE WHEN json_valid(NEW.network_config) THEN json_extract(NEW.network_config,'$.virtual_ipv4','$.dhcp','$.dhcp_ipv4_cidr','$.network_length','$.proxy_cidrs','$.network_identity') ELSE NEW.network_config END)
BEGIN
    UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id IN (SELECT id FROM asset_devices WHERE owner_user_id=NEW.user_id AND machine_id=NEW.device_id AND network_instance_id=NEW.network_instance_id)
      AND state NOT IN ('closed','failed');
    UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
      WHERE device_id IN (SELECT id FROM asset_devices WHERE owner_user_id=NEW.user_id AND machine_id=NEW.device_id AND network_instance_id=NEW.network_instance_id)
      AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','outcome_unknown','cleanup_pending');
END;

CREATE TRIGGER discovery_device_deleted BEFORE DELETE ON asset_devices BEGIN
UPDATE asset_devices SET endpoint_revision=endpoint_revision+1 WHERE id IN (SELECT OLD.id);
UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT OLD.id) AND state NOT IN ('closed','failed');
UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT OLD.id) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending');
INSERT INTO discovery_endpoint_events(event_id,owner_user_id,machine_id,group_id,device_id,adb_service_id,endpoint_revision,old_target_json,reason,created_at,updated_at)
 SELECT lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-4'||substr(lower(hex(randomblob(2))),2)||'-a'||substr(lower(hex(randomblob(2))),2)||'-'||lower(hex(randomblob(6))),a.owner_user_id,a.machine_id,a.gateway_group_id,a.id,s.id,a.endpoint_revision,json_object('device_id',OLD.id,'ip',OLD.overlay_ip,'adb_port',s.port,'adb_service_id',s.id,'connection_mode',s.connection_mode),'device_deleted',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')
 FROM asset_devices a LEFT JOIN asset_services s ON s.device_id=a.id AND s.service_type='adb' WHERE a.id IN (SELECT OLD.id);
END;

CREATE TRIGGER discovery_device_target_changed AFTER UPDATE OF overlay_ip,network_instance_id,binding_snapshot_json,rule_key,host_offset,gateway_group_id,address_mode,device_type_id ON asset_devices WHEN OLD.overlay_ip IS NOT NEW.overlay_ip OR OLD.network_instance_id IS NOT NEW.network_instance_id OR OLD.binding_snapshot_json IS NOT NEW.binding_snapshot_json OR OLD.rule_key IS NOT NEW.rule_key OR OLD.host_offset IS NOT NEW.host_offset OR OLD.gateway_group_id IS NOT NEW.gateway_group_id OR OLD.address_mode IS NOT NEW.address_mode OR OLD.device_type_id IS NOT NEW.device_type_id BEGIN
UPDATE asset_devices SET endpoint_revision=endpoint_revision+1 WHERE id IN (SELECT NEW.id);
UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT NEW.id) AND state NOT IN ('closed','failed');
UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT NEW.id) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending');
INSERT INTO discovery_endpoint_events(event_id,owner_user_id,machine_id,group_id,device_id,adb_service_id,endpoint_revision,old_target_json,reason,created_at,updated_at)
 SELECT lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-4'||substr(lower(hex(randomblob(2))),2)||'-a'||substr(lower(hex(randomblob(2))),2)||'-'||lower(hex(randomblob(6))),a.owner_user_id,a.machine_id,a.gateway_group_id,a.id,s.id,a.endpoint_revision,json_object('device_id',OLD.id,'ip',OLD.overlay_ip,'adb_port',s.port,'adb_service_id',s.id,'connection_mode',s.connection_mode),'device_target_changed',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')
 FROM asset_devices a LEFT JOIN asset_services s ON s.device_id=a.id AND s.service_type='adb' WHERE a.id IN (SELECT NEW.id);
END;

CREATE TRIGGER discovery_gateway_changed AFTER UPDATE OF binding_snapshot_json ON asset_gateway_groups WHEN OLD.binding_snapshot_json IS NOT NEW.binding_snapshot_json BEGIN
UPDATE asset_devices SET endpoint_revision=endpoint_revision+1 WHERE id IN (SELECT id FROM asset_devices WHERE gateway_group_id=NEW.id);
UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT id FROM asset_devices WHERE gateway_group_id=NEW.id) AND state NOT IN ('closed','failed');
UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT id FROM asset_devices WHERE gateway_group_id=NEW.id) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending');
INSERT INTO discovery_endpoint_events(event_id,owner_user_id,machine_id,group_id,device_id,adb_service_id,endpoint_revision,old_target_json,reason,created_at,updated_at)
 SELECT lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-4'||substr(lower(hex(randomblob(2))),2)||'-a'||substr(lower(hex(randomblob(2))),2)||'-'||lower(hex(randomblob(6))),a.owner_user_id,a.machine_id,a.gateway_group_id,a.id,s.id,a.endpoint_revision,NULL,'gateway_changed',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')
 FROM asset_devices a LEFT JOIN asset_services s ON s.device_id=a.id AND s.service_type='adb' WHERE a.id IN (SELECT id FROM asset_devices WHERE gateway_group_id=NEW.id);
END;

CREATE TRIGGER discovery_type_changed AFTER UPDATE OF semantic_kind ON asset_device_types WHEN OLD.semantic_kind IS NOT NEW.semantic_kind BEGIN
UPDATE asset_devices SET endpoint_revision=endpoint_revision+1 WHERE id IN (SELECT id FROM asset_devices WHERE device_type_id=NEW.id);
UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT id FROM asset_devices WHERE device_type_id=NEW.id) AND state NOT IN ('closed','failed');
UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT id FROM asset_devices WHERE device_type_id=NEW.id) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending');
INSERT INTO discovery_endpoint_events(event_id,owner_user_id,machine_id,group_id,device_id,adb_service_id,endpoint_revision,old_target_json,reason,created_at,updated_at)
 SELECT lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-4'||substr(lower(hex(randomblob(2))),2)||'-a'||substr(lower(hex(randomblob(2))),2)||'-'||lower(hex(randomblob(6))),a.owner_user_id,a.machine_id,a.gateway_group_id,a.id,s.id,a.endpoint_revision,NULL,'type_changed',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')
 FROM asset_devices a LEFT JOIN asset_services s ON s.device_id=a.id AND s.service_type='adb' WHERE a.id IN (SELECT id FROM asset_devices WHERE device_type_id=NEW.id);
END;

CREATE TRIGGER discovery_instance_deleted BEFORE DELETE ON user_running_network_configs BEGIN
UPDATE asset_devices SET endpoint_revision=endpoint_revision+1 WHERE id IN (SELECT id FROM asset_devices WHERE owner_user_id=OLD.user_id AND machine_id=OLD.device_id AND network_instance_id=OLD.network_instance_id);
UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT id FROM asset_devices WHERE owner_user_id=OLD.user_id AND machine_id=OLD.device_id AND network_instance_id=OLD.network_instance_id) AND state NOT IN ('closed','failed');
UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT id FROM asset_devices WHERE owner_user_id=OLD.user_id AND machine_id=OLD.device_id AND network_instance_id=OLD.network_instance_id) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending');
INSERT INTO discovery_endpoint_events(event_id,owner_user_id,machine_id,group_id,device_id,adb_service_id,endpoint_revision,old_target_json,reason,created_at,updated_at)
 SELECT lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-4'||substr(lower(hex(randomblob(2))),2)||'-a'||substr(lower(hex(randomblob(2))),2)||'-'||lower(hex(randomblob(6))),a.owner_user_id,a.machine_id,a.gateway_group_id,a.id,s.id,a.endpoint_revision,NULL,'instance_deleted',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')
 FROM asset_devices a LEFT JOIN asset_services s ON s.device_id=a.id AND s.service_type='adb' WHERE a.id IN (SELECT id FROM asset_devices WHERE owner_user_id=OLD.user_id AND machine_id=OLD.device_id AND network_instance_id=OLD.network_instance_id);
END;

CREATE TRIGGER discovery_instance_route_changed AFTER UPDATE OF network_config,disabled ON user_running_network_configs
WHEN OLD.disabled IS NOT NEW.disabled
  OR (CASE WHEN json_valid(OLD.network_config) THEN json_extract(OLD.network_config,'$.virtual_ipv4','$.dhcp','$.dhcp_ipv4_cidr','$.network_length','$.proxy_cidrs','$.network_identity') ELSE OLD.network_config END)
    IS NOT (CASE WHEN json_valid(NEW.network_config) THEN json_extract(NEW.network_config,'$.virtual_ipv4','$.dhcp','$.dhcp_ipv4_cidr','$.network_length','$.proxy_cidrs','$.network_identity') ELSE NEW.network_config END)
BEGIN
UPDATE asset_devices SET endpoint_revision=endpoint_revision+1 WHERE id IN (SELECT id FROM asset_devices WHERE owner_user_id=NEW.user_id AND machine_id=NEW.device_id AND network_instance_id=NEW.network_instance_id);
UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT id FROM asset_devices WHERE owner_user_id=NEW.user_id AND machine_id=NEW.device_id AND network_instance_id=NEW.network_instance_id) AND state NOT IN ('closed','failed');
UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT id FROM asset_devices WHERE owner_user_id=NEW.user_id AND machine_id=NEW.device_id AND network_instance_id=NEW.network_instance_id) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending');
INSERT INTO discovery_endpoint_events(event_id,owner_user_id,machine_id,group_id,device_id,adb_service_id,endpoint_revision,old_target_json,reason,created_at,updated_at)
 SELECT lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-4'||substr(lower(hex(randomblob(2))),2)||'-a'||substr(lower(hex(randomblob(2))),2)||'-'||lower(hex(randomblob(6))),a.owner_user_id,a.machine_id,a.gateway_group_id,a.id,s.id,a.endpoint_revision,NULL,'instance_route_changed',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')
 FROM asset_devices a LEFT JOIN asset_services s ON s.device_id=a.id AND s.service_type='adb' WHERE a.id IN (SELECT id FROM asset_devices WHERE owner_user_id=NEW.user_id AND machine_id=NEW.device_id AND network_instance_id=NEW.network_instance_id);
END;

CREATE TRIGGER discovery_adb_created AFTER INSERT ON asset_services WHEN NEW.service_type='adb' BEGIN
UPDATE asset_devices SET endpoint_revision=endpoint_revision+1 WHERE id IN (SELECT NEW.device_id);
UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT NEW.device_id) AND state NOT IN ('closed','failed');
UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT NEW.device_id) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending');
INSERT INTO discovery_endpoint_events(event_id,owner_user_id,machine_id,group_id,device_id,adb_service_id,endpoint_revision,old_target_json,reason,created_at,updated_at)
 SELECT lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-4'||substr(lower(hex(randomblob(2))),2)||'-a'||substr(lower(hex(randomblob(2))),2)||'-'||lower(hex(randomblob(6))),a.owner_user_id,a.machine_id,a.gateway_group_id,a.id,s.id,a.endpoint_revision,NULL,'adb_created',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')
 FROM asset_devices a LEFT JOIN asset_services s ON s.device_id=a.id AND s.service_type='adb' WHERE a.id IN (SELECT NEW.device_id);
END;

CREATE TRIGGER discovery_adb_deleted BEFORE DELETE ON asset_services WHEN OLD.service_type='adb' BEGIN
UPDATE asset_devices SET endpoint_revision=endpoint_revision+1 WHERE id IN (SELECT OLD.device_id);
UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT OLD.device_id) AND state NOT IN ('closed','failed');
UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT OLD.device_id) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending');
INSERT INTO discovery_endpoint_events(event_id,owner_user_id,machine_id,group_id,device_id,adb_service_id,endpoint_revision,old_target_json,reason,created_at,updated_at)
 SELECT lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-4'||substr(lower(hex(randomblob(2))),2)||'-a'||substr(lower(hex(randomblob(2))),2)||'-'||lower(hex(randomblob(6))),a.owner_user_id,a.machine_id,a.gateway_group_id,a.id,OLD.id,a.endpoint_revision,json_object('device_id',OLD.device_id,'ip',a.overlay_ip,'adb_port',OLD.port,'adb_service_id',OLD.id,'connection_mode',OLD.connection_mode),'adb_deleted',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')
 FROM asset_devices a LEFT JOIN asset_services s ON s.device_id=a.id AND s.service_type='adb' WHERE a.id IN (SELECT OLD.device_id);
END;

CREATE TRIGGER discovery_adb_changed AFTER UPDATE OF port,connection_mode,service_type,device_id ON asset_services
WHEN (OLD.service_type='adb' OR NEW.service_type='adb') AND (OLD.port IS NOT NEW.port OR OLD.connection_mode IS NOT NEW.connection_mode OR OLD.service_type IS NOT NEW.service_type OR OLD.device_id IS NOT NEW.device_id) BEGIN
UPDATE asset_services SET endpoint_revision=OLD.endpoint_revision+1 WHERE id=NEW.id;
UPDATE asset_devices SET endpoint_revision=endpoint_revision+1 WHERE id IN (SELECT OLD.device_id);
UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT OLD.device_id) AND state NOT IN ('closed','failed');
UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT OLD.device_id) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending');
INSERT INTO discovery_endpoint_events(event_id,owner_user_id,machine_id,group_id,device_id,adb_service_id,endpoint_revision,old_target_json,reason,created_at,updated_at)
 SELECT lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-4'||substr(lower(hex(randomblob(2))),2)||'-a'||substr(lower(hex(randomblob(2))),2)||'-'||lower(hex(randomblob(6))),a.owner_user_id,a.machine_id,a.gateway_group_id,a.id,s.id,a.endpoint_revision,json_object('device_id',OLD.device_id,'ip',a.overlay_ip,'adb_port',OLD.port,'adb_service_id',OLD.id,'connection_mode',OLD.connection_mode),'adb_changed',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')
 FROM asset_devices a LEFT JOIN asset_services s ON s.device_id=a.id AND s.service_type='adb' WHERE a.id IN (SELECT OLD.device_id);
UPDATE asset_devices SET endpoint_revision=endpoint_revision+1 WHERE id IN (SELECT NEW.device_id WHERE NEW.device_id<>OLD.device_id);
UPDATE automation_sessions SET state='revoked',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT NEW.device_id WHERE NEW.device_id<>OLD.device_id) AND state NOT IN ('closed','failed');
UPDATE automation_run_devices SET withdraw_requested=1,state='cancel_requested',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE device_id IN (SELECT NEW.device_id WHERE NEW.device_id<>OLD.device_id) AND cleanup_confirmed=0 AND state IN ('dispatch_pending','queued','accepted','starting','running','cancel_requested','outcome_unknown','cleanup_pending');
INSERT INTO discovery_endpoint_events(event_id,owner_user_id,machine_id,group_id,device_id,adb_service_id,endpoint_revision,old_target_json,reason,created_at,updated_at)
 SELECT lower(hex(randomblob(4)))||'-'||lower(hex(randomblob(2)))||'-4'||substr(lower(hex(randomblob(2))),2)||'-a'||substr(lower(hex(randomblob(2))),2)||'-'||lower(hex(randomblob(6))),a.owner_user_id,a.machine_id,a.gateway_group_id,a.id,s.id,a.endpoint_revision,NULL,'adb_reassigned',strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')
 FROM asset_devices a LEFT JOIN asset_services s ON s.device_id=a.id AND s.service_type='adb' WHERE a.id IN (SELECT NEW.device_id WHERE NEW.device_id<>OLD.device_id);
END;
