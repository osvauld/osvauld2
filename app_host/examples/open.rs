//! Open an app folder headlessly, and optionally poke it with a pointer.
//!
//!     cargo run -p app_host --example open -- demo_apps/pie
//!     cargo run -p app_host --example open -- demo_apps/pie --hover 120,120
//!     cargo run -p app_host --example open -- demo_apps/voronoi --drag 200,200:260,240
//!
//! No window, no GPU: it loads the folder's `.lua` files the way the shell would and builds the
//! view. Pointer actions run through the runtime's real dispatch — the same layout, the same hit
//! regions, the same handler call the window would make — so they exercise what they claim to.
//! After each action it reports new console lines and whether the view changed.
//! Exits non-zero if anything landed on the console, so it works as a check in a loop.

use std::cell::RefCell;
use std::rc::Rc;

use app_host::{LuaApp, LuaMsg};
use loro::{LoroDoc, LoroText};
use runtime::{El, ElInfo, Headless};

/// The shell wraps a `LuaApp` in its own screens and tabs; on its own it is already an app.
struct Solo(LuaApp<LuaMsg>);

impl runtime::App for Solo {
    type Msg = LuaMsg;
    fn view(&self) -> El<LuaMsg> {
        self.0.view()
    }
    fn update(&mut self, msg: LuaMsg) {
        self.0.update(msg);
    }
}

enum Action {
    Hover(f32, f32),
    Click(f32, f32),
    Drag((f32, f32), (f32, f32), usize),
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(folder) = args.next() else {
        eprintln!("usage: open <app folder> [--size WxH] [--hover X,Y] [--click X,Y] [--drag X0,Y0:X1,Y1[:steps]] [--tree]");
        std::process::exit(2);
    };
    let (mut size, mut actions, mut every_tree) = ((1200.0f32, 800.0f32), Vec::new(), false);
    while let Some(flag) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| die(&format!("{flag} needs a value")));
        match flag.as_str() {
            "--size" => size = pair(&value(), 'x'),
            "--hover" => {
                let (x, y) = pair(&value(), ',');
                actions.push(Action::Hover(x, y));
            }
            "--click" => {
                let (x, y) = pair(&value(), ',');
                actions.push(Action::Click(x, y));
            }
            "--drag" => actions.push(drag(&value())),
            "--tree" => every_tree = true,
            other => die(&format!("unknown flag {other}")),
        }
    }

    let src = LoroDoc::new();
    let files = src.get_map("files");
    let mut loaded = Vec::new();
    for entry in walk(std::path::Path::new(&folder)) {
        let name = entry
            .strip_prefix(&folder)
            .unwrap_or(&entry)
            .to_string_lossy()
            .trim_start_matches('/')
            .to_string();
        let body = std::fs::read_to_string(&entry).unwrap();
        files
            .insert_container(name.as_str(), LoroText::new())
            .unwrap()
            .insert(0, &body)
            .unwrap();
        loaded.push(name);
    }
    if !loaded.iter().any(|n| n == "main.lua") {
        eprintln!("no main.lua in {folder}");
        std::process::exit(2);
    }
    src.commit();
    println!("loaded: {}", loaded.join(", "));

    let app = LuaApp::open(
        src,
        Rc::new(|_| Ok(None)),
        std::sync::Arc::new(|| {}),
        Rc::new(|m: LuaMsg| m),
    )
    .unwrap();

    let mut driver = Headless::new(Solo(app), size);
    let mut tree = render(&driver.app().0.view().info());
    print!("{tree}");
    println!("\n{}", counts(&tree));

    let mut seen = driver.app().0.console(512).len();
    for action in &actions {
        match *action {
            Action::Hover(x, y) => {
                println!("\n--- hover {x},{y}");
                driver.move_to(x, y);
            }
            Action::Click(x, y) => {
                println!("\n--- click {x},{y}");
                driver.click_at(x, y);
            }
            Action::Drag(from, to, steps) => {
                println!(
                    "\n--- drag {},{} → {},{} in {steps}",
                    from.0, from.1, to.0, to.1
                );
                driver.drag(from, to, steps);
            }
        }
        let log = driver.app().0.console(512);
        for line in &log[seen.min(log.len())..] {
            println!("  console: {line}");
        }
        seen = log.len();

        let now = render(&driver.app().0.view().info());
        if now == tree {
            println!("  view: unchanged");
        } else {
            println!("  view: changed");
            if every_tree {
                print!("{now}");
            }
        }
        tree = now;
    }

    // The console is cumulative, so a late look catches anything the per-action reports missed.
    let console = driver.app().0.console(100);
    if console.is_empty() {
        println!("\nconsole: clean");
    } else {
        println!("\nconsole:");
        for line in &console {
            println!("  {line}");
        }
        std::process::exit(1);
    }
}

fn die(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}

/// "12,34" or "900x600" — both halves required, because half a point is never what was meant.
fn pair(s: &str, sep: char) -> (f32, f32) {
    let Some((a, b)) = s.split_once(sep) else {
        die(&format!("expected A{sep}B, got {s:?}"));
    };
    match (a.trim().parse(), b.trim().parse()) {
        (Ok(a), Ok(b)) => (a, b),
        _ => die(&format!("expected numbers, got {s:?}")),
    }
}

/// `X0,Y0:X1,Y1` with an optional `:steps`. More steps means more intermediate moves, which is
/// what a handler watching `delta` between frames will see.
fn drag(s: &str) -> Action {
    let mut parts = s.split(':');
    let (from, to) = match (parts.next(), parts.next()) {
        (Some(f), Some(t)) => (pair(f, ','), pair(t, ',')),
        _ => die(&format!("expected X0,Y0:X1,Y1, got {s:?}")),
    };
    let steps = parts.next().map_or(8, |n| n.parse().unwrap_or(8));
    Action::Drag(from, to, steps)
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else if path.extension().is_some_and(|e| e == "lua") {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn counts(tree: &str) -> String {
    let elements = tree.lines().count();
    let handlers: usize = tree
        .lines()
        .filter_map(|l| l.split_once('[')?.1.split_once(']'))
        .map(|(h, _)| h.split_whitespace().count())
        .sum();
    format!("{elements} elements, {handlers} handlers")
}

/// The tree as text, so two of them can be compared for "did anything move".
fn render(el: &ElInfo) -> String {
    let out = RefCell::new(String::new());
    show(el, 0, &out);
    out.into_inner()
}

/// One line per element: its kind, its id, its handlers, and a clipped look at any text.
fn show(el: &ElInfo, depth: usize, out: &RefCell<String>) {
    use std::fmt::Write;
    let id = el.id.as_deref().map(|i| format!("#{i}")).unwrap_or_default();
    let handlers = match el.handlers.as_slice() {
        [] => String::new(),
        hs => format!("  [{}]", hs.join(" ")),
    };
    let text = match el.text.as_deref() {
        Some(t) if !t.is_empty() => format!("  {:?}", clip(t, 48)),
        _ => String::new(),
    };
    let _ = writeln!(
        out.borrow_mut(),
        "{:indent$}{}{id}{handlers}{text}",
        "",
        el.kind,
        indent = depth * 2
    );
    for child in &el.children {
        show(child, depth + 1, out);
    }
}

fn clip(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}
