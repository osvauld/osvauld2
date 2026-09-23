use super::*;

fn source(text: &str) -> LoroDoc {
    let doc = LoroDoc::new();
    write_source_file(&doc, "main.lua", text).unwrap();
    doc
}

fn edit(old_text: &str, new_text: &str) -> SourceEdit {
    SourceEdit {
        old_text: old_text.to_string(),
        new_text: new_text.to_string(),
    }
}

fn content(doc: &LoroDoc) -> String {
    read_source_file_versioned(doc, "main.lua").unwrap().content
}

#[test]
fn edits_multiple_ranges_and_unicode_without_touching_surroundings() {
    let doc = source("local greeting = 'नमस्ते'\nlocal size = 13\nreturn greeting\n");
    let before = read_source_file_versioned(&doc, "main.lua").unwrap();
    let result = edit_source_file(
        &doc,
        "main.lua",
        &before.revision,
        &[edit("'नमस्ते'", "'வணக்கம்'"), edit("size = 13", "size = 18")],
    )
    .unwrap();

    assert_eq!(
        content(&doc),
        "local greeting = 'வணக்கம்'\nlocal size = 18\nreturn greeting\n"
    );
    assert_ne!(result.revision, before.revision);
    let reopened = LoroDoc::new();
    reopened.import(&result.snapshot).unwrap();
    assert_eq!(content(&reopened), content(&doc));
}

#[test]
fn rejects_stale_missing_ambiguous_and_overlapping_edits_atomically() {
    let doc = source("alpha beta alpha\n");
    let before = read_source_file_versioned(&doc, "main.lua").unwrap();
    let cases = [
        (
            "wrong revision",
            vec![edit("beta", "x")],
            "stale source revision",
        ),
        (
            before.revision.as_str(),
            vec![edit("missing", "x")],
            "old_text was not found",
        ),
        (
            before.revision.as_str(),
            vec![edit("alpha", "x")],
            "old_text matched more than once",
        ),
        (
            before.revision.as_str(),
            vec![edit("alpha beta", "x"), edit("beta", "y")],
            "source edits overlap",
        ),
    ];
    for (revision, edits, expected) in cases {
        let error = edit_source_file(&doc, "main.lua", revision, &edits).unwrap_err();
        assert!(error.contains(expected), "unexpected error: {error}");
        assert_eq!(content(&doc), before.content, "failure must be atomic");
    }
}

#[test]
fn supports_contextual_insert_delete_and_noop() {
    let doc = source("one\ntwo\nthree\n");
    let before = read_source_file_versioned(&doc, "main.lua").unwrap();
    let inserted = edit_source_file(
        &doc,
        "main.lua",
        &before.revision,
        &[edit("one\ntwo", "one\none-half\ntwo"), edit("three\n", "")],
    )
    .unwrap();
    assert_eq!(content(&doc), "one\none-half\ntwo\n");

    let noop =
        edit_source_file(&doc, "main.lua", &inserted.revision, &[edit("two", "two")]).unwrap();
    assert_eq!(noop.revision, inserted.revision);
}

#[test]
fn rejects_empty_matches_and_missing_files() {
    let doc = source("return 1\n");
    let file = read_source_file_versioned(&doc, "main.lua").unwrap();
    assert!(edit_source_file(&doc, "main.lua", &file.revision, &[edit("", "x")]).is_err());
    assert!(read_source_file_versioned(&doc, "missing.lua").is_err());
}
