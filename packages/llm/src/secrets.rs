//! Secret resolution (native). Settings name a secret; the value comes, in
//! order, from:
//!
//! 1. `MOONKALE_SECRET_<NAME>` in the environment (name upper-cased, `-`→`_`),
//! 2. the classic variables for the well-known names (`ANTHROPIC_API_KEY`
//!    for `anthropic`, `OPENAI_API_KEY` for `openai`),
//! 3. `<config dir>/moonkale/secrets.json` (`{"name": "value"}`, mode 600),
//!    which the Settings panel writes.
//!
//! An OS keychain (`keyring`) is the intended next step; it needs a Secret
//! Service on Linux and is deliberately not a hard dependency yet.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// `$XDG_CONFIG_HOME/moonkale` (or the platform equivalent).
pub fn config_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("MOONKALE_CONFIG_DIR") {
        return Some(PathBuf::from(p));
    }
    dirs::config_dir().map(|d| d.join("moonkale"))
}

fn secrets_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("secrets.json"))
}

fn read_file() -> BTreeMap<String, String> {
    secrets_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn resolve(name: &str) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    let env_name = format!(
        "MOONKALE_SECRET_{}",
        name.to_ascii_uppercase().replace('-', "_")
    );
    if let Some(v) = var(&env_name) {
        return Some(v);
    }
    let classic = match name {
        "anthropic" => var("ANTHROPIC_API_KEY"),
        "openai" => var("OPENAI_API_KEY"),
        _ => None,
    };
    if classic.is_some() {
        return classic;
    }
    read_file().get(name).cloned()
}

/// Names known to the secrets file (never the values), for the panel.
pub fn stored_names() -> Vec<String> {
    read_file().keys().cloned().collect()
}

/// Write (or, with an empty value, remove) a secret in the secrets file.
pub fn store(name: &str, value: &str) -> Result<(), String> {
    let path = secrets_path().ok_or("no config directory on this platform")?;
    let mut map = read_file();
    if value.is_empty() {
        map.remove(name);
    } else {
        map.insert(name.to_string(), value.to_string());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Is a value available for `name` from any of the sources?
pub fn available(name: &str) -> bool {
    resolve(name).is_some()
}
