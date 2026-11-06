//! The rules evaluated (`owned-entries.md` §2.3): one engine, run in the writer's shell before
//! a commit, on the host before a merge, and on every pull. `write` is the permit; each
//! `forbid` that reaches an op narrows it. An op is admitted when no forbid is satisfied, a
//! change when every op is.
//!
//! Semantics pinned here, not left to the implementation: `and`/`or` short-circuit left to
//! right and never absorb an error; any error inside a rule — a missing operand, a type
//! mismatch — makes that rule refuse (a `where` that errors is satisfied, an `unless` that
//! errors lifts nothing), unlike Cedar's skip and CEL's `error || true`. `len` counts Unicode
//! scalars. Cost is the size of the rule: no recursion, no quantifier, no lookup beyond the
//! entry the op is about.

use std::collections::BTreeSet;

use loro::{LoroDoc, LoroValue, PeerID};
use manifest::{DocDecl, Forbid, Lit, Manifest, OpKind, Operand, Pred};

use crate::ops::{Kind, Op, Read};
use crate::{CourierError, Result};

/// Who is writing: the device's peer and the account's roles on this doc.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub peer: PeerID,
    pub roles: BTreeSet<String>,
}

/// Every op of `read` against `decl`'s rules, `before` being the doc the update was read
/// against. The first refusal names the rule's line and the op.
pub fn check(manifest: &Manifest, decl: &DocDecl, actor: &Actor, before: &LoroDoc, read: &Read) -> Result<()> {
    for op in &read.ops {
        for rule in &decl.rules {
            if !rule.path.covers(&op.path) {
                continue;
            }
            let depth = rule.path.depth(&op.path).min(op.path.len());
            let entry = op.path[..depth].join("/");
            // Seen from the entry the rule is about: an op inside it changes it, or is part
            // of inserting it when it was not there before.
            let kind = if op.path.len() > depth {
                if before.get_by_str_path(&entry).is_some() {
                    OpKind::Set
                } else {
                    OpKind::Insert
                }
            } else {
                kind_of(op.kind)
            };
            let ctx = Ctx {
                manifest,
                actor,
                op,
                entry,
                before,
                after: &read.after,
            };
            for forbid in &rule.forbids {
                if forbid.ops.contains(&kind) && applies(forbid, &ctx) {
                    return Err(CourierError::Forbidden(format!(
                        "manifest line {}: {} {}",
                        forbid.line,
                        verb(kind),
                        op.path.join("/")
                    )));
                }
            }
        }
    }
    Ok(())
}

/// A text edit inside an entry is an edit of the entry.
fn kind_of(kind: Kind) -> OpKind {
    match kind {
        Kind::Insert => OpKind::Insert,
        Kind::Set | Kind::Text => OpKind::Set,
        Kind::Delete => OpKind::Delete,
        Kind::Move => OpKind::Move,
    }
}

fn verb(kind: OpKind) -> &'static str {
    match kind {
        OpKind::Insert => "may not insert",
        OpKind::Set => "may not change",
        OpKind::Delete => "may not delete",
        OpKind::Move => "may not move",
    }
}

struct Ctx<'a> {
    manifest: &'a Manifest,
    actor: &'a Actor,
    op: &'a Op,
    /// The entry the rule is about, as a `/` path into either doc.
    entry: String,
    before: &'a LoroDoc,
    after: &'a LoroDoc,
}

/// Errors refuse: `where` holds unless it is false, `unless` lifts only when it is true.
fn applies(forbid: &Forbid, ctx: &Ctx) -> bool {
    let held = forbid.when.as_ref().is_none_or(|p| eval(p, ctx) != Ok(false));
    let lifted = forbid.unless.as_ref().is_some_and(|p| eval(p, ctx) == Ok(true));
    held && !lifted
}

/// What a rule can't make sense of. The kind is for a reader of the refusal, never branched on.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Fault {
    Missing,
    Type,
}

type Eval<T> = std::result::Result<T, Fault>;

