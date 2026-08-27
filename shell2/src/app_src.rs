use std::path::Path;

use loro::{LoroDoc, LoroText};
const KEEP: [&str; 2] = ["lua", "osv"];

pub fn read_app_folder(root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    walk(root, "", &mut out)?;
    if !out.iter().any(|(path, _)| path == "main.lua") {
        return Err(format!("no main.lua in {}", root.display()));
    }
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, String)>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}, {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(".") {
            continue;
        }
        let rel = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let file_type = entry.file_type().map_err(|e| format!("{rel}: {e}"))?;
        let path = entry.path();
        if file_type.is_dir() {
            walk(&path, &rel, out)?;
        } else if file_type.is_file() && keep(&path) {
            let bytes = std::fs::read(&path).map_err(|e| format!("{rel}: {e}"))?;
            let text =
                String::from_utf8(bytes).map_err(|e| format!("{rel}: {e}, not utf 8 text"))?;
            out.push((rel, text));
        }
    }

    Ok(())
}
fn keep(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| KEEP.contains(&e))
}

pub fn snapshot(files: &[(String, String)]) -> Result<Vec<u8>, String> {
    let doc = LoroDoc::new();
    let map = doc.get_map("files");
    for (path, contents) in files {
        let text = map
            .insert_container(path, LoroText::new())
            .map_err(|e| e.to_string())?;
        text.insert(0, &contents).map_err(|e| e.to_string())?;
    }
    doc.export(loro::ExportMode::Snapshot)
        .map_err(|e| e.to_string())
}

pub fn app_name(files: &[(String, String)], root: &Path) -> String {
    files
        .iter()
        .find(|(path, _)| path == "manifest.osv")
        .and_then(|(_, content)| manifest_name(content))
        .map(str::to_string)
        .unwrap_or_else(|| folder_name(root))
}
fn manifest_name(content: &str) -> Option<&str> {
    let Some((_, rest)) = content.split_once(r#"app ""#) else {
        return None;
    };

    let Some((name, _)) = rest.split_once('"') else {
        return None;
    };
    if name.trim().is_empty() {
        return None;
    }
    Some(name.trim())
}
fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".to_string())
}

#[cfg(test)]
mod tests;
