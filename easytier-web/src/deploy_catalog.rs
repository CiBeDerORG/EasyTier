//! Manually refreshed, read-only Deploy catalog index (T03-04/T03-05).
//! The index is DISPLAY DATA ONLY: it never authorizes installation, adoption,
//! script execution, or resource ownership. T03-07 must revalidate a pinned
//! manifest and every packaged file before granting deployment capability.
use crate::db::Db;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::{collections::{BTreeMap, BTreeSet}, path::{Path, PathBuf}};
use tokio::{process::Command, sync::Mutex, time::{Duration, timeout}};
use uuid::Uuid;

const REPO_URL: &str = "https://github.com/CiBeDerORG/deploy.git";
const MAX_MANIFEST: u64 = 128 * 1024;
const MAX_APPS: usize = 128;
const MAX_PACKAGE_FILES: usize = 128;
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_PACKAGE_BYTES: u64 = 24 * 1024 * 1024;
const REFRESH_TIMEOUT: Duration = Duration::from_secs(90);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Deploy catalog has not been refreshed")]
    Uninitialized,
    #[error("Deploy catalog refresh failed; the previous revision was retained")]
    Remote,
    #[error("Invalid Deploy catalog contents: {0}")]
    Invalid(&'static str),
    #[error("Deploy catalog storage unavailable")]
    Storage,
    #[error("Pinned catalog revision is not available locally; refresh the store again")]
    RevisionUnavailable,
    #[error("Immutable package cache conflicts with the pinned revision")]
    PackageConflict,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CatalogApp {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub category: String,
    pub compose_project: String,
    pub manifest_path: String,
    pub manifest_sha256: String,
    pub git_revision: String,
    pub detail: Value,
}


#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestV1 {
    schema_version: u32,
    id: String,
    name: String,
    summary: String,
    category: String,
    #[serde(default)]
    homepage: Option<String>,
    runtime: RuntimeSpec,
    compatibility: CompatibilitySpec,
    package: PackageSpec,
    config: Vec<ConfigSpec>,
    ports: Vec<PortSpec>,
    actions: ActionsSpec,
    verification: VerificationSpec,
    data: DataSpec,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeSpec {
    kind: String,
    compose_project: String,
    compose_file: String,
    #[serde(default)]
    requires_root: Option<bool>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompatibilitySpec {
    platforms: Vec<String>,
    architectures: Vec<String>,
    #[serde(default)]
    requires_docker: Option<bool>,
    #[serde(default)]
    requires_compose_v2: Option<bool>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageSpec { files: Vec<PackageFileSpec> }
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageFileSpec {
    source: String,
    destination: String,
    #[serde(default)]
    mode: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigSpec {
    key: String,
    label: String,
    #[serde(rename = "type")]
    kind: String,
    target: String,
    required: bool,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    default: Option<Value>,
    #[serde(default)]
    min_length: Option<usize>,
    #[serde(default)]
    max_length: Option<usize>,
    #[serde(default)]
    minimum: Option<i64>,
    #[serde(default)]
    maximum: Option<i64>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    generated_by_script: Option<bool>,
    #[serde(default)]
    editable: Option<bool>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortSpec {
    id: String,
    label: String,
    protocol: String,
    purpose: String,
    host_port_key: String,
    #[serde(default)]
    bind_address_key: Option<String>,
    container_port: u32,
    #[serde(default)]
    service: Option<String>,
    #[serde(default)]
    path: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionsSpec {
    install: ActionSpec,
    #[serde(default)]
    update: Option<ActionSpec>,
    #[serde(default)]
    uninstall: Option<ActionSpec>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActionSpec {
    strategy: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    argv: Vec<String>,
    #[serde(default)]
    remove_orphans: bool,
    #[serde(default)]
    description: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct VerificationSpec {
    mode: String,
    required_services: Vec<String>,
    state: String,
    timeout_seconds: u64,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DataSpec {
    paths: Vec<String>,
    #[serde(default)]
    named_volumes: Vec<String>,
    default_retain: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DeployPackageSummary {
    pub app_id: String,
    pub git_revision: String,
    pub manifest_sha256: String,
    pub package_sha256: String,
    pub package_size: u64,
    pub file_count: usize,
    pub built_at: String,
    pub reused: bool,
    pub ready_for_agent: bool,
    pub install_enabled: bool,
}
#[derive(Clone, Debug, Serialize)]
struct PackageFileDigest {
    source: String,
    destination: String,
    sha256: String,
    size: u64,
    mode: String,
}
#[derive(Serialize)]
struct PackageEnvelope {
    format_version: u32,
    app_id: String,
    git_revision: String,
    manifest_sha256: String,
    manifest_path: String,
    files: Vec<PackageFileDigest>,
}

pub struct Catalog {
    db: Db,
    base: PathBuf,
    refresh_lock: Mutex<()>,
    package_lock: Mutex<()>,
}

impl Catalog {
    pub fn new(db: Db, base: PathBuf) -> Self {
        Self { db, base, refresh_lock: Mutex::new(()), package_lock: Mutex::new(()) }
    }

    pub fn from_environment(db: Db) -> Self {
        let base = std::env::var_os("ET_DEPLOY_CATALOG_DIR")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".deploy-catalog"));
        Self::new(db, base)
    }

    pub async fn list(&self) -> Result<Value, Error> {
        // Read metadata and rows under one SQLite snapshot so a concurrent
        // manual refresh never returns a mixed revision to the UI.
        let pool = self.db.inner();
        let mut tx = pool.begin().await.map_err(|_| Error::Storage)?;
        let state: Option<(String, String, i64)> = sqlx::query_as(
            "SELECT git_revision,fetched_at,app_count FROM deploy_catalog_state WHERE singleton=1"
        ).fetch_optional(&mut *tx).await.map_err(|_| Error::Storage)?;
        let rows = sqlx::query(
            "SELECT app_id,git_revision,name,summary,category,compose_project,manifest_path,manifest_sha256,detail_json FROM deploy_catalog_apps ORDER BY name,app_id"
        ).fetch_all(&mut *tx).await.map_err(|_| Error::Storage)?;
        tx.commit().await.map_err(|_| Error::Storage)?;
        let mut apps = Vec::new();
        for row in rows {
            let raw: String = row.try_get("detail_json").map_err(|_| Error::Storage)?;
            apps.push(CatalogApp {
                id: row.try_get("app_id").map_err(|_| Error::Storage)?,
                git_revision: row.try_get("git_revision").map_err(|_| Error::Storage)?,
                name: row.try_get("name").map_err(|_| Error::Storage)?,
                summary: row.try_get("summary").map_err(|_| Error::Storage)?,
                category: row.try_get("category").map_err(|_| Error::Storage)?,
                compose_project: row.try_get("compose_project").map_err(|_| Error::Storage)?,
                manifest_path: row.try_get("manifest_path").map_err(|_| Error::Storage)?,
                manifest_sha256: row.try_get("manifest_sha256").map_err(|_| Error::Storage)?,
                detail: serde_json::from_str(&raw).map_err(|_| Error::Storage)?,
            });
        }
        Ok(match state {
            Some((revision, fetched_at, count)) => json!({
                "initialized":true,"git_revision":revision,"fetched_at":fetched_at,
                "count":count,"items":apps,"read_only":true
            }),
            None => json!({"initialized":false,"items":[],"count":0,"read_only":true}),
        })
    }

    pub async fn detail(&self, id: &str) -> Result<Value, Error> {
        if !safe_id(id) { return Err(Error::Invalid("Invalid application identifier")); }
        let all = self.list().await?;
        if all["initialized"] != true { return Err(Error::Uninitialized); }
        all["items"].as_array()
            .and_then(|rows| rows.iter().find(|row| row["id"].as_str() == Some(id)))
            .cloned()
            .ok_or(Error::Invalid("Application not in catalog"))
    }

    // Only the explicit admin POST calls git. Reading the catalog never calls
    // git/Agent/Docker or performs any network I/O.
    pub async fn refresh(&self) -> Result<Value, Error> {
        let _guard = self.refresh_lock.lock().await;
        let base = self.base.clone();
        tokio::fs::create_dir_all(&base).await.map_err(|_| Error::Storage)?;
        let stage = base.join(format!("stage-{}", Uuid::new_v4()));
        let result = self.stage_and_commit(&stage).await;
        if result.is_err() {
            let _ = tokio::fs::remove_dir_all(&stage).await;
        }
        result
    }

    async fn stage_and_commit(&self, stage: &Path) -> Result<Value, Error> {
        let token = std::env::var("ET_DEPLOY_GIT_TOKEN").map_err(|_| Error::Remote)?;
        if token.is_empty() || token.bytes().any(|x| x < 33 || x > 126) {
            return Err(Error::Remote);
        }
        // Token is passed only as child ENV, never command line, Git URL or
        // database field. Disable interactive prompts and inherited Git config.
        let authorization = format!("AUTHORIZATION: basic {}", base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            format!("x-access-token:{token}")
        ));
        let mut git = Command::new("git");
        git.arg("clone").arg("--quiet").arg("--depth=1").arg("--single-branch")
            .arg("--branch=main").arg("--").arg(REPO_URL).arg(stage)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
            .env("GIT_CONFIG_VALUE_0", authorization)
            .kill_on_drop(true);
        let clone_status = timeout(REFRESH_TIMEOUT, git.status()).await
            .map_err(|_| Error::Remote)?
            .map_err(|_| Error::Remote)?;
        if !clone_status.success() { return Err(Error::Remote); }
        let rev = timeout(Duration::from_secs(10),
            Command::new("git").arg("-C").arg(stage).args(["rev-parse", "HEAD"])
                .env("GIT_CONFIG_NOSYSTEM","1").env("GIT_CONFIG_GLOBAL","/dev/null")
                .kill_on_drop(true).output()
        ).await.map_err(|_| Error::Remote)?.map_err(|_| Error::Remote)?;
        if !rev.status.success() { return Err(Error::Remote); }
        let sha = String::from_utf8(rev.stdout).map_err(|_| Error::Remote)?
            .trim().to_ascii_lowercase();
        if sha.len() != 40 || !sha.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::Remote);
        }
        let apps = scan_catalog(stage, &sha)?;
        if apps.is_empty() { return Err(Error::Invalid("No published app.yaml manifests")); }
        // One immutable checkout per commit lets T03-07 rebuild a package
        // from the exact revision recorded in SQLite, never from moving main.
        let destination = self.base.join(format!("revision-{sha}"));
        match tokio::fs::symlink_metadata(&destination).await {
            Ok(meta) => {
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    return Err(Error::Storage);
                }
                let existing = timeout(Duration::from_secs(10),
                    Command::new("git").arg("-C").arg(&destination).args(["rev-parse", "HEAD"])
                        .env("GIT_CONFIG_NOSYSTEM","1").env("GIT_CONFIG_GLOBAL","/dev/null")
                        .kill_on_drop(true).output()
                ).await.map_err(|_| Error::Storage)?.map_err(|_| Error::Storage)?;
                let existing_sha = String::from_utf8(existing.stdout).map_err(|_| Error::Storage)?;
                if !existing.status.success() || existing_sha.trim() != sha {
                    return Err(Error::PackageConflict);
                }
                tokio::fs::remove_dir_all(stage).await.map_err(|_| Error::Storage)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tokio::fs::rename(stage, &destination).await.map_err(|_| Error::Storage)?;
            }
            Err(_) => return Err(Error::Storage),
        }
        let pool = self.db.inner();
        let mut tx = pool.begin().await.map_err(|_| Error::Storage)?;
        sqlx::query("DELETE FROM deploy_catalog_apps").execute(&mut *tx).await
            .map_err(|_| Error::Storage)?;
        for app in &apps {
            sqlx::query("INSERT INTO deploy_catalog_apps(app_id,git_revision,name,summary,category,compose_project,manifest_path,manifest_sha256,detail_json) VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(&app.id).bind(&app.git_revision).bind(&app.name)
                .bind(&app.summary).bind(&app.category).bind(&app.compose_project)
                .bind(&app.manifest_path).bind(&app.manifest_sha256)
                .bind(app.detail.to_string())
                .execute(&mut *tx).await.map_err(|_| Error::Storage)?;
        }
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO deploy_catalog_state(singleton,git_revision,fetched_at,app_count) VALUES(1,?,?,?) ON CONFLICT(singleton) DO UPDATE SET git_revision=excluded.git_revision,fetched_at=excluded.fetched_at,app_count=excluded.app_count")
            .bind(&sha).bind(&now).bind(apps.len() as i64)
            .execute(&mut *tx).await.map_err(|_| Error::Storage)?;
        tx.commit().await.map_err(|_| Error::Storage)?;
        Ok(json!({"updated":true,"git_revision":sha,"fetched_at":now,
            "count":apps.len(),"read_only":true}))
    }
}

fn safe_id(id: &str) -> bool {
    id.len() >= 2 && id.len() <= 64
        && id.starts_with(|c: char| c.is_ascii_lowercase())
        && id.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
        && !id.ends_with('-') && !id.contains("--")
}


fn parse_yaml<T: serde::de::DeserializeOwned>(text: &str, max_bytes: u64) -> Result<T, Error> {
    if text.len() as u64 > max_bytes {
        return Err(Error::Invalid("YAML document exceeds size limit"));
    }
    let mut options = serde_saphyr::Options::default();
    options.merge_keys = serde_saphyr::MergeKeyPolicy::Error;
    options.reject_unsupported_tags = true;
    options.reject_tagged_field_names = true;
    options.alias_limits.max_total_replayed_events = 0;
    options.alias_limits.max_replay_stack_depth = 0;
    options.alias_limits.max_alias_expansions_per_anchor = 0;
    if let Some(budget) = options.budget.as_mut() {
        budget.max_reader_input_bytes = Some(max_bytes as usize);
        budget.max_events = 100_000;
        budget.max_aliases = 0;
        budget.max_anchors = 0;
        budget.max_depth = 32;
        budget.max_nodes = 50_000;
        budget.max_total_scalar_bytes = max_bytes as usize;
        budget.max_documents = 1;
        budget.max_merge_keys = 0;
        budget.max_recorded_anchor_events = 0;
        budget.max_recorded_anchor_bytes = 0;
    }
    serde_saphyr::from_str_with_options(text, options)
        .map_err(|_| Error::Invalid("YAML is invalid, duplicated, tagged or outside the manifest contract"))
}

fn parse_manifest(text: &str) -> Result<ManifestV1, Error> {
    parse_yaml(text, MAX_MANIFEST)
}

fn safe_relative(raw: &str, reject_runtime_data: bool) -> Result<Vec<&str>, Error> {
    if raw.is_empty() || raw.len() > 256 || raw.starts_with('/')
        || raw.contains('\\') || raw.contains('\0') {
        return Err(Error::Invalid("Unsafe manifest path"));
    }
    let parts: Vec<&str> = raw.split('/').collect();
    if parts.iter().any(|p| p.is_empty() || *p == "." || *p == ".."
        || p.starts_with('.') || !p.bytes().all(|c|
            c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'.')) {
        return Err(Error::Invalid("Unsafe manifest path component"));
    }
    const BLOCKED_NAMES: &[&str] = &[".env", ".secrets", ".git", ".ssh", "id_rsa", "id_ed25519"];
    const BLOCKED_PARTS: &[&str] = &["data", "state", "backup", "runtime", "workspace"];
    for part in &parts {
        let lower = part.to_ascii_lowercase();
        if BLOCKED_NAMES.contains(&lower.as_str())
            || (reject_runtime_data && BLOCKED_PARTS.contains(&lower.as_str())) {
            return Err(Error::Invalid("Manifest path points to protected data or credentials"));
        }
    }
    Ok(parts)
}

fn valid_env_key(key: &str) -> bool {
    key.len() >= 2 && key.len() <= 120
        && key.as_bytes()[0].is_ascii_uppercase()
        && key.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
}
fn json_integer(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_u64().and_then(|n| i64::try_from(n).ok()))
}
fn config_value_matches(field: &ConfigSpec, value: &Value) -> bool {
    match field.kind.as_str() {
        "string" => value.is_string(),
        "secret" => value.as_str().is_some_and(str::is_empty),
        "number" | "port" => json_integer(value).is_some(),
        "boolean" => value.is_boolean(),
        "ipv4" => value.as_str().is_some_and(|s| s.parse::<std::net::Ipv4Addr>().is_ok()),
        _ => false,
    }
}

fn validate_manifest_shape(manifest: &ManifestV1, expected_id: &str) -> Result<(), Error> {
    if manifest.schema_version != 1 || manifest.id != expected_id || !safe_id(&manifest.id)
        || manifest.name.trim().is_empty() || manifest.name.len() > 100
        || manifest.summary.trim().is_empty() || manifest.summary.len() > 500 {
        return Err(Error::Invalid("Manifest version, ID, name or summary is invalid"));
    }
    if !["infrastructure", "network", "developer", "media", "productivity", "other"]
        .contains(&manifest.category.as_str()) {
        return Err(Error::Invalid("Unknown catalog category"));
    }
    if manifest.homepage.as_ref().is_some_and(|u|
        !(u.starts_with("https://") || u.starts_with("http://")) || u.len() > 500) {
        return Err(Error::Invalid("Invalid application homepage"));
    }
    if manifest.runtime.kind != "docker_compose" || !safe_id(&manifest.runtime.compose_project) {
        return Err(Error::Invalid("Unsupported runtime or Compose project"));
    }
    safe_relative(&manifest.runtime.compose_file, true)?;
    if manifest.compatibility.platforms.is_empty()
        || manifest.compatibility.platforms.iter().any(|p| !["linux", "fnos"].contains(&p.as_str()))
        || manifest.compatibility.architectures.is_empty()
        || manifest.compatibility.architectures.iter().any(|a| !["amd64", "arm64"].contains(&a.as_str()))
        || manifest.compatibility.platforms.iter().collect::<BTreeSet<_>>().len() != manifest.compatibility.platforms.len()
        || manifest.compatibility.architectures.iter().collect::<BTreeSet<_>>().len() != manifest.compatibility.architectures.len()
        || manifest.compatibility.requires_docker != Some(true)
        || manifest.compatibility.requires_compose_v2 != Some(true) {
        return Err(Error::Invalid("Invalid platform or architecture compatibility"));
    }
    if manifest.package.files.is_empty() || manifest.package.files.len() > MAX_PACKAGE_FILES {
        return Err(Error::Invalid("Package file count outside allowed bounds"));
    }
    let mut sources = BTreeSet::new();
    let mut destinations = BTreeSet::new();
    for file in &manifest.package.files {
        safe_relative(&file.source, true)?;
        safe_relative(&file.destination, true)?;
        if !(file.source.starts_with(&format!("{}/", manifest.id)) || file.source.starts_with("lib/"))
            || !(file.destination.starts_with(&format!("{}/", manifest.id))
                || file.destination.starts_with("deploy/lib/")) {
            return Err(Error::Invalid("Package path outside app/shared-library ownership"));
        }
        if file.source.starts_with("lib/") {
            if file.destination != format!("deploy/{}", file.source) {
                return Err(Error::Invalid("Shared library destination must preserve source path"));
            }
        } else if file.source != file.destination {
            return Err(Error::Invalid("Application package destination must preserve source path"));
        }
        if !sources.insert(file.source.to_ascii_lowercase())
            || !destinations.insert(file.destination.to_ascii_lowercase()) {
            return Err(Error::Invalid("Duplicate package source/destination"));
        }
        if file.mode.as_deref().is_some_and(|m| m != "0644" && m != "0755") {
            return Err(Error::Invalid("Unsupported package file mode"));
        }
    }
    let mut config = BTreeMap::<String, &ConfigSpec>::new();
    if manifest.config.len() > 120 { return Err(Error::Invalid("Too many configuration fields")); }
    for field in &manifest.config {
        if !valid_env_key(&field.key) || field.label.trim().is_empty()
            || !config.insert(field.key.clone(), field).is_none() {
            return Err(Error::Invalid("Invalid or duplicate configuration field"));
        }
        if !["string", "number", "boolean", "port", "ipv4", "secret"].contains(&field.kind.as_str())
            || !["env", "secrets"].contains(&field.target.as_str())
            || (field.kind == "secret") != (field.target == "secrets") {
            return Err(Error::Invalid("Configuration type and target mismatch"));
        }
        if let Some(default) = &field.default {
            if !config_value_matches(field, default) {
                return Err(Error::Invalid("Configuration default has the wrong type"));
            }
            if field.kind == "secret" && default.as_str() != Some("") {
                return Err(Error::Invalid("Secret defaults must be blank"));
            }
        }
        if field.min_length.is_some_and(|n| n > 4096)
            || field.max_length.is_some_and(|n| n == 0 || n > 4096)
            || field.min_length.zip(field.max_length).is_some_and(|(min, max)| min > max)
            || field.pattern.as_ref().is_some_and(|p| p.len() > 500)
            || field.generated_by_script == Some(true) && field.kind != "secret" {
            return Err(Error::Invalid("Invalid configuration constraints"));
        }
        if field.kind == "port" {
            let min = field.minimum.unwrap_or(1);
            let max = field.maximum.unwrap_or(65535);
            if min < 1 || max > 65535 || min > max
                || field.default.as_ref().and_then(json_integer)
                    .is_some_and(|v| v < min || v > max) {
                return Err(Error::Invalid("Invalid port range"));
            }
        } else if field.minimum.zip(field.maximum).is_some_and(|(min, max)| min > max) {
            return Err(Error::Invalid("Invalid numeric range"));
        }
    }
    let mut port_ids = BTreeSet::new();
    if manifest.ports.len() > 40 { return Err(Error::Invalid("Too many port definitions")); }
    for port in &manifest.ports {
        if !safe_id(&port.id) || !port_ids.insert(port.id.to_ascii_lowercase())
            || !["tcp", "udp"].contains(&port.protocol.as_str())
            || !["web", "other"].contains(&port.purpose.as_str())
            || port.container_port == 0 || port.container_port > 65535
            || port.service.as_deref().is_none_or(str::is_empty)
            || port.path.as_ref().is_some_and(|p| p.len() > 255 || !p.starts_with('/')
                || p.contains('?') || p.contains('#')) {
            return Err(Error::Invalid("Invalid port definition"));
        }
        if config.get(&port.host_port_key).is_none_or(|f| f.kind != "port") {
            return Err(Error::Invalid("Port host_port_key must reference a declared port field"));
        }
        if port.bind_address_key.as_ref().is_some_and(|k|
            config.get(k).is_none_or(|f| f.kind != "ipv4")) {
            return Err(Error::Invalid("Port bind_address_key must reference an IPv4 field"));
        }
    }
    let package_destinations = manifest.package.files.iter()
        .map(|f| f.destination.as_str()).collect::<BTreeSet<_>>();
    let compose_dest = format!("{}/{}", manifest.id, manifest.runtime.compose_file);
    if !package_destinations.contains(compose_dest.as_str()) {
        return Err(Error::Invalid("Compose file is not included in the package"));
    }
    let check_action = |name: &str, action: &ActionSpec| -> Result<(), Error> {
        if action.remove_orphans {
            return Err(Error::Invalid("Removing orphan containers is not authorized"));
        }
        if name == "uninstall" {
            if action.strategy != "compose_down" || action.path.is_some() || !action.argv.is_empty() {
                return Err(Error::Invalid("Uninstall must use the bounded compose_down strategy"));
            }
        } else {
            if action.strategy != "script" {
                return Err(Error::Invalid("Install/update must use a packaged script"));
            }
            let path = action.path.as_deref().ok_or(Error::Invalid("Action script path is required"))?;
            safe_relative(path, true)?;
            let expected = format!("{}/{}", manifest.id, path);
            if !package_destinations.contains(expected.as_str()) {
                return Err(Error::Invalid("Action script is missing from the package"));
            }
            let allowed = if name == "install" { "install" } else { "update" };
            if action.argv.len() > 1 || action.argv.iter().any(|arg| arg != allowed) {
                return Err(Error::Invalid("Action argv is not in the allowlist"));
            }
        }
        Ok(())
    };
    check_action("install", &manifest.actions.install)?;
    if let Some(action) = &manifest.actions.update { check_action("update", action)?; }
    if let Some(action) = &manifest.actions.uninstall { check_action("uninstall", action)?; }
    if manifest.verification.mode != "docker_compose"
        || !["running", "healthy"].contains(&manifest.verification.state.as_str())
        || manifest.verification.timeout_seconds == 0 || manifest.verification.timeout_seconds > 1800
        || manifest.verification.required_services.is_empty()
        || manifest.verification.required_services.iter().collect::<BTreeSet<_>>().len()
            != manifest.verification.required_services.len() {
        return Err(Error::Invalid("Invalid post-install verification contract"));
    }
    for data_path in &manifest.data.paths {
        safe_relative(data_path, false)?;
        if !data_path.starts_with(&format!("{}/data/", manifest.id)) {
            return Err(Error::Invalid("Data paths must be owned app data directories"));
        }
    }
    for volume in &manifest.data.named_volumes {
        if !safe_id(volume) { return Err(Error::Invalid("Invalid named volume")); }
    }
    if !manifest.data.default_retain {
        return Err(Error::Invalid("Application data must be retained by default"));
    }
    Ok(())
}

fn config_json(field: &ConfigSpec) -> Value {
    let mut out = json!({
        "key":field.key, "label":field.label, "type":field.kind, "target":field.target,
        "required":field.required, "editable":field.editable.unwrap_or(true),
        "generated_by_script":field.generated_by_script.unwrap_or(false)
    });
    if let Some(description) = &field.description { out["description"] = json!(description); }
    // Never send even an empty default for secrets. This makes it impossible for
    // a future non-empty secret default to be reflected in the UI by accident.
    if field.kind != "secret" {
        if let Some(default) = &field.default { out["default"] = default.clone(); }
    }
    if let Some(n) = field.min_length { out["min_length"] = json!(n); }
    if let Some(n) = field.max_length { out["max_length"] = json!(n); }
    if let Some(n) = field.minimum { out["minimum"] = json!(n); }
    if let Some(n) = field.maximum { out["maximum"] = json!(n); }
    if let Some(p) = &field.pattern { out["pattern"] = json!(p); }
    out
}
fn manifest_detail(m: &ManifestV1) -> Value {
    json!({
        "runtime_kind":m.runtime.kind,
        "compose_file":m.runtime.compose_file,
        "platforms":m.compatibility.platforms,
        "architectures":m.compatibility.architectures,
        "actions":{
            "install":m.actions.install.strategy,
            "update":m.actions.update.as_ref().map(|a| a.strategy.as_str()),
            "uninstall":m.actions.uninstall.as_ref().map(|a| a.strategy.as_str())
        },
        "config":m.config.iter().map(config_json).collect::<Vec<_>>(),
        "ports":m.ports.iter().map(|p| json!({
            "id":p.id,"label":p.label,"protocol":p.protocol,"purpose":p.purpose,
            "host_port_key":p.host_port_key,"bind_address_key":p.bind_address_key,
            "container_port":p.container_port,"service":p.service,"path":p.path
        })).collect::<Vec<_>>(),
        "install_available":false,
        "management_rights":false,
        "validation":"strict_manifest_v1; package must be reverified before Agent execution"
    })
}


fn verified_file_bytes(root: &Path, relative: &str, max_bytes: u64) -> Result<(PathBuf, Vec<u8>), Error> {
    let parts = safe_relative(relative, true)?;
    let root_meta = std::fs::symlink_metadata(root).map_err(|_| Error::RevisionUnavailable)?;
    if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
        return Err(Error::RevisionUnavailable);
    }
    let mut path = root.to_path_buf();
    for (index, part) in parts.iter().enumerate() {
        path.push(part);
        let meta = std::fs::symlink_metadata(&path).map_err(|_| Error::Invalid("Package source file is missing"))?;
        if meta.file_type().is_symlink() {
            return Err(Error::Invalid("Package source symlinks are forbidden"));
        }
        if index + 1 < parts.len() {
            if !meta.is_dir() { return Err(Error::Invalid("Package path contains a non-directory")); }
        } else if !meta.is_file() || meta.len() > max_bytes {
            return Err(Error::Invalid("Package source is not a bounded regular file"));
        }
    }
    let bytes = std::fs::read(&path).map_err(|_| Error::Invalid("Package source cannot be read"))?;
    if bytes.len() as u64 > max_bytes { return Err(Error::Invalid("Package source is too large")); }
    Ok((path, bytes))
}

fn has_compose_variable(value: &str, key: &str) -> bool {
    let plain = format!("\${{{key}}}");
    let with_operator = format!("\${{{key}:");
    value.contains(&plain) || value.find(&with_operator)
        .and_then(|start| value[start..].find('}')).is_some()
}

fn compose_port_matches(entry: &Value, port: &PortSpec) -> bool {
    if let Some(text) = entry.as_str() {
        let target = port.container_port.to_string();
        let target_suffix = format!(":{target}");
        let protocol_suffix = format!(":{target}/{}", port.protocol);
        if !text.ends_with(&target_suffix) && !text.ends_with(&protocol_suffix) {
            return false;
        }
        if !has_compose_variable(text, &port.host_port_key) { return false; }
        return port.bind_address_key.as_deref()
            .is_none_or(|key| has_compose_variable(text, key));
    }
    let Some(map) = entry.as_object() else { return false };
    let target_matches = map.get("target").is_some_and(|v|
        v.as_u64() == Some(port.container_port as u64)
            || v.as_str().and_then(|s| s.parse::<u32>().ok()) == Some(port.container_port));
    let protocol_matches = map.get("protocol").and_then(Value::as_str).unwrap_or("tcp") == port.protocol;
    let published = map.get("published").and_then(Value::as_str).unwrap_or("");
    let bind = map.get("host_ip").and_then(Value::as_str).unwrap_or("");
    target_matches && protocol_matches && has_compose_variable(published, &port.host_port_key)
        && port.bind_address_key.as_deref().is_none_or(|key| has_compose_variable(bind, key))
}

fn normalize_mount_source(app_id: &str, source: &str) -> Option<String> {
    if source.starts_with('/') || source.contains('\\') || source.contains('\0') { return None; }
    let mut parts = vec![app_id.to_string(), "deploy".to_string()];
    for item in source.split('/') {
        match item {
            "" | "." => {},
            ".." => {
                if parts.len() <= 1 { return None; }
                parts.pop();
            }
            value if value.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'.') => {
                parts.push(value.to_string());
            }
            _ => return None,
        }
    }
    if parts.first().is_some_and(|part| part == app_id) {
        Some(parts.join("/"))
    } else { None }
}

fn validate_compose(manifest: &ManifestV1, sources: &BTreeMap<String, Vec<u8>>) -> Result<(), Error> {
    let relative = format!("{}/{}", manifest.id, manifest.runtime.compose_file);
    let bytes = sources.get(&relative).ok_or(Error::Invalid("Compose file is not in package sources"))?;
    let text = std::str::from_utf8(bytes).map_err(|_| Error::Invalid("Compose file is not UTF-8"))?;
    let compose: Value = parse_yaml(text, MAX_FILE_BYTES)?;
    if compose.get("name").and_then(Value::as_str) != Some(manifest.runtime.compose_project.as_str()) {
        return Err(Error::Invalid("Compose project does not match manifest"));
    }
    let services = compose.get("services").and_then(Value::as_object)
        .filter(|services| !services.is_empty())
        .ok_or(Error::Invalid("Compose must define services"))?;
    for service in &manifest.verification.required_services {
        if !services.contains_key(service) {
            return Err(Error::Invalid("Verification references a missing Compose service"));
        }
        if manifest.verification.state == "healthy"
            && services[service].get("healthcheck").is_none() {
            return Err(Error::Invalid("Healthy verification requires a Compose healthcheck"));
        }
    }
    for port in &manifest.ports {
        let service = port.service.as_deref().ok_or(Error::Invalid("Port service is required"))?;
        let entries = services.get(service).and_then(|s| s.get("ports")).and_then(Value::as_array)
            .ok_or(Error::Invalid("Port service has no Compose port mapping"))?;
        if !entries.iter().any(|entry| compose_port_matches(entry, port)) {
            return Err(Error::Invalid("Manifest port does not match Compose binding"));
        }
    }
    let mut mounted_data = BTreeSet::new();
    for service in services.values() {
        if let Some(volumes) = service.get("volumes").and_then(Value::as_array) {
            for volume in volumes {
                let source = if let Some(spec) = volume.as_str() {
                    spec.split(':').next().unwrap_or("")
                } else {
                    volume.get("source").and_then(Value::as_str).unwrap_or("")
                };
                if source.starts_with("../") {
                    if let Some(path) = normalize_mount_source(&manifest.id, source) {
                        mounted_data.insert(path);
                    }
                }
            }
        }
    }
    if manifest.data.paths.iter().any(|path| !mounted_data.contains(path)) {
        return Err(Error::Invalid("Manifest data retention path is not mounted by Compose"));
    }
    Ok(())
}

fn validate_package_files(root: &Path, manifest: &ManifestV1) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    let mut total = 0u64;
    let mut sources = BTreeMap::<String, Vec<u8>>::new();
    for file in &manifest.package.files {
        let (_, bytes) = verified_file_bytes(root, &file.source, MAX_FILE_BYTES)?;
        total = total.checked_add(bytes.len() as u64).ok_or(Error::Invalid("Package size overflow"))?;
        if total > MAX_PACKAGE_BYTES { return Err(Error::Invalid("Package exceeds aggregate size limit")); }
        sources.insert(file.source.clone(), bytes);
    }
    for field in &manifest.config {
        if field.generated_by_script == Some(true) {
            let action = &manifest.actions.install;
            let script = action.path.as_ref()
                .and_then(|p| sources.get(&format!("{}/{}", manifest.id, p)))
                .ok_or(Error::Invalid("Generated secret install script is missing from package"))?;
            let script = std::str::from_utf8(script).map_err(|_| Error::Invalid("Install script is not UTF-8"))?;
            if !script.contains(&field.key)
                || !(script.contains("ensure_hex_secret") || script.contains("generate")) {
                return Err(Error::Invalid("Generated secret has no matching install implementation"));
            }
        }
    }
    validate_compose(manifest, &sources)?;
    Ok(sources)
}

fn safe_manifest(root: &Path, id: &str) -> Result<Option<PathBuf>, Error> {
    if !safe_id(id) { return Ok(None); }
    let dir = root.join(id);
    let dir_meta = std::fs::symlink_metadata(&dir).map_err(|_| Error::Remote)?;
    if dir_meta.file_type().is_symlink() {
        return Err(Error::Invalid("Application directory symlinks are forbidden"));
    }
    if !dir_meta.is_dir() { return Ok(None); }
    let manifest = dir.join("app.yaml");
    match std::fs::symlink_metadata(&manifest) {
        Ok(m) if m.is_file() && !m.file_type().is_symlink() && m.len() <= MAX_MANIFEST =>
            Ok(Some(manifest)),
        Ok(_) => Err(Error::Invalid("Invalid app.yaml file type or length")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(Error::Remote),
    }
}

fn scan_catalog(root: &Path, revision: &str) -> Result<Vec<CatalogApp>, Error> {
    let mut apps = Vec::new();
    let mut seen = BTreeSet::new();
    for item in std::fs::read_dir(root).map_err(|_| Error::Remote)? {
        let item = item.map_err(|_| Error::Remote)?;
        let name = item.file_name().to_string_lossy().to_string();
        let Some(path) = safe_manifest(root, &name)? else { continue };
        let bytes = std::fs::read(&path).map_err(|_| Error::Remote)?;
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err(Error::Invalid("Manifest too large"));
        }
        let content = std::str::from_utf8(&bytes).map_err(|_| Error::Invalid("Non-UTF8 manifest"))?;
        let manifest = parse_manifest(content)?;
        validate_manifest_shape(&manifest, &name)?;
        let app = CatalogApp {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            summary: manifest.summary.clone(),
            category: manifest.category.clone(),
            compose_project: manifest.runtime.compose_project.clone(),
            manifest_path: format!("{name}/app.yaml"),
            manifest_sha256: format!("{:x}", Sha256::digest(&bytes)),
            git_revision: revision.to_owned(),
            detail: manifest_detail(&manifest),
        };
        if !seen.insert(name) || apps.len() >= MAX_APPS {
            return Err(Error::Invalid("Too many or repeated application IDs"));
        }
        apps.push(app);
    }
    apps.sort_by(|a,b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(apps)
}

#[cfg(test)]
mod tests {
    use super::*;
    const PORTAINER: &str = "schema_version: 1\nid: portainer\nname: Portainer\nsummary: Docker management\ncategory: infrastructure\nruntime:\n  kind: docker_compose\n  compose_project: portainer\n  compose_file: deploy/docker-compose.yaml\ncompatibility:\n  platforms: [linux, fnos]\n  architectures: [amd64, arm64]\n";

    #[test]
    fn restricted_catalog_index_rejects_ambiguous_yaml_and_missing_display_fields() {
        assert_eq!(fields(PORTAINER).unwrap()["runtime.compose_project"], "portainer");
        assert!(fields(&(PORTAINER.to_string()+"id: other\n")).is_err());
        assert!(fields("id: portainer\nname: !!python/object/apply:os.system\n").is_err());
        assert!(fields("id: portainer\nname: &shared secret\n").is_err());
        assert!(!safe_id("../bad"));
        assert!(!safe_id("PORTAINER"));
    }

    #[tokio::test]
    async fn refresh_failure_retains_last_index_and_never_reads_git_on_get() {
        let db = Db::memory_db().await;
        let pool = db.inner();
        let old = "0123456789abcdef0123456789abcdef01234567";
        sqlx::query("INSERT INTO deploy_catalog_state(singleton,git_revision,fetched_at,app_count) VALUES(1,?,?,1)")
            .bind(old).bind("2026-10-28T10:00:00Z")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO deploy_catalog_apps(app_id,git_revision,name,summary,category,compose_project,manifest_path,manifest_sha256,detail_json) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind("portainer").bind(old).bind("Portainer").bind("Management")
            .bind("infrastructure").bind("portainer").bind("portainer/app.yaml")
            .bind("a".repeat(64)).bind(json!({"install_available":false}).to_string())
            .execute(&pool).await.unwrap();

        let root = std::env::temp_dir().join(format!("catalog-fail-{}", Uuid::new_v4()));
        std::fs::write(&root, "not-a-directory").unwrap();
        let catalog = Catalog::new(db, root.clone());
        let before = catalog.list().await.unwrap();
        assert_eq!(before["items"].as_array().unwrap().len(), 1);
        assert_eq!(catalog.detail("portainer").await.unwrap()["git_revision"], old);
        // A valid Git token, permission or network cannot make this refresh
        // succeed: its local staging directory is deliberately not a folder.
        assert!(catalog.refresh().await.is_err());
        let after = catalog.list().await.unwrap();
        assert_eq!(before, after, "failed refresh must preserve the last revision");
        std::fs::remove_file(root).unwrap();
    }

    #[test]
    fn display_lists_never_include_secret_values() {
        let yaml = r#"schema_version: 1
config:
  - key: ADMIN_PASS
    label: Admin password
    type: secret
    target: secrets
    required: true
    default: ""
ports:
  - id: web
    label: Web interface
    host_port_key: PORT
    container_port: 9000
"#;
        let config = display_items(yaml, "config", &["key","label","type","target","required"]).unwrap();
        assert_eq!(config.len(), 1);
        assert_eq!(config[0]["type"], "secret");
        assert!(config[0].get("default").is_none());
        let ports = display_items(yaml, "ports", &["id","label","host_port_key","container_port"]).unwrap();
        assert_eq!(ports[0]["container_port"], "9000");
    }

    #[test]
    fn scan_reads_only_normal_published_manifests_not_contract_fixtures() {
        let dir = std::env::temp_dir().join(format!("t0304-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("portainer")).unwrap();
        std::fs::write(dir.join("portainer/app.yaml"), PORTAINER).unwrap();
        let apps = scan_catalog(&dir, &"a".repeat(40)).unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].detail["install_available"], false);
        assert_eq!(apps[0].manifest_path, "portainer/app.yaml");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
