use app_host::{LuaApp, LuaMsg};
use loro::LoroText;
use runtime::{App, El, Headless};
use std::{cell::RefCell, rc::Rc, sync::Arc};

struct Mounted(LuaApp<LuaMsg>, Rc<RefCell<Vec<String>>>);
impl App for Mounted {
    type Msg = LuaMsg;
    fn view(&self) -> El<LuaMsg> {
        runtime::col().full().pad(40.0).child(self.0.view())
    }
    fn update(&mut self, message: LuaMsg) {
        self.0.update(message);
    }
}

fn mounted(rows: &str) -> Mounted {
    let source = super::source::bundled().unwrap();
    source.get_map("files").insert_container("main.lua", LoroText::new()).unwrap()
        .insert(0, &format!(
            "local view = require('spaces')\nreturn function() return view({rows}, select_workspace) end"
        )).unwrap();
    let selected = Rc::new(RefCell::new(Vec::new()));
    let recorded = selected.clone();
    let setup: app_host::host::VmSetup = Rc::new(move |vm, context| {
        let recorded = recorded.clone();
        vm.globals().set(
            "select_workspace",
            vm.create_function(move |_, id: String| {
                context.require_handler()?;
                recorded.borrow_mut().push(id);
                Ok(())
            })?,
        )
    });
    Mounted(
        LuaApp::open_with_setup(
            source,
            Rc::new(|_| Ok(None)),
            Arc::new(|| {}),
            Rc::new(|m| m),
            Some(setup),
        )
        .unwrap(),
        selected,
    )
}

#[test]
fn workspace_list_cards_select_their_own_ids_only_from_handlers() {
    let app = mounted("{{id='one',name='First workspace'}, {id='two',name='Second workspace'}}");
    let mut view = app.view();
    assert!(view.contains_id("launcher:workspace-list"));
    assert!(app.1.borrow().is_empty(), "selection ran during view/load");
    assert!(app.0.console(10).is_empty());
    let message = view
        .trigger("launcher:space:two", runtime::Action::Click)
        .unwrap();
    let mut app = app;
    app.update(message);
    assert_eq!(*app.1.borrow(), vec!["two"]);
    assert!(app.0.console(10).is_empty());
}

#[test]
fn workspace_list_empty_and_narrow_layouts_remain_visible() {
    let empty = mounted("{}");
    assert!(
        crate::info_json(&empty.view().info())
            .to_string()
            .contains("no workspaces yet")
    );
    assert!(empty.0.console(10).is_empty());
    for width in [320.0, 900.0] {
        let app = mounted("{{id='one',name='Workspace with a longer name'}}");
        let mut driver = Headless::new(app, (width, 700.0));
        let rects = driver.rects();
        let card = rects
            .iter()
            .find(|r| r.id == "launcher:space:one")
            .expect("missing workspace card");
        assert!(card.x >= 0.0 && card.x + card.w <= width + 0.5);
        assert!(card.h > 0.0);
        assert!(driver.app().0.console(10).is_empty());
    }
}
