use super::*;
use mlua::Table;

#[test]
fn vm_runs_lua() {
    let lua = sandboxed_vm().unwrap();
    let res = lua.load("return 1+2").eval::<i64>();
    assert_eq!(res.unwrap(), 3);
}

#[test]
fn vm_interrupt() {
    let lua = sandboxed_vm().unwrap();
    let res = lua.load("while true do end").exec();
    assert!(res.is_err(), "inifinite loop should have been killed")
}
#[test]
fn now() {
    let lua = sandboxed_vm().unwrap();
    let res = lua.load("return now()").eval::<i64>();
    assert!(res.unwrap() > 1);
}

#[test]
fn prelude_tags_tables() {
    let lua = sandboxed_vm().unwrap();
    let node: Table = lua.load(r#"return ui.col{ui.text{"hi"}}"#).eval().unwrap();
    let tag: String = node.get("tag").unwrap();
    assert_eq!(tag, "col");
    let child: Table = node.get(1).unwrap();
    let ctag: String = child.get("tag").unwrap();
    assert_eq!(ctag, "text");
    let label: String = child.get(1).unwrap();
    assert_eq!(label, "hi");
}

#[test]
fn walk_builds_el() {
    let lua = sandboxed_vm().unwrap();
    let node: Table = lua
        .load(r#"return ui.col{ui.text{"a"}, ui.row{ui.text{"b"}, ui.text{"c"}}}"#)
        .eval()
        .unwrap();
    let mut handlers: Vec<Function> = Vec::new();
    assert!(walk(node, &mut handlers).is_ok());
}
#[test]
fn walk_collects_handlers() {
    let lua = sandboxed_vm().unwrap();
    let node: Table = lua
        .load(r#"return ui.button{"add", on_click = function() end }"#)
        .eval()
        .unwrap();

    let mut handlers: Vec<Function> = Vec::new();
    let _el = walk(node, &mut handlers).unwrap();
    assert_eq!(handlers.len(), 1);
    assert!(handlers[0].call::<()>(()).is_ok());
}
