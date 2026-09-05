//! full-moon's concrete tree -> the schema. Lossy on purpose: formatting is normalised and the
//! printer owns it (§6). Anything without a case becomes `Opaque` and round-trips as text, so a
//! gap in coverage costs capability, never correctness (§10.7).

use crate::schema::*;
use full_moon::ast;
use full_moon::node::Node;
use full_moon::tokenizer::{Token, TokenReference, TokenType};

pub fn lower_ast(ast: &ast::Ast) -> Block {
    lower_block(ast.nodes(), Some(ast.eof()))
}

/// `end_token` is the construct's terminator — its leading trivia is where a comment sitting
/// after the last statement ends up, and that comment has no other home.
fn lower_block(b: &ast::Block, end_token: Option<&TokenReference>) -> Block {
    Block {
        stmts: b.stmts().map(lower_stmt).collect(),
        last: b.last_stmt().map(lower_last),
        trailing: end_token.map(leading_comments).unwrap_or_default(),
    }
}

fn lower_stmt(s: &ast::Stmt) -> Stmt {
    let kind = match s {
        ast::Stmt::LocalAssignment(a) => StmtKind::Local {
            names: a.names().iter().map(text).collect(),
            values: a.expressions().iter().map(lower_expr).collect(),
        },
        ast::Stmt::Assignment(a) => StmtKind::Assign {
            targets: a.variables().iter().map(lower_var).collect(),
            values: a.expressions().iter().map(lower_expr).collect(),
        },
        ast::Stmt::FunctionCall(c) => StmtKind::Call(lower_call(c)),
        ast::Stmt::If(i) => {
            let mut arms = vec![(lower_expr(i.condition()), lower_block(i.block(), None))];
            for e in i.else_if().into_iter().flatten() {
                arms.push((lower_expr(e.condition()), lower_block(e.block(), None)));
            }
            StmtKind::If {
                arms,
                else_: i.else_block().map(|b| lower_block(b, Some(i.end_token()))),
            }
        }
        ast::Stmt::FunctionDeclaration(f) => StmtKind::Func {
            local: false,
            name: f.name().to_string().trim().to_string(),
            params: params(f.body()),
            body: lower_block(f.body().block(), Some(f.body().end_token())),
        },
        ast::Stmt::LocalFunction(f) => StmtKind::Func {
            local: true,
            name: text(f.name()),
            params: params(f.body()),
            body: lower_block(f.body().block(), Some(f.body().end_token())),
        },
        ast::Stmt::NumericFor(f) => StmtKind::NumFor {
            name: text(f.index_variable()),
            from: lower_expr(f.start()),
            to: lower_expr(f.end()),
            step: f.step().map(lower_expr),
            body: lower_block(f.block(), Some(f.end_token())),
        },
        ast::Stmt::GenericFor(f) => StmtKind::GenFor {
            names: f.names().iter().map(text).collect(),
            exprs: f.expressions().iter().map(lower_expr).collect(),
            body: lower_block(f.block(), Some(f.end_token())),
        },
        other => return opaque_stmt(other),
    };
    Stmt {
        leading: first_comments(s),
        trailing: last_comment(s),
        kind,
    }
}

fn opaque_stmt(s: &ast::Stmt) -> Stmt {
    // The verbatim text already carries its own trivia, so re-attaching comments would
    // print them twice.
    Stmt {
        leading: vec![],
        trailing: None,
        kind: StmtKind::Opaque(s.to_string().trim().into()),
    }
}

fn lower_last(l: &ast::LastStmt) -> Last {
    let kind = match l {
        ast::LastStmt::Return(r) => LastKind::Return(r.returns().iter().map(lower_expr).collect()),
        ast::LastStmt::Break(_) => LastKind::Break,
        ast::LastStmt::Continue(_) => LastKind::Continue,
        other => {
            let text = other.to_string().trim().to_string();
            return Last {
                leading: vec![],
                trailing: None,
                kind: LastKind::Opaque(text),
            };
        }
    };
    Last {
        leading: first_comments(l),
        trailing: last_comment(l),
        kind,
    }
}

fn params(b: &ast::FunctionBody) -> Vec<String> {
    b.parameters()
        .iter()
        .map(|p| match p {
            ast::Parameter::Name(t) => text(t),
            ast::Parameter::Ellipsis(_) => "...".into(),
            other => other.to_string().trim().to_string(),
        })
        .collect()
}

fn lower_expr(e: &ast::Expression) -> Expr {
    match e {
        ast::Expression::Number(t) => Expr::Num(text(t)),
        ast::Expression::String(t) => Expr::Str(text(t)),
        ast::Expression::Symbol(t) => Expr::Sym(text(t)),
        ast::Expression::TableConstructor(t) => Expr::Table(lower_table(t)),
        ast::Expression::Var(v) => lower_var(v),
        ast::Expression::FunctionCall(c) => lower_call(c),
        ast::Expression::Function(f) => Expr::Fn {
            params: params(f.body()),
            body: lower_block(f.body().block(), Some(f.body().end_token())),
        },
        ast::Expression::BinaryOperator { lhs, binop, rhs } => Expr::Bin {
            op: binop.to_string().trim().to_string(),
            lhs: Box::new(lower_expr(lhs)),
            rhs: Box::new(lower_expr(rhs)),
        },
        ast::Expression::UnaryOperator { unop, expression } => Expr::Un {
            op: unop.to_string().trim().to_string(),
            expr: Box::new(lower_expr(expression)),
        },
        ast::Expression::Parentheses { expression, .. } => {
            Expr::Paren(Box::new(lower_expr(expression)))
        }
        other => Expr::Opaque(other.to_string().trim().into()),
    }
}

