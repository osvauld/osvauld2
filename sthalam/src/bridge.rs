use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use doc_editor::{Doc, TreeID};
use osvauld_rpc::{BlockSummary, ItemSummary, Request, Response, WorkspaceSummary};
use vault::Vault;

pub fn socket_path() -> String {
    std::env::var("OSVAULD_SOCKET").unwrap_or_else(|_| "/tmp/osvauld.sock".to_string())
}

/// Sent to the UI thread when the bridge writes a doc that may be open in a tab.
pub struct DocRefresh {
    pub ws_id: String,
    pub item_id: String,
    pub snapshot: Vec<u8>,
}

pub fn start(path: impl AsRef<Path>, vault: Vault) -> Receiver<DocRefresh> {
    let path = path.as_ref().to_owned();
    let (refresh_tx, refresh_rx) = mpsc::channel::<DocRefresh>();
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
        }
    });
    refresh_rx
}

fn handle(vault: &Vault, refresh_tx: &Sender<DocRefresh>, req: Request) -> Response {
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
            let id = match TreeID::try_from(block.as_str()) {
                Ok(id) => id,
                Err(_) => return Response::err("invalid block id"),
            };
            match load_doc(vault, &ws_id, &item_id) {
                Some(doc) => {
                    doc.set_block_text(id, &text);
                    doc.commit();
                    let snapshot = doc.export_snapshot();
                    let _ = vault.put_layer(&ws_id, &item_id, "text", &snapshot);
                    // Notify the UI to merge this snapshot into any open tab.
                    let _ = refresh_tx.send(DocRefresh {
                        ws_id,
                        item_id,
                        snapshot,
                    });
                    Response::ok(serde_json::Value::Null)
                }
                None => Response::err("doc not found"),
            }
        }
    }
}

fn load_doc(vault: &Vault, ws_id: &str, item_id: &str) -> Option<Doc> {
    match vault.get_layer(ws_id, item_id, "text") {
        Ok(Some(bytes)) => Doc::from_snapshot(&bytes).ok(),
        _ => None,
    }
}

fn doc_to_summaries(doc: &Doc) -> Vec<BlockSummary> {
    doc.blocks()
        .into_iter()
        .map(|(id, depth)| BlockSummary {
            id: id.to_string(),
            kind: doc.kind(id).as_str().to_string(),
            text: doc.text(id),
            depth,
        })
        .collect()
}
