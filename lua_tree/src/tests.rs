//! The spike's pass conditions, run against the real corpus: `shell2/src/kanban`, every line of
//! Lua this implementation has (docs/design/code-as-tree.md §9).
//!
//! `app_engine/examples` is deliberately absent — it is sthalam's DSL from the previous
//! implementation, useful as corroboration and wrong as a fixture.

use crate::schema::*;
use crate::{parse, print};
use full_moon::node::Node;
use full_moon::tokenizer::TokenType;

fn corpus() -> Vec<(String, String)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../shell2/src/kanban");
    let mut out = Vec::new();
    collect(std::path::Path::new(dir), &mut out);
    out.sort();
    assert!(!out.is_empty(), "corpus is empty — has kanban moved?");
    out
}

fn collect(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            collect(&p, out);
        } else if p.extension().is_some_and(|x| x == "lua") {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            out.push((name, std::fs::read_to_string(&p).unwrap()));
        }
    }
}

/// Read out of the source text rather than out of our own tree, so this cannot agree with the
/// lowering by construction. Sorted, because the printer is free to move a comment as long as it
/// does not lose one.
fn comments(src: &str) -> Vec<String> {
    let ast = full_moon::parse(src).expect("corpus parses");
    let is_comment = |t: &&full_moon::tokenizer::Token| {
        matches!(
            t.token_type(),
            TokenType::SingleLineComment { .. } | TokenType::MultiLineComment { .. }
        )
    };
    let of = |t: &full_moon::tokenizer::TokenReference| {
        t.leading_trivia()
            .chain(t.trailing_trivia())
            .filter(is_comment)
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
    };
    let mut out: Vec<String> = ast.nodes().tokens().flat_map(of).collect();
    out.extend(of(ast.eof()));
    out.sort();
    out
}

fn opaques(b: &Block) -> usize {
    let mut n = 0;
    for s in &b.stmts {
        n += match &s.kind {
            StmtKind::Opaque(_) => 1,
            StmtKind::Local { values, .. } => values.iter().map(opaques_expr).sum(),
            StmtKind::Assign { targets, values } => {
                targets.iter().chain(values).map(opaques_expr).sum()
            }
            StmtKind::Call(e) => opaques_expr(e),
            StmtKind::If { arms, else_ } => {
                arms.iter()
                    .map(|(c, b)| opaques_expr(c) + opaques(b))
                    .sum::<usize>()
                    + else_.as_ref().map_or(0, opaques)
            }
            StmtKind::Func { body, .. } => opaques(body),
            StmtKind::NumFor {
                from,
                to,
                step,
                body,
                ..
            } => {
                opaques_expr(from)
                    + opaques_expr(to)
                    + step.as_ref().map_or(0, opaques_expr)
                    + opaques(body)
            }
            StmtKind::GenFor { exprs, body, .. } => {
                exprs.iter().map(opaques_expr).sum::<usize>() + opaques(body)
            }
        };
    }
    match b.last.as_ref().map(|l| &l.kind) {
        Some(LastKind::Return(vs)) => n + vs.iter().map(opaques_expr).sum::<usize>(),
        Some(LastKind::Opaque(_)) => n + 1,
        _ => n,
    }
}

fn opaques_expr(e: &Expr) -> usize {
    match e {
        Expr::Opaque(_) => 1,
        Expr::Index { base, key, .. } => opaques_expr(base) + opaques_expr(key),
        Expr::Call { callee, args, .. } => {
            opaques_expr(callee) + args.iter().map(opaques_expr).sum::<usize>()
        }
        Expr::Bin { lhs, rhs, .. } => opaques_expr(lhs) + opaques_expr(rhs),
        Expr::Un { expr, .. } | Expr::Paren(expr) => opaques_expr(expr),
        Expr::Fn { body, .. } => opaques(body),
        Expr::Table(t) => t
            .entries
            .iter()
            .map(|e| match &e.kind {
                EntryKind::Named { value, .. } | EntryKind::Positional(value) => {
                    opaques_expr(value)
                }
            })
            .sum(),
        Expr::Name(_) | Expr::Str(_) | Expr::Num(_) | Expr::Sym(_) => 0,
    }
}

fn ids(b: &Block) -> Vec<String> {
    b.tables().iter().map(|t| t.id.clone()).collect()
}

// ---------------------------------------------------------------- pass conditions

/// Condition 1. Not byte-identical to the input — the printer normalises formatting and that is
/// intended — but the *second* print must equal the first, or the tree is not a stable artifact.
#[test]
fn print_is_idempotent() {
    for (name, src) in corpus() {
        let once = print(&parse(&src).expect(&name));
        let twice = print(&parse(&once).expect(&name));
        assert_eq!(once, twice, "{name}: second print differs from the first");
    }
}