fn lower_var(v: &ast::Var) -> Expr {
    match v {
        ast::Var::Name(t) => Expr::Name(text(t)),
        ast::Var::Expression(e) => suffixed(lower_prefix(e.prefix()), e.suffixes()),
        other => Expr::Opaque(other.to_string().trim().into()),
    }
}

fn lower_call(c: &ast::FunctionCall) -> Expr {
    suffixed(lower_prefix(c.prefix()), c.suffixes())
}

fn lower_prefix(p: &ast::Prefix) -> Expr {
    match p {
        ast::Prefix::Name(t) => Expr::Name(text(t)),
        ast::Prefix::Expression(e) => Expr::Paren(Box::new(lower_expr(e))),
        other => Expr::Opaque(other.to_string().trim().into()),
    }
}

/// `C.text` and `ui.text({…})` differ only in which suffixes follow the same prefix, so both
/// fold left over one list — which is why the schema has no `Var` node.
fn suffixed<'a>(base: Expr, suffixes: impl Iterator<Item = &'a ast::Suffix>) -> Expr {
    let mut acc = base;
    for s in suffixes {
        acc = match s {
            ast::Suffix::Index(ast::Index::Dot { name, .. }) => Expr::Index {
                base: Box::new(acc),
                key: Box::new(Expr::Name(text(name))),
                dot: true,
            },
            ast::Suffix::Index(ast::Index::Brackets { expression, .. }) => Expr::Index {
                base: Box::new(acc),
                key: Box::new(lower_expr(expression)),
                dot: false,
            },
            ast::Suffix::Call(ast::Call::AnonymousCall(a)) => Expr::Call {
                callee: Box::new(acc),
                method: None,
                args: call_args(a),
            },
            ast::Suffix::Call(ast::Call::MethodCall(m)) => Expr::Call {
                callee: Box::new(acc),
                method: Some(text(m.name())),
                args: call_args(m.args()),
            },
            other => Expr::Opaque(format!("{acc_}{other}", acc_ = crate::print::expr(&acc))),
        };
    }
    acc
}

/// `f "s"` and `f {…}` are sugar for `f("s")` and `f({…})`. Normalising them here is the same
/// call to Lua and one fewer shape downstream.
fn call_args(a: &ast::FunctionArgs) -> Vec<Expr> {
    match a {
        ast::FunctionArgs::Parentheses { arguments, .. } => {
            arguments.iter().map(lower_expr).collect()
        }
        ast::FunctionArgs::String(t) => vec![Expr::Str(text(t))],
        ast::FunctionArgs::TableConstructor(t) => vec![Expr::Table(lower_table(t))],
        other => vec![Expr::Opaque(other.to_string().trim().into())],
    }
}

fn lower_table(t: &ast::TableConstructor) -> Table {
    let mut entries = Vec::new();
    let mut id = None;
    for f in t.fields().iter() {
        let (leading, trailing) = (first_comments(f), last_comment(f));
        let kind = match f {
            ast::Field::NameKey { key, value, .. } => {
                // A printed `_nid` is this table's identity coming home, not a field. Consuming
                // it is what makes ids survive print -> reparse instead of being reissued.
                if text(key) == NID_KEY {
                    if let ast::Expression::String(s) = value {
                        id = Some(unquote(&text(s)));
                        continue;
                    }
                }
                EntryKind::Named {
                    name: text(key),
                    value: lower_expr(value),
                }
            }
            ast::Field::NoKey(e) => EntryKind::Positional(lower_expr(e)),
            other => EntryKind::Positional(Expr::Opaque(other.to_string().trim().into())),
        };
        entries.push(Entry {
            leading,
            trailing,
            kind,
        });
    }
    Table {
        id: id.unwrap_or_else(new_nid),
        entries,
    }
}

fn unquote(s: &str) -> String {
    s.trim_matches(['"', '\'']).to_string()
}

/// The token's own text, without the trivia around it.
fn text(t: &TokenReference) -> String {
    t.token().to_string()
}

fn as_comment(t: &Token) -> Option<String> {
    matches!(
        t.token_type(),
        TokenType::SingleLineComment { .. } | TokenType::MultiLineComment { .. }
    )
    .then(|| t.to_string())
}

fn leading_comments(t: &TokenReference) -> Vec<String> {
    t.leading_trivia().filter_map(as_comment).collect()
}

// `Node::tokens()` yields in *struct field* order, not source order — a table's braces are
// visited before its fields, so `.last()` lands on the final field, not on `}`. Position is the
// only reliable way to ask which token is physically first or last.
fn first_comments<N: Node>(n: &N) -> Vec<String> {
    n.tokens()
        .min_by_key(|t| t.token().start_position().bytes())
        .map(leading_comments)
        .unwrap_or_default()
}

fn last_comment<N: Node>(n: &N) -> Option<String> {
    n.tokens()
        .max_by_key(|t| t.token().end_position().bytes())?
        .trailing_trivia()
        .find_map(as_comment)
}
