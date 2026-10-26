use super::*;
use app_host::{LuaApp, LuaMsg};
use loro::{LoroDoc, LoroText};
use vault::ItemKind;

fn inventory() -> Rc<RefCell<Snapshot>> {
    Rc::new(RefCell::new(Snapshot {
        workspace: WorkspaceMeta {
            id: "workspace".into(),
            name: "My workspace".into(),
            created: 0,
        },
        items: vec![WorkspaceItem {
            id: "item".into(),
            ws_id: "workspace".into(),
            name: "My app".into(),
            kind: ItemKind::App,
            created: 0,
        }],
    }))
}

fn app(
    main: &str,
    snapshot: Rc<RefCell<Snapshot>>,
    commands: Rc<RefCell<Vec<OpenItem>>>,
) -> LuaApp<LuaMsg> {
    let source = LoroDoc::new();
    let text = source
        .get_map("files")
        .insert_container("main.lua", LoroText::new())
        .unwrap();
    text.insert(0, main).unwrap();
    LuaApp::open_with_setup(
        source,
        Rc::new(|_| Ok(None)),
        std::sync::Arc::new(|| {}),
        Rc::new(|msg| msg),
        Some(setup(
            snapshot,
            Rc::new(move |c| commands.borrow_mut().push(c)),
        )),
    )
    .unwrap()
}

fn click(app: &mut LuaApp<LuaMsg>) {
    let message = app.view().trigger("open", runtime::Action::Click).unwrap();
    app.update(message);
}

