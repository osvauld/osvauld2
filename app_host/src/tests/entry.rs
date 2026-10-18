use super::*;

fn resolver_empty() -> Resolve {
    Rc::new(|_| Ok(None))
}

fn entry_source(label: &str) -> LoroDoc {
    let source = LoroDoc::new();
    for (path, body) in [
        (
            "main.lua",
            "error('workspace entry must not run')".to_string(),
        ),
        (
            "spaces_view.lua",
            format!("return function() return ui.text({{'{label}'}}) end"),
        ),
    ] {
        source
            .get_map("files")
            .insert_container(path, LoroText::new())
            .unwrap()
            .insert(0, &body)
            .unwrap();
    }
    source
}

#[test]
fn trusted_source_entry_is_retained_through_staging_and_reload() {
    let mut app = LuaApp::open_with_entry(
        entry_source("first"),
        resolver_empty(),
        noop_wake(),
        identity(),
        None,
        "spaces_view.lua",
    )
    .unwrap();
    assert_eq!(app.view().info().text.as_deref(), Some("first"));
    app = app.stage_source(entry_source("second")).unwrap();
    app.reload().unwrap();
    assert_eq!(app.view().info().text.as_deref(), Some("second"));
    assert!(app.console(10).is_empty());
    let mut ordinary = LuaApp::open(
        entry_source("unused"),
        resolver_empty(),
        noop_wake(),
        identity(),
    )
    .unwrap();
    ordinary.view();
    assert!(
        ordinary
            .console(10)
            .iter()
            .any(|line| line.contains("workspace entry must not run"))
    );
}
