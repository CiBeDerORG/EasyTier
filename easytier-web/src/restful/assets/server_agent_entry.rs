//! Server-only read-only Agent onboarding. No network requests or pairing.
use super::*;

fn eligible(device: &Value) -> bool {
    device["semantic_kinds"]
        .as_array()
        .is_some_and(|kinds| kinds.iter().any(|kind| kind.as_str() == Some("server")))
}

// Shared scope check for GET entry and explicit T01-06 mutating routes.
pub(crate) async fn load_authorized(
    auth: &AuthSession,
    config: &ConsoleInfoConfig,
    service: &CentralNetworkService,
    owner: i32,
    node: &str,
    id: &str,
) -> HttpResult<Value> {
    authorize(auth, config, service, owner).await?;
    let node = machine_id(node)?;
    let store = AssetsStore::new(service.db.clone());
    let device = store
        .get_device(owner, &node, id)
        .await
        .map_err(asset_error)?;
    if !eligible(&device) {
        return Err(asset_error(AssetError::conflict(
            "agent_requires_server_semantic",
            "Only existing asset devices with server semantics may bind an Agent.",
        )));
    }
    Ok(device)
}

pub(super) async fn get(
    auth: AuthSession,
    Extension(service): Service,
    Extension(config): Config,
    Path(id): Path<String>,
    query: ScopeInput,
) -> HttpResult<Json<Value>> {
    let (_, owner, node) = scoped_context(&auth, &config, &service, query).await?;
    let store = AssetsStore::new(service.db.clone());
    let device = store
        .get_device(owner, &node, &id)
        .await
        .map_err(asset_error)?;
    if !eligible(&device) {
        return Err(asset_error(AssetError::conflict(
            "agent_requires_server_semantic",
            "Only existing asset devices with the server semantic type can add an Agent.",
        )));
    }
    Ok(Json(json!({
        "owner_user_id": owner,
        "machine_id": node,
        "asset_device_id": device["id"],
        "device_name": device["name"],
        "overlay_ip": device["overlay_ip"],
        "lan_ip": device["lan_ip"],
        "agent_status": "not_checked",
        "pairing_available": false,
        "source": "existing_asset_device",
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_server_in_multi_semantics_is_eligible() {
        assert!(eligible(&json!({"semantic_kinds": ["openwrt", "server"]})));
        assert!(eligible(&json!({"semantic_kinds": ["server"]})));
        assert!(!eligible(&json!({"semantic_kinds": ["phone"]})));
        assert!(!eligible(
            &json!({"semantic_kind": "server", "semantic_kinds": []})
        ));
        assert!(!eligible(&json!({"semantic_kinds": null})));
    }
}
