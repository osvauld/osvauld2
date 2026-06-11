use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use code_editor::{BlockDoc, BlockId};
use doc_editor::{BlockKind, Doc, TreeID};
use eframe::egui;
use loro::{ExportMode, LoroDoc};
use osvauld_rpc::{BlockSummary, ItemSummary, MarkSpan, Request, Response, WorkspaceSummary};
use serde_json::json;
use vault::{ItemKind, Vault, WorkspaceItem};

pub fn socket_path() -> String {
    std::env::var("OSVAULD_SOCKET").unwrap_or_else(|_| "/tmp/osvauld.sock".to_string())
}

/// Sent to the UI thread when the bridge writes an item that may be open in a tab, so the live
/// tab re-reads the durable state an external client (MCP) just changed.
pub enum Refresh {
    /// A .doc's block tree changed; merge `snapshot` into the open editor.
    Doc { ws_id: String, item_id: String, snapshot: Vec<u8> },
    /// An .app's source files changed; reload the engine from the vault.
    App { ws_id: String, item_id: String },
    /// One `.lua` file changed block-wise (a per-block MCP edit); merge `snapshot` into the open
    /// editor if it's the file showing, preserving the human's live edits + caret.
    Lua { ws_id: String, item_id: String, path: String, snapshot: Vec<u8> },
    /// A workspace's item list changed (e.g. the bridge created an item); the open
    /// workspace tab must re-read its items so the new one becomes visible.
    Workspace { ws_id: String },
    /// An .app's runtime data CRDT changed (an MCP app-data write); merge `snapshot` into the
    /// live engine so the open run pane updates this frame.
    AppData { ws_id: String, item_id: String, snapshot: Vec<u8> },
}

pub fn start(path: impl AsRef<Path>, vault: Vault, ctx: egui::Context) -> Receiver<Refresh> {
    let path = path.as_ref().to_owned();
    let (refresh_tx, refresh_rx) = mpsc::channel::<Refresh>();
    thread::spawn(move || {
        let _ = std::fs::remove_file(&path);
        let listener = match UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => { eprintln!("bridge: bind failed: {e}"); return; }
        };
        for stream in listener.incoming() {
            let mut stream = match stream { Ok(s) => s, Err(_) => continue };
            let bytes = match osvauld_rpc::read_msg(&mut stream) { Ok(b) => b, Err(_) => continue };
            let req: Request = match serde_json::from_slice(&bytes) {
                Ok(r) => r,
                Err(e) => {
                    let resp = Response::err(e.to_string());
                    let _ = osvauld_rpc::write_msg(&mut stream, &serde_json::to_vec(&resp).unwrap());
                    continue;
                }
            };
            let resp = handle(&vault, &refresh_tx, req);
            let _ = osvauld_rpc::write_msg(&mut stream, &serde_json::to_vec(&resp).unwrap());
            // The bridge thread can't draw; wake the UI thread so a write made by an
            // external client (MCP) is merged into open tabs on the next frame.
            ctx.request_repaint();
        }
    });
    refresh_rx
}

