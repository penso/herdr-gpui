//! Upstream client/endpoint/catalog.rs schema and config/io.rs paths.
use crate::{invalid, session_socket};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    env,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedHost {
    pub id: String,
    pub label: String,
    pub target: String,
    pub session: String,
    pub enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    version: u32,
    #[serde(default)]
    selected_profile: Option<String>,
    #[serde(default)]
    ssh: Vec<SavedHost>,
}

/// Load profiles only, like upstream `load_profiles`; selection is client-local.
/// This performs bounded filesystem I/O; call it from a background task.
pub fn load_saved_hosts(development: bool) -> io::Result<Vec<SavedHost>> {
    load_path(&catalog_path(
        development,
        env::var("XDG_STATE_HOME").ok(),
        env::var("HOME").ok(),
    ))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    version: u32,
    selected_profile: Option<String>,
}

/// Load the startup catalog and desired profile (None means Local). Missing,
/// malformed or stale selection files retain the catalog's legacy selection.
/// Live clients should subsequently use `load_saved_hosts`, not reload selection.
pub fn load_saved_host_selection(
    development: bool,
) -> io::Result<(Vec<SavedHost>, Option<String>)> {
    load_with_selection(&catalog_path(
        development,
        env::var("XDG_STATE_HOME").ok(),
        env::var("HOME").ok(),
    ))
}

fn load_with_selection(path: &Path) -> io::Result<(Vec<SavedHost>, Option<String>)> {
    let catalog = load_catalog(path)?;
    let mut selected = catalog.selected_profile;
    // Selection errors must never discard an otherwise valid catalog.
    if let Ok(Some(selection)) = read_selection(&path.with_file_name("endpoint-selection.json"))
        && selection.selected_profile.as_ref().is_none_or(|id| {
            catalog
                .ssh
                .iter()
                .any(|host| host.enabled && &host.id == id)
        })
    {
        selected = selection.selected_profile;
    }
    Ok((catalog.ssh, selected))
}

fn read_selection(path: &Path) -> io::Result<Option<Selection>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if !file.metadata()?.is_file() {
        return Err(invalid("endpoint selection is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(invalid("endpoint selection exceeds storage limit"));
    }
    let selection: Selection =
        serde_json::from_slice(&bytes).map_err(|_| invalid("invalid endpoint selection schema"))?;
    if selection.version != 1 {
        return Err(invalid("unsupported endpoint selection version"));
    }
    Ok(Some(selection))
}

/// Persist an explicit choice without rewriting profiles. Validates against the
/// current catalog, atomically replaces a private file, and syncs its directory.
/// All selection APIs perform filesystem I/O and belong on a background worker.
pub fn store_saved_host_selection(development: bool, selected: Option<&str>) -> io::Result<()> {
    store_selection(
        &catalog_path(
            development,
            env::var("XDG_STATE_HOME").ok(),
            env::var("HOME").ok(),
        ),
        selected,
    )
}

fn store_selection(catalog: &Path, selected: Option<&str>) -> io::Result<()> {
    let hosts = load_path(catalog)?;
    if selected.is_some_and(|id| !hosts.iter().any(|host| host.enabled && host.id == id)) {
        return Err(invalid("selected endpoint is absent or disabled"));
    }
    let content = serde_json::to_vec_pretty(&Selection {
        version: 1,
        selected_profile: selected.map(str::to_owned),
    })?;
    let path = catalog.with_file_name("endpoint-selection.json");
    let parent = path
        .parent()
        .ok_or_else(|| invalid("invalid selection path"))?;
    fs::create_dir_all(parent)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if !metadata.is_file() => {
            return Err(invalid("selection path is not a regular file"));
        }
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);
    let temp = parent.join(format!(
        ".endpoints-{}-{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    let result = (|| {
        file.write_all(&content)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &path)?;
        File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn catalog_path(development: bool, xdg: Option<String>, home: Option<String>) -> PathBuf {
    let app = if development { "herdr-dev" } else { "herdr" };
    xdg.map(PathBuf::from)
        .or_else(|| home.map(|home| PathBuf::from(home).join(".local/state")))
        .map(|base| base.join(app))
        .unwrap_or_else(|| env::temp_dir().join(format!("{app}-state")))
        .join("client/endpoints.json")
}

fn load_path(path: &Path) -> io::Result<Vec<SavedHost>> {
    load_catalog(path).map(|catalog| catalog.ssh)
}

fn load_catalog(path: &Path) -> io::Result<Catalog> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Ok(Catalog {
                version: 1,
                selected_profile: None,
                ssh: Vec::new(),
            });
        }
        Err(e) => return Err(e),
    };
    if !file.metadata()?.is_file() {
        return Err(invalid("endpoint catalog is not a regular file"));
    }
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes)?;
    parse_catalog(&bytes)
}

