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

/// Times `f` over `runs` runs after one to warm up, and prints `BENCH <name> <µs>` for
/// `scripts/bench.py`, which builds release.
fn bench(name: &str, runs: u32, mut f: impl FnMut()) {
    f();
    let started = std::time::Instant::now();
    for _ in 0..runs {
        f();
    }
    let us = started.elapsed().as_secs_f64() * 1e6 / f64::from(runs);
    let debug = if cfg!(debug_assertions) { " debug" } else { "" };
    println!("BENCH {name} {us:.1}{debug}");
}

/// U1 (`ui-reuse.md`): a frame of the workspace list with nothing changing — view, layout,
/// hit-testing and paint, as `Headless` runs them.
#[test]
#[ignore = "benchmark: scripts/bench.py"]
fn bench_workspace_list_idle_frame() {
    for n in [8, 40] {
        let rows: Vec<String> = (0..n).map(|i| format!("{{id='w{i}',name='Workspace {i}'}}")).collect();
        let mut driver = Headless::new(mounted(&format!("{{{}}}", rows.join(","))), (1000.0, 800.0));
        bench(&format!("workspace_list_{n}_idle_frame"), 100, || driver.frame());
    }
}
