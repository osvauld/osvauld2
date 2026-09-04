use super::Store;

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
