use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                CREATE TABLE node_asset_metadata (
                    owner_user_id INTEGER NOT NULL,
                    machine_id TEXT NOT NULL,
                    is_public_server INTEGER NOT NULL DEFAULT 0
                        CHECK (is_public_server IN (0, 1)),
                    PRIMARY KEY (owner_user_id, machine_id),
                    FOREIGN KEY (owner_user_id, machine_id)
                        REFERENCES devices(user_id, machine_id) ON DELETE CASCADE
                );

                CREATE TABLE asset_catalog_state (
                    owner_user_id INTEGER NOT NULL PRIMARY KEY,
                    initialized_at TEXT NOT NULL,
                    FOREIGN KEY (owner_user_id) REFERENCES users(id) ON DELETE CASCADE
                );

                CREATE TABLE asset_device_types (
                    id TEXT NOT NULL PRIMARY KEY,
                    owner_user_id INTEGER NOT NULL,
                    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 128),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    UNIQUE (id, owner_user_id),
                    FOREIGN KEY (owner_user_id) REFERENCES users(id) ON DELETE RESTRICT
                );
                CREATE INDEX idx_asset_device_types_owner
                    ON asset_device_types(owner_user_id, created_at, id);

                CREATE TABLE asset_gateway_groups (
                    id TEXT NOT NULL PRIMARY KEY,
                    owner_user_id INTEGER NOT NULL,
                    machine_id TEXT NOT NULL,
                    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 128),
                    remarks TEXT CHECK (remarks IS NULL OR length(remarks) <= 2000),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    UNIQUE (id, owner_user_id, machine_id),
                    FOREIGN KEY (owner_user_id, machine_id)
                        REFERENCES devices(user_id, machine_id) ON DELETE RESTRICT
                );
                CREATE INDEX idx_asset_gateway_groups_node
                    ON asset_gateway_groups(owner_user_id, machine_id, created_at DESC, id DESC);

                CREATE TABLE asset_gateway_subnets (
                    group_id TEXT NOT NULL,
                    owner_user_id INTEGER NOT NULL,
                    machine_id TEXT NOT NULL,
                    cidr TEXT NOT NULL,
                    PRIMARY KEY (group_id, cidr),
                    FOREIGN KEY (group_id, owner_user_id, machine_id)
                        REFERENCES asset_gateway_groups(id, owner_user_id, machine_id)
                        ON DELETE CASCADE
                );
                CREATE INDEX idx_asset_gateway_subnets_scope
                    ON asset_gateway_subnets(owner_user_id, machine_id, group_id);

                CREATE TABLE asset_devices (
                    id TEXT NOT NULL PRIMARY KEY,
                    owner_user_id INTEGER NOT NULL,
                    machine_id TEXT NOT NULL,
                    gateway_group_id TEXT,
                    device_type_id TEXT,
                    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 128),
                    model TEXT CHECK (model IS NULL OR length(model) <= 128),
                    overlay_ip TEXT NOT NULL CHECK (length(trim(overlay_ip)) > 0),
                    lan_ip TEXT CHECK (lan_ip IS NULL OR length(trim(lan_ip)) > 0),
                    is_gateway_self INTEGER NOT NULL DEFAULT 0
                        CHECK (is_gateway_self IN (0, 1)),
                    remarks TEXT CHECK (remarks IS NULL OR length(remarks) <= 2000),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    UNIQUE (id, owner_user_id, machine_id),
                    UNIQUE (owner_user_id, machine_id, lan_ip),
                    CHECK (is_gateway_self = 0 OR gateway_group_id IS NOT NULL),
                    FOREIGN KEY (owner_user_id, machine_id)
                        REFERENCES devices(user_id, machine_id) ON DELETE RESTRICT,
                    FOREIGN KEY (gateway_group_id, owner_user_id, machine_id)
                        REFERENCES asset_gateway_groups(id, owner_user_id, machine_id)
                        ON DELETE RESTRICT,
                    FOREIGN KEY (device_type_id, owner_user_id)
                        REFERENCES asset_device_types(id, owner_user_id) ON DELETE RESTRICT
                );
                CREATE UNIQUE INDEX idx_asset_devices_gateway_self
                    ON asset_devices(gateway_group_id) WHERE is_gateway_self = 1;
                CREATE INDEX idx_asset_devices_node_created
                    ON asset_devices(owner_user_id, machine_id, created_at DESC, id DESC);
                CREATE INDEX idx_asset_devices_group
                    ON asset_devices(owner_user_id, machine_id, gateway_group_id, created_at DESC, id DESC);
                CREATE INDEX idx_asset_devices_type
                    ON asset_devices(owner_user_id, machine_id, device_type_id, created_at DESC, id DESC);
                CREATE INDEX idx_asset_devices_type_reference
                    ON asset_devices(device_type_id, owner_user_id);

                CREATE TABLE asset_services (
                    id TEXT NOT NULL PRIMARY KEY,
                    owner_user_id INTEGER NOT NULL,
                    machine_id TEXT NOT NULL,
                    device_id TEXT NOT NULL,
                    name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 128),
                    port INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
                    remarks TEXT CHECK (remarks IS NULL OR length(remarks) <= 2000),
                    access_url TEXT CHECK (access_url IS NULL OR length(access_url) <= 2048),
                    icon TEXT CHECK (icon IS NULL OR icon IN (
                        'server', 'globe', 'database', 'mobile', 'desktop', 'home',
                        'cloud', 'cog', 'play', 'link', 'shield', 'wifi'
                    )),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    UNIQUE (id, owner_user_id, machine_id, device_id),
                    FOREIGN KEY (device_id, owner_user_id, machine_id)
                        REFERENCES asset_devices(id, owner_user_id, machine_id)
                        ON DELETE CASCADE
                );
                CREATE INDEX idx_asset_services_device
                    ON asset_services(owner_user_id, machine_id, device_id, created_at DESC, id DESC);

                CREATE TABLE asset_service_bindings (
                    service_id TEXT NOT NULL,
                    owner_user_id INTEGER NOT NULL,
                    machine_id TEXT NOT NULL,
                    device_id TEXT NOT NULL,
                    protocol TEXT NOT NULL CHECK (protocol IN ('tcp', 'udp')),
                    port INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
                    PRIMARY KEY (service_id, protocol),
                    UNIQUE (owner_user_id, machine_id, device_id, protocol, port),
                    FOREIGN KEY (service_id, owner_user_id, machine_id, device_id)
                        REFERENCES asset_services(id, owner_user_id, machine_id, device_id)
                        ON DELETE CASCADE
                );

                CREATE TABLE asset_operations (
                    actor_user_id INTEGER NOT NULL,
                    owner_user_id INTEGER NOT NULL,
                    operation TEXT NOT NULL,
                    request_id TEXT NOT NULL,
                    payload_hash TEXT NOT NULL,
                    resource_kind TEXT NOT NULL,
                    resource_id TEXT NOT NULL,
                    response_json TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    PRIMARY KEY (actor_user_id, owner_user_id, operation, request_id)
                );
                CREATE INDEX idx_asset_operations_resource
                    ON asset_operations(owner_user_id, resource_kind, resource_id);
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Explicit rollback removes only this feature's tables. Restoring a
        // pre-upgrade backup is required when the asset records must be retained.
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                DROP TABLE asset_operations;
                DROP TABLE asset_service_bindings;
                DROP TABLE asset_services;
                DROP TABLE asset_devices;
                DROP TABLE asset_gateway_subnets;
                DROP TABLE asset_gateway_groups;
                DROP TABLE asset_device_types;
                DROP TABLE asset_catalog_state;
                DROP TABLE node_asset_metadata;
                "#,
            )
            .await?;
        Ok(())
    }
}
