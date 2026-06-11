use std::io::{self, BufRead, Write};
use std::os::unix::net::UnixStream;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use osvauld_rpc::{read_msg, write_msg, Request, Response};

// ── JSON-RPC 2.0 wire types ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct RpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct RpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

impl RpcResponse {
    fn ok(id: Value, result: Value) -> Self {
        Self { jsonrpc: "2.0", id, result: Some(result), error: None }
    }
    fn err(id: Value, code: i32, message: impl Into<String>) -> Self {
        Self { jsonrpc: "2.0", id, result: None, error: Some(RpcError { code, message: message.into() }) }
    }
}

// ── Tool definitions ──────────────────────────────────────────────────────────

fn tools_list() -> Value {
    json!({
        "tools": [
            {
                "name": "list_workspaces",
                "description": "List all workspaces in the vault.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            },
            {
                "name": "list_items",
                "description": "List all items (docs, apps, tables) in a workspace.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" }
                    },
                    "required": ["ws_id"]
                }
            },
            {
                "name": "read_doc",
                "description": "Read all blocks of a .doc file. Returns a list of blocks with stable IDs, kinds, and text.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" }
                    },
                    "required": ["ws_id", "item_id"]
                }
            },
            {
                "name": "set_block_text",
                "description": "Replace the text of a block in a .doc file, identified by its stable block ID.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "block": { "type": "string", "description": "Block ID (from read_doc)" },
                        "text": { "type": "string", "description": "New text content" }
                    },
                    "required": ["ws_id", "item_id", "block", "text"]
                }
            },
            {
                "name": "insert_block",
                "description": "Insert a new block into a .doc. It lands immediately after the block given by 'after'; omit 'after' to append at the end. 'kind' is one of: paragraph, h1, h2, h3, li (bullet), ol (numbered), todo, quote, code, divider. Returns the new block's stable id (use it as the next 'after' to build a doc top-to-bottom).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "after": { "type": "string", "description": "Block ID to insert after (from read_doc); omit to append at the end" },
                        "kind": { "type": "string", "description": "Block kind: paragraph, h1, h2, h3, li, ol, todo, quote, code, divider" },
                        "text": { "type": "string", "description": "Initial text content (may be empty)" }
                    },
                    "required": ["ws_id", "item_id", "kind", "text"]
                }
            },
            {
                "name": "set_block_kind",
                "description": "Change an existing block's kind (e.g. turn a paragraph into a heading, list item, or code block), identified by its stable block ID.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "block": { "type": "string", "description": "Block ID (from read_doc)" },
                        "kind": { "type": "string", "description": "New kind: paragraph, h1, h2, h3, li, ol, todo, quote, code, divider" }
                    },
                    "required": ["ws_id", "item_id", "block", "kind"]
                }
            },
            {
                "name": "delete_block",
                "description": "Delete a block from a .doc, identified by its stable block ID. Any nested child blocks are promoted into its place rather than deleted.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "block": { "type": "string", "description": "Block ID (from read_doc)" }
                    },
                    "required": ["ws_id", "item_id", "block"]
                }
            },
            {
                "name": "indent_block",
                "description": "Indent a block one nesting level (make it a child of its previous sibling) — e.g. to create a sub-bullet. Returns whether it moved (false if it has no previous sibling to nest under).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "block": { "type": "string", "description": "Block ID (from read_doc)" }
                    },
                    "required": ["ws_id", "item_id", "block"]
                }
            },
            {
                "name": "outdent_block",
                "description": "Outdent a block one nesting level (promote it out of its parent). Returns whether it moved (false if already at the top level).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "block": { "type": "string", "description": "Block ID (from read_doc)" }
                    },
                    "required": ["ws_id", "item_id", "block"]
                }
            },
            {
                "name": "move_block",
                "description": "Move a block to a new position relative to a target block. 'position' is 'before' or 'after' (as a sibling of target) or 'into' (as the last child of target). Fails if target is the block itself or one of its descendants.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "block": { "type": "string", "description": "Block ID to move (from read_doc)" },
                        "position": { "type": "string", "description": "before | after | into" },
                        "target": { "type": "string", "description": "Block ID to move relative to" }
                    },
                    "required": ["ws_id", "item_id", "block", "position", "target"]
                }
            },
            {
                "name": "set_todo_done",
                "description": "Set the checked state of a 'todo' block.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "block": { "type": "string", "description": "Block ID (from read_doc)" },
                        "done": { "type": "boolean", "description": "true = checked, false = unchecked" }
                    },
                    "required": ["ws_id", "item_id", "block", "done"]
                }
            },
            {
                "name": "set_code_lang",
                "description": "Set the language tag of a 'code' block (e.g. 'rust', 'python'), which drives syntax highlighting.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "block": { "type": "string", "description": "Block ID (from read_doc)" },
                        "lang": { "type": "string", "description": "Language tag, e.g. rust, python, javascript" }
                    },
                    "required": ["ws_id", "item_id", "block", "lang"]
                }
            },
            {
                "name": "apply_mark",
                "description": "Apply an inline formatting mark over a character range of a block's text. 'mark' is bold, italic, strike, or code. 'start'/'end' are code-point offsets (apply AFTER setting the block's final text, since set_block_text replaces text and clears its marks). Use apply_link for links.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "block": { "type": "string", "description": "Block ID (from read_doc)" },
                        "start": { "type": "integer", "description": "Start offset (code points, inclusive)" },
                        "end": { "type": "integer", "description": "End offset (code points, exclusive)" },
                        "mark": { "type": "string", "description": "bold | italic | strike | code" }
                    },
                    "required": ["ws_id", "item_id", "block", "start", "end", "mark"]
                }
            },
            {
                "name": "apply_link",
                "description": "Apply a link mark carrying a URL over a character range of a block's text. 'start'/'end' are code-point offsets.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "block": { "type": "string", "description": "Block ID (from read_doc)" },
                        "start": { "type": "integer", "description": "Start offset (code points, inclusive)" },
                        "end": { "type": "integer", "description": "End offset (code points, exclusive)" },
                        "url": { "type": "string", "description": "Link target URL" }
                    },
                    "required": ["ws_id", "item_id", "block", "start", "end", "url"]
                }
            },
            {
                "name": "clear_mark",
                "description": "Remove an inline mark (bold, italic, strike, code, or link) over a character range of a block's text. 'start'/'end' are code-point offsets.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "block": { "type": "string", "description": "Block ID (from read_doc)" },
                        "start": { "type": "integer", "description": "Start offset (code points, inclusive)" },
                        "end": { "type": "integer", "description": "End offset (code points, exclusive)" },
                        "mark": { "type": "string", "description": "bold | italic | strike | code | link" }
                    },
                    "required": ["ws_id", "item_id", "block", "start", "end", "mark"]
                }
            },
            {
                "name": "create_doc",
                "description": "Create a new .doc item in a workspace. It starts with a single empty paragraph block; call read_doc to get that block's ID, then set_block_text to fill it. Returns the new item (with its ID).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "name": { "type": "string", "description": "Doc name" }
                    },
                    "required": ["ws_id", "name"]
                }
            },
            {
                "name": "create_app",
                "description": "Create a new .app item in a workspace. It is seeded with a starter source tree (manifest.osv + main.lua); use write_file to build it out. Returns the new item (with its ID).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "name": { "type": "string", "description": "App name" }
                    },
                    "required": ["ws_id", "name"]
                }
            },
            {
                "name": "list_files",
                "description": "List the source-file paths of an .app's folder tree (e.g. 'main.lua', 'lib/state.lua', 'manifest.osv').",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" }
                    },
                    "required": ["ws_id", "item_id"]
                }
            },
            {
                "name": "read_file",
                "description": "Read one source file from an .app's folder tree.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "path": { "type": "string", "description": "File path within the app (e.g. 'main.lua')" }
                    },
                    "required": ["ws_id", "item_id", "path"]
                }
            },
            {
                "name": "write_file",
                "description": "Write (create or overwrite) one source file in an .app's folder tree. The folder structure is the path itself (e.g. 'lib/state.lua'). The entry point is 'main.lua', which must `return function() ... end`; other .lua files are require-able as modules (lib/state.lua -> require('lib.state')). An open app tab reloads immediately.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "path": { "type": "string", "description": "File path within the app (e.g. 'main.lua')" },
                        "content": { "type": "string", "description": "Full file content" }
                    },
                    "required": ["ws_id", "item_id", "path", "content"]
                }
            },
            {
                "name": "read_file_blocks",
                "description": "Read a .lua file as its blocks (one per top-level construct: a function, a statement, a comment), each with a stable block ID — the per-block counterpart of read_file. Use this to target a single construct with set_file_block_text / insert_file_block / delete_file_block instead of rewriting the whole file (which preserves block identity and won't clobber a human's concurrent edits).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "path": { "type": "string", "description": "File path within the app (e.g. 'main.lua')" }
                    },
                    "required": ["ws_id", "item_id", "path"]
                }
            },
            {
                "name": "set_file_block_text",
                "description": "Replace the text of one block of a .lua file, identified by its stable block ID (from read_file_blocks). Every other block keeps its identity. The block's text is raw Lua; to restructure into more/fewer constructs use insert/delete_file_block.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "path": { "type": "string", "description": "File path within the app (e.g. 'main.lua')" },
                        "block": { "type": "string", "description": "Block ID (from read_file_blocks)" },
                        "text": { "type": "string", "description": "New Lua text for the block" }
                    },
                    "required": ["ws_id", "item_id", "path", "block", "text"]
                }
            },
            {
                "name": "insert_file_block",
                "description": "Insert a new block into a .lua file. It lands immediately after the block given by 'after'; omit 'after' to append at the end. 'kind' is a structural tag (statement, comment, function) and is advisory. Returns the new block's stable id.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "path": { "type": "string", "description": "File path within the app (e.g. 'main.lua')" },
                        "after": { "type": "string", "description": "Block ID to insert after (from read_file_blocks); omit to append at the end" },
                        "kind": { "type": "string", "description": "Structural tag: statement, comment, function (advisory; defaults to statement)" },
                        "text": { "type": "string", "description": "Lua text for the new block" }
                    },
                    "required": ["ws_id", "item_id", "path", "text"]
                }
            },
            {
                "name": "delete_file_block",
                "description": "Delete one block of a .lua file by its stable block ID (from read_file_blocks).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "path": { "type": "string", "description": "File path within the app (e.g. 'main.lua')" },
                        "block": { "type": "string", "description": "Block ID (from read_file_blocks)" }
                    },
                    "required": ["ws_id", "item_id", "path", "block"]
                }
            },
            {
                "name": "app_data_get",
                "description": "Read an .app's runtime data CRDT as JSON — the named top-level containers the app's Lua reads via doc:text/map/list. Use this to see the app's current content (e.g. a letterhead's body text) before editing it with app_data_set_text.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" }
                    },
                    "required": ["ws_id", "item_id"]
                }
            },
            {
                "name": "app_data_set_text",
                "description": "Replace the content of one top-level text container in an .app's runtime data CRDT (the container the app's Lua opens as doc:text(name) and binds to a ui.editor). This is the same CRDT op the app's own editor makes, so an open run pane updates live. Edits the app's DATA — use write_file / set_file_block_text for its CODE.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "name": { "type": "string", "description": "Top-level text container name (a key from app_data_get, e.g. 'body')" },
                        "text": { "type": "string", "description": "New full content for the container" }
                    },
                    "required": ["ws_id", "item_id", "name", "text"]
                }
            },
            {
                "name": "export_pdf",
                "description": "Export a page-declaring .app (one with `page = { size = 'A4', ... }` in its Lua) to a PDF laid out exactly as its page preview renders. Writes ~/Downloads/<name>.pdf and returns the path. Errors if the app declares no page.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" }
                    },
                    "required": ["ws_id", "item_id"]
                }
            },
            {
                "name": "screenshot",
                "description": "Render an .app off-screen and return the image — exactly what its run pane shows, no open tab needed. A page-declaring app defaults to its page size on white; others default to 900x700 on the shell canvas. Use it to see the result of code or data edits.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "ws_id": { "type": "string", "description": "Workspace ID" },
                        "item_id": { "type": "string", "description": "Item ID" },
                        "width": { "type": "number", "description": "Viewport width in logical px (default: page width, else 900)" },
                        "height": { "type": "number", "description": "Viewport height in logical px (default: page height, else 700)" },
                        "scale": { "type": "number", "description": "Pixels per logical px, 0.5-4 (default 2)" }
                    },
                    "required": ["ws_id", "item_id"]
                }
            }
        ]
    })
}

