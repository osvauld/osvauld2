//! T1–T3 of docs/design/search.md §0, against a real vault on disk.

use super::*;
use std::collections::BTreeMap;
use tempfile::TempDir;
use vault::{ItemKind, Vault};

/// An unlocked vault with one workspace and one item, plus the passphrase-free way back in.
fn fresh() -> (Vault, TempDir, String, String, String) {
    let tmp = TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.signup("me", "pw").unwrap();
    let did = vault.current().unwrap().did;
    let ws = vault.create_workspace("ws").unwrap();
    let item = vault.create_item(&ws.id, "app", ItemKind::App).unwrap();
    (vault, tmp, did, ws.id, item.id)
}

fn rec(doc: &str, id: &str, body: &str) -> Record {
    Record {
        doc: doc.into(),
        id: id.into(),
        body: Some(body.into()),
        ..Record::default()
    }
}

fn ids(hits: &[Hit]) -> Vec<String> {
    hits.iter().map(|h| h.id.clone()).collect()
}

fn find(index: &Index, q: &str) -> Vec<String> {
    ids(&index.query(q, &QueryOpts::default()).unwrap())
}

// ── T1, T2: the index lives in the vault ─────────────────────────────────────

#[test]
fn the_index_survives_closing_and_reopening_the_vault() {
    let (mut vault, tmp, did, ws, item) = fresh();
    {
        let mut index = Index::open_vault(&vault, &ws, &item).unwrap();
        index.upsert(&rec("board", "c1", "wire the bridge")).unwrap();
        index.upsert(&rec("board", "c2", "paint the fence")).unwrap();
        index.commit().unwrap();
    }
    vault.lock();
    drop(vault);

    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.login(&did, "pw").unwrap();
    let index = Index::open_vault(&vault, &ws, &item).unwrap();
    assert_eq!(find(&index, "bridge"), ["c1"]);
    assert_eq!(find(&index, "fence"), ["c2"]);
}

#[test]
fn indexes_of_two_items_do_not_see_each_other() {
    let (vault, _tmp, _did, ws, item) = fresh();
    let other = vault.create_item(&ws, "other", ItemKind::App).unwrap().id;
    let mut a = Index::open_vault(&vault, &ws, &item).unwrap();
    a.upsert(&rec("d", "1", "alpha")).unwrap();
    a.commit().unwrap();
    let b = Index::open_vault(&vault, &ws, &other).unwrap();
    assert!(find(&b, "alpha").is_empty());
}

#[test]
fn nothing_indexed_reaches_disk_unsealed() {
    let (vault, tmp, _did, ws, item) = fresh();
    let mut index = Index::open_vault(&vault, &ws, &item).unwrap();
    index.upsert(&rec("board", "marigold-id", "marigold rollout")).unwrap();
    index.commit().unwrap();
    // Searchable, so it was written somewhere — and nowhere in plaintext.
    assert_eq!(find(&index, "marigold"), ["marigold-id"]);
    for entry in std::fs::read_dir(tmp.path()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes.windows(8).any(|w| w == b"marigold"),
                "plaintext term in {path:?}"
            );
        }
    }
}

#[test]
fn a_locked_vault_refuses_rather_than_indexing_into_nothing() {
    let (mut vault, _tmp, _did, ws, item) = fresh();
    let mut index = Index::open_vault(&vault, &ws, &item).unwrap();
    vault.lock();
    index.upsert(&rec("d", "1", "alpha")).unwrap();
    assert!(index.commit().is_err());
}

// ── T3: the index API ────────────────────────────────────────────────────────

#[test]
fn upsert_replaces_and_delete_removes() {
    let mut index = Index::in_memory().unwrap();
    index.upsert(&rec("d", "1", "old words")).unwrap();
    index.commit().unwrap();
    index.upsert(&rec("d", "1", "new words")).unwrap();
    index.commit().unwrap();
    assert!(find(&index, "old").is_empty());
    assert_eq!(find(&index, "new"), ["1"]);

    index.delete("d", "1").unwrap();
    index.commit().unwrap();
    assert!(find(&index, "words").is_empty());
}

#[test]
fn the_same_id_in_two_docs_is_two_records() {
    let mut index = Index::in_memory().unwrap();
    index.upsert(&rec("channel:a", "m1", "hello")).unwrap();
    index.upsert(&rec("channel:b", "m1", "hello")).unwrap();
    index.commit().unwrap();
    let hits = index.query("hello", &QueryOpts::default()).unwrap();
    let mut docs: Vec<_> = hits.iter().map(|h| h.doc.as_str()).collect();
    docs.sort();
    assert_eq!(docs, ["channel:a", "channel:b"]);

    index.delete("channel:a", "m1").unwrap();
    index.commit().unwrap();
    let hits = index.query("hello", &QueryOpts::default()).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].doc, "channel:b");
}

