use super::*;
use app_host::{LuaApp, LuaMsg};
use loro::ExportMode;

fn app(main: &str) -> (LuaApp<LuaMsg>, Rc<RefCell<Vec<Command>>>) {
    let committed = crate::launcher::source::bundled().unwrap();
    let src =
        crate::launcher::source::import(&committed.export(ExportMode::Snapshot).unwrap()).unwrap();
    app_host::write_source_file(&src, "main.lua", main).unwrap();
    let state = Rc::new(RefCell::new(State {
        doc: committed,
        generation: 7,
    }));
    let commands = Rc::new(RefCell::new(vec![]));
    let install = setup(state, "workspace".into(), commands.clone());
    let app = LuaApp::open_with_setup(
        src,
        Rc::new(|_| Ok(None)),
        std::sync::Arc::new(|| {}),
        Rc::new(|msg| msg),
        Some(Rc::new(move |vm, context| {
            let api = vm.create_table()?;
            api.set_readonly(true);
            vm.globals().set("shell", api)?;
            install(vm, context)
        })),
    )
    .unwrap();
    (app, commands)
}

#[test]
fn source_reads_are_fresh_copies_and_the_api_stays_frozen() {
    let (app, commands) = app(r#"
local a = shell.read_launcher_file('main.lua')
assert(a.generation == 7)
a.content = 'tampered'
assert(shell.read_launcher_file('main.lua').content ~= a.content)
local ok = pcall(function() shell.edit_launcher_file = nil end)
assert(not ok)
return function() return ui.text({'good'}) end
"#);
    app.view();
    assert!(app.console(10).is_empty());
    assert!(commands.borrow().is_empty());
}

#[test]
fn source_edits_queue_plain_scoped_data_only_in_handlers() {
    let (mut app, commands) = app(r#"
return function()
    return ui.button({id='save', 'save', on_click=function()
        local source = shell.read_launcher_file('main.lua')
        shell.edit_launcher_file('main.lua', source.revision, {{old_text='Your apps', new_text='Mine'}})
    end})
end
"#);
    let msg = app.view().trigger("save", runtime::Action::Click).unwrap();
    app.update(msg);
    assert!(
        matches!(&commands.borrow()[0], Command::EditSource { ws_id, path, edits, .. }
        if ws_id == "workspace" && path == "main.lua" && edits[0].new_text == "Mine")
    );
    commands.borrow_mut().clear();
    let bad = "shell.edit_launcher_file('main.lua', '', {}); return function() return ui.text({'bad'}) end";
    let trial = crate::launcher::source::bundled().unwrap();
    app_host::write_source_file(&trial, "main.lua", bad).unwrap();
    assert!(app.stage_source(trial).is_err());
    assert!(commands.borrow().is_empty());
}

#[test]
fn editor_tab_projection_preserves_unedited_bytes_including_unicode_and_string_literals() {
    let (app, _) = app(r#"
local t = require('editor_text')
assert(t.display('\tfoo') == '    foo')
assert(t.apply('\tfoo\tbar', '    foo    baz') == '\tfoo\tbaz')
assert(t.apply('\tfoo', '  foo') == '  foo')
assert(t.apply('\tfoo', '     foo') == '\t foo')
assert(t.apply('\té', '    ë') == '\të')
local literal = 'return [[\\n\tkeep\tliteral\\n]]'
assert(t.apply(literal, '-- comment\\n' .. t.display(literal)) == '-- comment\\n' .. literal)
assert(t.apply(literal, t.display(literal)) == literal)
return function() return ui.text({'good projection'}) end
"#);
    app.view();
    assert!(app.console(10).is_empty(), "{:?}", app.console(10));
}

#[test]
fn replacement_shape_is_strict() {
    let vm = mlua::Lua::new();
    for bad in [
        "{extra=true}",
        "{[2]={old_text='a',new_text='b'}}",
        "{{old_text='a',new_text='b',typo=true}}",
    ] {
        assert!(
            replacements(vm.load(format!("return {bad}")).eval().unwrap()).is_err(),
            "accepted {bad}"
        );
    }
    assert_eq!(
        replacements(
            vm.load("return {{old_text='a',new_text='b'}}")
                .eval()
                .unwrap()
        )
        .unwrap(),
        vec![SourceEdit {
            old_text: "a".into(),
            new_text: "b".into()
        }]
    );
}
