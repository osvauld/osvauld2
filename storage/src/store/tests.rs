use super::{Op, Store};

fn mem() -> Store {
    Store::open_in_memory().unwrap()
}

#[test]
fn put_get_round_trip() {
    let store = mem();
    store.put("space/page/layer", b"snapshot").unwrap();
    assert_eq!(
        store.get("space/page/layer").unwrap(),
        Some(b"snapshot".to_vec())
    );
}

#[test]
fn get_missing_returns_none() {
    let store = mem();
    assert_eq!(store.get("space/page/missing").unwrap(), None);
}

#[test]
fn put_overwrites() {
    let store = mem();
    store.put("k", b"v1").unwrap();
    store.put("k", b"v2").unwrap();
    assert_eq!(store.get("k").unwrap(), Some(b"v2".to_vec()));
}

#[test]
fn delete_removes() {
    let store = mem();
    store.put("k", b"v").unwrap();
    store.delete("k").unwrap();
    assert_eq!(store.get("k").unwrap(), None);
}

#[test]
fn delete_missing_is_ok() {
    let store = mem();
    assert!(store.delete("ghost").is_ok());
}

#[test]
fn persists_across_reopen() {
    let path = std::env::temp_dir().join(format!("osv-storage-{}.redb", rand::random::<u64>()));
    {
        let store = Store::open(&path).unwrap();
        store.put("space/page/layer", b"durable").unwrap();
    }
    let store = Store::open(&path).unwrap();
    assert_eq!(
        store.get("space/page/layer").unwrap(),
        Some(b"durable".to_vec())
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn open_readonly_reads_existing() {
    let path = std::env::temp_dir().join(format!("osv-storage-ro-{}.redb", rand::random::<u64>()));
    {
        let store = Store::open(&path).unwrap();
        store.put("k", b"v").unwrap();
    }
    let store = Store::open_readonly(&path).unwrap();
    assert_eq!(store.get("k").unwrap(), Some(b"v".to_vec()));
    std::fs::remove_file(&path).ok();
}

#[test]
fn open_readonly_missing_file_errors() {
    let path = std::env::temp_dir().join(format!(
        "osv-storage-missing-{}.redb",
        rand::random::<u64>()
    ));
    assert!(Store::open_readonly(&path).is_err());
}

#[test]
fn apply_runs_ops_in_order_in_one_commit() {
    let store = mem();
    store.put("a/1", b"old").unwrap();
    store.put("a/2", b"two").unwrap();
    store.put("b/1", b"keep").unwrap();
    store
        .apply(&[
            Op::DeletePrefix("a/".into()),
            Op::Put("a/1".into(), b"new".to_vec()),
            Op::Put("c/1".into(), b"c".to_vec()),
            Op::Delete("c/1".into()),
        ])
        .unwrap();
    assert_eq!(store.get("a/1").unwrap(), Some(b"new".to_vec()));
    assert_eq!(store.get("a/2").unwrap(), None);
    assert_eq!(store.get("b/1").unwrap(), Some(b"keep".to_vec()));
    assert_eq!(store.get("c/1").unwrap(), None);
}

#[test]
fn delete_prefix_stops_at_the_prefix() {
    let store = mem();
    store.put("order", b"1").unwrap();
    store.put("orders/1", b"2").unwrap();
    store.put("orders/2", b"3").unwrap();
    store.delete_prefix("orders/").unwrap();
    assert_eq!(store.list_prefixed("").unwrap(), vec!["order".to_string()]);
}

#[test]
fn scan_returns_rows_under_prefix_in_key_order() {
    let store = mem();
    store.put("x/2", b"second").unwrap();
    store.put("x/1", b"first").unwrap();
    store.put("y/1", b"other").unwrap();
    assert_eq!(
        store.scan("x/").unwrap(),
        vec![
            ("x/1".to_string(), b"first".to_vec()),
            ("x/2".to_string(), b"second".to_vec()),
        ]
    );
    assert!(store.scan("z/").unwrap().is_empty());
}
