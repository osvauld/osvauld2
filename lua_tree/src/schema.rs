//! The node types. Drawn from a census of `shell2/src/kanban` rather than the Lua grammar,
//! so the variants here are the constructs that code actually uses; everything else lowers
//! to `Opaque` and round-trips as text (docs/design/code-as-tree.md §10.7).

/// A node id. Ours, not Loro's — these are printed into source that people read and quote,
/// so they have to mean the same thing on every peer.
pub type Nid = String;

/// A file body or a function body.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub last: Option<Last>,
    /// Comments sitting after the final statement, before `end`. They belong to no statement,
    /// and dropping them is the classic way a round trip loses a file's commentary.
    pub trailing: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub leading: Vec<String>,
    pub trailing: Option<String>,
    pub kind: StmtKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Local {
        names: Vec<String>,
        values: Vec<Expr>,
    },
    Assign {
        targets: Vec<Expr>,
        values: Vec<Expr>,
    },
    /// A call in statement position, e.g. `update(x)`.
    Call(Expr),
    If {
        arms: Vec<(Expr, Block)>,
        else_: Option<Block>,
    },
    Func {
        local: bool,
        name: String,
        params: Vec<String>,
        body: Block,
    },
    NumFor {
        name: String,
        from: Expr,
        to: Expr,
        step: Option<Expr>,
        body: Block,
    },
    GenFor {
        names: Vec<String>,
        exprs: Vec<Expr>,
        body: Block,
    },
    /// Verbatim source for a construct the schema has no case for. Prints back exactly, and
    /// carries its own trivia — so `leading`/`trailing` stay empty beside it.
    Opaque(String),
}

/// Carries trivia for the same reason `Stmt` does: `-- ROOT` above a file's `return function()`
/// is a comment on the last statement, and a `Last` without leading trivia has nowhere to put it.
#[derive(Clone, Debug, PartialEq)]
pub struct Last {
    pub leading: Vec<String>,
    pub trailing: Option<String>,
    pub kind: LastKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LastKind {
    Return(Vec<Expr>),
    Break,
    Continue,
    Opaque(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Name(String),
    Index {
        base: Box<Expr>,
        key: Box<Expr>,
        dot: bool,
    },
    Call {
        callee: Box<Expr>,
        method: Option<String>,
        args: Vec<Expr>,
    },
    /// Source spelling, delimiters included. See the note on `Num`.
    Str(String),
    /// Source spelling, not an `f64`. Luau numbers are all doubles so parsing would be lossless
    /// in *value*, but `0xff` and `1e3` would not survive as written — and the v1 lowering keeps
    /// literals byte-exact so that a round-trip failure means a structural bug, never an
    /// escaping one. Normalising these is a later tightening, not a redesign.
    Num(String),
    Sym(String),
    Table(Table),
    Bin {
        op: String,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Un {
        op: String,
        expr: Box<Expr>,
    },
    Fn {
        params: Vec<String>,
        body: Block,
    },
    Paren(Box<Expr>),
    Opaque(String),
}

/// The only node with a printed id, because it is the only one that can hold a field and the
/// only one a click can land on — in this DSL an element *is* a table constructor (§10.2).
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub id: Nid,
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub leading: Vec<String>,
    pub trailing: Option<String>,
    pub kind: EntryKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EntryKind {
    Named { name: String, value: Expr },
    Positional(Expr),
}

/// The field the printer stamps ids into, and the one the lowering consumes back out. Reading
/// it on the way in is what makes ids survive a print/reparse cycle instead of being reissued.
pub const NID_KEY: &str = "_nid";

pub fn new_nid() -> Nid {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

impl Table {
    pub fn get(&self, name: &str) -> Option<&Expr> {
        self.entries.iter().find_map(|e| match &e.kind {
            EntryKind::Named { name: n, value } if n == name => Some(value),
            _ => None,
        })
    }
}

impl Block {
    /// Every table in the block, depth first. The lookup behind `set`/`insert`/`replace`.
    pub fn tables(&self) -> Vec<&Table> {
        let mut out = Vec::new();
        walk_block(self, &mut |t| out.push(t));
        out
    }
}

fn walk_block<'a>(b: &'a Block, f: &mut impl FnMut(&'a Table)) {
    for s in &b.stmts {
        walk_stmt(s, f);
    }
    if let Some(Last {
        kind: LastKind::Return(vs),
        ..
    }) = &b.last
    {
        for v in vs {
            walk_expr(v, f);
        }
    }
}

fn walk_stmt<'a>(s: &'a Stmt, f: &mut impl FnMut(&'a Table)) {
    match &s.kind {
        StmtKind::Local { values, .. } => values.iter().for_each(|v| walk_expr(v, f)),
        StmtKind::Assign { targets, values } => {
            targets.iter().chain(values).for_each(|v| walk_expr(v, f))
        }
        StmtKind::Call(e) => walk_expr(e, f),
        StmtKind::If { arms, else_ } => {
            for (c, b) in arms {
                walk_expr(c, f);
                walk_block(b, f);
            }
            if let Some(b) = else_ {
                walk_block(b, f);
            }
        }
        StmtKind::Func { body, .. } => walk_block(body, f),
        StmtKind::NumFor {
            from,
            to,
            step,
            body,
            ..
        } => {
            walk_expr(from, f);
            walk_expr(to, f);
            if let Some(s) = step {
                walk_expr(s, f);
            }
            walk_block(body, f);
        }
        StmtKind::GenFor { exprs, body, .. } => {
            exprs.iter().for_each(|v| walk_expr(v, f));
            walk_block(body, f);
        }
        StmtKind::Opaque(_) => {}
    }
}

fn walk_expr<'a>(e: &'a Expr, f: &mut impl FnMut(&'a Table)) {
    match e {
        Expr::Table(t) => {
            f(t);
            for entry in &t.entries {
                match &entry.kind {
                    EntryKind::Named { value, .. } | EntryKind::Positional(value) => {
                        walk_expr(value, f)
                    }
                }
            }
        }
        Expr::Index { base, key, .. } => {
            walk_expr(base, f);
            walk_expr(key, f);
        }
        Expr::Call { callee, args, .. } => {
            walk_expr(callee, f);
            args.iter().for_each(|a| walk_expr(a, f));
        }
        Expr::Bin { lhs, rhs, .. } => {
            walk_expr(lhs, f);
            walk_expr(rhs, f);
        }
        Expr::Un { expr, .. } | Expr::Paren(expr) => walk_expr(expr, f),
        Expr::Fn { body, .. } => walk_block(body, f),
        Expr::Name(_) | Expr::Str(_) | Expr::Num(_) | Expr::Sym(_) | Expr::Opaque(_) => {}
    }
}
