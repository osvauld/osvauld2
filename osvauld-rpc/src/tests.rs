use std::io::Cursor;

use crate::{
    AccountSummary, ItemSummary, Request, Response, WorkspaceSummary, read_msg, write_msg,
};

fn roundtrip<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(value: &T) -> T {
    let payload = serde_json::to_vec(value).unwrap();
    let mut buf = Vec::new();
    write_msg(&mut buf, &payload).unwrap();
    let back = read_msg(&mut Cursor::new(&buf)).unwrap();
    serde_json::from_slice(&back).unwrap()
}

/// Every variant crosses the wire and comes back byte-identical — the table a Python client
/// reads as the vocabulary's completeness check.
#[test]
fn every_request_roundtrips() {
    let all: Vec<Request> = vec![
        Request::Ping,
        Request::ListAccounts,
        Request::Signup {
            name: "abe".into(),
            passphrase: "correct horse".into(),
        },
        Request::Unlock {
            account: "abe".into(),
            passphrase: "prüfen ✓".into(),
        },
        Request::Lock,
        Request::ListWorkspaces,
        Request::CreateWorkspace {
            name: "personal".into(),
        },
        Request::ListItems {
            ws_id: "ws-1".into(),
        },
        Request::CreateItem {
            ws_id: "ws-1".into(),
            name: "tally".into(),
            kind: "app".into(),
        },
        Request::OpenItem {
            item_id: "i-9".into(),
        },
        Request::ListFiles {
            item_id: "i-9".into(),
        },
        Request::ReadFile {
            item_id: "i-9".into(),
            path: "main.lua".into(),
        },
        Request::WriteFile {
            item_id: "i-9".into(),
            path: "main.lua".into(),
            content: "ui.col({})".into(),
        },
        Request::ReloadItem {
            item_id: "i-9".into(),
        },
        Request::DumpTree {
            item_id: "i-9".into(),
        },
        Request::Click {
            item_id: "i-9".into(),
            el_id: "7f".into(),
        },
        Request::Type {
            item_id: "i-9".into(),
            el_id: "field".into(),
            content: "hello".into(),
        },
        Request::Key {
            item_id: "i-9".into(),
            el_id: "field".into(),
            key: "enter".into(),
        },
        Request::ReadConsole {
            item_id: "i-9".into(),
            last: 50,
        },
        Request::Screenshot {
            item_id: "i-9".into(),
            width: None,
            height: None,
            scale: None,
        },
        Request::AppDataGet {
            item_id: "i-9".into(),
        },
    ];
    for req in &all {
        let back: Request = roundtrip(req);
        assert_eq!(
            serde_json::to_string(req).unwrap(),
            serde_json::to_string(&back).unwrap(),
            "roundtrip changed the wire form of {req:?}"
        );
    }
}

/// Passphrases are the payload's most sensitive field — a unicode passphrase must survive
/// the framing byte-for-byte, not as a mangled replacement.
#[test]
fn unlock_carries_a_unicode_passphrase_verbatim() {
    let req = Request::Unlock {
        account: "abe".into(),
        passphrase: "rücksicht — ✓".into(),
    };
    let back: Request = roundtrip(&req);
    assert!(matches!(back, Request::Unlock { passphrase, .. } if passphrase == *"rücksicht — ✓"));
}

/// The redaction contract: a printed request must not contain the passphrase.
#[test]
fn debug_never_prints_a_passphrase() {
    let req = Request::Unlock {
        account: "abe".into(),
        passphrase: "hunter2".into(),
    };
    let printed = format!("{req:?}");
    assert!(!printed.contains("hunter2"), "leaked: {printed}");
    assert!(printed.contains("redacted"));
}

/// The vocabulary is closed, on purpose — an unknown op is an error, not a warning. `Eval`
/// (the old repo's escape hatch) is deliberately absent; pin that it stays out until it is
/// a decided, sandbox-reviewed addition.
#[test]
fn unknown_op_is_rejected() {
    let bad = br#"{"op":"Eval","code":"return 1"}"#;
    assert!(serde_json::from_slice::<Request>(bad).is_err());
}

#[test]
fn responses_and_summaries_roundtrip() {
    let back: Response = roundtrip(&Response::ok(vec![WorkspaceSummary {
        id: "ws-1".into(),
        name: "personal".into(),
    }]));
    assert!(matches!(back, Response::Ok { .. }));

    let back: Response = roundtrip(&Response::ok(vec![
        AccountSummary {
            id: "a-1".into(),
            name: "abe".into(),
        },
        AccountSummary {
            id: "a-2".into(),
            name: "kim".into(),
        },
    ]));
    assert!(matches!(back, Response::Ok { .. }));

    let back: Response = roundtrip(&Response::ok(vec![ItemSummary {
        id: "i-9".into(),
        ws_id: "ws-1".into(),
        name: "tally".into(),
        kind: "app".into(),
    }]));
    assert!(matches!(back, Response::Ok { .. }));

    let back: Response = roundtrip(&Response::err("vault is locked"));
    assert!(matches!(back, Response::Err { message } if message == "vault is locked"));
}

#[test]
fn framing_length_prefix_is_correct() {
    let mut buf = Vec::new();
    write_msg(&mut buf, b"hello").unwrap();
    assert_eq!(&buf[..4], &5u32.to_be_bytes());
    assert_eq!(&buf[4..], b"hello");
}
