//! Theme files on the server (spec 030): the web client's themes are the
//! server's `<config>/moonkale/themes/*.json`, as JSON texts. The shell parses
//! and checks them (`moonkale_shell::theme`).

use dioxus::prelude::*;

/// The theme files of the server's config directory (none when it has none).
#[post("/api/themes")]
pub async fn theme_files() -> Result<Vec<String>, ServerFnError> {
    #[cfg(feature = "server")]
    {
        Ok(read_dir())
    }
    #[cfg(not(feature = "server"))]
    {
        Ok(Vec::new())
    }
}

/// `<config>/moonkale/themes/*.json` of this machine, sorted by name; files
/// over 64 KB are skipped. Also what the desktop and phone read locally.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_dir() -> Vec<String> {
    let Some(dir) = moonkale_llm::secrets::config_dir().map(|d| d.join("themes")) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() < 64 * 1024))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect()
}

/// The client's [`theme_files`] as a future for the shell's `ThemeFiles`.
pub fn remote_themes() -> moonkale_ext_api::SettingsFuture<Vec<String>> {
    Box::pin(async { theme_files().await.map_err(|e| e.to_string()) })
}

/// This machine's theme files as a future for the shell's `ThemeFiles`.
#[cfg(not(target_arch = "wasm32"))]
pub fn local_themes() -> moonkale_ext_api::SettingsFuture<Vec<String>> {
    Box::pin(async { Ok(read_dir()) })
}