#[test]
fn snapshot_is_a_fresh_copy_and_contains_only_workspace_inventory() {
    let snapshot = inventory();
    let mut other = snapshot.borrow().items[0].clone();
    other.id = "foreign".into();
    other.ws_id = "elsewhere".into();
    snapshot.borrow_mut().items.push(other);
    let (vm, _) = app_host::sandboxed_vm().unwrap();
    setup(
        snapshot.clone(),
        Rc::new(|_| panic!("read issued a command")),
    )(&vm, Default::default())
    .unwrap();
    vm.load(
        r#"
local a = shell.snapshot()
assert(a.workspace.id == 'workspace' and a.workspace.name == 'My workspace')
assert(#a.items == 1 and a.items[1].id == 'item' and a.items[1].kind == 'app')
a.items[1].name = 'changed by Lua'
assert(shell.snapshot().items[1].name == 'My app')
assert(not pcall(function() shell.open_item = function() end end))
"#,
    )
    .exec()
    .unwrap();
    snapshot.borrow_mut().items[0].name = "Changed by host".into();
    vm.load("assert(shell.snapshot().items[1].name == 'Changed by host')")
        .exec()
        .unwrap();
}

#[test]
fn handlers_queue_workspace_scoped_commands_and_reload_keeps_the_binding() {
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut app = app(
        r#"
local open = shell.open_item
return function()
    return ui.button({id = 'open', 'Open', on_click = function() open('item') end})
end
"#,
        inventory(),
        commands.clone(),
    );
    click(&mut app);
    app.reload().unwrap();
    click(&mut app);
    assert_eq!(
        *commands.borrow(),
        vec![
            OpenItem {
                ws_id: "workspace".into(),
                item_id: "item".into()
            },
            OpenItem {
                ws_id: "workspace".into(),
                item_id: "item".into()
            },
        ]
    );
    assert!(app.console(10).is_empty());
}

#[test]
fn load_view_and_reload_trials_cannot_queue_commands() {
    for main in [
        "shell.open_item('item'); return function() return ui.text({'bad'}) end",
        "return function() shell.open_item('item'); return ui.text({'bad'}) end",
    ] {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let mut app = app(main, inventory(), commands.clone());
        app.view();
        assert!(app.reload().is_err());
        assert!(commands.borrow().is_empty());
        assert!(
            app.console(10)
                .iter()
                .any(|e| e.contains("only allowed in handlers"))
        );
    }
}

#[test]
fn unknown_foreign_and_removed_items_cannot_be_opened() {
    for target in ["unknown", "foreign", "item"] {
        let snapshot = inventory();
        let mut foreign = snapshot.borrow().items[0].clone();
        foreign.id = "foreign".into();
        foreign.ws_id = "elsewhere".into();
        snapshot.borrow_mut().items.push(foreign);
        let commands = Rc::new(RefCell::new(Vec::new()));
        let mut app = app(
            &format!(
                r#"
return function()
    return ui.button({{id = 'open', 'Open', on_click = function() shell.open_item('{target}') end}})
end
"#
            ),
            snapshot.clone(),
            commands.clone(),
        );
        if target == "item" {
            snapshot.borrow_mut().items.retain(|i| i.id != "item");
        }
        click(&mut app);
        assert!(commands.borrow().is_empty());
        assert!(
            app.console(10)
                .iter()
                .any(|e| e.contains("not in this workspace"))
        );
    }
}

fn bundled(
    snapshot: Rc<RefCell<Snapshot>>,
    commands: Rc<RefCell<Vec<OpenItem>>>,
) -> LuaApp<LuaMsg> {
    let source = LoroDoc::new();
    for (path, content) in [
        ("main.lua", include_str!("../launcher/main.lua")),
        ("theme.lua", include_str!("../launcher/theme.lua")),
        ("editor.lua", include_str!("../launcher/editor.lua")),
        (
            "editor_model.lua",
            include_str!("../launcher/editor_model.lua"),
        ),
        (
            "editor_text.lua",
            include_str!("../launcher/editor_text.lua"),
        ),
        (
            "cursor.lua",
            include_str!("../../../demo_apps/pointer/cursor.lua"),
        ),
    ] {
        let text = source
            .get_map("files")
            .insert_container(path, LoroText::new())
            .unwrap();
        text.insert(0, content).unwrap();
    }
    LuaApp::open_with_setup(
        source,
        Rc::new(|_| Ok(None)),
        std::sync::Arc::new(|| {}),
        Rc::new(|msg| msg),
        Some(setup(
            snapshot,
            Rc::new(move |c| commands.borrow_mut().push(c)),
        )),
    )
    .unwrap()
}

#[test]
fn bundled_launcher_opens_items_and_refreshes_its_inventory() {
    let snapshot = inventory();
    let commands = Rc::new(RefCell::new(Vec::new()));
    let mut app = bundled(snapshot.clone(), commands.clone());
    let message = app
        .view()
        .trigger("launcher:item:item", runtime::Action::Click)
        .unwrap();
    app.update(message);
    assert_eq!(
        *commands.borrow(),
        vec![OpenItem {
            ws_id: "workspace".into(),
            item_id: "item".into()
        }]
    );
    snapshot.borrow_mut().items.clear();
    assert!(
        app.view()
            .trigger("launcher:item:item", runtime::Action::Click)
            .is_err()
    );
    assert!(app.console(10).is_empty());
    let mut item = inventory().borrow().items[0].clone();
    item.id = "new-item".into();
    snapshot.borrow_mut().items.push(item);
    let message = app
        .view()
        .trigger("launcher:item:new-item", runtime::Action::Click)
        .unwrap();
    app.update(message);
    assert_eq!(commands.borrow().last().unwrap().item_id, "new-item");
    assert!(app.console(10).is_empty());
}

#[test]
fn bundled_launcher_cards_fit_narrow_and_desktop_viewports() {
    struct Mounted(LuaApp<LuaMsg>);
    impl runtime::App for Mounted {
        type Msg = LuaMsg;
        fn view(&self) -> runtime::El<LuaMsg> {
            runtime::col().full().pad(40.0).child(self.0.view())
        }
        fn update(&mut self, msg: LuaMsg) {
            self.0.update(msg);
        }
    }
    for width in [320.0, 393.0, 900.0] {
        let app = bundled(inventory(), Rc::new(RefCell::new(Vec::new())));
        let mut driver = runtime::Headless::new(Mounted(app), (width, 700.0));
        let rects = driver.rects();
        let card = rects.iter().find(|r| r.id == "launcher:item:item").unwrap();
        assert!(
            card.w >= 199.0 && card.h >= 143.0,
            "clipped at {width}: {card:?}"
        );
        assert!(card.x >= 0.0 && card.x + card.w <= width);
        assert!(driver.app().0.console(10).is_empty());
    }
}