fn handle(vault: &Vault, refresh_tx: &Sender<Refresh>, req: Request) -> Response {
    match req {
        Request::ListWorkspaces => {
            let list: Vec<WorkspaceSummary> = vault
                .workspaces()
                .unwrap_or_default()
                .into_iter()
                .map(|w| WorkspaceSummary { id: w.id, name: w.name })
                .collect();
            Response::ok(list)
        }
        Request::ListItems { ws_id } => {
            let list: Vec<ItemSummary> = vault
                .items(&ws_id)
                .unwrap_or_default()
                .into_iter()
                .map(|i| ItemSummary {
                    id: i.id,
                    ws_id: i.ws_id,
                    name: i.name,
                    kind: i.kind.as_str().to_string(),
                })
                .collect();
            Response::ok(list)
        }
        Request::ReadDoc { ws_id, item_id } => {
            match load_doc(vault, &ws_id, &item_id) {
                Some(doc) => Response::ok(doc_to_summaries(&doc)),
                None => Response::err("doc not found"),
            }
        }
        Request::SetBlockText { ws_id, item_id, block, text } => {
            mutate_doc(vault, refresh_tx, &ws_id, &item_id, |doc| {
                let id = parse_block(&block)?;
                doc.set_block_text(id, &text);
                Ok(serde_json::Value::Null)
            })
        }
        Request::InsertBlock { ws_id, item_id, after, kind, text } => {
            mutate_doc(vault, refresh_tx, &ws_id, &item_id, |doc| {
                let kind = parse_block_kind(&kind)?;
                let new_id = match after {
                    Some(s) => doc.insert_after(parse_block(&s)?, kind, &text),
                    // Append: after the last existing block, or seed one if (somehow) empty.
                    None => match doc.blocks().last() {
                        Some(&(last, _)) => doc.insert_after(last, kind, &text),
                        None => doc.create_block(0, kind, &text),
                    },
                };
                Ok(json!({ "id": new_id.to_string() }))
            })
        }
        Request::SetBlockKind { ws_id, item_id, block, kind } => {
            mutate_doc(vault, refresh_tx, &ws_id, &item_id, |doc| {
                doc.set_kind(parse_block(&block)?, parse_block_kind(&kind)?);
                Ok(serde_json::Value::Null)
            })
        }
        Request::DeleteBlock { ws_id, item_id, block } => {
            mutate_doc(vault, refresh_tx, &ws_id, &item_id, |doc| {
                doc.delete_block(parse_block(&block)?);
                Ok(serde_json::Value::Null)
            })
        }
        Request::IndentBlock { ws_id, item_id, block } => {
            mutate_doc(vault, refresh_tx, &ws_id, &item_id, |doc| {
                Ok(json!({ "moved": doc.indent(parse_block(&block)?) }))
            })
        }
        Request::OutdentBlock { ws_id, item_id, block } => {
            mutate_doc(vault, refresh_tx, &ws_id, &item_id, |doc| {
                Ok(json!({ "moved": doc.outdent(parse_block(&block)?) }))
            })
        }
        Request::MoveBlock { ws_id, item_id, block, position, target } => {
            mutate_doc(vault, refresh_tx, &ws_id, &item_id, |doc| {
                let (b, t) = (parse_block(&block)?, parse_block(&target)?);
                let moved = match position.as_str() {
                    "before" => doc.move_before(b, t),
                    "after" => doc.move_after(b, t),
                    "into" => doc.move_into(b, t),
                    other => return Err(format!("invalid position '{other}' (want before|after|into)")),
                };
                if moved { Ok(serde_json::Value::Null) } else { Err("move rejected (target is the block itself or a descendant)".into()) }
            })
        }
        Request::SetTodoDone { ws_id, item_id, block, done } => {
            mutate_doc(vault, refresh_tx, &ws_id, &item_id, |doc| {
                doc.set_done(parse_block(&block)?, done);
                Ok(serde_json::Value::Null)
            })
        }
        Request::SetCodeLang { ws_id, item_id, block, lang } => {
            mutate_doc(vault, refresh_tx, &ws_id, &item_id, |doc| {
                doc.set_lang(parse_block(&block)?, &lang);
                Ok(serde_json::Value::Null)
            })
        }
        Request::ApplyMark { ws_id, item_id, block, start, end, mark } => {
            mutate_doc(vault, refresh_tx, &ws_id, &item_id, |doc| {
                let id = parse_block(&block)?;
                if !matches!(mark.as_str(), "bold" | "italic" | "strike" | "code") {
                    return Err(format!("invalid mark '{mark}' (want bold|italic|strike|code; use apply_link for links)"));
                }
                let (start, end) = clamp_range(doc, id, start, end);
                doc.mark(id, start, end, &mark);
                Ok(serde_json::Value::Null)
            })
        }
        Request::ApplyLink { ws_id, item_id, block, start, end, url } => {
            mutate_doc(vault, refresh_tx, &ws_id, &item_id, |doc| {
                let id = parse_block(&block)?;
                let (start, end) = clamp_range(doc, id, start, end);
                doc.mark_link(id, start, end, &url);
                Ok(serde_json::Value::Null)
            })
        }
        Request::ClearMark { ws_id, item_id, block, start, end, mark } => {
            mutate_doc(vault, refresh_tx, &ws_id, &item_id, |doc| {
                let id = parse_block(&block)?;
                if !matches!(mark.as_str(), "bold" | "italic" | "strike" | "code" | "link") {
                    return Err(format!("invalid mark '{mark}' (want bold|italic|strike|code|link)"));
                }
                let (start, end) = clamp_range(doc, id, start, end);
                doc.unmark(id, start, end, &mark);
                Ok(serde_json::Value::Null)
            })
        }
        Request::CreateItem { ws_id, name, kind } => {
            let kind = match parse_kind(&kind) {
                Some(k) => k,
                None => return Response::err(format!("unknown item kind '{kind}'")),
            };
            match vault.create_item(&ws_id, &name, kind) {
                Ok(item) => {
                    seed_item_state(vault, &item);
                    let summary = ItemSummary {
                        id: item.id,
                        ws_id: item.ws_id,
                        name: item.name,
                        kind: item.kind.as_str().to_string(),
                    };
                    // Make the new item show up in the open workspace tab without a manual reload.
                    let _ = refresh_tx.send(Refresh::Workspace { ws_id });
                    Response::ok(summary)
                }
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::ListFiles { ws_id, item_id } => match vault.list_files(&ws_id, &item_id) {
            Ok(paths) => Response::ok(paths),
            Err(e) => Response::err(e.to_string()),
        },
        Request::ReadFile { ws_id, item_id, path } => match vault.get_file(&ws_id, &item_id, &path) {
            // `.lua` files are stored as block snapshots; `decode` emits them back to source.
            Ok(Some(bytes)) => Response::ok(json!({ "content": code_editor::store::decode(&path, &bytes) })),
            Ok(None) => Response::err(format!("no such file '{path}'")),
            Err(e) => Response::err(e.to_string()),
        },
        Request::WriteFile { ws_id, item_id, path, content } => {
            // Whole-file write: `.lua` source is split into blocks and stored as a snapshot.
            let bytes = code_editor::store::encode(&path, &content);
            match vault.put_file(&ws_id, &item_id, &path, &bytes) {
                Ok(()) => {
                    // Reload any open app tab against the new source.
                    let _ = refresh_tx.send(Refresh::App { ws_id, item_id });
                    Response::ok(serde_json::Value::Null)
                }
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::ReadFileBlocks { ws_id, item_id, path } => match load_lua(vault, &ws_id, &item_id, &path) {
            Ok(doc) => Response::ok(lua_to_summaries(&doc)),
            Err(e) => Response::err(e),
        },
        Request::SetFileBlockText { ws_id, item_id, path, block, text } => {
            mutate_lua(vault, refresh_tx, &ws_id, &item_id, &path, |doc| {
                let id = parse_lua_block(doc, &block)?;
                doc.set_block_text(id, &text);
                Ok(serde_json::Value::Null)
            })
        }
        Request::InsertFileBlock { ws_id, item_id, path, after, kind, text } => {
            mutate_lua(vault, refresh_tx, &ws_id, &item_id, &path, |doc| {
                let kind = if kind.is_empty() { "statement" } else { &kind };
                let new_id = match after {
                    Some(s) => doc.insert_after(parse_lua_block(doc, &s)?, kind, &text),
                    None => doc.push(kind, &text),
                };
                Ok(json!({ "id": new_id.to_string() }))
            })
        }
        Request::DeleteFileBlock { ws_id, item_id, path, block } => {
            mutate_lua(vault, refresh_tx, &ws_id, &item_id, &path, |doc| {
                doc.delete_block(parse_lua_block(doc, &block)?);
                Ok(serde_json::Value::Null)
            })
        }
        Request::AppDataGet { ws_id, item_id } => {
            let doc = load_app_state(vault, &ws_id, &item_id);
            match serde_json::to_value(doc.get_deep_value()) {
                Ok(v) => Response::ok(v),
                Err(e) => Response::err(e.to_string()),
            }
        }
        Request::AppDataSetText { ws_id, item_id, name, text } => {
            // Whole-container replace, the same op the Lua binding's `text:set` makes; since the
            // data is a CRDT the open run pane merges it like any peer edit.
            let doc = load_app_state(vault, &ws_id, &item_id);
            let t = doc.get_text(name.as_str());
            let len = t.len_unicode();
            let res: Result<(), loro::LoroError> = (|| {
                if len > 0 {
                    t.delete(0, len)?;
                }
                t.insert(0, &text)
            })();
            if let Err(e) = res {
                return Response::err(e.to_string());
            }
            doc.commit();
            let snapshot = match doc.export(ExportMode::Snapshot) {
                Ok(s) => s,
                Err(e) => return Response::err(e.to_string()),
            };
            if let Err(e) = vault.put_state(&ws_id, &item_id, &snapshot) {
                return Response::err(e.to_string());
            }
            let _ = refresh_tx.send(Refresh::AppData { ws_id, item_id, snapshot });
            Response::ok(serde_json::Value::Null)
        }
        Request::ExportPdf { ws_id, item_id } => export_app_pdf(vault, &ws_id, &item_id),
        Request::Screenshot { ws_id, item_id, width, height, scale } => {
            screenshot_app(vault, &ws_id, &item_id, width, height, scale)
        }
    }
}

/// Render a headless app off-screen and return base64 PNG. A page app frames its page on white
/// (the run pane's sheet); anything else renders the requested/default viewport on the shell
/// canvas colour. Explicit `width`/`height` win over the page.
fn screenshot_app(
    vault: &Vault,
    ws_id: &str,
    item_id: &str,
    width: Option<f32>,
    height: Option<f32>,
    scale: Option<f32>,
) -> Response {
    use base64::Engine as _;
    let (_, mut app) = match headless_app(vault, ws_id, item_id) {
        Ok(v) => v,
        Err(e) => return Response::err(e),
    };
    let page = app.page();
    let (w, h) = match (width, height, page) {
        (Some(w), Some(h), _) => (w, h),
        (_, _, Some(p)) => (width.unwrap_or(p.width), height.unwrap_or(p.height)),
        _ => (width.unwrap_or(900.0), height.unwrap_or(700.0)),
    };
    let clear = if page.is_some() { egui::Color32::WHITE } else { crate::theme::BG_PAGE };
    let scale = scale.unwrap_or(2.0).clamp(0.5, 4.0);
    match app.screenshot(w, h, scale, clear, host_fonts()) {
        Ok(png) => Response::ok(serde_json::json!({
            "png_base64": base64::engine::general_purpose::STANDARD.encode(&png),
            "width_px": (w * scale).round() as u32,
            "height_px": (h * scale).round() as u32,
        })),
        Err(e) => Response::err(e),
    }
}

/// An .app's runtime data CRDT from its persisted snapshot (empty when never persisted).
fn load_app_state(vault: &Vault, ws_id: &str, item_id: &str) -> LoroDoc {
    let doc = LoroDoc::new();
    if let Ok(Some(bytes)) = vault.get_state(ws_id, item_id) {
        let _ = doc.import(&bytes);
    }
    doc
}

/// Build a headless engine from the vault's source tree + runtime state — the same inputs an
/// open tab uses — plus the item's name. The render paths (PDF, screenshot) start here.
fn headless_app(vault: &Vault, ws_id: &str, item_id: &str) -> Result<(String, app_host::App), String> {
    let item = vault
        .items(ws_id)
        .unwrap_or_default()
        .into_iter()
        .find(|i| i.id == item_id)
        .ok_or("item not found")?;
    let mut files = Vec::new();
    for path in vault.list_files(ws_id, item_id).unwrap_or_default() {
        if let Ok(Some(bytes)) = vault.get_file(ws_id, item_id, &path) {
            files.push((path.clone(), code_editor::store::decode(&path, &bytes)));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let state = vault.get_state(ws_id, item_id).ok().flatten();
    Ok((item.name, app_host::App::from_files(&files, state.as_deref())))
}

/// The shell's faces, so headless renders shape like the live tab.
fn host_fonts() -> app_host::FontBytes<'static> {
    app_host::FontBytes {
        regular: crate::theme::FONT_SANS,
        bold: crate::theme::FONT_SANS_SB,
        mono: crate::theme::FONT_MONO,
    }
}

/// Export a headless app's declared page to [`pdf_path`]. Returns the written path.
fn export_app_pdf(vault: &Vault, ws_id: &str, item_id: &str) -> Response {
    let (name, mut app) = match headless_app(vault, ws_id, item_id) {
        Ok(v) => v,
        Err(e) => return Response::err(e),
    };
    match app.export_pdf(host_fonts()) {
        Ok(bytes) => {
            let path = pdf_path(&name);
            match std::fs::write(&path, &bytes) {
                Ok(()) => Response::ok(
                    serde_json::json!({ "path": path.display().to_string(), "bytes": bytes.len() }),
                ),
                Err(e) => Response::err(format!("write failed: {e}")),
            }
        }
        Err(e) => Response::err(e),
    }
}

/// `~/Downloads/<sanitized-name>.pdf`, falling back to the temp dir.
pub(crate) fn pdf_path(name: &str) -> std::path::PathBuf {
    let safe: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let dir = std::env::var_os("HOME")
        .map(|h| std::path::Path::new(&h).join("Downloads"))
        .filter(|d| d.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    dir.join(format!("{safe}.pdf"))
}

/// Persist a starter CRDT snapshot for a freshly created item so it is readable and writable the
/// instant it exists — without a human first opening it in the GUI. The vault seeds an app's
/// `manifest.osv` but deliberately knows nothing about Loro, so the layers that do are seeded here:
///   - a `.doc`'s live block tree (`doc_editor`), and
///   - an `.app`'s `main.lua`, split into blocks and stored as a `block_doc` snapshot (`code_editor`).
/// Seeding as blocks makes each block's `TreeID` durable from birth, so an agent's read and the
/// `set_block_text` that follows it address the *same* block. Called by every create path (the
/// bridge below and the GUI's `Action::CreateItem`).
pub fn seed_item_state(vault: &Vault, item: &WorkspaceItem) {
    match item.kind {
        ItemKind::Doc => {
            let _ = vault.put_state(&item.ws_id, &item.id, &Doc::new().export_snapshot());
        }
        ItemKind::App => {
            let bytes = code_editor::store::encode("main.lua", vault::APP_MAIN_SEED);
            let _ = vault.put_file(&item.ws_id, &item.id, "main.lua", &bytes);
        }
        _ => {}
    }
}

/// Map a lower-case kind tag to an [`ItemKind`] (the inverse of [`ItemKind::as_str`]).
fn parse_kind(kind: &str) -> Option<ItemKind> {
    match kind {
        "doc" => Some(ItemKind::Doc),
        "table" => Some(ItemKind::Table),
        "app" => Some(ItemKind::App),
        "canvas" => Some(ItemKind::Canvas),
        _ => None,
    }
}

/// Load a .doc, run a mutation, then commit-export-persist-and-refresh in one place — the shared
/// spine of every doc-editing request. `f` returns the JSON result (e.g. a new block's id) or an
/// error string for a bad argument. The doc is only persisted if `f` succeeds.
fn mutate_doc(
    vault: &Vault,
    refresh_tx: &Sender<Refresh>,
    ws_id: &str,
    item_id: &str,
    f: impl FnOnce(&Doc) -> Result<serde_json::Value, String>,
) -> Response {
    let Some(doc) = load_doc(vault, ws_id, item_id) else {
        return Response::err("doc not found");
    };
    let result = match f(&doc) {
        Ok(v) => v,
        Err(e) => return Response::err(e),
    };
    doc.commit();
    let snapshot = doc.export_snapshot();
    let _ = vault.put_state(ws_id, item_id, &snapshot);
    // Notify the UI to merge this snapshot into any open tab.
    let _ = refresh_tx.send(Refresh::Doc {
        ws_id: ws_id.to_string(),
        item_id: item_id.to_string(),
        snapshot,
    });
    Response::ok(result)
}

/// Parse a block id, mapping the failure to a client-facing message.
fn parse_block(id: &str) -> Result<TreeID, String> {
    TreeID::try_from(id).map_err(|_| format!("invalid block id '{id}'"))
}

/// Load a `.lua` file as its block doc (identity-preserving), with client-facing errors for a
/// missing or non-block file — the read spine of every per-block `.lua` request.
fn load_lua(vault: &Vault, ws_id: &str, item_id: &str, path: &str) -> Result<BlockDoc, String> {
    let bytes = vault
        .get_file(ws_id, item_id, path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no such file '{path}'"))?;
    code_editor::store::doc_from_bytes(path, &bytes)
        .ok_or_else(|| format!("'{path}' is not a .lua block file"))
}

/// Load a `.lua` block doc, run a per-block mutation, then persist the *live* snapshot (identity
/// preserved, never re-split) and notify the UI to merge it — the `.lua` counterpart of
/// [`mutate_doc`]. `f` returns the JSON result or a client-facing error; the file is written only
/// on success.
fn mutate_lua(
    vault: &Vault,
    refresh_tx: &Sender<Refresh>,
    ws_id: &str,
    item_id: &str,
    path: &str,
    f: impl FnOnce(&BlockDoc) -> Result<serde_json::Value, String>,
) -> Response {
    let doc = match load_lua(vault, ws_id, item_id, path) {
        Ok(doc) => doc,
        Err(e) => return Response::err(e),
    };
    let result = match f(&doc) {
        Ok(v) => v,
        Err(e) => return Response::err(e),
    };
    doc.commit();
    let snapshot = code_editor::store::snapshot_from_doc(&doc);
    if let Err(e) = vault.put_file(ws_id, item_id, path, &snapshot) {
        return Response::err(e.to_string());
    }
    let _ = refresh_tx.send(Refresh::Lua {
        ws_id: ws_id.to_string(),
        item_id: item_id.to_string(),
        path: path.to_string(),
        snapshot,
    });
    Response::ok(result)
}

/// Parse a `.lua` block id and verify it exists in `doc` (a stale id from before a delete would
/// otherwise panic the Loro op).
fn parse_lua_block(doc: &BlockDoc, id: &str) -> Result<BlockId, String> {
    let bid = BlockId::try_from(id).map_err(|_| format!("invalid block id '{id}'"))?;
    if doc.contains(bid) {
        Ok(bid)
    } else {
        Err(format!("no such block '{id}'"))
    }
}

/// Flatten a `.lua` block doc into block summaries (flat — Lua blocks don't nest — and without the
/// `.doc`-only fields), the per-block read shape an agent edits against.
fn lua_to_summaries(doc: &BlockDoc) -> Vec<BlockSummary> {
    doc.block_ids()
        .into_iter()
        .map(|id| BlockSummary {
            id: id.to_string(),
            kind: doc.kind(id),
            text: doc.text(id),
            depth: 0,
            done: None,
            lang: None,
            marks: Vec::new(),
        })
        .collect()
}

/// Parse a block-kind tag, erroring on an unknown one (the model would silently fall back to a
/// paragraph, which hides the agent's mistake).
fn parse_block_kind(kind: &str) -> Result<BlockKind, String> {
    match kind {
        "paragraph" => Ok(BlockKind::Paragraph),
        "h1" => Ok(BlockKind::H1),
        "h2" => Ok(BlockKind::H2),
        "h3" => Ok(BlockKind::H3),
        "li" => Ok(BlockKind::BulletList),
        "ol" => Ok(BlockKind::NumberedList),
        "todo" => Ok(BlockKind::Todo),
        "quote" => Ok(BlockKind::Quote),
        "code" => Ok(BlockKind::Code),
        "divider" => Ok(BlockKind::Divider),
        _ => Err(format!("unknown block kind '{kind}'")),
    }
}

fn load_doc(vault: &Vault, ws_id: &str, item_id: &str) -> Option<Doc> {
    match vault.get_state(ws_id, item_id) {
        Ok(Some(bytes)) => Doc::from_snapshot(&bytes).ok(),
        _ => None,
    }
}

/// Clamp a `[start, end)` request to the block's text length (in code points), keeping `start <=
/// end`, so an over-long range from the agent is a no-op edge rather than a panic.
fn clamp_range(doc: &Doc, id: TreeID, start: usize, end: usize) -> (usize, usize) {
    let len = doc.text_len(id);
    let end = end.min(len);
    (start.min(end), end)
}

fn doc_to_summaries(doc: &Doc) -> Vec<BlockSummary> {
    doc.blocks()
        .into_iter()
        .map(|(id, depth)| {
            let kind = doc.kind(id);
            BlockSummary {
                id: id.to_string(),
                kind: kind.as_str().to_string(),
                text: doc.text(id),
                depth,
                done: (kind == BlockKind::Todo).then(|| doc.done(id)),
                lang: doc.lang(id),
                marks: mark_spans(doc, id),
            }
        })
        .collect()
}

/// Flatten a block's styled runs into inline-mark spans over code-point offsets, coalescing
/// adjacent runs that carry the same mark+value so a single bold word is one span, not many.
fn mark_spans(doc: &Doc, id: TreeID) -> Vec<MarkSpan> {
    let mut spans: Vec<MarkSpan> = Vec::new();
    let mut offset = 0usize;
    for run in doc.runs(id) {
        let len = run.text.chars().count();
        for key in ["bold", "italic", "strike", "code", "link"] {
            if run.marks.has(key) {
                let value = run.marks.value(key).filter(|v| !v.is_empty()).map(str::to_string);
                // Extend the previous span if this run continues the same contiguous mark.
                match spans.last_mut() {
                    Some(prev) if prev.mark == key && prev.end == offset && prev.value == value => {
                        prev.end = offset + len;
                    }
                    _ => spans.push(MarkSpan { start: offset, end: offset + len, mark: key.to_string(), value }),
                }
            }
        }
        offset += len;
    }
    spans
}
