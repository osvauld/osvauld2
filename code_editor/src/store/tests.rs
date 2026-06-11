use super::*;

const SAMPLE: &str = "\
-- a counter app
local count = 0

function inc()
  count = count + 1
end

return function() return ui.text(tostring(count)) end
";

#[test]
fn source_round_trips_through_a_snapshot() {
    let bytes = snapshot_from_source(SAMPLE);
    assert_eq!(source_from_snapshot(&bytes).unwrap(), SAMPLE);
}

#[test]
fn doc_holds_one_block_per_split_block() {
    let doc = doc_from_source(SAMPLE);
    assert_eq!(doc.len(), crate::lua::split(SAMPLE).len());
    assert_eq!(source_from_doc(&doc), SAMPLE);
}

#[test]
fn lua_files_encode_to_a_snapshot_not_raw() {
    let bytes = encode("main.lua", SAMPLE);
    // A block snapshot is Loro binary, not the raw UTF-8 source.
    assert_ne!(bytes, SAMPLE.as_bytes());
    assert_eq!(decode("main.lua", &bytes), SAMPLE);
}

#[test]
fn non_lua_files_are_stored_raw() {
    let manifest = "app \"x\" {}\n";
    let bytes = encode("manifest.osv", manifest);
    assert_eq!(bytes, manifest.as_bytes());
    assert_eq!(decode("manifest.osv", &bytes), manifest);
}

#[test]
fn decode_falls_back_to_raw_for_non_snapshot_lua() {
    // A `.lua` whose bytes aren't a valid snapshot (legacy raw seed, hand-written) decodes as-is
    // rather than erroring.
    let raw = "return function() end\n";
    assert_eq!(decode("legacy.lua", raw.as_bytes()), raw);
}

#[test]
fn app_seed_shape_round_trips() {
    // The shape `seed_item_state` stores for a fresh `.app`: one multiline `return function()`
    // construct with a nested table. Must survive split→snapshot→emit byte-exact, or a new app
    // would fail to load.
    let seed = "\
return function()
  return ui.col{ style = { padding = 28, gap = 12 },
    ui.text{ \"new app\", style = { font = 22 } },
  }
end
";
    assert_eq!(decode("main.lua", &encode("main.lua", seed)), seed);
}

#[test]
fn doc_from_bytes_preserves_block_identity_but_doc_from_source_does_not() {
    // The load-bearing reason the editor loads via `doc_from_bytes`: a save→load round-trip keeps
    // the *same* block IDs (so the human, the vault, and an agent all anchor to one lineage), while
    // re-splitting the emitted source mints brand-new IDs and would desync them.
    let doc = doc_from_source(SAMPLE);
    let ids: Vec<_> = doc.block_ids();
    let bytes = snapshot_from_doc(&doc);

    let reloaded = doc_from_bytes("main.lua", &bytes).expect("valid .lua snapshot");
    assert_eq!(reloaded.block_ids(), ids, "snapshot round-trip keeps identity");
    assert_eq!(source_from_doc(&reloaded), SAMPLE, "and the text");

    let resplit = doc_from_source(&source_from_doc(&doc));
    assert_ne!(resplit.block_ids(), ids, "re-splitting source mints fresh IDs");
}

#[test]
fn doc_from_bytes_is_none_for_non_block_or_invalid_bytes() {
    // Non-`.lua` paths aren't block files; a `.lua` whose bytes aren't a snapshot (legacy raw seed)
    // returns `None` so the caller can fall back to `doc_from_source` on the decoded text.
    assert!(doc_from_bytes("manifest.osv", &snapshot_from_source(SAMPLE)).is_none());
    assert!(doc_from_bytes("legacy.lua", b"return function() end\n").is_none());
}

#[test]
fn is_block_file_matches_only_lua() {
    assert!(is_block_file("main.lua"));
    assert!(is_block_file("lib/state.lua"));
    assert!(!is_block_file("manifest.osv"));
    assert!(!is_block_file("assets/logo.png"));
}
