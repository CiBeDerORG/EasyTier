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

pub struct Catalog {
    db: Db,
    base: PathBuf,
    refresh_lock: Mutex<()>,
}

impl Catalog {
    pub fn new(db: Db, base: PathBuf) -> Self {
        Self { db, base, refresh_lock: Mutex::new(()) }
    }

    pub fn from_environment(db: Db) -> Self {
        let base = std::env::var_os("ET_DEPLOY_CATALOG_DIR")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".deploy-catalog"));
        Self::new(db, base)
    }

    pub async fn list(&self) -> Result<Value, Error> {
        let state: Option<(String, String, i64)> = sqlx::query_as(
            "SELECT git_revision,fetched_at,app_count FROM deploy_catalog_state WHERE singleton=1"
        ).fetch_optional(&self.db.inner()).await.map_err(|_| Error::Storage)?;
        let rows = sqlx::query(
            "SELECT app_id,git_revision,name,summary,category,compose_project,manifest_path,manifest_sha256,detail_json FROM deploy_catalog_apps ORDER BY name,app_id"
        ).fetch_all(&self.db.inner()).await.map_err(|_| Error::Storage)?;
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
        // Retain immutable local checkouts. Existing revisions are never
        // overwritten; a failed clone/scan does not affect the previous index.
        let destination = self.base.join(format!("revision-{}", Uuid::new_v4()));
        tokio::fs::rename(stage, &destination).await.map_err(|_| Error::Storage)?;
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

fn scalar(line: &str, key: &str) -> Option<String> {
    let trimmed = line.trim();
    let raw = trimmed.strip_prefix(key)?.strip_prefix(':')?.trim();
    if raw.is_empty() || raw.starts_with('#') { return None; }
    let raw = raw.split(" #").next()?.trim();
    Some(raw.trim_matches('"').trim_matches('\'').to_owned())
}

fn fields(text: &str) -> Result<BTreeMap<String, String>, Error> {
    if text.contains('\t') || text.contains('\r') || text.contains("!!")
        || text.contains("<<:") || text.contains("&") || text.contains("*") {
        return Err(Error::Invalid("Unsupported YAML catalog syntax"));
    }
    let mut map = BTreeMap::new();
    let mut section = String::new();
    for line in text.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') { continue; }
        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            section = line.split(':').next().unwrap_or("").trim().to_string();
            if let Some(value) = scalar(line, &section) {
                if map.insert(section.clone(), value).is_some() {
                    return Err(Error::Invalid("Duplicate catalog display field"));
                }
            }
        } else if indent == 2 && (section == "runtime" || section == "compatibility") {
            let key = line.trim_start().split(':').next().unwrap_or("");
            if let Some(value) = scalar(line, key) {
                if map.insert(format!("{section}.{key}"), value).is_some() {
                    return Err(Error::Invalid("Duplicate catalog runtime field"));
                }
            }
        }
    }
    Ok(map)
}

// Parse only the flat, non-executable fields of two known manifest lists.
// This deliberately is NOT an app.yaml execution validator. Unhandled YAML
// constructs cannot grant installation rights.
fn display_items(text: &str, section_name: &str, allowed: &[&str]) -> Result<Vec<Value>, Error> {
    let mut active = false;
    let mut current = BTreeMap::<String, String>::new();
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') { continue; }
        let indent = line.len() - line.trim_start().len();
        if indent == 0 {
            if active && !current.is_empty() {
                rows.push(json!(current));
                current.clear();
            }
            active = line.trim() == format!("{section_name}:");
            continue;
        }
        if !active { continue; }
        if indent == 2 && line.trim_start().starts_with("- ") {
            if !current.is_empty() {
                rows.push(json!(current));
                current.clear();
            }
            if rows.len() >= 120 { return Err(Error::Invalid("Too many manifest display fields")); }
            let field = line.trim_start().trim_start_matches("- ");
            let key = field.split(':').next().unwrap_or("").trim();
            if allowed.contains(&key) {
                if let Some(value) = scalar(field, key) {
                    current.insert(key.to_owned(), value);
                }
            }
        } else if indent == 4 && !current.is_empty() {
            let field = line.trim();
            let key = field.split(':').next().unwrap_or("").trim();
            if allowed.contains(&key) {
                if let Some(value) = scalar(field, key) {
                    if current.insert(key.to_owned(), value).is_some() {
                        return Err(Error::Invalid("Repeated manifest display field"));
                    }
                }
            }
        }
    }
    if active && !current.is_empty() { rows.push(json!(current)); }
    Ok(rows)
}

fn safe_manifest(root: &Path, id: &str) -> Result<Option<PathBuf>, Error> {
    if !safe_id(id) { return Ok(None); }
    let dir = root.join(id);
    let dir_meta = std::fs::symlink_metadata(&dir).map_err(|_| Error::Remote)?;
    if !dir_meta.is_dir() || dir_meta.file_type().is_symlink() { return Ok(None); }
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
        if bytes.len() as u64 > MAX_MANIFEST { return Err(Error::Invalid("Manifest too large")); }
        let content = std::str::from_utf8(&bytes).map_err(|_| Error::Invalid("Non-UTF8 manifest"))?;
        let values = fields(content)?;
        let get = |k: &str| -> Result<String, Error> {
            values.get(k).filter(|s| !s.is_empty()).cloned()
                .ok_or(Error::Invalid("Missing catalog display field"))
        };
        if get("schema_version")? != "1" || get("id")? != name
            || get("runtime.kind")? != "docker_compose"
        {
            return Err(Error::Invalid("Unsupported manifest version, app ID or runtime"));
        }
        let category = get("category")?;
        if !["infrastructure", "network", "developer", "media", "productivity", "other"]
            .contains(&category.as_str()) {
            return Err(Error::Invalid("Unknown catalog category"));
        }
        let project = get("runtime.compose_project")?;
        if !safe_id(&project) { return Err(Error::Invalid("Invalid Compose project")); }
        let app = CatalogApp {
            id: name.clone(),
            name: get("name")?,
            summary: get("summary")?,
            category,
            compose_project: project,
            manifest_path: format!("{name}/app.yaml"),
            manifest_sha256: format!("{:x}", Sha256::digest(&bytes)),
            git_revision: revision.to_owned(),
            detail: json!({
                "runtime_kind":"docker_compose",
                "compose_file":get("runtime.compose_file")?,
                "platforms":values.get("compatibility.platforms").cloned(),
                "architectures":values.get("compatibility.architectures").cloned(),
                "config":display_items(content, "config",
                    &["key","label","type","target","required"])?,
                "ports":display_items(content, "ports",
                    &["id","label","protocol","purpose","host_port_key",
                      "container_port","service"])?,
                "install_available":false,
                "management_rights":false,
                "validation":"display_only_requires_T03_07"
            }),
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
