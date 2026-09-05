//! The schema -> Lua text. The printer owns formatting entirely (§6), so its rules only have to
//! be *deterministic* — that is what makes `print(lower(parse(print(x)))) == print(x)`.
//!
//! It also stamps `_nid` into every table, which is where provenance comes from: the id reaches
//! the VM as an ordinary field, rides through `walk` onto `El`, and a click resolves to a node
//! with no lookup table anywhere (§4).

use crate::schema::*;
use std::fmt::Write;

pub fn print(b: &Block) -> String {
    let mut out = String::new();
    block(&mut out, b, 0);
    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push('\t');
    }
}

fn block(out: &mut String, b: &Block, depth: usize) {
    for s in &b.stmts {
        for c in &s.leading {
            indent(out, depth);
            let _ = writeln!(out, "{c}");
        }
        indent(out, depth);
        stmt(out, &s.kind, depth);
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
                let _ = write!(out, "return {}", list(vs));
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

fn stmt(out: &mut String, k: &StmtKind, depth: usize) {
    match k {
        StmtKind::Local { names, values } if values.is_empty() => {
            let _ = write!(out, "local {}", names.join(", "));
        }
        StmtKind::Local { names, values } => {
            let _ = write!(out, "local {} = {}", names.join(", "), list(values));
        }
        StmtKind::Assign { targets, values } => {
            let _ = write!(out, "{} = {}", list(targets), list(values));
        }
        StmtKind::Call(e) => out.push_str(&expr(e)),
        StmtKind::If { arms, else_ } => {
            for (i, (cond, body)) in arms.iter().enumerate() {
                let kw = if i == 0 { "if" } else { "elseif" };
                let _ = writeln!(out, "{kw} {} then", expr(cond));
                block(out, body, depth + 1);
                indent(out, depth);
            }
            if let Some(b) = else_ {
                out.push_str("else\n");
                block(out, b, depth + 1);
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
            block(out, body, depth + 1);
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
            let _ = write!(out, "for {name} = {}, {}", expr(from), expr(to));
            if let Some(s) = step {
                let _ = write!(out, ", {}", expr(s));
            }
            out.push_str(" do\n");
            block(out, body, depth + 1);
            indent(out, depth);
            out.push_str("end");
        }
        StmtKind::GenFor { names, exprs, body } => {
            let _ = writeln!(out, "for {} in {} do", names.join(", "), list(exprs));
            block(out, body, depth + 1);
            indent(out, depth);
            out.push_str("end");
        }
        // Verbatim, including its own newlines — an opaque statement is text we do not model.
        StmtKind::Opaque(t) => out.push_str(t),
    }
}

fn list(es: &[Expr]) -> String {
    es.iter().map(expr).collect::<Vec<_>>().join(", ")
}

pub fn expr(e: &Expr) -> String {
    expr_at(e, 0)
}

fn expr_at(e: &Expr, depth: usize) -> String {
    match e {
        Expr::Name(n) => n.clone(),
        Expr::Str(s) | Expr::Num(s) | Expr::Sym(s) | Expr::Opaque(s) => s.clone(),
        Expr::Index {
            base,
            key,
            dot: true,
        } => {
            format!("{}.{}", expr_at(base, depth), expr_at(key, depth))
        }
        Expr::Index {
            base,
            key,
            dot: false,
        } => {
            format!("{}[{}]", expr_at(base, depth), expr_at(key, depth))
        }
        Expr::Call {
            callee,
            method,
            args,
        } => {
            let m = method.as_ref().map(|m| format!(":{m}")).unwrap_or_default();
            let a: Vec<_> = args.iter().map(|a| expr_at(a, depth)).collect();
            format!("{}{m}({})", expr_at(callee, depth), a.join(", "))
        }
        // `not x` needs the space that `-x` and `#x` do not.
        Expr::Un { op, expr: inner } => {
            let sep = if op.chars().next().is_some_and(char::is_alphabetic) {
                " "
            } else {
                ""
            };
            format!("{op}{sep}{}", expr_at(inner, depth))
        }
        Expr::Bin { op, lhs, rhs } => {
            format!("{} {op} {}", expr_at(lhs, depth), expr_at(rhs, depth))
        }
        Expr::Paren(inner) => format!("({})", expr_at(inner, depth)),
        Expr::Fn { params, body } => {
            let mut s = format!("function({})\n", params.join(", "));
            block(&mut s, body, depth + 1);
            for _ in 0..depth {
                s.push('\t');
            }
            s.push_str("end");
            s
        }
        Expr::Table(t) => table(t, depth),
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

fn entry(e: &Entry, depth: usize) -> String {
    match &e.kind {
        EntryKind::Named { name, value } => format!("{name} = {}", expr_at(value, depth)),
        EntryKind::Positional(v) => expr_at(v, depth),
    }
}

fn table(t: &Table, depth: usize) -> String {
    let nid = format!("{NID_KEY} = \"{}\"", t.id);
    if !multiline(t) {
        let mut parts: Vec<String> = t.entries.iter().map(|e| entry(e, depth)).collect();
        parts.push(nid);
        return format!("{{ {} }}", parts.join(", "));
    }

    let mut s = String::from("{\n");
    for e in &t.entries {
        for c in &e.leading {
            indent(&mut s, depth + 1);
            let _ = writeln!(s, "{c}");
        }
        indent(&mut s, depth + 1);
        s.push_str(&entry(e, depth + 1));
        s.push(',');
        if let Some(c) = &e.trailing {
            let _ = write!(s, " {c}");
        }
        s.push('\n');
    }
    indent(&mut s, depth + 1);
    let _ = writeln!(s, "{nid},");
    indent(&mut s, depth);
    s.push('}');
    s
}
