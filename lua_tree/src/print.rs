//! The schema -> Lua text. The printer owns formatting entirely (§6), so its rules only have to
//! be *deterministic* — that is what makes `print(lower(parse(print(x)))) == print(x)`.
//!
//! [`print`] stamps `_nid` into every table, which §4 expected to be where provenance comes from:
//! the id reaches the VM as an ordinary field, rides through `walk` onto `El`, and a click resolves
//! to a node with no lookup table anywhere. That does not hold — a table constructor is not always
//! an element, and `app_host`'s `round_trip` tests show the seed rejecting the key before `view()`
//! is ever reached. [`print_bare`] is the same text without it, and is what runs today.

use crate::schema::*;
use std::fmt::Write;

pub fn print(b: &Block) -> String {
    let mut out = String::new();
    block(&mut out, b, 0, true);
    out
}

/// The same text without the `_nid` entries — the tree's shape, printed for a reader or a VM
/// rather than for a reparse.
///
/// It exists because stamping ids into source is a claim about *every* table, and not every table
/// is an element: `doc.list({ … })` rejects named keys on purpose, so printed source with ids in
/// it does not run (see `app_host`'s `round_trip` tests). Under §12's level 3 identity lives on the
/// Loro node and the text carries none of it, which is exactly this function.
pub fn print_bare(b: &Block) -> String {
    let mut out = String::new();
    block(&mut out, b, 0, false);
    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push('\t');
    }
}

fn block(out: &mut String, b: &Block, depth: usize, ids: bool) {
    for s in &b.stmts {
        for c in &s.leading {
            indent(out, depth);
            let _ = writeln!(out, "{c}");
        }
        indent(out, depth);
        stmt(out, &s.kind, depth, ids);
        if let Some(c) = &s.trailing {
            let _ = write!(out, " {c}");
        }
        out.push('\n');
    }
    if let Some(l) = &b.last {
        for c in &l.leading {
            indent(out, depth);
            let _ = writeln!(out, "{c}");
        }
        indent(out, depth);
        match &l.kind {
            LastKind::Return(vs) if vs.is_empty() => out.push_str("return"),
            LastKind::Return(vs) => {
                let _ = write!(out, "return {}", list(vs, ids));
            }
            LastKind::Break => out.push_str("break"),
            LastKind::Continue => out.push_str("continue"),
            LastKind::Opaque(t) => out.push_str(t),
        }
        if let Some(c) = &l.trailing {
            let _ = write!(out, " {c}");
        }
        out.push('\n');
    }
    for c in &b.trailing {
        indent(out, depth);
        let _ = writeln!(out, "{c}");
    }
}

fn stmt(out: &mut String, k: &StmtKind, depth: usize, ids: bool) {
    match k {
        StmtKind::Local { names, values } if values.is_empty() => {
            let _ = write!(out, "local {}", names.join(", "));
        }
        StmtKind::Local { names, values } => {
            let _ = write!(out, "local {} = {}", names.join(", "), list(values, ids));
        }
        StmtKind::Assign { targets, values } => {
            let _ = write!(out, "{} = {}", list(targets, ids), list(values, ids));
        }
        StmtKind::Call(e) => out.push_str(&expr_at(e, depth, ids)),
        StmtKind::If { arms, else_ } => {
            for (i, (cond, body)) in arms.iter().enumerate() {
                let kw = if i == 0 { "if" } else { "elseif" };
                let _ = writeln!(out, "{kw} {} then", expr_at(cond, depth, ids));
                block(out, body, depth + 1, ids);
                indent(out, depth);
            }
            if let Some(b) = else_ {
                out.push_str("else\n");
                block(out, b, depth + 1, ids);
                indent(out, depth);
            }
            out.push_str("end");
        }
        StmtKind::Func {
            local,
            name,
            params,
            body,
        } => {
            let lead = if *local { "local function" } else { "function" };
            let _ = writeln!(out, "{lead} {name}({})", params.join(", "));
            block(out, body, depth + 1, ids);
            indent(out, depth);
            out.push_str("end");
        }
        StmtKind::NumFor {
            name,
            from,
            to,
            step,
            body,
        } => {
            let _ = write!(
                out,
                "for {name} = {}, {}",
                expr_at(from, depth, ids),
                expr_at(to, depth, ids)
            );
            if let Some(s) = step {
                let _ = write!(out, ", {}", expr_at(s, depth, ids));
            }
            out.push_str(" do\n");
            block(out, body, depth + 1, ids);
            indent(out, depth);
            out.push_str("end");
        }
        StmtKind::GenFor { names, exprs, body } => {
            let _ = writeln!(out, "for {} in {} do", names.join(", "), list(exprs, ids));
            block(out, body, depth + 1, ids);
            indent(out, depth);
            out.push_str("end");
        }
        // Verbatim, including its own newlines — an opaque statement is text we do not model.
        StmtKind::Opaque(t) => out.push_str(t),
    }
}

