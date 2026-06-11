//! Tests for the pure Lua splitter. The load-bearing invariant is the round-trip:
//! `emit(&split(src)) == src` for *every* input — that is what lets the CRDT store blocks (P2) and
//! still feed the VM byte-exact source.

use super::*;

/// The construct (non-comment) blocks, as `(kind, text)` — the easy shape to assert structure on
/// without pinning exactly how whitespace partitions into comment blocks.
fn constructs(blocks: &[Block]) -> Vec<(BlockKind, &str)> {
    blocks
        .iter()
        .filter(|b| b.kind != BlockKind::Comment)
        .map(|b| (b.kind, b.text.as_str()))
        .collect()
}

/// Round-trip must hold for arbitrary input — the splitter's core promise.
fn assert_round_trips(src: &str) {
    assert_eq!(emit(&split(src)), src, "split→emit must reproduce the source exactly");
}

const SAMPLE: &str = "\
-- header
local x = 1

function greet(name)
  print(\"hi \" .. name)
end

local function helper() return 1 end

greet(\"world\")

if x then print(x) end
";

#[test]
fn round_trips_a_representative_program() {
    assert_round_trips(SAMPLE);
}

#[test]
fn splits_top_level_constructs() {
    // One block per top-level construct, in order, with clean (no leading-whitespace) bodies.
    assert_eq!(
        constructs(&split(SAMPLE)),
        [
            (BlockKind::Statement, "local x = 1"),
            (BlockKind::Function, "function greet(name)\n  print(\"hi \" .. name)\nend"),
            (BlockKind::Function, "local function helper() return 1 end"),
            (BlockKind::Statement, "greet(\"world\")"),
            (BlockKind::Statement, "if x then print(x) end"),
        ]
    );
}

#[test]
fn function_bodies_stay_plain_text_in_one_block() {
    // The nested `print(...)` call inside `greet` is NOT its own block — bodies are flat text.
    let funcs: Vec<_> = split(SAMPLE)
        .into_iter()
        .filter(|b| b.kind == BlockKind::Function)
        .collect();
    assert_eq!(funcs.len(), 2);
    assert!(funcs[0].text.starts_with("function greet"));
    assert!(funcs[0].text.contains("print(\"hi \" .. name)"));
    assert!(funcs[0].text.ends_with("end"));
}

#[test]
fn standalone_comments_become_comment_blocks() {
    let blocks = split("-- a\n-- b\nlocal x = 1\n");
    // The two comment lines (and the surrounding newlines) coalesce into a single comment block
    // ahead of the one statement.
    assert_eq!(blocks[0].kind, BlockKind::Comment);
    assert_eq!(blocks[0].text, "-- a\n-- b\n");
    assert_eq!(constructs(&blocks), [(BlockKind::Statement, "local x = 1")]);
    assert_round_trips("-- a\n-- b\nlocal x = 1\n");
}

#[test]
fn trailing_comment_on_a_statement_line_is_its_own_block() {
    // The grammar leaves ` -- note` outside the statement node, so it lands in a comment block.
    let src = "x = 1 -- note\n";
    assert_eq!(constructs(&split(src)), [(BlockKind::Statement, "x = 1")]);
    assert_round_trips(src);
}

#[test]
fn empty_source_yields_no_blocks() {
    assert!(split("").is_empty());
    assert_eq!(emit(&[]), "");
}

#[test]
fn whitespace_or_comment_only_round_trips() {
    assert_round_trips("\n\n  \n");
    assert_round_trips("-- just a comment, no code\n");
    // Whitespace-only has no constructs — it is one comment block.
    let ws = split("\n\n  \n");
    assert_eq!(ws.len(), 1);
    assert_eq!(ws[0].kind, BlockKind::Comment);
}

#[test]
fn invalid_lua_still_round_trips() {
    // Byte-exact slicing means even a parse error never loses or reorders source.
    let src = "function oops( then end ]] = = local\n!!!\n";
    assert_round_trips(src);
}

#[test]
fn no_trailing_newline_round_trips() {
    assert_round_trips("local x = 1");
    assert_eq!(constructs(&split("local x = 1")), [(BlockKind::Statement, "local x = 1")]);
}

#[test]
fn emit_is_plain_concatenation() {
    let blocks = vec![
        Block { kind: BlockKind::Comment, text: "-- c\n".into() },
        Block { kind: BlockKind::Function, text: "function f() end".into() },
        Block { kind: BlockKind::Comment, text: "\n".into() },
    ];
    assert_eq!(emit(&blocks), "-- c\nfunction f() end\n");
}
