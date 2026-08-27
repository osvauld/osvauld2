use crate::app_src::{app_name, snapshot};

use super::read_app_folder;
use loro::{Container, LoroDoc, ValueOrContainer};
use std::{fs, path::Path};
use tempfile::TempDir;

fn folder(files: &[(&str, &str)]) -> TempDir {
    let tmp = TempDir::new().unwrap();
    for (path, contents) in files {
        let full = tmp.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, contents).unwrap();
    }
    tmp
}

#[test]
fn reads_nested_files_with_slash_separated_paths() {
    let tmp = folder(&[
        ("main.lua", "return {}"),
        ("lib/state.lua", "return 1"),
        ("manifest.osv", "app \"todo\" "),
        ("README.md", "ignored"),
    ]);
    let files = read_app_folder(tmp.path()).unwrap();
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["lib/state.lua", "main.lua", "manifest.osv"]);
    assert_eq!(files[0].1, "return 1");
}

#[test]
fn a_main_lua_below_the_root_is_not_an_entry_point() {
    let tmp = folder(&[("lib/main.lua", "return {}")]);
    let error = read_app_folder(tmp.path()).unwrap_err();
    assert!(error.contains("no main.lua"), "{error}");
}

#[test]
fn dot_directories_are_skipped() {
    let tmp = folder(&[
        ("main.lua", "return {}"),
        (".git/hooks/pre-commit.lua", "x"),
    ]);
    let files = read_app_folder(tmp.path()).unwrap();
    assert_eq!(files.len(), 1);
}

#[test]
fn non_utf8_names_the_file() {
    let tmp = folder(&[("main.lua", "return {}")]);
    fs::write(tmp.path().join("blob.lua"), [0xff, 0xfe]).unwrap();
    let error = read_app_folder(tmp.path()).unwrap_err();
    assert!(error.contains("blob.lua"), "{error}");
}

#[test]
fn snapshot_stores_each_file_as_loro_text() {
    let tmp = folder(&[
        ("main.lua", "return {}"),
        ("lib/state.lua", "return 1"),
        ("manifest.osv", "app \"todo\""),
    ]);
    let files = read_app_folder(tmp.path()).unwrap();
    let bytes = snapshot(&files).unwrap();
    let doc = LoroDoc::new();
    doc.import(&bytes).unwrap();
    let map = doc.get_map("files");

    // Nothing collided onto one key and nothing was dropped.
    assert_eq!(map.len(), 3);

    // The `/`-path is the map key verbatim — no sanitising, no nesting.
    for (path, contents) in &files {
        let Some(ValueOrContainer::Container(Container::Text(t))) = map.get(path) else {
            panic!("{path} should be a loro text");
        };
        assert_eq!(&t.to_string(), contents);
    }
}
#[test]
fn manifest_name_wins() {
    let tmp = folder(&[("main.lua", "return {}"), ("manifest.osv", "app \"todo\"")]);
    let files = read_app_folder(tmp.path()).unwrap();
    let name = app_name(&files, tmp.path());
    assert_eq!(name, "todo");
}
#[test]
fn default_folder_name() {
    let tmp = folder(&[("main.lua", "return {}")]);
    let files = read_app_folder(tmp.path()).unwrap();
    let root = Path::new("/x/kanban");
    let name = app_name(&files, root);
    assert_eq!(name, "kanban");
}

// `app_name` and `manifest_name` are pure functions over data — no disk needed, and a literal
// root is the only way to assert a folder name the test can name.
fn files(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(p, c)| (p.to_string(), c.to_string()))
        .collect()
}

#[test]
fn a_manifest_without_an_app_line_falls_back_to_the_folder() {
    let files = files(&[("main.lua", "return {}"), ("manifest.osv", "version \"0.1.0\"")]);
    assert_eq!(app_name(&files, Path::new("/x/kanban")), "kanban");
}

#[test]
fn an_empty_manifest_name_falls_back_to_the_folder() {
    for manifest in [r#"app """#, r#"app "   ""#] {
        let files = files(&[("main.lua", "return {}"), ("manifest.osv", manifest)]);
        assert_eq!(
            app_name(&files, Path::new("/x/kanban")),
            "kanban",
            "{manifest}"
        );
    }
}

#[test]
fn a_padded_manifest_name_is_trimmed() {
    let files = files(&[("manifest.osv", r#"app "  todo  " version "0.1.0""#)]);
    assert_eq!(app_name(&files, Path::new("/x/kanban")), "todo");
}

#[test]
fn a_rootless_path_falls_back_to_a_last_resort_name() {
    // `file_name()` is None for `/` and for anything ending in `..` — the picker won't hand us
    // those, but the Option forces a decision, so pin the one that was made.
    assert_eq!(app_name(&files(&[]), Path::new("/")), "app");
}