fn eval(pred: &Pred, ctx: &Ctx) -> Eval<bool> {
    Ok(match pred {
        Pred::And(a, b) => eval(a, ctx)? && eval(b, ctx)?,
        Pred::Or(a, b) => eval(a, ctx)? || eval(b, ctx)?,
        Pred::Not(a) => !eval(a, ctx)?,
        Pred::Is(a, b) => equal(&value(a, ctx)?, &value(b, ctx)?)?,
        Pred::IsNot(a, b) => !equal(&value(a, ctx)?, &value(b, ctx)?)?,
        Pred::In(a, lits) => {
            let a = value(a, ctx)?;
            let mut hit = false;
            for lit in lits {
                hit |= equal(&a, &Val::Lit(lit.clone()))?;
            }
            hit
        }
        Pred::Above(a, lit) => int(&value(a, ctx)?)? > int(&Val::Lit(lit.clone()))?,
        Pred::Below(a, lit) => int(&value(a, ctx)?)? < int(&Val::Lit(lit.clone()))?,
        Pred::Missing(a) => matches!(operand(a, ctx), Val::Missing),
        Pred::Has(role) => ctx
            .actor
            .roles
            .iter()
            .any(|held| ctx.manifest.satisfies(held, role)),
        Pred::Changed(field) => field_of(ctx.before, ctx, field) != field_of(ctx.after, ctx, field),
        Pred::ChangedOnly(fields) => {
            let before = record(ctx.before, ctx)?;
            let after = record(ctx.after, ctx)?;
            let keys = before.keys().chain(after.keys());
            keys.filter(|k| before.get(*k) != after.get(*k))
                .all(|k| fields.iter().any(|f| f == k))
        }
        Pred::From(field, pairs) => {
            let (from, to) = (
                field_of(ctx.before, ctx, field),
                field_of(ctx.after, ctx, field),
            );
            let mut hit = false;
            for (a, b) in pairs {
                hit |= equal_or_missing(&from, a)? && equal_or_missing(&to, b)?;
            }
            hit
        }
    })
}

#[derive(Debug, Clone, PartialEq)]
enum Val {
    Peer(PeerID),
    Lit(Lit),
    Value(LoroValue),
    Missing,
}

/// An operand as a value; `Missing` only where `is missing` can ask about it.
fn operand(which: &Operand, ctx: &Ctx) -> Val {
    match which {
        Operand::Actor => Val::Peer(ctx.actor.peer),
        // A new entry's creator is whoever is inserting it.
        Operand::Creator => Val::Peer(ctx.op.touches.unwrap_or(ctx.op.author)),
        Operand::Before(f) => field_of(ctx.before, ctx, f),
        Operand::After(f) => field_of(ctx.after, ctx, f),
        Operand::Len(inner) => match len(&operand(inner, ctx)) {
            Some(n) => Val::Lit(Lit::Int(n)),
            None => Val::Missing,
        },
        Operand::Lit(lit) => Val::Lit(lit.clone()),
    }
}

/// An operand that must be there: a comparison with nothing is a fault, not false.
fn value(operand_: &Operand, ctx: &Ctx) -> Eval<Val> {
    match operand(operand_, ctx) {
        Val::Missing => Err(Fault::Missing),
        v => Ok(v),
    }
}

fn field_of(doc: &LoroDoc, ctx: &Ctx, field: &str) -> Val {
    match doc.get_by_str_path(&format!("{}/{field}", ctx.entry)) {
        Some(v) => Val::Value(v.get_deep_value()),
        None => Val::Missing,
    }
}

/// The entry as a map of its fields, empty when it is not there.
fn record(doc: &LoroDoc, ctx: &Ctx) -> Eval<std::collections::HashMap<String, LoroValue>> {
    match doc.get_by_str_path(&ctx.entry).map(|v| v.get_deep_value()) {
        None => Ok(Default::default()),
        Some(LoroValue::Map(m)) => Ok(m.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()),
        Some(_) => Err(Fault::Type),
    }
}

/// Unicode scalars of a string, elements of a list, keys of a map.
fn len(v: &Val) -> Option<i64> {
    let n = match v {
        Val::Lit(Lit::Str(s)) => s.chars().count(),
        Val::Value(LoroValue::String(s)) => s.chars().count(),
        Val::Value(LoroValue::List(l)) => l.len(),
        Val::Value(LoroValue::Map(m)) => m.len(),
        _ => return None,
    };
    Some(n as i64)
}

fn int(v: &Val) -> Eval<i64> {
    match v {
        Val::Lit(Lit::Int(i)) => Ok(*i),
        Val::Value(LoroValue::I64(i)) => Ok(*i),
        _ => Err(Fault::Type),
    }
}

/// Equality across the value kinds a rule can name; a comparison of unlike kinds is a fault.
fn equal(a: &Val, b: &Val) -> Eval<bool> {
    Ok(match (a, b) {
        (Val::Peer(x), Val::Peer(y)) => x == y,
        (Val::Lit(x), Val::Lit(y)) => x == y,
        (Val::Value(x), Val::Value(y)) => x == y,
        (Val::Lit(lit), Val::Value(v)) | (Val::Value(v), Val::Lit(lit)) => match (lit, v) {
            (Lit::Str(s), LoroValue::String(t)) => s.as_str() == t.as_ref(),
            (Lit::Int(i), LoroValue::I64(j)) => i == j,
            (Lit::Bool(b), LoroValue::Bool(c)) => b == c,
            _ => return Err(Fault::Type),
        },
        _ => return Err(Fault::Type),
    })
}

/// For a transition: a field that is not there matches nothing, which is not a fault.
fn equal_or_missing(v: &Val, lit: &Lit) -> Eval<bool> {
    match v {
        Val::Missing => Ok(false),
        v => equal(v, &Val::Lit(lit.clone())),
    }
}

#[cfg(test)]
mod tests;
