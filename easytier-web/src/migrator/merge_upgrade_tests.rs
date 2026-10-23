use super::Migrator;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, Statement};
use sea_orm_migration::{MigrationTrait, MigratorTrait};

const MAP_MIGRATION: &str = "m20261024_000011_network_space";
const AGENT_MIGRATION: &str = "m20261024_000011_deploy_agents";

struct AgentBranch;
impl MigratorTrait for AgentBranch {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Migrator::migrations()
            .into_iter()
            .filter(|migration| migration.name() < "m20261022" || migration.name() == AGENT_MIGRATION)
            .collect()
    }
}

// Reproduce the two parent branches' migration histories, including their
// recorded versions, before running the combined migration registry.
struct WebBranch;
impl MigratorTrait for WebBranch {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Migrator::migrations()
            .into_iter()
            .filter(|migration| migration.name() < "m20261022" || migration.name() == MAP_MIGRATION)
            .collect()
    }
}

struct NodeBranch;
impl MigratorTrait for NodeBranch {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Migrator::migrations()
            .into_iter()
            .filter(|migration| migration.name() < "m20261024")
            .collect()
    }
}

async fn seed_shared_records(db: &DatabaseConnection) {
    db.execute_unprepared(
        r#"
        INSERT INTO users(id, username, password) VALUES (1001, 'merge-owner', 'test-hash');
        INSERT INTO devices(user_id, machine_id, hostname, alias, first_seen_time, last_seen_time)
            VALUES (1001, 'machine-1', 'gateway', 'saved alias', '2026-10-23', '2026-10-23');
        INSERT INTO networks(user_id, id, display_name, network_name, network_secret,
            networking_method, create_time, update_time)
            VALUES (1001, 'network-1', 'Home', 'home-network', 'test-secret',
                'Manual', '2026-10-23', '2026-10-23');
        "#,
    )
    .await
    .unwrap();
}

async fn assert_schema_complete_and_idempotent(db: &DatabaseConnection) {
    // An upgrade must also remain safe when the service starts a second time.
    Migrator::up(db, None).await.unwrap();
    for table in [
        "node_geo_locations",
        "asset_devices",
        "asset_services",
        "network_map_configs",
        "network_services",
        "deploy_agents",
        "deploy_tasks",
    ] {
        let sql = format!("SELECT COUNT(*) AS n FROM {table}");
        assert!(
            db.query_one(Statement::from_string(db.get_database_backend(), sql))
                .await
                .unwrap()
                .is_some()
        );
    }
    assert!(
        db.query_all(Statement::from_string(
            db.get_database_backend(),
            "PRAGMA foreign_key_check".to_owned(),
        ))
        .await
        .unwrap()
        .is_empty()
    );
}

#[tokio::test]
async fn agent_database_upgrades_without_losing_bindings_credentials_or_tasks() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    AgentBranch::up(&db, None).await.unwrap();
    seed_shared_records(&db).await;
    db.execute_unprepared(
        r#"
        INSERT INTO deploy_agents(id, user_id, alias, endpoint, ca_pem, credential, agent_id, updated_at)
            VALUES ('connection-1', 1001, 'NAS', 'https://nas:7014', 'certificate',
                'encrypted-credential', 'agent-1', '2026-10-23');
        INSERT INTO deploy_tasks(task_id, connection_id, user_id, request_cipher, result_json, logs, log_offset, updated_at)
            VALUES ('task-1', 'connection-1', 1001, 'encrypted-request', '{"status":"running"}',
                'saved log', 9, '2026-10-23');
        "#,
    ).await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    let row = db.query_one(Statement::from_string(db.get_database_backend(),
        "SELECT a.credential, a.agent_id, t.request_cipher, t.result_json, t.logs, t.log_offset FROM deploy_agents a JOIN deploy_tasks t ON t.connection_id = a.id".to_owned(),
    )).await.unwrap().unwrap();
    assert_eq!(row.try_get::<String>("", "credential").unwrap(), "encrypted-credential");
    assert_eq!(row.try_get::<String>("", "agent_id").unwrap(), "agent-1");
    assert_eq!(row.try_get::<String>("", "request_cipher").unwrap(), "encrypted-request");
    assert_eq!(row.try_get::<String>("", "result_json").unwrap(), "{\"status\":\"running\"}");
    assert_eq!(row.try_get::<String>("", "logs").unwrap(), "saved log");
    assert_eq!(row.try_get::<i64>("", "log_offset").unwrap(), 9);
    assert_schema_complete_and_idempotent(&db).await;
}

