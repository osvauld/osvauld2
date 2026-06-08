use std::io::Cursor;

use crate::{BlockSummary, ItemSummary, Request, Response, WorkspaceSummary, read_msg, write_msg};

fn roundtrip<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(value: &T) -> T {
    let payload = serde_json::to_vec(value).unwrap();
    let mut buf = Vec::new();
    write_msg(&mut buf, &payload).unwrap();
    let back = read_msg(&mut Cursor::new(&buf)).unwrap();
    serde_json::from_slice(&back).unwrap()
}

#[test]
fn list_workspaces_roundtrips() {
    let back: Request = roundtrip(&Request::ListWorkspaces);
    assert!(matches!(back, Request::ListWorkspaces));
}

#[test]
fn list_items_roundtrips() {
    let req = Request::ListItems { ws_id: "ws-abc".to_string() };
    let back: Request = roundtrip(&req);
    assert!(matches!(back, Request::ListItems { ws_id } if ws_id == "ws-abc"));
}

#[test]
fn read_doc_roundtrips() {
    let req = Request::ReadDoc { ws_id: "ws-abc".to_string(), item_id: "item-123".to_string() };
    let back: Request = roundtrip(&req);
    assert!(matches!(back, Request::ReadDoc { ws_id, item_id } if ws_id == "ws-abc" && item_id == "item-123"));
}

#[test]
fn set_block_text_roundtrips() {
    let req = Request::SetBlockText {
        ws_id: "ws-abc".to_string(),
        item_id: "item-123".to_string(),
        block: "0@12345678".to_string(),
        text: "hello world".to_string(),
    };
    let back: Request = roundtrip(&req);
    assert!(matches!(back, Request::SetBlockText { block, text, .. } if block == "0@12345678" && text == "hello world"));
}

#[test]
fn ok_response_with_workspace_list_roundtrips() {
    let ws = vec![WorkspaceSummary { id: "ws-1".to_string(), name: "personal".to_string() }];
    let back: Response = roundtrip(&Response::ok(&ws));
    assert!(matches!(back, Response::Ok { .. }));
}

#[test]
fn ok_response_with_item_list_roundtrips() {
    let items = vec![ItemSummary { id: "i-1".to_string(), ws_id: "ws-1".to_string(), name: "notes".to_string(), kind: "doc".to_string() }];
    let back: Response = roundtrip(&Response::ok(&items));
    assert!(matches!(back, Response::Ok { .. }));
}

#[test]
fn ok_response_with_block_list_roundtrips() {
    let blocks = vec![BlockSummary { id: "0@1".to_string(), kind: "paragraph".to_string(), text: "hi".to_string(), depth: 0 }];
    let back: Response = roundtrip(&Response::ok(&blocks));
    assert!(matches!(back, Response::Ok { .. }));
}

#[test]
fn err_response_roundtrips() {
    let back: Response = roundtrip(&Response::err("doc not found"));
    assert!(matches!(back, Response::Err { message } if message == "doc not found"));
}

#[test]
fn framing_length_prefix_is_correct() {
    let mut buf = Vec::new();
    write_msg(&mut buf, b"hello").unwrap();
    assert_eq!(&buf[..4], &5u32.to_be_bytes());
    assert_eq!(&buf[4..], b"hello");
}