#[test]
fn delete_doc_removes_every_record_of_that_doc_only() {
    let mut index = Index::in_memory().unwrap();
    index.upsert(&rec("a", "1", "shared")).unwrap();
    index.upsert(&rec("a", "2", "shared")).unwrap();
    index.upsert(&rec("b", "3", "shared")).unwrap();
    index.commit().unwrap();
    index.delete_doc("a").unwrap();
    index.commit().unwrap();
    assert_eq!(find(&index, "shared"), ["3"]);
}

#[test]
fn title_outranks_body() {
    let mut index = Index::in_memory().unwrap();
    index
        .upsert(&Record {
            doc: "d".into(),
            id: "in-body".into(),
            title: Some("something else".into()),
            body: Some("about the bridge".into()),
            ..Record::default()
        })
        .unwrap();
    index
        .upsert(&Record {
            doc: "d".into(),
            id: "in-title".into(),
            title: Some("the bridge".into()),
            body: Some("about something else".into()),
            ..Record::default()
        })
        .unwrap();
    index.commit().unwrap();
    assert_eq!(find(&index, "bridge"), ["in-title", "in-body"]);
}

#[test]
fn facets_filter_exactly() {
    let mut index = Index::in_memory().unwrap();
    for (id, author) in [("1", "anu"), ("2", "abe")] {
        index
            .upsert(&Record {
                doc: "d".into(),
                id: id.into(),
                body: Some("deploy today".into()),
                facets: BTreeMap::from([("author".into(), author.into())]),
                ..Record::default()
            })
            .unwrap();
    }
    index.commit().unwrap();
    assert_eq!(find(&index, "author:anu deploy"), ["1"]);
    assert_eq!(find(&index, "author:abe"), ["2"]);
    assert!(find(&index, "author:nobody deploy").is_empty());
    // A facet is not fuzzy: a prefix of the value is not the value.
    assert!(find(&index, "author:an").is_empty());
}

#[test]
fn recent_ranks_by_time_and_relevance_by_score() {
    let mut index = Index::in_memory().unwrap();
    for (id, body, t) in [
        ("old", "deploy deploy deploy", 1.0),
        ("new", "deploy and lots of other words around it", 3.0),
        ("mid", "deploy now", 2.0),
    ] {
        index
            .upsert(&Record {
                doc: "d".into(),
                id: id.into(),
                body: Some(body.into()),
                time: Some(t),
                ..Record::default()
            })
            .unwrap();
    }
    index.commit().unwrap();
    let recent = QueryOpts {
        rank: Rank::Recent,
        ..QueryOpts::default()
    };
    assert_eq!(ids(&index.query("deploy", &recent).unwrap()), ["new", "mid", "old"]);
    assert_eq!(find(&index, "deploy")[0], "old");
}

#[test]
fn limit_caps_hits_and_snippets_show_the_match() {
    let mut index = Index::in_memory().unwrap();
    for i in 0..10 {
        index
            .upsert(&rec("d", &i.to_string(), &format!("note {i} about the marigold fields")))
            .unwrap();
    }
    index.commit().unwrap();
    let opts = QueryOpts {
        limit: 3,
        ..QueryOpts::default()
    };
    let hits = index.query("marigold", &opts).unwrap();
    assert_eq!(hits.len(), 3);
    assert!(hits.iter().all(|h| h.snippet.contains("marigold")), "{hits:?}");
    assert!(hits.iter().all(|h| !h.snippet.contains('<')), "snippets are plain text");
}

#[test]
fn a_query_that_does_not_parse_is_an_error_not_a_panic() {
    let index = Index::in_memory().unwrap();
    // Empty and odd inputs either match nothing or error; neither may panic.
    assert!(index.query("", &QueryOpts::default()).unwrap().is_empty());
    let _ = index.query("\"unterminated", &QueryOpts::default());
    let _ = index.query("a:b:c ((", &QueryOpts::default());
}

#[test]
fn uncommitted_writes_are_not_visible() {
    let mut index = Index::in_memory().unwrap();
    index.upsert(&rec("d", "1", "pending")).unwrap();
    assert!(find(&index, "pending").is_empty());
    index.commit().unwrap();
    assert_eq!(find(&index, "pending"), ["1"]);
}

// ── fingerprints: what the shell compares to know what changed (§5) ─────────

#[test]
fn fingerprints_persist_beside_the_index() {
    let (mut vault, tmp, did, ws, item) = fresh();
    {
        let mut index = Index::open_vault(&vault, &ws, &item).unwrap();
        assert_eq!(index.prints("board").unwrap(), None);
        index.set_prints("board", b"opaque".to_vec());
        assert_eq!(index.prints("board").unwrap().as_deref(), Some(&b"opaque"[..]));
        index.commit().unwrap();
    }
    vault.lock();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.login(&did, "pw").unwrap();
    let index = Index::open_vault(&vault, &ws, &item).unwrap();
    assert_eq!(index.prints("board").unwrap().as_deref(), Some(&b"opaque"[..]));
    assert_eq!(index.prints("other").unwrap(), None);
}
