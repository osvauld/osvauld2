//! Sandboxed Luau app host for the new runtime.
//!
//! An app is an MVU app whose message = "call the handler at this key" (see [`LuaMsg`]). Its
//! `view()` walks a Lua `ui.*` tree directly into `runtime::El<LuaMsg>`; the runtime's update
//! loop is unchanged — dispatch just calls whatever the latest view registered under that key.
//! The author's guide — app shape, `ui.*`, the doc binding, state — is docs/lua-apps.md.
mod crdt;
mod gfx;
mod modules;
mod props;
pub use crdt::{Cores, Docs, Resolve, Wake};

use loro::{
    Container, EventTriggerKind, ExportMode, LoroDoc, LoroText, Subscription, ValueOrContainer,
};
use mlua::{AnyUserData, Error, Function, IntoLua, Lua, Table, Value};
use runtime::vello::peniko::Color;
use runtime::{
    Anchor, El, Placement, PlacementAlign, PlacementSide, col, frame as frame_el, row,
    scene3d as scene3d_el, text, text_area, text_input,
};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::crdt::patch_into;
/// Which handler a message calls: the element's `id` and the prop that set it.
///
/// Not a slot number. A click is minted at press and delivered at release, and views rebuild in
/// between — a peer edit or an `on_frame` tick can add a handler above this one, and a slot would
/// then call its neighbour. A key finds the same element's current handler, or nothing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub id: Arc<str>,
    pub name: &'static str,
}
impl Key {
    pub fn new(id: &str, name: &'static str) -> Self {
        Self {
            id: id.into(),
            name,
        }
    }
}

pub type Handlers = HashMap<Key, Function>;

/// What a drag hands Lua, in order: where the pointer is in the element's own units, how far it
/// has travelled from the press, the zoom scale, and the dragged element's screen origin — which
/// only a root-level ghost placing itself in screen space needs.
#[derive(Clone, Debug, PartialEq)]
pub struct DragArgs {
    pub phase: &'static str,
    pub at: (f32, f32),
    pub delta: (f32, f32),
    pub scale: f32,
    pub origin: (f32, f32),
    /// The shape the press grabbed, held for the whole gesture. See [`Shape`].
    pub shape: Shape,
    /// Monotonic seconds since the app opened, stamped when the pointer event arrived. Reaches
    /// Lua as `e.t`, and shares an epoch with `on_frame`'s `elapsed`.
    pub t: f64,
}

/// The trailing arguments a pointer handler carries: which named shape of the element's visual
/// the pointer is on, and where on that shape. All three reach Lua as `nil` when it is on none —
/// an element that draws no Frame never has one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape(pub Option<(String, f32, f32)>);