#[cfg(test)]
fn parse(bytes: &[u8]) -> io::Result<Vec<SavedHost>> {
    parse_catalog(bytes).map(|catalog| catalog.ssh)
}

fn parse_catalog(bytes: &[u8]) -> io::Result<Catalog> {
    if bytes.len() > 65536 {
        return Err(invalid("endpoint catalog exceeds storage limit"));
    }
    // Do not include serde's error text: unknown field names can contain secrets.
    let catalog: Catalog =
        serde_json::from_slice(bytes).map_err(|_| invalid("invalid endpoint catalog schema"))?;
    if catalog.version != 1 || catalog.ssh.len() > 64 {
        return Err(invalid("unsupported catalog version or too many profiles"));
    }
    let mut ids = HashSet::new();
    for host in &catalog.ssh {
        if host.id.len() != 32
            || !host
                .id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !ids.insert(&host.id)
        {
            return Err(invalid("invalid or duplicate endpoint profile id"));
        }
        let label = host.label.trim();
        if label.is_empty() || label.len() > 128 || label.chars().any(char::is_control) {
            return Err(invalid("invalid endpoint label"));
        }
        validate_target(&host.target)?;
        session_socket(Path::new(""), &host.session)?;
    }
    if catalog
        .selected_profile
        .as_ref()
        .is_some_and(|id| !catalog.ssh.iter().any(|h| &h.id == id && h.enabled))
    {
        return Err(invalid("selected endpoint is absent or disabled"));
    }
    Ok(catalog)
}