/// Condition 2. Losing commentary is the classic way a round trip fails, and it fails quietly.
#[test]
fn comments_survive() {
    for (name, src) in corpus() {
        let printed = print(&parse(&src).expect(&name));
        let (before, after) = (comments(&src), comments(&printed));
        let lost: Vec<_> = before.iter().filter(|c| !after.contains(c)).collect();
        assert!(lost.is_empty(), "{name}: lost {lost:#?}");
        assert_eq!(before.len(), after.len(), "{name}: comment count changed");
    }
}

/// Condition 3, in its weak form: the output is still Lua. The strong form — load it in Luau,
/// call `view()`, compare the element tree — needs `app_host` and is the next thing to add.
#[test]
fn printed_source_still_parses() {
    for (name, src) in corpus() {
        let printed = print(&parse(&src).expect(&name));
        assert!(
            full_moon::parse(&printed).is_ok(),
            "{name}: printed source does not parse"
        );
    }
}

/// Condition 4. Opaque is safe for correctness and expensive for capability — an opaque region
/// carries no ids, so everything inside it is dark to the UI (§10.7). This asserts the corpus is
/// fully modelled, and fails the day an app reaches for something the schema has no case for.
#[test]
fn nothing_in_the_corpus_is_opaque() {
    for (name, src) in corpus() {
        assert_eq!(
            opaques(&parse(&src).expect(&name)),
            0,
            "{name}: fell back to Opaque"
        );
    }
}

/// The mechanism the whole design rests on: an id, once printed, comes back as the same id. If
/// this fails, every stored reference breaks on reload rather than on reimport.
#[test]
fn ids_survive_a_reprint() {
    for (name, src) in corpus() {
        let first = parse(&src).expect(&name);
        let printed = print(&first);
        let second = parse(&printed).expect(&name);
        assert_eq!(ids(&first), ids(&second), "{name}: ids were reissued");
        assert!(!ids(&first).is_empty(), "{name}: no tables found at all");
    }
}

/// Reimport is the one path that *should* reset identity, and §3 says so out loud. Same text,
/// separate import, different ids.
#[test]
fn a_fresh_import_gets_fresh_ids() {
    let src = "return { a = 1 }";
    assert_ne!(ids(&parse(src).unwrap()), ids(&parse(src).unwrap()));
}

// ---------------------------------------------------------------- shape

/// Positional entries are the child list, so their order is meaning and not formatting. The
/// interleaving here is deliberate: it is neither sorted by name nor grouped by kind, so any
/// reordering — the tempting "named fields first, children after" included — shows up.
#[test]
fn a_table_is_an_ordered_list_of_entries() {
    let b = parse(r#"return ui.col({ z = 1, ui.text("a"), a = 2, ui.text("b") })"#).unwrap();
    let t = b.tables()[0];
    let kinds: Vec<_> = t
        .entries
        .iter()
        .map(|e| match &e.kind {
            EntryKind::Named { name, .. } => name.clone(),
            EntryKind::Positional(_) => "<pos>".into(),
        })
        .collect();
    assert_eq!(kinds, ["z", "<pos>", "a", "<pos>"]);
}

#[test]
fn dotted_access_and_calls_are_the_same_fold() {
    let b = parse("local x = ui.text({ 1 })").unwrap();
    let StmtKind::Local { values, .. } = &b.stmts[0].kind else {
        panic!()
    };
    let Expr::Call { callee, .. } = &values[0] else {
        panic!("expected a call")
    };
    assert!(matches!(**callee, Expr::Index { dot: true, .. }));
}

#[test]
fn parens_are_kept_because_they_change_meaning() {
    let flat = print(&parse("return a + b * c").unwrap());
    let bracketed = print(&parse("return (a + b) * c").unwrap());
    assert_ne!(flat, bracketed);
    assert!(bracketed.contains("(a + b)"));
}

#[test]
fn an_unmodelled_construct_becomes_opaque_and_still_round_trips() {
    // `while` is in the language and not in the schema — it must survive anyway.
    let src = "while running do\n\tstep()\nend\n";
    let b = parse(src).unwrap();
    assert_eq!(opaques(&b), 1);
    assert!(print(&b).contains("while running do"));
    assert!(full_moon::parse(&print(&b)).is_ok());
}

#[test]
fn import_rejects_malformed_source() {
    // The reason full-moon and not tree-sitter: a typo must not become an opaque node.
    assert!(parse("local x = ").is_err());
    assert!(parse("function f()\n\treturn 1\n").is_err());
}