// ── Bridge call ───────────────────────────────────────────────────────────────

fn call_gui(req: &Request) -> Result<Value, String> {
    let socket_path = std::env::var("OSVAULD_SOCKET")
        .unwrap_or_else(|_| "/tmp/osvauld.sock".to_string());

    let mut stream = UnixStream::connect(&socket_path)
        .map_err(|e| format!("cannot connect to osvauld GUI ({socket_path}): {e}"))?;

    let payload = serde_json::to_vec(req).map_err(|e| e.to_string())?;
    write_msg(&mut stream, &payload).map_err(|e| e.to_string())?;

    let bytes = read_msg(&mut stream).map_err(|e| e.to_string())?;
    let resp: Response = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;

    match resp {
        Response::Ok { result } => Ok(result),
        Response::Err { message } => Err(message),
    }
}

// ── Tool dispatch ─────────────────────────────────────────────────────────────

fn call_tool(name: &str, args: &Value) -> Result<Value, String> {
    let req = match name {
        "list_workspaces" => Request::ListWorkspaces,
        "list_items" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            Request::ListItems { ws_id }
        }
        "read_doc" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            Request::ReadDoc { ws_id, item_id }
        }
        "set_block_text" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            let text = args["text"].as_str().ok_or("missing text")?.to_string();
            Request::SetBlockText { ws_id, item_id, block, text }
        }
        "insert_block" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let after = args["after"].as_str().map(|s| s.to_string());
            let kind = args["kind"].as_str().ok_or("missing kind")?.to_string();
            let text = args["text"].as_str().ok_or("missing text")?.to_string();
            Request::InsertBlock { ws_id, item_id, after, kind, text }
        }
        "set_block_kind" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            let kind = args["kind"].as_str().ok_or("missing kind")?.to_string();
            Request::SetBlockKind { ws_id, item_id, block, kind }
        }
        "delete_block" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            Request::DeleteBlock { ws_id, item_id, block }
        }
        "indent_block" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            Request::IndentBlock { ws_id, item_id, block }
        }
        "outdent_block" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            Request::OutdentBlock { ws_id, item_id, block }
        }
        "move_block" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            let position = args["position"].as_str().ok_or("missing position")?.to_string();
            let target = args["target"].as_str().ok_or("missing target")?.to_string();
            Request::MoveBlock { ws_id, item_id, block, position, target }
        }
        "set_todo_done" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            let done = args["done"].as_bool().ok_or("missing done")?;
            Request::SetTodoDone { ws_id, item_id, block, done }
        }
        "set_code_lang" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            let lang = args["lang"].as_str().ok_or("missing lang")?.to_string();
            Request::SetCodeLang { ws_id, item_id, block, lang }
        }
        "apply_mark" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            let start = args["start"].as_u64().ok_or("missing start")? as usize;
            let end = args["end"].as_u64().ok_or("missing end")? as usize;
            let mark = args["mark"].as_str().ok_or("missing mark")?.to_string();
            Request::ApplyMark { ws_id, item_id, block, start, end, mark }
        }
        "apply_link" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            let start = args["start"].as_u64().ok_or("missing start")? as usize;
            let end = args["end"].as_u64().ok_or("missing end")? as usize;
            let url = args["url"].as_str().ok_or("missing url")?.to_string();
            Request::ApplyLink { ws_id, item_id, block, start, end, url }
        }
        "clear_mark" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            let start = args["start"].as_u64().ok_or("missing start")? as usize;
            let end = args["end"].as_u64().ok_or("missing end")? as usize;
            let mark = args["mark"].as_str().ok_or("missing mark")?.to_string();
            Request::ClearMark { ws_id, item_id, block, start, end, mark }
        }
        "create_doc" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let name = args["name"].as_str().ok_or("missing name")?.to_string();
            Request::CreateItem { ws_id, name, kind: "doc".to_string() }
        }
        "create_app" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let name = args["name"].as_str().ok_or("missing name")?.to_string();
            Request::CreateItem { ws_id, name, kind: "app".to_string() }
        }
        "list_files" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            Request::ListFiles { ws_id, item_id }
        }
        "read_file" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let path = args["path"].as_str().ok_or("missing path")?.to_string();
            Request::ReadFile { ws_id, item_id, path }
        }
        "write_file" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let path = args["path"].as_str().ok_or("missing path")?.to_string();
            let content = args["content"].as_str().ok_or("missing content")?.to_string();
            Request::WriteFile { ws_id, item_id, path, content }
        }
        "read_file_blocks" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let path = args["path"].as_str().ok_or("missing path")?.to_string();
            Request::ReadFileBlocks { ws_id, item_id, path }
        }
        "set_file_block_text" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let path = args["path"].as_str().ok_or("missing path")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            let text = args["text"].as_str().ok_or("missing text")?.to_string();
            Request::SetFileBlockText { ws_id, item_id, path, block, text }
        }
        "insert_file_block" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let path = args["path"].as_str().ok_or("missing path")?.to_string();
            let after = args["after"].as_str().map(|s| s.to_string());
            let kind = args["kind"].as_str().unwrap_or("statement").to_string();
            let text = args["text"].as_str().ok_or("missing text")?.to_string();
            Request::InsertFileBlock { ws_id, item_id, path, after, kind, text }
        }
        "delete_file_block" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let path = args["path"].as_str().ok_or("missing path")?.to_string();
            let block = args["block"].as_str().ok_or("missing block")?.to_string();
            Request::DeleteFileBlock { ws_id, item_id, path, block }
        }
        "app_data_get" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            Request::AppDataGet { ws_id, item_id }
        }
        "app_data_set_text" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let name = args["name"].as_str().ok_or("missing name")?.to_string();
            let text = args["text"].as_str().ok_or("missing text")?.to_string();
            Request::AppDataSetText { ws_id, item_id, name, text }
        }
        "export_pdf" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            Request::ExportPdf { ws_id, item_id }
        }
        "screenshot" => {
            let ws_id = args["ws_id"].as_str().ok_or("missing ws_id")?.to_string();
            let item_id = args["item_id"].as_str().ok_or("missing item_id")?.to_string();
            let width = args["width"].as_f64().map(|v| v as f32);
            let height = args["height"].as_f64().map(|v| v as f32);
            let scale = args["scale"].as_f64().map(|v| v as f32);
            Request::Screenshot { ws_id, item_id, width, height, scale }
        }
        _ => return Err(format!("unknown tool: {name}")),
    };

    call_gui(&req)
}

