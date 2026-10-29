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


    /// Build or reuse an immutable tar package from the exact Git commit stored
    /// in the catalog. This never executes an install/update script and never
    /// accepts configuration values or secrets from the caller.
    pub async fn prepare_package(&self, id: &str) -> Result<DeployPackageSummary, Error> {
        if !safe_id(id) { return Err(Error::Invalid("Invalid application identifier")); }
        let _guard = self.package_lock.lock().await;
        let index = self.list().await?;
        if index["initialized"] != true { return Err(Error::Uninitialized); }
        let raw_app = index["items"].as_array()
            .and_then(|items| items.iter().find(|item| item["id"].as_str() == Some(id)))
            .cloned().ok_or(Error::Invalid("Application not in catalog"))?;
        let app: CatalogApp = serde_json::from_value(raw_app).map_err(|_| Error::Storage)?;
        if app.git_revision.len() != 40 || !app.git_revision.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(Error::PackageConflict);
        }
        let root = self.base.join(format!("revision-{}", app.git_revision));
        let root_meta = tokio::fs::symlink_metadata(&root).await
            .map_err(|_| Error::RevisionUnavailable)?;
        if root_meta.file_type().is_symlink() || !root_meta.is_dir() {
            return Err(Error::RevisionUnavailable);
        }
        let head = timeout(Duration::from_secs(10),
            Command::new("git").arg("-C").arg(&root).args(["rev-parse", "HEAD"])
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_OPTIONAL_LOCKS", "0")
                .kill_on_drop(true).output()
        ).await.map_err(|_| Error::RevisionUnavailable)?
            .map_err(|_| Error::RevisionUnavailable)?;
        if !head.status.success()
            || String::from_utf8_lossy(&head.stdout).trim() != app.git_revision {
            return Err(Error::RevisionUnavailable);
        }
        // Do not package mutable working-tree content even when HEAD still
        // matches. Any tracked or untracked change invalidates this checkout.
        let status = timeout(Duration::from_secs(10),
            Command::new("git").arg("-C").arg(&root)
                .args(["status", "--porcelain", "--untracked-files=all"])
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_OPTIONAL_LOCKS", "0")
                .kill_on_drop(true).output()
        ).await.map_err(|_| Error::RevisionUnavailable)?
            .map_err(|_| Error::RevisionUnavailable)?;
        if !status.status.success() || !status.stdout.is_empty() {
            return Err(Error::RevisionUnavailable);
        }
        let (_, manifest_bytes) = verified_file_bytes(&root, &app.manifest_path, MAX_MANIFEST)?;
        let manifest_sha256 = format!("{:x}", Sha256::digest(&manifest_bytes));
        if manifest_sha256 != app.manifest_sha256 { return Err(Error::PackageConflict); }

        tokio::fs::create_dir_all(&self.base).await.map_err(|_| Error::Storage)?;
        let packages_dir = self.base.join("packages");
        match tokio::fs::symlink_metadata(&packages_dir).await {
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => return Err(Error::Storage),
            Ok(_) => {},
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tokio::fs::create_dir(&packages_dir).await.map_err(|_| Error::Storage)?;
            }
            Err(_) => return Err(Error::Storage),
        }
        let packages_dir = tokio::fs::canonicalize(&packages_dir).await.map_err(|_| Error::Storage)?;
        let archive_path = packages_dir.join(format!("{}-{}.tar", app.id, app.git_revision));
        let pool = self.db.inner();
        let existing = sqlx::query(
            "SELECT manifest_sha256,package_sha256,package_size,file_count,built_at FROM deploy_catalog_packages WHERE app_id=? AND git_revision=?"
        ).bind(&app.id).bind(&app.git_revision)
            .fetch_optional(&pool).await.map_err(|_| Error::Storage)?;
        if let Some(row) = existing {
            let stored_manifest: String = row.try_get("manifest_sha256").map_err(|_| Error::Storage)?;
            let stored_sha: String = row.try_get("package_sha256").map_err(|_| Error::Storage)?;
            let stored_size: i64 = row.try_get("package_size").map_err(|_| Error::Storage)?;
            let stored_count: i64 = row.try_get("file_count").map_err(|_| Error::Storage)?;
            let built_at: String = row.try_get("built_at").map_err(|_| Error::Storage)?;
            let meta = tokio::fs::symlink_metadata(&archive_path).await.map_err(|_| Error::PackageConflict)?;
            if meta.file_type().is_symlink() || !meta.is_file() || stored_manifest != manifest_sha256
                || stored_size <= 0 || stored_count <= 0 {
                return Err(Error::PackageConflict);
            }
            let bytes = tokio::fs::read(&archive_path).await.map_err(|_| Error::PackageConflict)?;
            if bytes.len() as i64 != stored_size
                || format!("{:x}", Sha256::digest(&bytes)) != stored_sha {
                return Err(Error::PackageConflict);
            }
            return Ok(DeployPackageSummary {
                app_id: app.id, git_revision: app.git_revision, manifest_sha256,
                package_sha256: stored_sha, package_size: stored_size as u64,
                file_count: stored_count as usize, built_at, reused: true,
                ready_for_agent: false, install_enabled: false,
            });
        }
        match tokio::fs::symlink_metadata(&archive_path).await {
            Ok(_) => {
                // Never overwrite an orphaned or tampered immutable artifact.
                return Err(Error::PackageConflict);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Err(_) => return Err(Error::Storage),
        }

        let manifest_text = std::str::from_utf8(&manifest_bytes)
            .map_err(|_| Error::Invalid("Manifest is not UTF-8"))?;
        let manifest = parse_manifest(manifest_text)?;
        validate_manifest_shape(&manifest, &app.id)?;
        let source_bytes = validate_package_files(&root, &manifest)?;
        let stage = packages_dir.join(format!(".stage-{}", Uuid::new_v4()));
        let archive_tmp = packages_dir.join(format!(".{}-{}.tmp-{}.tar",
            app.id, app.git_revision, Uuid::new_v4()));
        tokio::fs::create_dir(&stage).await.map_err(|_| Error::Storage)?;

        let artifact_result: Result<(String, u64, usize), Error> = async {
            let mut file_digests = Vec::with_capacity(manifest.package.files.len());
            for file in &manifest.package.files {
                let bytes = source_bytes.get(&file.source)
                    .ok_or(Error::Invalid("Validated package source disappeared"))?;
                let destination = stage.join(&file.destination);
                let parent = destination.parent().ok_or(Error::Invalid("Invalid package destination"))?;
                tokio::fs::create_dir_all(parent).await.map_err(|_| Error::Storage)?;
                tokio::fs::write(&destination, bytes).await.map_err(|_| Error::Storage)?;
                set_file_mode(&destination, file.mode.as_deref().unwrap_or("0644"))?;
                file_digests.push(PackageFileDigest {
                    source: file.source.clone(),
                    destination: file.destination.clone(),
                    sha256: format!("{:x}", Sha256::digest(bytes)),
                    size: bytes.len() as u64,
                    mode: file.mode.clone().unwrap_or_else(|| "0644".to_string()),
                });
            }
            let manifest_dest = stage.join("manifest").join("app.yaml");
            tokio::fs::create_dir_all(manifest_dest.parent().ok_or(Error::Storage)?)
                .await.map_err(|_| Error::Storage)?;
            tokio::fs::write(&manifest_dest, &manifest_bytes).await.map_err(|_| Error::Storage)?;
            set_file_mode(&manifest_dest, "0644")?;
            let envelope = PackageEnvelope {
                format_version: 1,
                app_id: app.id.clone(),
                git_revision: app.git_revision.clone(),
                manifest_sha256: manifest_sha256.clone(),
                manifest_path: "manifest/app.yaml".to_string(),
                files: file_digests,
            };
            let envelope_bytes = serde_json::to_vec_pretty(&envelope).map_err(|_| Error::Storage)?;
            tokio::fs::write(stage.join("bundle.json"), envelope_bytes).await.map_err(|_| Error::Storage)?;

            let mut tar = Command::new("tar");
            tar.arg("-cf").arg(&archive_tmp).arg("-C").arg(&stage)
                .arg("bundle.json").arg("manifest/app.yaml");
            for file in &manifest.package.files { tar.arg(&file.destination); }
            let status = timeout(Duration::from_secs(45), tar.kill_on_drop(true).status())
                .await.map_err(|_| Error::Storage)?.map_err(|_| Error::Storage)?;
            if !status.success() { return Err(Error::Storage); }
            let archive = tokio::fs::read(&archive_tmp).await.map_err(|_| Error::Storage)?;
            if archive.is_empty() || archive.len() as u64 > MAX_PACKAGE_BYTES + 2 * 1024 * 1024 {
                return Err(Error::Invalid("Built package exceeds archive size limit"));
            }
            Ok((format!("{:x}", Sha256::digest(&archive)), archive.len() as u64,
                manifest.package.files.len()))
        }.await;

        if artifact_result.is_err() {
            let _ = tokio::fs::remove_dir_all(&stage).await;
            let _ = tokio::fs::remove_file(&archive_tmp).await;
        }
        let (package_sha256, package_size, file_count) = artifact_result?;
        // Hard-link publication is atomic and fails if another process has
        // already published this immutable key; unlike rename, it cannot
        // overwrite an existing package artifact.
        if let Err(error) = tokio::fs::hard_link(&archive_tmp, &archive_path).await {
            let _ = tokio::fs::remove_file(&archive_tmp).await;
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return if error.kind() == std::io::ErrorKind::AlreadyExists {
                Err(Error::PackageConflict)
            } else {
                Err(Error::Storage)
            };
        }
        let _ = tokio::fs::remove_file(&archive_tmp).await;
        let built_at = chrono::Utc::now().to_rfc3339();
        let inserted = sqlx::query(
            "INSERT INTO deploy_catalog_packages(app_id,git_revision,manifest_sha256,package_sha256,package_size,file_count,built_at) VALUES(?,?,?,?,?,?,?)"
        ).bind(&app.id).bind(&app.git_revision).bind(&manifest_sha256)
            .bind(&package_sha256).bind(package_size as i64).bind(file_count as i64).bind(&built_at)
            .execute(&pool).await;
        if inserted.is_err() {
            let _ = tokio::fs::remove_file(&archive_path).await;
            let _ = tokio::fs::remove_dir_all(&stage).await;
            return Err(Error::Storage);
        }
        let _ = tokio::fs::remove_dir_all(&stage).await;
        Ok(DeployPackageSummary {
            app_id: app.id, git_revision: app.git_revision, manifest_sha256,
            package_sha256, package_size, file_count, built_at, reused: false,
            ready_for_agent: false, install_enabled: false,
        })
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
                let clean = timeout(Duration::from_secs(10),
                    Command::new("git").arg("-C").arg(&destination)
                        .args(["status", "--porcelain", "--untracked-files=all"])
                        .env("GIT_CONFIG_NOSYSTEM", "1")
                        .env("GIT_CONFIG_GLOBAL", "/dev/null")
                        .env("GIT_OPTIONAL_LOCKS", "0")
                        .kill_on_drop(true).output()
                ).await.map_err(|_| Error::PackageConflict)?
                    .map_err(|_| Error::PackageConflict)?;
                if !clean.status.success() || !clean.stdout.is_empty() {
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
        if field.min_length.is_some_and(|n| n == 0 || n > 4096)
            || field.max_length.is_some_and(|n| n == 0 || n > 4096)
            || field.min_length.zip(field.max_length).is_some_and(|(min, max)| min > max)
            || field.pattern.as_ref().is_some_and(|p| p.len() > 500 || regex::Regex::new(p).is_err())
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
        || manifest.verification.timeout_seconds < 5 || manifest.verification.timeout_seconds > 1200
        || manifest.verification.required_services.is_empty()
        || manifest.verification.required_services.iter().any(|s|
            s.is_empty() || !s.as_bytes()[0].is_ascii_alphanumeric()
                || !s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-'))
        || manifest.verification.required_services.iter().collect::<BTreeSet<_>>().len()
            != manifest.verification.required_services.len() {
        return Err(Error::Invalid("Invalid post-install verification contract"));
    }
    let mut data_paths = BTreeSet::new();
    for data_path in &manifest.data.paths {
        safe_relative(data_path, false)?;
        let data_root = format!("{}/data", manifest.id);
        if (data_path != &data_root && !data_path.starts_with(&format!("{data_root}/")))
            || !data_paths.insert(data_path.to_ascii_lowercase()) {
            return Err(Error::Invalid("Invalid or repeated app data path"));
        }
    }
    let mut named_volumes = BTreeSet::new();
    for volume in &manifest.data.named_volumes {
        if !safe_id(volume) || !named_volumes.insert(volume.to_ascii_lowercase()) {
            return Err(Error::Invalid("Invalid or repeated named volume"));
        }
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


fn set_file_mode(path: &Path, mode: &str) -> Result<(), Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let numeric = match mode {
            "0644" => 0o644,
            "0755" => 0o755,
            _ => return Err(Error::Invalid("Unsupported package file mode")),
        };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(numeric))
            .map_err(|_| Error::Storage)?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
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
    let plain = format!("${{{key}}}");
    let with_operator = format!("${{{key}:");
    value.contains(&plain) || value.find(&with_operator)
        .and_then(|start| value[start..].find('}')).is_some()
}

fn compose_variable_pattern(key: &str) -> String {
    format!(r"\$\{{{}(?::[^}}]*)?\}}", regex::escape(key))
}

fn compose_port_matches(entry: &Value, port: &PortSpec) -> bool {
    if let Some(text) = entry.as_str() {
        let mut expected = String::new();
        if let Some(bind) = port.bind_address_key.as_deref() {
            expected.push_str(&compose_variable_pattern(bind));
            expected.push(':');
        }
        expected.push_str(&compose_variable_pattern(&port.host_port_key));
        expected.push(':');
        expected.push_str(&port.container_port.to_string());
        if port.protocol == "tcp" {
            expected.push_str("(?:/tcp)?");
        } else {
            expected.push_str("/udp");
        }
        return regex::Regex::new(&format!("^{expected}$"))
            .is_ok_and(|pattern| pattern.is_match(text));
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
    use std::process::Command as StdCommand;

    const PORTAINER: &str = r#"schema_version: 1
id: portainer
name: Portainer
summary: Docker and Compose management UI
category: infrastructure
runtime:
  kind: docker_compose
  compose_project: portainer
  compose_file: deploy/docker-compose.yaml
  requires_root: true
compatibility:
  platforms: [linux, fnos]
  architectures: [amd64, arm64]
  requires_docker: true
  requires_compose_v2: true
package:
  files:
    - source: portainer/deploy/docker-compose.yaml
      destination: portainer/deploy/docker-compose.yaml
    - source: portainer/deploy/install.sh
      destination: portainer/deploy/install.sh
      mode: "0755"
    - source: portainer/deploy/update.sh
      destination: portainer/deploy/update.sh
      mode: "0755"
config:
  - key: PORTAINER_HTTP_PORT
    label: Web port
    type: port
    target: env
    required: true
    default: 7004
    minimum: 1
    maximum: 65535
  - key: PORTAINER_ADMIN_PASSWORD
    label: Administrator password
    type: secret
    target: secrets
    required: true
    default: ""
    min_length: 12
ports:
  - id: web
    label: Web UI
    protocol: tcp
    purpose: web
    host_port_key: PORTAINER_HTTP_PORT
    container_port: 9000
    service: portainer
    path: /
actions:
  install:
    strategy: script
    path: deploy/install.sh
    argv: [install]
  update:
    strategy: script
    path: deploy/update.sh
    argv: [update]
  uninstall:
    strategy: compose_down
    remove_orphans: false
verification:
  mode: docker_compose
  required_services: [portainer]
  state: running
  timeout_seconds: 180
data:
  paths: [portainer/data]
  named_volumes: []
  default_retain: true
"#;

    fn write_test_checkout(root: &Path) {
        std::fs::create_dir_all(root.join("portainer/deploy")).unwrap();
        std::fs::write(root.join("portainer/app.yaml"), PORTAINER).unwrap();
        std::fs::write(root.join("portainer/deploy/docker-compose.yaml"), r#"name: portainer
services:
  portainer:
    image: portainer/portainer-ce:latest
    ports:
      - "${PORTAINER_HTTP_PORT:?required}:9000"
    volumes:
      - ../data:/data
"#).unwrap();
        std::fs::write(root.join("portainer/deploy/install.sh"), "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::write(root.join("portainer/deploy/update.sh"), "#!/bin/sh\nexit 0\n").unwrap();
    }

    #[test]
    fn strict_yaml_rejects_duplicate_keys_anchors_tags_and_unknown_fields() {
        let manifest = parse_manifest(PORTAINER).unwrap();
        validate_manifest_shape(&manifest, "portainer").unwrap();
        assert!(parse_manifest(&PORTAINER.replace("name: Portainer\n", "name: Portainer\nname: Spoofed\n")).is_err());
        assert!(parse_manifest(&PORTAINER.replace("name: Portainer", "name: &shared Portainer")).is_err());
        assert!(parse_manifest(&PORTAINER.replace("name: Portainer", "name: !!str Portainer")).is_err());
        assert!(parse_manifest(&(PORTAINER.to_string() + "unexpected: true\n")).is_err());
        assert!(!safe_id("../bad"));
        assert!(!safe_id("PORTAINER"));
    }

    #[test]
    fn secret_defaults_are_never_returned_to_catalog_or_dynamic_form() {
        let manifest = parse_manifest(PORTAINER).unwrap();
        let detail = manifest_detail(&manifest);
        let config = detail["config"].as_array().unwrap();
        let secret = config.iter().find(|field| field["type"] == "secret").unwrap();
        assert!(secret.get("default").is_none());
        let port = config.iter().find(|field| field["key"] == "PORTAINER_HTTP_PORT").unwrap();
        assert_eq!(port["default"], 7004);
        assert_eq!(port["minimum"], 1);
        assert_eq!(detail["install_available"], false);
    }

    #[test]
    fn strict_manifest_rejects_path_traversal_and_missing_package_scripts() {
        let mut manifest = parse_manifest(PORTAINER).unwrap();
        manifest.package.files[0].source = "../outside".to_string();
        assert!(validate_manifest_shape(&manifest, "portainer").is_err());

        let mut manifest = parse_manifest(PORTAINER).unwrap();
        manifest.package.files.retain(|f| f.destination != "portainer/deploy/install.sh");
        assert!(validate_manifest_shape(&manifest, "portainer").is_err());
    }

    #[test]
    fn compose_port_contract_requires_protocol_and_declared_variables() {
        let tcp = PortSpec {
            id: "web".into(), label: "Web".into(), protocol: "tcp".into(),
            purpose: "web".into(), host_port_key: "APP_PORT".into(),
            bind_address_key: None, container_port: 8080, service: Some("app".into()),
            path: None,
        };
        assert!(compose_port_matches(&json!("${APP_PORT:?required}:8080"), &tcp));
        let udp = PortSpec { protocol: "udp".into(), ..tcp.clone() };
        assert!(!compose_port_matches(&json!("${APP_PORT:?required}:8080"), &udp));
        assert!(compose_port_matches(&json!("${APP_PORT:?required}:8080/udp"), &udp));
        assert!(!compose_port_matches(&json!("9000:8080"), &tcp));
    }

    #[test]
    fn package_validator_checks_compose_contract_and_rejects_symlinks() {
        let root = std::env::temp_dir().join(format!("catalog-package-{}", Uuid::new_v4()));
        write_test_checkout(&root);
        let manifest = parse_manifest(PORTAINER).unwrap();
        validate_manifest_shape(&manifest, "portainer").unwrap();
        let sources = validate_package_files(&root, &manifest).unwrap();
        assert_eq!(sources.len(), 3);
        assert_eq!(sources["portainer/deploy/install.sh"], b"#!/bin/sh\nexit 0\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let outside = root.join("outside.sh");
            std::fs::write(&outside, "#!/bin/sh\nexit 0\n").unwrap();
            let update = root.join("portainer/deploy/update.sh");
            std::fs::remove_file(&update).unwrap();
            symlink(&outside, &update).unwrap();
            assert!(validate_package_files(&root, &manifest).is_err(),
                "package source symlinks must be rejected");
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn package_builder_is_pinned_immutable_and_reuses_verified_artifact() {
        let base = std::env::temp_dir().join(format!("catalog-build-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        let work = base.join("work");
        write_test_checkout(&work);
        let run = |args: &[&str]| {
            let output = StdCommand::new("git").arg("-C").arg(&work).args(args).output().unwrap();
            assert!(output.status.success(), "git command failed");
            output
        };
        let init = StdCommand::new("git").args(["init", "--quiet"]).arg(&work).status().unwrap();
        assert!(init.success());
        run(&["config", "user.name", "EasyTier CI"]);
        run(&["config", "user.email", "ci@example.invalid"]);
        run(&["add", "."]);
        run(&["commit", "--quiet", "-m", "fixture"]);
        let sha = String::from_utf8(run(&["rev-parse", "HEAD"]).stdout).unwrap().trim().to_string();
        let root = base.join(format!("revision-{sha}"));
        std::fs::rename(&work, &root).unwrap();
        let bytes = std::fs::read(root.join("portainer/app.yaml")).unwrap();
        let manifest = parse_manifest(std::str::from_utf8(&bytes).unwrap()).unwrap();
        let mut apps = scan_catalog(&root, &sha).unwrap();
        let app = apps.remove(0);
        let db = Db::memory_db().await;
        let pool = db.inner();
        sqlx::query("INSERT INTO deploy_catalog_state(singleton,git_revision,fetched_at,app_count) VALUES(1,?,?,1)")
            .bind(&sha).bind("2026-10-29T00:00:00Z").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO deploy_catalog_apps(app_id,git_revision,name,summary,category,compose_project,manifest_path,manifest_sha256,detail_json) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(&app.id).bind(&app.git_revision).bind(&app.name).bind(&app.summary)
            .bind(&app.category).bind(&app.compose_project).bind(&app.manifest_path)
            .bind(&app.manifest_sha256).bind(app.detail.to_string())
            .execute(&pool).await.unwrap();

        let catalog = Catalog::new(db, base.clone());
        let first = catalog.prepare_package("portainer").await.unwrap();
        assert_eq!(first.git_revision, sha);
        assert_eq!(first.manifest_sha256, format!("{:x}", Sha256::digest(&bytes)));
        assert_eq!(first.file_count, manifest.package.files.len());
        assert!(!first.ready_for_agent);
        assert!(!first.install_enabled);
        let archive_path = base.join("packages").join(format!("portainer-{sha}.tar"));
        assert!(archive_path.is_file());
        let listing = StdCommand::new("tar").arg("-tf").arg(&archive_path).output().unwrap();
        assert!(listing.status.success());
        let entries = String::from_utf8(listing.stdout).unwrap();
        assert!(entries.contains("bundle.json"));
        assert!(entries.contains("manifest/app.yaml"));
        assert!(entries.contains("portainer/deploy/docker-compose.yaml"));
        assert!(!entries.contains(".git"));
        assert!(!entries.contains("portainer/data"));

        let second = catalog.prepare_package("portainer").await.unwrap();
        assert!(second.reused);
        assert_eq!(first.package_sha256, second.package_sha256);
        std::fs::remove_dir_all(base).unwrap();
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
        assert!(catalog.refresh().await.is_err());
        let after = catalog.list().await.unwrap();
        assert_eq!(before, after, "failed refresh must preserve the last revision");
        std::fs::remove_file(root).unwrap();
    }
}