fn list(es: &[Expr], ids: bool) -> String {
    es.iter()
        .map(|e| expr_at(e, 0, ids))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn expr(e: &Expr) -> String {
    expr_at(e, 0, true)
}

fn expr_at(e: &Expr, depth: usize, ids: bool) -> String {
    match e {
        Expr::Name(n) => n.clone(),
        Expr::Str(s) | Expr::Num(s) | Expr::Sym(s) | Expr::Opaque(s) => s.clone(),
        Expr::Index {
            base,
            key,
            dot: true,
        } => {
            format!("{}.{}", expr_at(base, depth, ids), expr_at(key, depth, ids))
        }
        Expr::Index {
            base,
            key,
            dot: false,
        } => {
            format!(
                "{}[{}]",
                expr_at(base, depth, ids),
                expr_at(key, depth, ids)
            )
        }
        Expr::Call {
            callee,
            method,
            args,
        } => {
            let m = method.as_ref().map(|m| format!(":{m}")).unwrap_or_default();
            let a: Vec<_> = args.iter().map(|a| expr_at(a, depth, ids)).collect();
            format!("{}{m}({})", expr_at(callee, depth, ids), a.join(", "))
        }
        // `not x` needs the space that `-x` and `#x` do not.
        Expr::Un { op, expr: inner } => {
            let sep = if op.chars().next().is_some_and(char::is_alphabetic) {
                " "
            } else {
                ""
            };
            format!("{op}{sep}{}", expr_at(inner, depth, ids))
        }
        Expr::Bin { op, lhs, rhs } => {
            format!(
                "{} {op} {}",
                expr_at(lhs, depth, ids),
                expr_at(rhs, depth, ids)
            )
        }
        Expr::Paren(inner) => format!("({})", expr_at(inner, depth, ids)),
        Expr::Fn { params, body } => {
            let mut s = format!("function({})\n", params.join(", "));
            block(&mut s, body, depth + 1, ids);
            for _ in 0..depth {
                s.push('\t');
            }
            s.push_str("end");
            s
        }
        Expr::Table(t) => table(t, depth, ids),
    }
}

/// One rule, applied the same way every time — anything else would break idempotence. The `_nid`
/// entry is excluded from the decision because it is re-added on print and consumed on parse, so
/// counting it would make the second print disagree with the first.
fn multiline(t: &Table) -> bool {
    t.entries.len() > 3
        || t.entries.iter().any(|e| {
            !e.leading.is_empty()
                || e.trailing.is_some()
                || matches!(value(e), Expr::Table(_) | Expr::Fn { .. })
        })
}

fn value(e: &Entry) -> &Expr {
    match &e.kind {
        EntryKind::Named { value, .. } | EntryKind::Positional(value) => value,
    }
}

fn entry(e: &Entry, depth: usize, ids: bool) -> String {
    match &e.kind {
        EntryKind::Named { name, value } => format!("{name} = {}", expr_at(value, depth, ids)),
        EntryKind::Positional(v) => expr_at(v, depth, ids),
    }
}

fn table(t: &Table, depth: usize, ids: bool) -> String {
    let nid = format!("{NID_KEY} = \"{}\"", t.id);
    if !multiline(t) {
        let mut parts: Vec<String> = t.entries.iter().map(|e| entry(e, depth, ids)).collect();
        if ids {
            parts.push(nid);
        }
        if parts.is_empty() {
            return "{}".into();
        }
        return format!("{{ {} }}", parts.join(", "));
    }

    let mut s = String::from("{\n");
    for e in &t.entries {
        for c in &e.leading {
            indent(&mut s, depth + 1);
            let _ = writeln!(s, "{c}");
        }
        indent(&mut s, depth + 1);
        s.push_str(&entry(e, depth + 1, ids));
        s.push(',');
        if let Some(c) = &e.trailing {
            let _ = write!(s, " {c}");
        }
        s.push('\n');
    }
    if ids {
        indent(&mut s, depth + 1);
        let _ = writeln!(s, "{nid},");
    }
    indent(&mut s, depth);
    s.push('}');
    s
}