// ── MCP method handlers ───────────────────────────────────────────────────────

fn handle(method: &str, params: &Value) -> Result<Value, (i32, String)> {
    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "osvauld-mcp", "version": "0.1.0" }
        })),
        "tools/list" => Ok(tools_list()),
        "tools/call" => {
            let name = params["name"].as_str().ok_or((-32602, "missing name".to_string()))?;
            let args = &params["arguments"];
            match call_tool(name, args) {
                // A screenshot comes back as an MCP image block, not JSON text.
                Ok(val) if val.get("png_base64").is_some() => Ok(json!({
                    "content": [{
                        "type": "image",
                        "data": val["png_base64"],
                        "mimeType": "image/png"
                    }]
                })),
                Ok(val) => Ok(json!({
                    "content": [{ "type": "text", "text": val.to_string() }]
                })),
                Err(msg) => Ok(json!({
                    "content": [{ "type": "text", "text": msg }],
                    "isError": true
                })),
            }
        }
        // Notifications — no response needed.
        "notifications/initialized" | "notifications/cancelled" => {
            Err((-1, String::new())) // sentinel: skip writing a reply
        }
        _ => Err((-32601, format!("method not found: {method}"))),
    }
}

// ── Main loop ─────────────────────────────────────────────────────────────────

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            _ => continue,
        };

        let rpc: RpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = RpcResponse::err(Value::Null, -32700, format!("parse error: {e}"));
                let _ = writeln!(out, "{}", serde_json::to_string(&resp).unwrap());
                let _ = out.flush();
                continue;
            }
        };

        let id = rpc.id.clone().unwrap_or(Value::Null);

        match handle(&rpc.method, &rpc.params) {
            Ok(result) => {
                if rpc.id.is_some() {
                    let resp = RpcResponse::ok(id, result);
                    let _ = writeln!(out, "{}", serde_json::to_string(&resp).unwrap());
                    let _ = out.flush();
                }
            }
            Err((-1, _)) => {} // notification — no reply
            Err((code, msg)) => {
                let resp = RpcResponse::err(id, code, msg);
                let _ = writeln!(out, "{}", serde_json::to_string(&resp).unwrap());
                let _ = out.flush();
            }
        }
    }
}
