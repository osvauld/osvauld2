//! Apps shipped inside the app bundle, installed into a new account's "Apps" space at signup
//! (docs/design/ios.md I4). iOS has no folder picker and a friend's phone may have no node, so
//! this is how the first apps arrive. Signup only: unlock never reinstalls, and an account keeps
//! the copy it got even when the bundle later changes.
use std::path::{Path, PathBuf};

use vault::{ItemKind, Vault};

use crate::app_src::{app_name, read_app_folder, snapshot};

/// `OSVAULD_BUNDLED_APPS`, else on iOS `apps/` beside the executable inside the `.app`.
pub fn dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("OSVAULD_BUNDLED_APPS") {
        return Some(dir.into());
    }
    #[cfg(target_os = "ios")]
    return std::env::current_exe().ok()?.parent().map(|p| p.join("apps"));
    #[cfg(not(target_os = "ios"))]
    None
}

/// Every folder in `dir` with a `main.lua` becomes an app item in one new "Apps" space, in name
/// order. No such folder, no space. Small and once per account, so it runs on the UI thread.
pub fn install(vault: &Vault, dir: &Path) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    let mut roots: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("main.lua").is_file())
        .collect();
    if roots.is_empty() {
        return Ok(());
    }
    roots.sort();
    let peer = crate::device_peer(vault)?;
    let ws = vault.create_workspace("Apps").map_err(|e| e.to_string())?;
    for root in roots {
        let files = read_app_folder(&root)?;
        let item = vault
            .create_item(&ws.id, &app_name(&files, &root), ItemKind::App)
            .map_err(|e| e.to_string())?;
        vault
            .put_src(&ws.id, &item.id, &snapshot(&files, peer)?)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
