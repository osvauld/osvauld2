//! Rules as data (`owned-entries.md` §2.3): inside a `doc` block,
//!
//! ```text
//! own cards/* override admin
//! rule cards/* {
//!   forbid set, delete, move  unless actor is entry.creator or actor has admin
//!   forbid set where changed state  unless state from todo to doing, doing to done
//!   forbid insert, set where after.title is missing or len after.title above 200
//!   forbid set unless changed only title, state, body
//! }
//! ```
//!
//! `write` is the permit; a rule only narrows it, so two rules can never widen each other.
//! Parsed here, evaluated by `courier::rules`. Bounded by construction: an expression nests
//! no deeper than it is written, names no function, quantifies over no collection, and
//! neither concatenates nor matches patterns.

use super::{Error, Parser, Tok, at, is_ident, show, valid_segment};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub path: RulePath,
    pub forbids: Vec<Forbid>,
    pub line: usize,
}

/// `cards/*`, `blocks/**`: literal segments, `*` for one segment, `**` for the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulePath(pub Vec<RuleSeg>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleSeg {
    Lit(String),
    Any,
    Rest,
}

impl RulePath {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut segs = Vec::new();
        for seg in text.split('/') {
            if matches!(segs.last(), Some(RuleSeg::Rest)) {
                return Err(format!("`**` must end `{text}`"));
            }
            segs.push(match seg {
                "" => return Err(format!("empty segment in `{text}`")),
                "*" => RuleSeg::Any,
                "**" => RuleSeg::Rest,
                s if valid_segment(s) => RuleSeg::Lit(s.to_string()),
                s => return Err(format!("bad segment `{s}` in `{text}`")),
            });
        }
        Ok(Self(segs))
    }

    /// Whether the rule reaches `path`: every segment matched, `**` taking one or more, and
    /// the path free to run deeper — an edit inside an entry is an edit of the entry.
    pub fn covers(&self, path: &[String]) -> bool {
        let mut path = path.iter();
        for seg in &self.0 {
            match seg {
                RuleSeg::Rest => return path.next().is_some(),
                RuleSeg::Any => {
                    if path.next().is_none() {
                        return false;
                    }
                }
                RuleSeg::Lit(lit) => {
                    if path.next() != Some(lit) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// How many leading segments of a covered path the rule names: the entry the rule is
    /// about is that prefix, and `before.x`/`after.x` read inside it.
    pub fn depth(&self, path: &[String]) -> usize {
        match self.0.last() {
            Some(RuleSeg::Rest) => path.len(),
            _ => self.0.len(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forbid {
    pub ops: Vec<OpKind>,
    pub when: Option<Pred>,
    pub unless: Option<Pred>,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Insert,
    Set,
    Delete,
    Move,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pred {
    Is(Operand, Operand),
    IsNot(Operand, Operand),
    In(Operand, Vec<Lit>),
    Above(Operand, Lit),
    Below(Operand, Lit),
    Missing(Operand),
    /// `actor has <role>`.
    Has(String),
    /// `changed <field>`: the entry's field differs before and after.
    Changed(String),
    /// `changed only <fields>`: nothing else in the entry differs.
    ChangedOnly(Vec<String>),
    /// `<field> from a to b, c to d`: one of the listed transitions.
    From(String, Vec<(Lit, Lit)>),
    And(Box<Pred>, Box<Pred>),
    Or(Box<Pred>, Box<Pred>),
    Not(Box<Pred>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Actor,
    /// `entry.creator`, read off the Loro ID of what the op touches.
    Creator,
    Before(String),
    After(String),
    Len(Box<Operand>),
    Lit(Lit),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lit {
    Str(String),
    Int(i64),
    Bool(bool),
}

const KEYWORDS: &[&str] = &[
    "forbid", "where", "unless", "is", "not", "in", "above", "below", "missing", "has",
    "changed", "only", "from", "to", "and", "or", "len", "actor", "override", "rule", "own",
];

fn is_keyword(w: &str) -> bool {
    KEYWORDS.contains(&w)
}

/// A number, `true`/`false`, or a bare word as a string — `todo` reads better than `"todo"`.
fn lit_of_word(w: &str, line: usize) -> Result<Lit, Error> {
    if let Ok(i) = w.parse::<i64>() {
        return Ok(Lit::Int(i));
    }
    match w {
        "true" => Ok(Lit::Bool(true)),
        "false" => Ok(Lit::Bool(false)),
        w if is_keyword(w) => Err(at(line, format!("`{w}` is a keyword, not a value"))),
        w => Ok(Lit::Str(w.to_string())),
    }
}

impl Parser {
    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Word(x)) if x == w)
    }

    fn word_at(&self, ahead: usize) -> Option<&str> {
        match self.tokens.get(self.at + ahead) {
            Some((Tok::Word(w), _)) => Some(w),
            _ => None,
        }
    }

    /// `rule <path> { forbid … }`, the `rule` word already read. Role names referenced go to
    /// `roles` for the end-of-file check.
    pub(super) fn rule_block(
        &mut self,
        line: usize,
        roles: &mut Vec<(String, usize)>,
    ) -> Result<Rule, Error> {
        let text = self.word("a rule path")?;
        let path = RulePath::parse(&text).map_err(|e| at(line, e))?;
        self.expect(Tok::Open, "`{`")?;
        let mut forbids = Vec::new();
        loop {
            let l = self.line();
            match self.next("`forbid` or `}`")? {
                Tok::Close => break,
                Tok::Word(w) if w == "forbid" => forbids.push(self.forbid(l, roles)?),
                t => {
                    return Err(at(l, format!("expected `forbid` or `}}`, found {}", show(&t))));
                }
            }
        }
        if forbids.is_empty() {
            return Err(at(line, "a rule needs at least one `forbid`".to_string()));
        }
        Ok(Rule {
            path,
            forbids,
            line,
        })
    }

    /// `own <path> [override <role>, …]`, the `own` word already read: sugar for
    /// `forbid set, delete, move unless actor is entry.creator [or actor has <role>]…`.
    pub(super) fn own_clause(
        &mut self,
        line: usize,
        roles: &mut Vec<(String, usize)>,
    ) -> Result<Rule, Error> {
        let text = self.word("a path")?;
        let path = RulePath::parse(&text).map_err(|e| at(line, e))?;
        let mut unless = Pred::Is(Operand::Actor, Operand::Creator);
        if self.is_word("override") {
            self.at += 1;
            loop {
                let l = self.line();
                let role = self.ident("a role")?;
                roles.push((role.clone(), l));
                unless = Pred::Or(Box::new(unless), Box::new(Pred::Has(role)));
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        Ok(Rule {
            path,
            forbids: vec![Forbid {
                ops: vec![OpKind::Set, OpKind::Delete, OpKind::Move],
                when: None,
                unless: Some(unless),
                line,
            }],
            line,
        })
    }

    fn forbid(&mut self, line: usize, roles: &mut Vec<(String, usize)>) -> Result<Forbid, Error> {
        let mut ops = Vec::new();
        loop {
            let l = self.line();
            let w = self.word("`insert`, `set`, `delete` or `move`")?;
            ops.push(match w.as_str() {
                "insert" => OpKind::Insert,
                "set" => OpKind::Set,
                "delete" => OpKind::Delete,
                "move" => OpKind::Move,
                other => {
                    return Err(at(
                        l,
                        format!("expected `insert`, `set`, `delete` or `move`, found `{other}`"),
                    ));
                }
            });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        let (mut when, mut unless) = (None, None);
        loop {
            if when.is_none() && self.is_word("where") {
                self.at += 1;
                when = Some(self.pred(roles)?);
            } else if unless.is_none() && self.is_word("unless") {
                self.at += 1;
                unless = Some(self.pred(roles)?);
            } else {
                break;
            }
        }
        Ok(Forbid {
            ops,
            when,
            unless,
            line,
        })
    }

    /// `or` binds loosest, then `and`, then `not`; parentheses group.
    fn pred(&mut self, roles: &mut Vec<(String, usize)>) -> Result<Pred, Error> {
        let mut left = self.and_pred(roles)?;
        while self.is_word("or") {
            self.at += 1;
            let right = self.and_pred(roles)?;
            left = Pred::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn and_pred(&mut self, roles: &mut Vec<(String, usize)>) -> Result<Pred, Error> {
        let mut left = self.unary(roles)?;
        while self.is_word("and") {
            self.at += 1;
            let right = self.unary(roles)?;
            left = Pred::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self, roles: &mut Vec<(String, usize)>) -> Result<Pred, Error> {
        if self.is_word("not") {
            self.at += 1;
            return Ok(Pred::Not(Box::new(self.unary(roles)?)));
        }
        if self.eat(&Tok::LParen) {
            let inner = self.pred(roles)?;
            self.expect(Tok::RParen, "`)`")?;
            return Ok(inner);
        }
        self.atom(roles)
    }

    fn atom(&mut self, roles: &mut Vec<(String, usize)>) -> Result<Pred, Error> {
        if self.is_word("actor") && self.word_at(1) == Some("has") {
            self.at += 2;
            let l = self.line();
            let role = self.ident("a role")?;
            roles.push((role.clone(), l));
            return Ok(Pred::Has(role));
        }
        if self.is_word("changed") {
            self.at += 1;
            if self.is_word("only") {
                self.at += 1;
                let mut fields = Vec::new();
                loop {
                    fields.push(self.ident("a field")?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                return Ok(Pred::ChangedOnly(fields));
            }
            return Ok(Pred::Changed(self.ident("a field")?));
        }
        if self.word_at(1) == Some("from")
            && self.word_at(0).is_some_and(|w| is_ident(w) && !is_keyword(w))
        {
            let field = self.ident("a field")?;
            self.at += 1;
            let mut pairs = Vec::new();
            loop {
                let from = self.lit()?;
                let l = self.line();
                let to = self.word("`to`")?;
                if to != "to" {
                    return Err(at(l, format!("expected `to`, found `{to}`")));
                }
                pairs.push((from, self.lit()?));
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            return Ok(Pred::From(field, pairs));
        }
        let left = self.operand()?;
        let l = self.line();
        match self.word("`is`, `in`, `above` or `below`")?.as_str() {
            "is" if self.is_word("not") => {
                self.at += 1;
                Ok(Pred::IsNot(left, self.operand()?))
            }
            "is" if self.is_word("missing") => {
                self.at += 1;
                Ok(Pred::Missing(left))
            }
            "is" => Ok(Pred::Is(left, self.operand()?)),
            "in" => {
                let mut lits = vec![self.lit()?];
                while self.eat(&Tok::Comma) {
                    lits.push(self.lit()?);
                }
                Ok(Pred::In(left, lits))
            }
            "above" => Ok(Pred::Above(left, self.lit()?)),
            "below" => Ok(Pred::Below(left, self.lit()?)),
            other => Err(at(
                l,
                format!("expected `is`, `in`, `above` or `below`, found `{other}`"),
            )),
        }
    }

    fn operand(&mut self) -> Result<Operand, Error> {
        let line = self.line();
        match self.next("an operand")? {
            Tok::Word(w) if w == "actor" => Ok(Operand::Actor),
            Tok::Word(w) if w == "entry.creator" => Ok(Operand::Creator),
            Tok::Word(w) if w == "len" => Ok(Operand::Len(Box::new(self.operand()?))),
            Tok::Word(w) => {
                if let Some(f) = w.strip_prefix("before.") {
                    return field(f, line).map(Operand::Before);
                }
                if let Some(f) = w.strip_prefix("after.") {
                    return field(f, line).map(Operand::After);
                }
                lit_of_word(&w, line).map(Operand::Lit)
            }
            Tok::Str(s) => Ok(Operand::Lit(Lit::Str(s))),
            t => Err(at(line, format!("expected an operand, found {}", show(&t)))),
        }
    }

    fn lit(&mut self) -> Result<Lit, Error> {
        let line = self.line();
        match self.next("a value")? {
            Tok::Str(s) => Ok(Lit::Str(s)),
            Tok::Word(w) => lit_of_word(&w, line),
            t => Err(at(line, format!("expected a value, found {}", show(&t)))),
        }
    }
}

fn field(f: &str, line: usize) -> Result<String, Error> {
    if !is_ident(f) {
        return Err(at(line, format!("`{f}` is not a valid field name")));
    }
    Ok(f.to_string())
}
