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