pub(crate) fn validate_target(target: &str) -> io::Result<()> {
    let authority = target.strip_prefix("ssh://").unwrap_or(target);
    if target.is_empty()
        || target.starts_with('-')
        || target.len() > 1024
        || target.chars().any(char::is_control)
        || authority
            .rsplit_once('@')
            .is_some_and(|(user, _)| user.contains(':'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid SSH target (options, controls, and passwords are forbidden)",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    #[test]
    fn selection_roundtrip_fallbacks_and_independent_clients() {
        use std::os::unix::fs::PermissionsExt;
        let root = env::temp_dir().join(format!("herdr-selection-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("endpoints.json");
        let selection = root.join("endpoint-selection.json");
        let id = "0123456789abcdef0123456789abcdef";
        let catalog = json!({"version":1,"ssh":[{"id":id,"label":"Build","target":"build","session":"default","enabled":true}]});
        fs::write(&path, catalog.to_string()).unwrap();
        assert_eq!(load_with_selection(&path).unwrap().1, None);
        store_selection(&path, Some(id)).unwrap();
        let first_client = load_with_selection(&path).unwrap();
        assert_eq!(first_client.1.as_deref(), Some(id));
        assert_eq!(
            fs::metadata(&selection).unwrap().permissions().mode() & 0o777,
            0o600
        );
        store_selection(&path, None).unwrap();
        assert_eq!(load_with_selection(&path).unwrap().1, None);
        assert_eq!(first_client.1.as_deref(), Some(id));
        assert_eq!(load_path(&path).unwrap(), first_client.0);
        assert_eq!(fs::read_to_string(&path).unwrap(), catalog.to_string());
        let before = fs::read(&selection).unwrap();
        assert!(store_selection(&path, Some("missing")).is_err());
        assert_eq!(fs::read(&selection).unwrap(), before);
        for bad in [
            "not json".into(),
            json!({"version":2,"selected_profile":id}).to_string(),
            json!({"version":1,"selected_profile":id,"unknown":"secret"}).to_string(),
            json!({"version":1,"selected_profile":"missing"}).to_string(),
            " ".repeat(65537),
        ] {
            fs::write(&selection, bad).unwrap();
            let (hosts, selected) = load_with_selection(&path).unwrap();
            assert_eq!(hosts.len(), 1);
            assert_eq!(selected, None);
        }
        store_selection(&path, Some(id)).unwrap();
        let mut disabled = catalog.clone();
        disabled["ssh"][0]["enabled"] = json!(false);
        fs::write(&path, disabled.to_string()).unwrap();
        assert_eq!(load_with_selection(&path).unwrap().1, None);
        assert!(store_selection(&path, Some(id)).is_err());
        fs::write(&path, json!({"version":1}).to_string()).unwrap();
        assert_eq!(load_with_selection(&path).unwrap().1, None);
        let mut legacy = catalog;
        legacy["selected_profile"] = json!(id);
        fs::write(&path, legacy.to_string()).unwrap();
        fs::remove_file(&selection).unwrap();
        assert_eq!(load_with_selection(&path).unwrap().1.as_deref(), Some(id));
        fs::write(&selection, "bad").unwrap();
        assert_eq!(load_with_selection(&path).unwrap().1.as_deref(), Some(id));
        store_selection(&path, None).unwrap();
        assert_eq!(load_with_selection(&path).unwrap().1, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selection_write_refuses_nonfiles_without_touching_other_state() {
        let root = env::temp_dir().join(format!("herdr-selection-failure-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let catalog = root.join("endpoints.json");
        let selection = root.join("endpoint-selection.json");
        let other = root.join("other");
        fs::write(&other, "untouched").unwrap();
        std::os::unix::fs::symlink(&other, &selection).unwrap();
        assert!(store_selection(&catalog, None).is_err());
        assert_eq!(fs::read_to_string(&other).unwrap(), "untouched");
        fs::remove_file(&selection).unwrap();
        fs::create_dir(&selection).unwrap();
        assert!(store_selection(&catalog, None).is_err());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        assert!(store_selection(&other.join("endpoints.json"), None).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn paths_use_state_and_explicit_development() {
        assert_eq!(
            catalog_path(false, Some("/state".into()), Some("/home".into())),
            PathBuf::from("/state/herdr/client/endpoints.json")
        );
        assert_eq!(
            catalog_path(true, None, Some("/home".into())),
            PathBuf::from("/home/.local/state/herdr-dev/client/endpoints.json")
        );
        for (development, app) in [(false, "herdr"), (true, "herdr-dev")] {
            assert_eq!(
                catalog_path(development, Some("/state".into()), None)
                    .with_file_name("endpoint-selection.json"),
                PathBuf::from(format!("/state/{app}/client/endpoint-selection.json"))
            );
            assert_eq!(
                catalog_path(development, None, None),
                env::temp_dir().join(format!("{app}-state/client/endpoints.json"))
            );
        }
    }
    #[test]
    fn schema_limits_and_security() {
        let host = json!({"id":"0123456789abcdef0123456789abcdef", "label":"Build", "target":"ssh://dev@[::1]:2222", "session":"agents", "enabled":false});
        let valid = json!({"version":1,"ssh":[host.clone()]});
        assert!(parse(valid.to_string().as_bytes()).is_ok());
        for (field, value) in [
            ("password", json!("secret")),
            ("id", json!("BAD")),
            ("session", json!("../escape")),
            ("target", json!("-oProxyCommand=bad")),
            ("target", json!("ssh://dev:secret@host")),
            ("label", json!("\n")),
        ] {
            let mut bad = valid.clone();
            bad["ssh"][0][field] = value;
            assert!(parse(bad.to_string().as_bytes()).is_err(), "{field}");
        }
        assert!(
            parse(
                json!({"version":1,"ssh":[host.clone(),host]})
                    .to_string()
                    .as_bytes()
            )
            .is_err()
        );
        assert!(parse(json!({"version":2}).to_string().as_bytes()).is_err());
        assert!(
            parse(
                json!({"version":1,"selected_profile":"missing"})
                    .to_string()
                    .as_bytes()
            )
            .is_err()
        );
        assert!(parse(&vec![b' '; 65537]).is_err());
        let mut oversized = valid.clone();
        oversized["ssh"] = json!(vec![valid["ssh"][0].clone(); 65]);
        assert!(parse(oversized.to_string().as_bytes()).is_err());
        for field in ["label", "target"] {
            let mut oversized = valid.clone();
            oversized["ssh"][0][field] = json!("x".repeat(1025));
            assert!(parse(oversized.to_string().as_bytes()).is_err());
        }
        let mut selected = valid.clone();
        selected["selected_profile"] = valid["ssh"][0]["id"].clone();
        assert!(parse(selected.to_string().as_bytes()).is_err());
        selected["ssh"][0]["enabled"] = json!(true);
        assert!(parse(selected.to_string().as_bytes()).is_ok());
        assert!(
            matches!(load_path(&env::temp_dir().join(format!("herdr-missing-catalog-{}", std::process::id()))), Ok(hosts) if hosts.is_empty())
        );
    }
}
