use super::Layer;
use storage::Store;

#[test]
fn create_edit_save_reopen() {
    let store = Store::open_in_memory().unwrap();
    let key = "ws/w1/file/abc";

    // Create, edit, persist — then drop the live doc.
    {
        let layer = Layer::create(key);
        layer.text("body").insert(0, "hello world").unwrap();
        layer.save(&store).unwrap();
    }

    // Reopen from the stored snapshot into a fresh LoroDoc.
    let reopened = Layer::open(&store, key).unwrap().expect("layer should exist");
    assert_eq!(reopened.text("body").to_string(), "hello world");
}

#[test]
fn open_missing_returns_none() {
    let store = Store::open_in_memory().unwrap();
    assert!(Layer::open(&store, "ws/w1/file/none").unwrap().is_none());
}

#[test]
fn save_overwrites_with_latest_state() {
    let store = Store::open_in_memory().unwrap();
    let key = "ws/w1/file/edit";

    let layer = Layer::create(key);
    layer.text("body").insert(0, "draft").unwrap();
    layer.save(&store).unwrap();

    layer.text("body").insert(5, " v2").unwrap();
    layer.save(&store).unwrap();

    let reopened = Layer::open(&store, key).unwrap().unwrap();
    assert_eq!(reopened.text("body").to_string(), "draft v2");
}

#[test]
fn persists_across_store_reopen() {
    let path = std::env::temp_dir().join(format!("osv-data-{}.redb", rand::random::<u64>()));
    let key = "ws/w1/file/durable";
    {
        let store = Store::open(&path).unwrap();
        let layer = Layer::create(key);
        layer.text("body").insert(0, "durable note").unwrap();
        layer.save(&store).unwrap();
    }
    let store = Store::open(&path).unwrap();
    let layer = Layer::open(&store, key).unwrap().unwrap();
    assert_eq!(layer.text("body").to_string(), "durable note");
    std::fs::remove_file(&path).ok();
}