#[tokio::test]
async fn web_database_upgrades_without_losing_map_or_navigation_services() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    WebBranch::up(&db, None).await.unwrap();
    seed_shared_records(&db).await;
    db.execute_unprepared(
        r#"
        INSERT INTO network_map_configs(user_id, network_id, revision, config_json)
            VALUES (1001, 'network-1', 4, '{"nodes":{"machine-1":{"latitude":31.2}}}');
        INSERT INTO network_services(user_id, id, network_id, device_id, config_json)
            VALUES (1001, 'service-1', 'network-1', 'machine-1', '{"name":"NAS"}');
        "#,
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    let row = db.query_one(Statement::from_string(db.get_database_backend(),
        "SELECT m.revision, m.config_json, s.config_json AS service_json, d.alias, d.ip_location_status FROM network_map_configs m JOIN network_services s ON s.network_id = m.network_id JOIN devices d ON d.machine_id = s.device_id".to_owned(),
    )).await.unwrap().unwrap();
    assert_eq!(row.try_get::<i32>("", "revision").unwrap(), 4);
    assert_eq!(
        row.try_get::<String>("", "config_json").unwrap(),
        "{\"nodes\":{\"machine-1\":{\"latitude\":31.2}}}"
    );
    assert_eq!(
        row.try_get::<String>("", "service_json").unwrap(),
        "{\"name\":\"NAS\"}"
    );
    assert_eq!(row.try_get::<String>("", "alias").unwrap(), "saved alias");
    assert_eq!(
        row.try_get::<String>("", "ip_location_status").unwrap(),
        "unknown"
    );
    assert_schema_complete_and_idempotent(&db).await;
}

#[tokio::test]
async fn node_database_upgrades_without_losing_attributes_assets_or_coordinates() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    NodeBranch::up(&db, None).await.unwrap();
    seed_shared_records(&db).await;
    db.execute_unprepared(
        r#"
        UPDATE devices SET device_model = 'Router A' WHERE machine_id = 'machine-1';
        INSERT INTO node_geo_locations(owner_user_id, machine_id, city, latitude, longitude)
            VALUES (1001, 'machine-1', 'Shanghai', 31.2, 121.5);
        INSERT INTO node_asset_metadata(owner_user_id, machine_id, is_public_server)
            VALUES (1001, 'machine-1', 1);
        INSERT INTO asset_devices(id, owner_user_id, machine_id, name, overlay_ip, created_at, updated_at)
            VALUES ('asset-1', 1001, 'machine-1', 'NAS', '10.0.0.2', '2026-10-23', '2026-10-23');
        INSERT INTO asset_services(id, owner_user_id, machine_id, device_id, name, port, created_at, updated_at)
            VALUES ('asset-service-1', 1001, 'machine-1', 'asset-1', 'Files', 443, '2026-10-23', '2026-10-23');
        "#,
    ).await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    let row = db.query_one(Statement::from_string(db.get_database_backend(),
        "SELECT d.device_model, g.latitude, g.longitude, m.is_public_server, a.overlay_ip, s.port, s.show_in_navigation FROM devices d JOIN node_geo_locations g ON g.machine_id = d.machine_id JOIN node_asset_metadata m ON m.machine_id = d.machine_id JOIN asset_devices a ON a.machine_id = d.machine_id JOIN asset_services s ON s.device_id = a.id".to_owned(),
    )).await.unwrap().unwrap();
    assert_eq!(
        row.try_get::<String>("", "device_model").unwrap(),
        "Router A"
    );
    assert_eq!(row.try_get::<f64>("", "latitude").unwrap(), 31.2);
    assert_eq!(row.try_get::<f64>("", "longitude").unwrap(), 121.5);
    assert_eq!(row.try_get::<i32>("", "is_public_server").unwrap(), 1);
    assert_eq!(row.try_get::<String>("", "overlay_ip").unwrap(), "10.0.0.2");
    assert_eq!(row.try_get::<i32>("", "port").unwrap(), 443);
    assert!(!row.try_get::<bool>("", "show_in_navigation").unwrap());
    assert_schema_complete_and_idempotent(&db).await;
}