impl From<Option<runtime::frame::FrameHit>> for Shape {
    fn from(hit: Option<runtime::frame::FrameHit>) -> Self {
        Self(hit.map(|h| (h.id.to_string(), h.local.x as f32, h.local.y as f32)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectHit {
    pub id: String,
    pub distance: f32,
    pub point: [f32; 3],
    pub normal: [f32; 3],
}

impl From<runtime::scene3d::SceneHit> for ObjectHit {
    fn from(hit: runtime::scene3d::SceneHit) -> Self {
        Self {
            id: hit.id.to_string(),
            distance: hit.distance,
            point: hit.world_position.to_array(),
            normal: hit.world_normal.to_array(),
        }
    }
}

impl Shape {
    /// Absent on an element that draws no frame, or when the pointer is on none of its named
    /// shapes — so the three keys are simply missing rather than present and nil.
    fn write(self, event: &Table) -> mlua::Result<()> {
        if let Some((id, x, y)) = self.0 {
            event.set("shape", id)?;
            event.set("sx", x)?;
            event.set("sy", y)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum LuaMsg {
    Call(Key),
    CallAt(Key, f32, f32, Shape, Option<ObjectHit>),
    CallStr(Key, String),
    CallPhase(Key, &'static str, f32, f32, Shape),
    CallDrag(Key, DragArgs),
    CallWheel(Key, f32, f32),
    CallFrame(Key, f32, f64),
    CallKey(Key, runtime::KeyInput),
}

/// A second element with the same id and handler would silently take the first one's events.
pub(crate) fn register(
    handlers: &mut Handlers,
    id: &str,
    name: &'static str,
    f: Function,
) -> mlua::Result<Key> {
    match handlers.entry(Key::new(id, name)) {
        Entry::Occupied(_) => Err(Error::runtime(format!(
            "duplicate id {id}: another element already has {name}"
        ))),
        Entry::Vacant(slot) => {
            let key = slot.key().clone();
            slot.insert(f);
            Ok(key)
        }
    }
}

pub struct Ctx<'a, M> {
    pub handlers: &'a mut Handlers,
    pub dev: bool,
    pub errors: Vec<String>,
    pub path: String,
    pub to_msg: Rc<dyn Fn(LuaMsg) -> M>,
}
impl<'a, M> Ctx<'a, M> {
    fn new(handlers: &'a mut Handlers, to_msg: Rc<dyn Fn(LuaMsg) -> M>) -> Self {
        Self {
            handlers,
            dev: true,
            errors: Vec::new(),
            path: String::new(),
            to_msg,
        }
    }
}

/// The app's source, and the counter that says when it moved.
///
/// Shared rather than owned for the same reason [`Cores`] is: it outlives every VM built from it.
/// The subscription has to outlive them too — it unsubscribes on drop, so a rebuild that owned
/// one would either lose the watch or start a second one and double-count every later edit.
pub struct Source {
    pub doc: LoroDoc,
    /// Bumped on every commit to `doc`, local or imported, and compared against the per-VM
    /// watermark in [`LuaApp::reload_if_stale`].
    version: Arc<AtomicU64>,
    _sub: Subscription,
}

impl Source {
    /// Start watching a source doc.
    ///
    /// The `Import` gate is the same one the data docs use, and for the same reason: a *local*
    /// source write — a code block edited in-app — already happens inside a frame the host asked
    /// for, and the message that carried it runs the staleness check on its way out. An import
    /// has no such frame, so it has to ask for one. The gate decides whether to schedule an extra
    /// frame; it never decides whether the edit counts, which is why the bump sits above it.
    pub fn new(doc: LoroDoc, wake: Wake) -> Self {
        let version = Arc::new(AtomicU64::new(0));
        let v = version.clone();
        let sub = doc.subscribe_root(Arc::new(move |ev| {
            v.fetch_add(1, Ordering::Relaxed);
            if ev.triggered_by == EventTriggerKind::Import {
                wake();
            }
        }));
        Self {
            doc,
            version,
            _sub: sub,
        }
    }
}

pub struct LuaApp<M> {
    vm: Lua, // never read, but every `Function` below borrows from it — dropping it invalidates them
    view_fn: Option<Function>, //the closure the source returns
    handlers: RefCell<Handlers>, // refilled every frame
    error: Option<String>,
    /// The last reload that failed, rendered by `view()` as a banner *above* the still-running
    /// app. `error` is the other kind: a source that never loaded at all, which leaves nothing to
    /// run. Cleared by the next successful reload, since that replaces the whole struct.
    reload_error: Option<String>,
    fires: Arc<AtomicU64>,
    to_msg: Rc<dyn Fn(LuaMsg) -> M>,
    /// Which version of the source this VM was built from. Read *before* the build, so an edit
    /// landing mid-build leaves the VM stale rather than falsely current.
    src_seen: u64,
    /// The four below outlive the VM, and that is the whole reason [`reload`](Self::reload) can
    /// build a replacement beside the running one: `cores` keeps the open docs (and so their
    /// unflushed writes and their single subscription), `src` keeps the watch, and the rest is
    /// what `build` needs to make a VM at all.
    src: Rc<Source>,
    docs: Docs,
    cores: Cores,
    /// The app's console: errors from view builds, handler runs and reloads — newest last,
    /// a repeat of the previous line collapses (a broken source re-errors every frame).
    /// Shared like [`Self::cores`] so the whole-struct swap in [`reload`](Self::reload) keeps
    /// the log: the console, like the cores, outlives the VM it reports on.
    console: Rc<RefCell<VecDeque<String>>>,
    resolve: Resolve,
    wake: Wake,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFile {
    pub content: String,
    pub revision: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEdit {
    pub old_text: String,
    pub new_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditedSource {
    pub revision: String,
    pub snapshot: Vec<u8>,
}

fn source_revision(content: &str) -> String {
    format!("{:x}", Sha256::digest(content.as_bytes()))
}

pub fn read_source_file_versioned(doc: &LoroDoc, path: &str) -> Result<SourceFile, String> {
    let content = match doc.get_map("files").get(path) {
        Some(ValueOrContainer::Container(Container::Text(text))) => text.to_string(),
        Some(_) => return Err(format!("{path} is not a text source file")),
        None => return Err(format!("source file not found: {path}")),
    };
    Ok(SourceFile {
        revision: source_revision(&content),
        content,
    })
}

pub fn edit_source_file(
    doc: &LoroDoc,
    path: &str,
    expected_revision: &str,
    edits: &[SourceEdit],
) -> Result<EditedSource, String> {
    let file = read_source_file_versioned(doc, path)?;
    if file.revision != expected_revision {
        return Err(format!("stale source revision for {path}"));
    }

    let mut ranges = Vec::with_capacity(edits.len());
    for edit in edits {
        if edit.old_text.is_empty() {
            return Err("old_text must not be empty".to_string());
        }
        let mut matches = file.content.match_indices(&edit.old_text);
        let Some((start, _)) = matches.next() else {
            return Err("old_text was not found".to_string());
        };
        if matches.next().is_some() {
            return Err("old_text matched more than once".to_string());
        }
        ranges.push((start, start + edit.old_text.len(), &edit.new_text));
    }
    ranges.sort_by_key(|(start, _, _)| *start);
    if ranges.windows(2).any(|pair| pair[1].0 < pair[0].1) {
        return Err("source edits overlap".to_string());
    }

    let files = doc.get_map("files");
    let Some(ValueOrContainer::Container(Container::Text(text))) = files.get(path) else {
        return Err(format!("source file changed while editing: {path}"));
    };
    let mut changed = false;
    for (start, end, replacement) in ranges.into_iter().rev() {
        if &file.content[start..end] == replacement {
            continue;
        }
        let char_start = file.content[..start].chars().count();
        let char_len = file.content[start..end].chars().count();
        text.delete(char_start, char_len)
            .map_err(|e| e.to_string())?;
        text.insert(char_start, replacement)
            .map_err(|e| e.to_string())?;
        changed = true;
    }
    if changed {
        doc.commit();
    }
    let content = text.to_string();
    let snapshot = doc
        .export(ExportMode::Snapshot)
        .map_err(|e| e.to_string())?;
    Ok(EditedSource {
        revision: source_revision(&content),
        snapshot,
    })
}

pub fn write_source_file(doc: &LoroDoc, path: &str, content: &str) -> Result<Vec<u8>, String> {
    let files = doc.get_map("files");
    let text = match files.get(path) {
        Some(ValueOrContainer::Container(Container::Text(t))) => t,
        Some(_) => return Err(format!("{path} is not a text source file")),
        None => files
            .insert_container(path, LoroText::new())
            .map_err(|e| e.to_string())?,
    };
    text.delete(0, text.len_unicode())
        .map_err(|e| e.to_string())?;
    text.insert(0, content).map_err(|e| e.to_string())?;
    doc.commit();
    doc.export(ExportMode::Snapshot).map_err(|e| e.to_string())
}

impl<M: 'static> LuaApp<M> {
    pub fn open(
        src: LoroDoc,
        resolve: Resolve,
        wake: Wake,
        to_msg: Rc<dyn Fn(LuaMsg) -> M>,
    ) -> mlua::Result<Self> {
        let cores: Cores = Rc::new(RefCell::new(HashMap::new()));
        let src = Rc::new(Source::new(src, wake.clone()));
        let app = Self::build(src, cores, resolve, wake, to_msg)?;
        // A source that never loaded is rendered by every view; log it once, here.
        if let Some(e) = &app.error {
            app.log(e.clone());
        }
        Ok(app)
    }

    /// Everything that makes a VM, with the doc cores and the source watch handed in rather than
    /// created — so `open` starts with an empty set and `reload` starts with the running app's.
    ///
    /// A source that fails to compile is **not** an error here: it lands in `self.error` and
    /// `view()` renders it. `reload` is the caller that wants the opposite, and it checks.
    fn build(
        src: Rc<Source>,
        cores: Cores,
        resolve: Resolve,
        wake: Wake,
        to_msg: Rc<dyn Fn(LuaMsg) -> M>,
    ) -> mlua::Result<Self> {
        // Before reading a single file, so a write landing mid-build is still counted as unseen.
        // The other order marks this VM current for an edit it never read, and that edit is then
        // lost until the next one happens to arrive.
        let src_seen = src.version.load(Ordering::Relaxed);
        let (vm, fires) = sandboxed_vm()?;
        let map = src.doc.get_map("files");
        let Some(ValueOrContainer::Container(Container::Text(t))) = map.get("main.lua") else {
            return Err(Error::runtime("no main.lua in app source"));
        };
        let docs: Docs = Rc::new(RefCell::new(HashMap::new()));
        crdt::install(
            &vm,
            docs.clone(),
            cores.clone(),
            resolve.clone(),
            wake.clone(),
        )?;
        // Before `main.lua` runs, because its first line will be a `require`.
        modules::install(&vm, &src.doc)?;

        let main_src = t.to_string();
        let (view_fn, error) = match vm.load(main_src).set_name("main.lua").eval::<Function>() {
            Ok(f) => (Some(f), None),
            Err(e) => (None, Some(e.to_string())),
        };
        Ok(Self {
            vm,
            view_fn,
            handlers: RefCell::new(HashMap::new()),
            error,
            reload_error: None,
            fires,
            to_msg,
            src_seen,
            src,
            docs,
            cores,
            resolve,
            wake,
            console: Rc::new(RefCell::new(VecDeque::new())),
        })
    }

    /// Rebuild the VM from the current source, keeping the docs and as much per-viewer state as
    /// can cross a VM boundary. On any failure **nothing changes** and the running app is
    /// untouched.
    ///
    /// That guarantee is the reason this builds a whole second app rather than re-evaluating in
    /// place. Lua cannot unload a chunk: re-running `main.lua` here would leave every global the
    /// old version set that the new one does not, and every closure already in `handlers` would
    /// still point at the old upvalues. Building beside and swapping is also what makes the
    /// failure path free — the staged `Docs` map is dropped, and the cores it borrowed are still
    /// held by `self.cores`.
    ///
    /// The three stages are load, setup and **first render**, and the third is not optional: a
    /// `main.lua` that compiles and returns a closure which throws on its first call is the
    /// ordinary case, and without the trial frame we would have swapped before finding out.
    pub fn reload(&mut self) -> Result<(), String> {
        let mut staged = match Self::build(
            self.src.clone(),
            self.cores.clone(),
            self.resolve.clone(),
            self.wake.clone(),
            self.to_msg.clone(),
        ) {
            Ok(s) => s,
            Err(e) => {
                let e = e.to_string();
                self.log(format!("reload error: {e}"));
                return Err(e);
            }
        };
        if let Some(e) = &staged.error {
            self.log(format!("reload error: {e}"));
            return Err(e.clone());
        }
        let dropped = match carry_state(&self.vm, &staged.vm) {
            Ok(d) => d,
            Err(e) => {
                let e = e.to_string();
                self.log(format!("reload error: {e}"));
                return Err(e);
            }
        };
        let view_fn = staged.view_fn.as_ref().ok_or_else(|| {
            self.log("reload error: no view loaded".to_string());
            "no view loaded".to_string()
        })?;
        if let Err(e) = view_fn.call::<Table>(()) {
            let e = e.to_string();
            self.log(format!("reload error: {e}"));
            return Err(e);
        }
        // A real frame, not half of one. `_sweep` is what drops carried state belonging to an
        // element the new source no longer draws — skip it and that state lingers until whenever
        // the next frame happens to be.
        if let Ok(f) = staged.vm.globals().get::<Function>("_sweep") {
            if let Err(e) = f.call::<()>(()) {
                let e = e.to_string();
                self.log(format!("reload error: {e}"));
                return Err(e);
            }
        }
        // The trial frame filled the staged app's handler table with closures nothing will ever
        // dispatch to; clear it so the first real frame starts from an empty one.
        staged.handlers.borrow_mut().clear();
        // The console reports on VMs; it must not be reset by swapping to a new one.
        staged.console = self.console.clone();
        *self = staged;
        for name in dropped {
            let note =
                format!("reload: dropped ui.state({name:?}) — it holds a value tied to the old VM");
            eprintln!("{note}");
            self.log(note);
        }
        Ok(())
    }

    /// Reload if the source has moved since this VM was built; `None` if it had not.
    ///
    /// This is the caller [`reload`](Self::reload) was written for, and it is the one that keeps
    /// the books — which is what lets `reload`'s "on failure nothing changes" stay literally true.
    /// Two entries:
    ///
    /// The **watermark advances either way**. On success it comes from the staged app, which read
    /// it before building. On failure it is advanced here anyway, so a source that does not
    /// compile is retried once per *edit* rather than once per mouse move — and the next edit is
    /// the fix, which is exactly when a retry is worth anything.
    ///
    /// The **error is recorded**, because a failed reload that says nothing is the worst outcome
    /// there is: you change a file, the app keeps running the old code, and nothing anywhere
    /// connects the two.
    pub fn reload_if_stale(&mut self) -> Option<Result<(), String>> {
        let v = self.src.version.load(Ordering::Relaxed);
        if v <= self.src_seen {
            return None;
        }
        let r = self.reload();
        self.src_seen = self.src_seen.max(v);
        self.reload_error = r.as_ref().err().cloned();
        Some(r)
    }

    pub fn source_files(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .src
            .doc
            .get_map("files")
            .keys()
            .map(|k| k.to_string())
            .collect();
        out.sort();
        out
    }

    pub fn read_source_file(&self, path: &str) -> Option<String> {
        match self.src.doc.get_map("files").get(path) {
            Some(ValueOrContainer::Container(Container::Text(t))) => Some(t.to_string()),
            _ => None,
        }
    }

    pub fn read_source_file_versioned(&self, path: &str) -> Result<SourceFile, String> {
        read_source_file_versioned(&self.src.doc, path)
    }

    pub fn edit_source_file(
        &self,
        path: &str,
        expected_revision: &str,
        edits: &[SourceEdit],
    ) -> Result<EditedSource, String> {
        edit_source_file(&self.src.doc, path, expected_revision, edits)
    }

    pub fn write_source_file(&self, path: &str, content: &str) -> Result<Vec<u8>, String> {
        write_source_file(&self.src.doc, path, content)
    }

    pub fn source_snapshot(&self) -> Result<Vec<u8>, String> {
        self.src
            .doc
            .export(ExportMode::Snapshot)
            .map_err(|e| e.to_string())
    }

    /// Append one console line. A line identical to the current last one collapses — the
    /// per-frame paths (`view`, handlers) would otherwise flood the log on a persistent
    /// error — and the log is bounded: it is a console, not a history.
    fn log(&self, line: String) {
        let mut c = self.console.borrow_mut();
        if c.back() != Some(&line) {
            c.push_back(line);
        }
        while c.len() > 512 {
            c.pop_front();
        }
    }

    /// The last `last` console lines, newest last.
    pub fn console(&self, last: usize) -> Vec<String> {
        self.console
            .borrow()
            .iter()
            .rev()
            .take(last)
            .rev()
            .cloned()
            .collect()
    }

    /// Every open runtime-data doc as `{ name: deep JSON }`. Names sorted, so two dumps of
    /// the same state are byte-comparable. This is the *live* core state — writes the app
    /// has not flushed to the vault yet are already visible here. (loro's own `ToJson`
    /// trait is exactly `serde_json::to_value`, used directly here — one less import that
    /// lives behind an internal-crate re-export.)
    pub fn docs_json(&self) -> serde_json::Value {
        let cores = self.cores.borrow();
        let mut names: Vec<&String> = cores.keys().collect();
        names.sort();
        serde_json::Value::Object(
            names
                .into_iter()
                .map(|n| {
                    (
                        n.clone(),
                        serde_json::to_value(cores[n].doc.get_deep_value())
                            .unwrap_or(serde_json::Value::Null),
                    )
                })
                .collect(),
        )
    }

    /// Every currently open doc's name — what a host-side sync loop needs before it can reach
    /// any of them, since `Cores` itself stays private.
    pub fn open_doc_names(&self) -> Vec<String> {
        self.cores.borrow().keys().cloned().collect()
    }

    /// Reach one open doc's live `LoroDoc` by name, for a host-side concern (sync) that has to
    /// read or `import` it directly rather than through the Lua mirror. `None` if nothing has
    /// opened that name — the same "not open" a Lua `doc:open` would otherwise report.
    pub fn with_doc<R>(&self, name: &str, f: impl FnOnce(&LoroDoc) -> R) -> Option<R> {
        self.cores.borrow().get(name).map(|core| f(&core.doc))
    }

    pub fn view(&self) -> El<M> {
        //reset budget
        self.fires.store(0, Ordering::Relaxed);
        let body = self.body();
        let Some(e) = &self.reload_error else {
            return body;
        };
        // Above the app, not instead of it. What is on screen is still the last version that
        // worked, and it stays interactive — the banner only says that it is not what is on disk.
        col()
            .full()
            .child(err_box(&format!(
                "source changed but does not load — still running the last good version\n{e}"
            )))
            .child(body)
    }

    fn body(&self) -> El<M> {
        if let Some(e) = &self.error {
            return text(format!("reload error\n{e}"));
        }
        let Some(view_fn) = &self.view_fn else {
            return text("no view loaded");
        };
        {
            let mut docs = self.docs.borrow_mut();
            for e in docs.values_mut() {
                let v = e.core.version.load(Ordering::Relaxed);
                if v > e.mirrored {
                    if let Err(err) = patch_into(&self.vm, &e.mirror, &e.core.doc.get_deep_value())
                    {
                        return err_box(&format!("mirror: {err}"));
                    }
                    e.mirrored = v;
                }
            }
        }
        let tree = match view_fn.call::<Table>(()) {
            Ok(t) => t,
            Err(e) => {
                self.log(format!("View error: {e}"));
                return text(format!("View error: {e}"));
            }
        };
        if let Ok(f) = self.vm.globals().get::<Function>("_sweep") {
            let _ = f.call::<()>(());
        }
        let mut handlers = self.handlers.borrow_mut();
        handlers.clear();
        let mut context = Ctx::new(&mut handlers, self.to_msg.clone());
        let el = match walk(tree, &mut context) {
            Ok(el) => el,
            Err(e) => {
                let msg = reason(&e);
                context.errors.push(msg.clone());
                err_box(&msg)
            }
        };
        for e in &context.errors {
            eprintln!("{e}");
            self.log(e.clone());
        }
        el
    }
    /// A handler carrying more than one value is called with a single table, never positional
    /// arguments. Positionally, a short or mis-ordered signature binds the wrong values *and
    /// keeps running*: `shape` given `scale` arrives as the number 1, looks like a shape id, and
    /// fails every lookup in silence. A wrong key is `nil`, which is loud the moment it is used,
    /// and a field added later can never shift the meaning of one already there.
    fn event(&self, msg: LuaMsg) -> mlua::Result<Table> {
        let event = self.vm.create_table()?;
        match msg {
            LuaMsg::CallAt(_, x, y, shape, object) => {
                event.set("x", x)?;
                event.set("y", y)?;
                shape.write(&event)?;
                if let Some(hit) = object {
                    event.set("object", hit.id)?;
                    event.set("distance", hit.distance)?;
                    event.set("world_x", hit.point[0])?;
                    event.set("world_y", hit.point[1])?;
                    event.set("world_z", hit.point[2])?;
                    event.set("normal_x", hit.normal[0])?;
                    event.set("normal_y", hit.normal[1])?;
                    event.set("normal_z", hit.normal[2])?;
                }
            }
            LuaMsg::CallPhase(_, phase, x, y, shape) => {
                event.set("phase", phase)?;
                event.set("x", x)?;
                event.set("y", y)?;
                shape.write(&event)?;
            }
            LuaMsg::CallDrag(_, a) => {
                event.set("phase", a.phase)?;
                event.set("x", a.at.0)?;
                event.set("y", a.at.1)?;
                event.set("dx", a.delta.0)?;
                event.set("dy", a.delta.1)?;
                event.set("scale", a.scale)?;
                event.set("origin_x", a.origin.0)?;
                event.set("origin_y", a.origin.1)?;
                event.set("t", a.t)?;
                a.shape.write(&event)?;
            }
            LuaMsg::CallWheel(_, dx, dy) => {
                event.set("dx", dx)?;
                event.set("dy", dy)?;
            }
            LuaMsg::CallFrame(_, dt, elapsed) => {
                event.set("dt", dt)?;
                event.set("elapsed", elapsed)?;
            }
            LuaMsg::CallKey(_, input) => {
                event.set("cancelled", input.cancelled)?;
                if !input.cancelled {
                    if let Some(code) = input.code { event.set("code", code)?; }
                    if !input.key.is_empty() { event.set("key", input.key)?; }
                    event.set("down", input.down)?;
                    event.set("repeated", input.repeat)?;
                    event.set("shift", input.mods.shift)?;
                    event.set("ctrl", input.mods.ctrl)?;
                    event.set("alt", input.mods.alt)?;
                    event.set("super", input.mods.super_)?;
                }
            }
            LuaMsg::Call(_) | LuaMsg::CallStr(_, _) => {}
        }
        Ok(event)
    }

    pub fn update(&mut self, msg: LuaMsg) {
        // reset budget
        self.fires.store(0, Ordering::Relaxed);
        let handlers = self.handlers.borrow();

        // A message can outlive the view that registered its key (a click spans press to
        // release), so a key with no handler now means the element is gone — drop it.
        let key = match &msg {
            LuaMsg::Call(k) | LuaMsg::CallAt(k, _, _, _, _) | LuaMsg::CallStr(k, _) => k,
            LuaMsg::CallPhase(k, _, _, _, _)
            | LuaMsg::CallDrag(k, _)
            | LuaMsg::CallWheel(k, _, _)
            | LuaMsg::CallFrame(k, _, _)
            | LuaMsg::CallKey(k, _) => k,
        };
        let Some(h) = handlers.get(key) else {
            return;
        };
        let result = match msg {
            // Nothing to mis-order: no arguments, and one string that can only be itself.
            LuaMsg::Call(_) => h.call::<()>(()),
            LuaMsg::CallStr(_, s) => h.call::<()>(s),
            carried => match self.event(carried) {
                Ok(event) => h.call::<()>(event),
                Err(e) => Err(e),
            },
        };
        if let Err(e) = result {
            eprintln!("handler error: {e}");
            self.log(format!("handler error: {e}"));
        }
    }

    /// Save every doc whose version has moved since its last successful save. `put` is the
    /// vault write — `(doc name, snapshot bytes)`.
    ///
    /// The shell calls this **after** `update`, not from inside it: MCP and peer writes never
    /// pass through `update` at all, and one call site has to cover all three writers.
    ///
    /// A failed `put` leaves `saved` where it was, so the next flush retries. Advancing it
    /// first would present as "my card vanished after a restart", which is unfindable.
    pub fn flush(
        &mut self,
        mut put: impl FnMut(&str, &[u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        // Over the *cores*, not the current VM's open docs. A doc the previous source opened and
        // the new one does not still holds unflushed writes, and iterating the mirrors would
        // silently stop saving it the moment a reload dropped it from the view.
        for (name, core) in self.cores.borrow().iter() {
            // Read the counter *before* exporting: a write landing mid-flush then stays dirty
            // rather than being marked saved by a snapshot taken before it.
            let v = core.version.load(Ordering::Relaxed);
            if v <= core.saved.get() {
                continue;
            }
            let bytes = core
                .doc
                .export(ExportMode::Snapshot)
                .map_err(|err| err.to_string())?;
            put(name, &bytes)?;
            core.saved.set(v);
        }
        Ok(())
    }
}

/// A `ui.state` entry reduced to data. What *cannot* be represented here — a function, a thread,
/// userdata — is exactly what makes an entry uncarryable.
enum Plain {
    Nil,
    Bool(bool),
    Int(i64),
    Num(f64),
    Str(String),
    Table(Vec<(Plain, Plain)>),
}

/// How deep a `ui.state` entry may nest before we give up. This is not a size limit — it is the
/// cycle guard. A table that contains itself would otherwise recurse forever, and returning
/// `None` funnels it into the same "dropped, and said so" path as a coroutine.
const MAX_DEPTH: u32 = 16;

/// Carry `ui.state` across a rebuild, returning the names of the entries that could not come.
///
/// A **filter, not a copy**, and the difference is the point. `_state` holds per-viewer scratch —
/// an unsent draft, whether a panel is open — but it is also where retained *execution* state
/// lands once `ui.run` exists, and a suspended coroutine is a live stack of closures belonging to
/// a chunk that no longer exists. No mechanism can move that to another VM: not serialization,
/// not a Rust-side mirror.
///
/// Dropping it is correct rather than a compromise — an animation whose code just changed should
/// restart, which is the same rule that makes `_sweep` kill an animation when its element leaves
/// the tree. What is not acceptable is doing it *silently*, so the names come back to the caller.
///
/// `_live` is deliberately not carried. The trial frame runs after this, marks whatever the new
/// source actually touches, and `_sweep` drops the rest — so state belonging to an element the
/// new code no longer draws is cleaned up on the way in, for free.
fn carry_state(from: &Lua, to: &Lua) -> mlua::Result<Vec<String>> {
    let old: Table = from.globals().get::<Function>("_dump_state")?.call(())?;
    let new = to.create_table()?;
    let mut dropped = Vec::new();
    for pair in old.pairs::<Value, Value>() {
        let (k, v) = pair?;
        match (plain(&k, 0), plain(&v, 0)) {
            (Some(k), Some(v)) => new.set(into_lua(to, &k)?, into_lua(to, &v)?)?,
            _ => dropped.push(match &k {
                Value::String(s) => s.to_string_lossy(),
                other => format!("{other:?}"),
            }),
        }
    }
    to.globals()
        .get::<Function>("_load_state")?
        .call::<()>(new)?;
    Ok(dropped)
}

/// `None` for anything carrying VM identity, and for anything nested past [`MAX_DEPTH`].
fn plain(v: &Value, depth: u32) -> Option<Plain> {
    if depth > MAX_DEPTH {
        return None;
    }
    match v {
        Value::Nil => Some(Plain::Nil),
        Value::Boolean(b) => Some(Plain::Bool(*b)),
        // Luau integers are i32, so this widens rather than truncating.
        Value::Integer(i) => Some(Plain::Int((*i).into())),
        Value::Number(n) => Some(Plain::Num(*n)),
        Value::String(s) => Some(Plain::Str(s.to_str().ok()?.to_owned())),
        Value::Table(t) => {
            // `pairs` is raw, so a metatable does not come along — which is right: the only
            // metatables in reach here are the mirror's and the container tags, and neither
            // belongs to per-viewer scratch.
            let mut out = Vec::new();
            for pair in t.pairs::<Value, Value>() {
                let (k, v) = pair.ok()?;
                out.push((plain(&k, depth + 1)?, plain(&v, depth + 1)?));
            }
            Some(Plain::Table(out))
        }
        _ => None,
    }
}

fn into_lua(lua: &Lua, p: &Plain) -> mlua::Result<Value> {
    Ok(match p {
        Plain::Nil => Value::Nil,
        Plain::Bool(b) => Value::Boolean(*b),
        Plain::Int(i) => (*i).into_lua(lua)?,
        Plain::Num(n) => Value::Number(*n),
        Plain::Str(s) => Value::String(lua.create_string(s)?),
        Plain::Table(entries) => {
            let t = lua.create_table()?;
            for (k, v) in entries {
                t.set(into_lua(lua, k)?, into_lua(lua, v)?)?;
            }
            Value::Table(t)
        }
    })
}

const PRELUDE: &str = r#"
local _state,_live = {},{}
local function tagger(tag)
    return function(t)
        t.tag = tag
        t.line = debug.info(2, "l")
        return t
    end
end


function _sweep()
    for id in pairs(_state) do
    if not _live[id] then _state[id] = nil end
    end
    _live={}
end

-- The host's seam onto `ui.state`, used only by `reload` to carry per-viewer scratch across a
-- VM rebuild. `_state` is a local so an app cannot replace the table wholesale; these two are
-- the same deliberate exception `_sweep` already is.
function _dump_state() return _state end
function _load_state(t) _state = t end

ui = {
    col = tagger("col"),
    row = tagger("row"),
    text = tagger("text"),
    button = tagger("button"),
    input = tagger("input"),
    text_area = tagger("text_area"),
    frame = tagger("frame"),
    scene3d = tagger("scene3d"),
    overlay = tagger("overlay"),
}

function ui.state(id, init) 
    _live[id] = true
    local s = _state[id]
    if s== nil then
    s = init or {}
    _state[id] = s
    end
    return s
end
"#;

/// Seconds since the Unix epoch. The clock for recording *when* — it can step backwards, so it
/// is never the one to measure a duration with.
fn wall_clock() -> mlua::Result<f64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .map_err(mlua::Error::external)
}

/// Luau ships an `os` table we never asked for, and `os.clock` in it is a real monotonic clock
/// (`clock_gettime(CLOCK_MONOTONIC)`). An app reaching for it would bypass the runtime's clock
/// entirely: offscreen that clock is virtual, so a gesture timed with `os.clock` measures real
/// elapsed time instead and comes out different on every machine. Its epoch is the machine's,
/// unrelated to `e.t` and `e.elapsed`, so the two cannot even be compared.
///
/// This must run before `sandbox(true)` freezes the globals — which is also what makes it hold,
/// since a frozen `os` is one an app cannot put back.
fn shadow_os(vm: &Lua) -> mlua::Result<()> {
    let os: Table = vm.globals().get("os")?;
    os.set(
        "clock",
        vm.create_function(|_, _: mlua::MultiValue| -> mlua::Result<f64> {
            Err(Error::runtime(
                "os.clock is the machine's clock, not the app's: use e.t on a pointer event, or \
                 e.elapsed in on_frame",
            ))
        })?,
    )?;
    // Whole seconds, per Lua. Routed through our own clock so the sandbox has one wall clock
    // rather than two that can disagree by a leap second or a mid-call NTP step.
    os.set(
        "time",
        vm.create_function(|_, _: mlua::MultiValue| Ok(wall_clock()?.floor()))?,
    )?;
    // Bare `os.date()` formats real time in the machine's locale and zone. An explicit-timestamp
    // form is defensible and can come back when an app actually wants one.
    os.set(
        "date",
        vm.create_function(|_, _: mlua::MultiValue| -> mlua::Result<String> {
            Err(Error::runtime(
                "os.date reads real time and the machine's locale; format from now() instead",
            ))
        })?,
    )?;
    // `os.difftime` is arithmetic on numbers the caller supplies and reads no clock — left alone.
    Ok(())
}

pub fn sandboxed_vm() -> mlua::Result<(Lua, Arc<AtomicU64>)> {
    let vm = Lua::new();
    let now_fn = vm.create_function(|_, ()| wall_clock())?;
    let uuid_fn = vm.create_function(|_, ()| Ok(uuid::Uuid::new_v4().to_string()))?;
    vm.load(PRELUDE).exec()?;
    gfx::install(&vm)?;
    vm.globals().set("now", now_fn)?;
    vm.globals().set("uuid", uuid_fn)?;
    shadow_os(&vm)?;
    let _ = vm.sandbox(true)?;
    let fires = Arc::new(AtomicU64::new(0));
    let f = fires.clone();
    vm.set_interrupt(move |_lua| {
        if f.fetch_add(1, Ordering::Relaxed) > 1_000_000 {
            Err(mlua::Error::runtime("interrupt budget exceeded"))
        } else {
            Ok(mlua::VmState::Continue)
        }
    });
    Ok((vm, fires))
}
fn fail<M>(context: &mut Ctx<M>, msg: String) -> El<M> {
    let msg = if context.path.is_empty() {
        msg
    } else {
        format!("{} > {msg}", context.path)
    };
    context.errors.push(msg.clone());
    err_box(&msg)
}

/// The text an error card shows. Every error the host itself mints — in `build`, `children`,
/// `props` — is an [`mlua::Error::Runtime`], whose `Display` prepends `runtime error: `. That
/// prefix is mlua's plumbing, not something an app author can act on, so it comes off here.
/// Anything else (a Lua-raised error, carrying its `file:line`) passes through untouched.
fn reason(e: &Error) -> String {
    match e {
        Error::RuntimeError(msg) => msg.clone(),
        other => other.to_string(),
    }
}
fn children<M: 'static>(
    mut el: El<M>,
    node: &Table,
    context: &mut Ctx<M>,
    tag: &str,
) -> mlua::Result<El<M>> {
    for i in 1..=max_index(node) {
        let mark = context.path.len();
        if context.dev {
            if !context.path.is_empty() {
                context.path.push_str(" > ");
            }
            context.path.push_str(&format!("[{i}]"));
        }
        let child = node.get::<Value>(i)?;
        match child {
            Value::Boolean(false) => {}
            Value::String(s) => {
                el = el.child(text(s.to_str()?.to_owned()));
            }
            Value::Table(t) => {
                if t.contains_key("tag")? {
                    el = el.child(match walk(t, context) {
                        Ok(c) => c,
                        Err(e) => fail(context, reason(&e)),
                    });
                    context.path.truncate(mark);
                } else {
                    if max_index(&t) == 0 && t.pairs::<Value, Value>().count() > 0 {
                        el = el.child(fail(
                            context,
                            format!(
                                "plain table, not an element — missing `ui.col{{...}}`? has: {}",
                                keys(&t)
                            ),
                        ));
                    }
                    el = children(el, &t, context, tag)?;
                }
            }
            Value::Nil => {
                el = el.child(fail(
                    context,
                    "is nil — a helper that forgot to `return`?".into(),
                ))
            }
            Value::Boolean(true) => {
                el = el.child(fail(
                    context,
                    "is `true` — did you write `el and cond` backwards?".into(),
                ))
            }
            other => {
                el = el.child(fail(
                    context,
                    format!("must be an element, string or false, got {}", show(&other)),
                ))
            }
        }
        context.path.truncate(mark);
    }
    Ok(el)
}
fn err_box<M>(msg: &str) -> El<M> {
    let red = Color::from_rgba8(0xEF, 0x44, 0x44, 0xFF);
    let mut el = col()
        .pad(8.0)
        .gap(2.0)
        .radius(4.0)
        .stroke(1.0, red)
        .fill(Color::from_rgba8(0x2A, 0x11, 0x11, 0xFF));
    for chunk in msg.as_bytes().chunks(64) {
        el = el.child(
            text(String::from_utf8_lossy(chunk).into_owned())
                .color(red)
                .font_size(11.0),
        );
    }
    el
}

fn walk<M: 'static>(node: Table, context: &mut Ctx<M>) -> mlua::Result<El<M>> {
    let tag: String = node.get("tag")?;
    // The breadcrumb is dev-only scaffolding, and it isn't cheap: a boundary get for
    // `line`, another for `id`, a `format!`, and a `push_str` — per element, per frame.
    // `walk` is ~80% of a frame's Lua cost (see the `cost_curve` test), so this stays off
    // unless someone is going to read it. With it off, `fail`'s message loses its path.
    let mark = context.path.len();
    if context.dev {
        let line: String = node.get("line")?;
        let seg = match node.get::<Option<String>>("id")? {
            Some(id) => format!("{tag}#{id}:[{line}]"),
            None => format!("{tag}:[{line}]"),
        };
        if !context.path.is_empty() {
            context.path.push_str(" > ");
        }
        context.path.push_str(&seg);
    }

    let r = build(node, context, &tag);
    if r.is_ok() {
        context.path.truncate(mark);
    }
    r
}

fn show(v: &Value) -> String {
    match v {
        Value::String(s) => format!("{:?}", s.to_string_lossy()),
        Value::Table(t) => match t.get::<Option<String>>("tag") {
            Ok(Some(tag)) => format!("<{tag}>"),
            _ => "{...}".into(),
        },

        Value::Function(_) => "function".into(),
        other => other
            .to_string()
            .unwrap_or_else(|_| other.type_name().into()),
    }
}

// Error cards only, never the hot walk — so it can sort where `pairs` promises no
// order: children ascending, then names alphabetical and bare (quotes read as typos in
// a key list), then any other key by its display form. `tag`/`line` stay hidden.
fn keys(node: &Table) -> String {
    let mut ints: Vec<(mlua::Integer, Value)> = Vec::new();
    let mut names: Vec<(String, Value)> = Vec::new();
    let mut rest: Vec<(String, Value)> = Vec::new();
    for pair in node.pairs::<Value, Value>() {
        let Ok((k, v)) = pair else { continue };
        match k {
            Value::Integer(i) if i > 0 => ints.push((i, v)),
            Value::String(s) if s == "tag" || s == "line" => {}
            Value::String(s) => names.push((s.to_string_lossy(), v)),
            k => rest.push((show(&k), v)),
        }
    }
    ints.sort_by_key(|&(i, _)| i);
    names.sort_by(|(a, _), (b, _)| a.cmp(b));
    rest.sort_by(|(a, _), (b, _)| a.cmp(b));
    ints.into_iter()
        .map(|(i, v)| format!("[{i}] = {}", show(&v)))
        .chain(
            names
                .into_iter()
                .map(|(name, v)| format!("{name}={}", show(&v))),
        )
        .chain(
            rest.into_iter()
                .map(|(shown, v)| format!("{shown}={}", show(&v))),
        )
        .collect::<Vec<_>>()
        .join(",")
}

fn build_overlay<M: 'static>(node: Table, context: &mut Ctx<M>) -> mlua::Result<El<M>> {
    if max_index(&node) != 2 {
        return Err(mlua::Error::runtime(
            "overlay needs exactly two children: anchor, then panel",
        ));
    }
    for pair in node.pairs::<Value, Value>() {
        let (key, _) = pair?;
        let allowed = match key {
            Value::Integer(1 | 2) => true,
            Value::String(ref key) => matches!(
                key.to_str()?.as_ref(),
                "tag" | "line" | "id" | "side" | "align" | "on_dismiss"
            ),
            _ => false,
        };
        if !allowed {
            let key = match key {
                Value::String(key) => key.to_string_lossy(),
                other => show(&other),
            };
            return Err(mlua::Error::runtime(format!(
                "overlay: unknown field {key}"
            )));
        }
    }
    let anchor = walk(node.get::<Table>(1)?, context)?;
    let panel = walk(node.get::<Table>(2)?, context)?;
    let side = match node.get::<Option<String>>("side")?.as_deref() {
        None | Some("bottom") => PlacementSide::Bottom,
        Some("top") => PlacementSide::Top,
        Some("left") => PlacementSide::Left,
        Some("right") => PlacementSide::Right,
        Some(v) => return Err(mlua::Error::runtime(format!("unknown overlay side {v}"))),
    };
    let align = match node.get::<Option<String>>("align")?.as_deref() {
        None | Some("start") => PlacementAlign::Start,
        Some("center") => PlacementAlign::Center,
        Some("end") => PlacementAlign::End,
        Some(v) => return Err(mlua::Error::runtime(format!("unknown overlay align {v}"))),
    };
    let dismiss = match node.get::<Option<Function>>("on_dismiss")? {
        Some(handler) => {
            let id = node
                .get::<Option<String>>("id")?
                .ok_or_else(|| mlua::Error::runtime("on_dismiss needs an id"))?;
            let key = register(context.handlers, &id, "on_dismiss", handler)?;
            Some((context.to_msg)(LuaMsg::Call(key)))
        }
        None => None,
    };
    Ok(anchor.overlay(panel, dismiss, Placement { side, align }, Anchor::Element))
}

fn build<M: 'static>(node: Table, context: &mut Ctx<M>, tag: &str) -> mlua::Result<El<M>> {
    if tag == "overlay" {
        return build_overlay(node, context);
    }
    let mut el = match tag {
        "col" | "row" | "button" => {
            let el = if tag == "col" { col() } else { row() };
            children(el, &node, context, &tag)?
        }
        "text" => match node.get::<Value>(1)? {
            Value::String(s) => text(s.to_str()?.to_owned()),
            other => {
                return Err(mlua::Error::runtime(format!(
                    "needs its label as child 1, got  {} - has {}",
                    other.type_name(),
                    keys(&node)
                )));
            }
        },
        "frame" => {
            if max_index(&node) > 0 {
                return Err(mlua::Error::runtime("frame takes no children"));
            }
            let visual = node
                .get::<AnyUserData>("visual")?
                .borrow::<gfx::LuaFrame>()?
                .0
                .clone();
            frame_el(visual)
        }
        "scene3d" => {
            if max_index(&node) > 0 {
                return Err(mlua::Error::runtime("scene3d takes no children"));
            }
            let scene = node
                .get::<AnyUserData>("scene")?
                .borrow::<gfx::LuaScene3d>()?
                .0
                .clone();
            scene3d_el(scene)
        }
        "input" | "text_area" => {
            if max_index(&node) > 0 {
                return Err(mlua::Error::runtime(format!(
                    "takes no children — put the button beside it, not inside. got: {}",
                    keys(&node)
                )));
            }
            // Three mandatory props, one boundary read each: `nil` names the tag, a wrong
            // type keeps its conversion error. Read order decides which card surfaces.
            let value = node
                .get::<Option<String>>("value")?
                .ok_or_else(|| mlua::Error::runtime(format!("{tag} needs a value")))?;
            let id = node
                .get::<Option<String>>("id")?
                .ok_or_else(|| mlua::Error::runtime(format!("{tag} needs an id")))?;
            let f = node
                .get::<Option<mlua::Function>>("on_input")?
                .ok_or_else(|| mlua::Error::runtime(format!("{tag} needs on_input")))?;
            let key = register(context.handlers, &id, "on_input", f)?;
            let to_msg = context.to_msg.clone();
            let map = move |s| to_msg(LuaMsg::CallStr(key.clone(), s));
            if tag == "input" {
                text_input(value, id, map)
            } else {
                text_area(value, id, map)
            }
        }

        // Raised rather than carded here: the boundary in `children` owns the record, the
        // breadcrumb and the sibling-alive card — a card built this deep skips all three.
        other => return Err(mlua::Error::runtime(format!("unknown tag {other}"))),
    };
    let id: Option<String> = node.get("id")?;
    if let Some(s) = &id {
        el = el.id(s.as_str());
    }

    // Scroll offset is stored per element id, so a scroller without one silently never scrolls.
    if id.is_none() && (node.contains_key("scroll_x")? || node.contains_key("scroll_y")?) {
        return Err(mlua::Error::runtime(format!("{tag}: scroll needs an id")));
    }
    let consumed: &[&str] = match tag {
        "frame" => &["visual"],
        "scene3d" => &["scene"],
        _ => &[],
    };
    el = props::apply(el, &node, context, id.as_deref(), consumed)?;
    Ok(el)
}

pub fn parse_color(s: &str) -> mlua::Result<Color> {
    let c = csscolorparser::parse(s).map_err(mlua::Error::external)?;
    let [r, g, b, a] = c.to_rgba8();
    Ok(Color::from_rgba8(r, g, b, a))
}
fn max_index(node: &Table) -> usize {
    let mut max = 0;
    for pair in node.pairs::<Value, Value>() {
        if let Ok((Value::Integer(i), _)) = pair {
            if i > 0 {
                max = max.max(i as usize);
            }
        }
    }
    max
}
#[cfg(test)]
mod tests;
