//! `manifest.osv`: what an app declares about itself, read by the node before any app code
//! runs (`docs/design/app-permissions.md` §3). Pure: no Loro, no Lua, no I/O. The node asks it
//! two kinds of question — which declaration governs a doc name ([`Manifest::resolve`]), and
//! what a role may hand out or stands for ([`Manifest::can_grant`], [`Manifest::satisfies`]).
//!
//! ```text
//! app "chat" {
//!   roles admin, moderator, member
//!   role admin { grant moderator, member }
//!   doc group/{gid}/{day} { shard by day  history all
//!                           read members(group/{gid}/meta)  write members(group/{gid}/meta)
//!                           validate "rules.message" }
//!   uses "people" read
//!   channel typing { send member  rate 4/s  slot sender }
//! }
//! ```
//!
//! `--` starts a comment. A file holding only `name = "..."` (the older form) parses as a bare
//! manifest: it declares nothing, so the app keeps membership-only behaviour.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

/// The implicit root role: may grant any declared role and satisfies every one.
pub const OWNER: &str = "owner";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("manifest.osv line {line}: {message}")]
pub struct Error {
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    pub name: String,
    pub roles: Vec<String>,
    /// Direct `grant` edges; [`Manifest::can_grant`] follows them transitively.
    pub grants: BTreeMap<String, Vec<String>>,
    pub docs: Vec<DocDecl>,
    pub uses: Vec<Uses>,
    pub channels: Vec<Channel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocDecl {
    pub pattern: Pattern,
    pub read: Vec<Who>,
    pub write: Vec<Who>,
    pub shard: Option<Shard>,
    pub history: Option<History>,
    pub validate: Option<String>,
    pub line: usize,
}

/// Who a read, write or send rule admits. Declarative only, so the node knows every input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Who {
    Role(String),
    /// A path variable holding a DID: `dm/{a}/{b}` admits `a` and `b`.
    Var(String),
    Node,
    /// Whoever the named doc lists as members, with this doc's variables substituted.
    Members(Pattern),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shard {
    Day,
    Month,
    Field(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum History {
    All,
    SinceJoin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uses {
    pub namespace: String,
    pub write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub name: String,
    pub send: Vec<Who>,
    pub rate_per_sec: Option<u32>,
    pub slot_sender: bool,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern(Vec<Seg>);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Seg {
    Lit(String),
    Var(String),
}

pub type Vars = BTreeMap<String, String>;

impl Pattern {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut segs = Vec::new();
        let mut seen = HashSet::new();
        for seg in text.split('/') {
            if seg.is_empty() {
                return Err(format!("empty segment in `{text}`"));
            }
            if let Some(name) = seg.strip_prefix('{') {
                let name = name
                    .strip_suffix('}')
                    .ok_or_else(|| format!("unclosed `{{` in `{text}`"))?;
                if !is_ident(name) {
                    return Err(format!("bad variable `{seg}` in `{text}`"));
                }
                if name == "node" || name == OWNER {
                    return Err(format!("`{{{name}}}` is reserved in `{text}`"));
                }
                if !seen.insert(name) {
                    return Err(format!("variable `{name}` twice in `{text}`"));
                }
                segs.push(Seg::Var(name.to_string()));
            } else if !valid_segment(seg) {
                return Err(format!("bad segment `{seg}` in `{text}`"));
            } else {
                segs.push(Seg::Lit(seg.to_string()));
            }
        }
        Ok(Self(segs))
    }

    pub fn vars(&self) -> impl Iterator<Item = &str> {
        self.0.iter().filter_map(|s| match s {
            Seg::Var(v) => Some(v.as_str()),
            Seg::Lit(_) => None,
        })
    }

    pub fn has_var(&self, name: &str) -> bool {
        self.vars().any(|v| v == name)
    }

    /// The concrete name with `vars` substituted; `None` if one is unbound.
    pub fn fill(&self, vars: &Vars) -> Option<String> {
        let segs: Option<Vec<&str>> = self
            .0
            .iter()
            .map(|s| match s {
                Seg::Lit(l) => Some(l.as_str()),
                Seg::Var(v) => vars.get(v).map(String::as_str),
            })
            .collect();
        Some(segs?.join("/"))
    }

    /// Same names matched, whatever the variables are called.
    fn same_shape(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self.0.iter().zip(&other.0).all(|pair| match pair {
                (Seg::Lit(a), Seg::Lit(b)) => a == b,
                (Seg::Var(_), Seg::Var(_)) => true,
                _ => false,
            })
    }
}

impl fmt::Display for Pattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, seg) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("/")?;
            }
            match seg {
                Seg::Lit(l) => f.write_str(l)?,
                Seg::Var(v) => write!(f, "{{{v}}}")?,
            }
        }
        Ok(())
    }
}

impl Manifest {
    pub fn parse(src: &str) -> Result<Self, Error> {
        let tokens = lex(src)?;
        let manifest = Parser { tokens, at: 0 }.manifest()?;
        manifest.check()?;
        Ok(manifest)
    }

    /// Declares nothing: the app keeps membership-only behaviour.
    pub fn is_bare(&self) -> bool {
        self.roles.is_empty() && self.docs.is_empty() && self.channels.is_empty()
    }

    /// The declaration governing `doc` and its variables. Where two patterns match, the one
    /// with a literal at the first position they differ wins (`group/{gid}/meta` over
    /// `group/{gid}/{day}`).
    pub fn resolve(&self, doc: &str) -> Option<(&DocDecl, Vars)> {
        if !workspace::valid_doc_name(doc) {
            return None;
        }
        let parts: Vec<&str> = doc.split('/').collect();
        let mut best: Option<(&DocDecl, Vars)> = None;
        for decl in &self.docs {
            let Some(vars) = decl.matches(&parts) else {
                continue;
            };
            if best.as_ref().is_none_or(|(b, _)| decl.beats(b)) {
                best = Some((decl, vars));
            }
        }
        best
    }

    /// May a holder of `granter` hand out `target`? Owner may hand out any declared role;
    /// any other role, exactly the roles reachable from it along `grant` edges.
    pub fn can_grant(&self, granter: &str, target: &str) -> bool {
        if !self.roles.iter().any(|r| r == target) {
            return false;
        }
        granter == OWNER || self.reachable(granter, target)
    }

    /// Does holding `held` meet a rule naming `required`? Itself, owner, or anything in its
    /// cone — an admin who can make moderators is at least one.
    pub fn satisfies(&self, held: &str, required: &str) -> bool {
        held == required || held == OWNER || self.reachable(held, required)
    }

    fn reachable(&self, from: &str, target: &str) -> bool {
        let mut seen = HashSet::new();
        let mut stack = vec![from];
        while let Some(role) = stack.pop() {
            if !seen.insert(role) {
                continue;
            }
            for granted in self.grants.get(role).into_iter().flatten() {
                if granted == target {
                    return true;
                }
                stack.push(granted);
            }
        }
        false
    }

    fn check(&self) -> Result<(), Error> {
        for (i, decl) in self.docs.iter().enumerate() {
            if let Some(earlier) = self.docs[..i]
                .iter()
                .find(|d| d.pattern.same_shape(&decl.pattern))
            {
                return Err(at(
                    decl.line,
                    format!(
                        "`{}` matches the same names as `{}`",
                        decl.pattern, earlier.pattern
                    ),
                ));
            }
            let needs = match &decl.shard {
                Some(Shard::Day) => Some("day"),
                Some(Shard::Month) => Some("month"),
                _ => None,
            };
            if let Some(var) = needs.filter(|v| !decl.pattern.has_var(v)) {
                return Err(at(
                    decl.line,
                    format!("`shard by {var}` needs a `{{{var}}}` segment"),
                ));
            }
        }
        Ok(())
    }
}

impl DocDecl {
    fn matches(&self, parts: &[&str]) -> Option<Vars> {
        if parts.len() != self.pattern.0.len() {
            return None;
        }
        let mut vars = Vars::new();
        for (seg, part) in self.pattern.0.iter().zip(parts) {
            match seg {
                Seg::Lit(l) if l == part => {}
                Seg::Lit(_) => return None,
                Seg::Var(v) => {
                    let ok = match (v.as_str(), &self.shard) {
                        ("day", Some(Shard::Day)) => is_day(part),
                        ("month", Some(Shard::Month)) => is_month(part),
                        _ => true,
                    };
                    if !ok {
                        return None;
                    }
                    vars.insert(v.clone(), part.to_string());
                }
            }
        }
        Some(vars)
    }

    fn beats(&self, other: &Self) -> bool {
        for (a, b) in self.pattern.0.iter().zip(&other.pattern.0) {
            match (a, b) {
                (Seg::Lit(_), Seg::Var(_)) => return true,
                (Seg::Var(_), Seg::Lit(_)) => return false,
                _ => {}
            }
        }
        false
    }
}

/// A doc name segment: the address grammar (`workspace::valid_id`), so every doc a manifest
/// matches can be stored and addressed.
fn valid_segment(s: &str) -> bool {
    workspace::valid_id(s)
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn digits(s: &str, n: usize) -> Option<u32> {
    (s.len() == n && s.bytes().all(|b| b.is_ascii_digit()))
        .then(|| s.parse().ok())
        .flatten()
}

fn is_month(s: &str) -> bool {
    let mut p = s.split('-');
    matches!(
        (
            p.next().and_then(|y| digits(y, 4)),
            p.next().and_then(|m| digits(m, 2)),
            p.next()
        ),
        (Some(_), Some(1..=12), None)
    )
}

fn is_day(s: &str) -> bool {
    if !(s.len() == 10 && s.is_ascii() && is_month(&s[..7]) && s.as_bytes()[7] == b'-') {
        return false;
    }
    let (Some(year), Some(month), Some(day)) =
        (digits(&s[..4], 4), digits(&s[5..7], 2), digits(&s[8..], 2))
    else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let last = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=last).contains(&day)
}

fn at(line: usize, message: impl Into<String>) -> Error {
    Error {
        line,
        message: message.into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Tok {
    Word(String),
    Str(String),
    Open,
    Close,
    Comma,
    LParen,
    RParen,
}

/// Words run to whitespace or `"` `,` `(` `)`, so patterns keep their `{var}` braces; a brace
/// standing alone, or left over at either end of a word, is block punctuation.
fn lex(src: &str) -> Result<Vec<(Tok, usize)>, Error> {
    let mut out = Vec::new();
    for (n, text) in src.lines().enumerate() {
        let line = n + 1;
        let mut chars = text.char_indices().peekable();
        while let Some(&(i, c)) = chars.peek() {
            if c.is_whitespace() {
                chars.next();
            } else if text[i..].starts_with("--") {
                break;
            } else if c == '"' {
                chars.next();
                let mut s = String::new();
                loop {
                    match chars.next() {
                        Some((_, '"')) => break,
                        Some((_, c)) => s.push(c),
                        None => return Err(at(line, "unclosed string")),
                    }
                }
                out.push((Tok::Str(s), line));
            } else if let Some(t) = match c {
                ',' => Some(Tok::Comma),
                '(' => Some(Tok::LParen),
                ')' => Some(Tok::RParen),
                _ => None,
            } {
                chars.next();
                out.push((t, line));
            } else {
                let mut word = String::new();
                while let Some(&(_, c)) = chars.peek() {
                    if c.is_whitespace() || matches!(c, '"' | ',' | '(' | ')') {
                        break;
                    }
                    word.push(c);
                    chars.next();
                }
                split_braces(&word, line, &mut out);
            }
        }
    }
    Ok(out)
}

fn split_braces(word: &str, line: usize, out: &mut Vec<(Tok, usize)>) {
    let mut w = word;
    let mut leading = 0;
    while let Some(rest) = w.strip_prefix('{') {
        // `{x}` balanced at the front is a variable, not a block.
        if rest
            .find('}')
            .is_some_and(|c| c > 0 && is_ident(&rest[..c]))
        {
            break;
        }
        leading += 1;
        w = rest;
    }
    let mut trailing = 0;
    while w.ends_with('}') && w.matches('}').count() > w.matches('{').count() {
        trailing += 1;
        w = &w[..w.len() - 1];
    }
    out.extend(std::iter::repeat_n((Tok::Open, line), leading));
    if !w.is_empty() {
        out.push((Tok::Word(w.to_string()), line));
    }
    out.extend(std::iter::repeat_n((Tok::Close, line), trailing));
}

/// Raw names from a rule, each with its line, resolved once the whole file is read.
type Names = Vec<(String, usize)>;

struct Parser {
    tokens: Vec<(Tok, usize)>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at).map(|(t, _)| t)
    }

    fn line(&self) -> usize {
        self.tokens
            .get(self.at)
            .or(self.tokens.last())
            .map_or(1, |(_, l)| *l)
    }

    fn next(&mut self, what: &str) -> Result<Tok, Error> {
        let tok = self.tokens.get(self.at).cloned();
        match tok {
            Some((t, _)) => {
                self.at += 1;
                Ok(t)
            }
            None => Err(at(
                self.line(),
                format!("expected {what}, found end of file"),
            )),
        }
    }

    fn expect(&mut self, want: Tok, what: &str) -> Result<(), Error> {
        let line = self.line();
        match self.next(what)? {
            t if t == want => Ok(()),
            t => Err(at(line, format!("expected {what}, found {}", show(&t)))),
        }
    }

    fn word(&mut self, what: &str) -> Result<String, Error> {
        let line = self.line();
        match self.next(what)? {
            Tok::Word(w) => Ok(w),
            t => Err(at(line, format!("expected {what}, found {}", show(&t)))),
        }
    }

    fn ident(&mut self, what: &str) -> Result<String, Error> {
        let line = self.line();
        let w = self.word(what)?;
        if !is_ident(&w) {
            return Err(at(line, format!("`{w}` is not a valid {what}")));
        }
        Ok(w)
    }

    fn string(&mut self, what: &str) -> Result<String, Error> {
        let line = self.line();
        match self.next(what)? {
            Tok::Str(s) => Ok(s),
            t => Err(at(line, format!("expected {what}, found {}", show(&t)))),
        }
    }

    fn eat(&mut self, want: &Tok) -> bool {
        if self.peek() == Some(want) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn manifest(mut self) -> Result<Manifest, Error> {
        let mut m = Manifest::default();
        let line = self.line();
        match self.word("`app`")?.as_str() {
            "app" => {}
            "name" => {
                self.expect(Tok::Word("=".into()), "`=`")?;
                m.name = self.string("a name")?;
                return self.end(m);
            }
            w => return Err(at(line, format!("expected `app`, found `{w}`"))),
        }
        m.name = self.string("the app's name")?;
        self.expect(Tok::Open, "`{`")?;
        // Names in rules are resolved once every role is known: a doc may precede `roles`.
        let mut pending: Vec<(usize, usize, bool, Names)> = Vec::new();
        let mut channel_names: Vec<(usize, Names)> = Vec::new();
        let mut grant_lines: Vec<(String, usize)> = Vec::new();
        loop {
            let line = self.line();
            match self.next("a statement or `}`")? {
                Tok::Close => break,
                Tok::Word(w) => match w.as_str() {
                    "roles" => loop {
                        let line = self.line();
                        let role = self.ident("role name")?;
                        if role == OWNER || role == "node" {
                            return Err(at(line, format!("`{role}` is reserved")));
                        }
                        if m.roles.contains(&role) {
                            return Err(at(line, format!("role `{role}` declared twice")));
                        }
                        m.roles.push(role);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    },
                    "role" => {
                        let role = self.ident("role name")?;
                        if m.grants.contains_key(&role) {
                            return Err(at(line, format!("`role {role}` declared twice")));
                        }
                        self.expect(Tok::Open, "`{`")?;
                        let kw = self.word("`grant`")?;
                        if kw != "grant" {
                            return Err(at(line, format!("expected `grant`, found `{kw}`")));
                        }
                        let granted = self.names()?;
                        self.expect(Tok::Close, "`}`")?;
                        for (name, l) in
                            std::iter::once((role.clone(), line)).chain(granted.clone())
                        {
                            grant_lines.push((name, l));
                        }
                        let entry = m.grants.entry(role).or_default();
                        entry.extend(granted.into_iter().map(|(n, _)| n));
                    }
                    "doc" => {
                        let text = self.word("a doc pattern")?;
                        let pattern = Pattern::parse(&text).map_err(|e| at(line, e))?;
                        let (decl, raw) = self.doc_body(pattern, line)?;
                        let i = m.docs.len();
                        m.docs.push(decl);
                        for (write, names) in raw {
                            pending.push((i, line, write, names));
                        }
                    }
                    "uses" => {
                        let namespace = self.string("a namespace")?;
                        let mode_line = self.line();
                        let write = match self.word("`read` or `write`")?.as_str() {
                            "read" => false,
                            "write" => true,
                            w => {
                                return Err(at(
                                    mode_line,
                                    format!("expected `read` or `write`, found `{w}`"),
                                ));
                            }
                        };
                        m.uses.push(Uses { namespace, write });
                    }
                    "channel" => {
                        let name = self.ident("channel name")?;
                        let (ch, names) = self.channel_body(name, line)?;
                        channel_names.push((m.channels.len(), names));
                        m.channels.push(ch);
                    }
                    "derive" | "local" | "sim" => {
                        return Err(at(line, format!("`{w}` is designed but not built yet")));
                    }
                    other => return Err(at(line, format!("unknown statement `{other}`"))),
                },
                t => {
                    return Err(at(
                        line,
                        format!("expected a statement, found {}", show(&t)),
                    ));
                }
            }
        }
        for (i, line, write, names) in pending {
            let pattern = m.docs[i].pattern.clone();
            let whos = names
                .into_iter()
                .map(|(n, l)| resolve_who(&m, n, Some(&pattern), l.max(line)))
                .collect::<Result<Vec<_>, _>>()?;
            let decl = &mut m.docs[i];
            if write {
                &mut decl.write
            } else {
                &mut decl.read
            }
            .extend(whos);
        }
        for (i, names) in channel_names {
            let whos = names
                .into_iter()
                .map(|(n, l)| resolve_who(&m, n, None, l))
                .collect::<Result<Vec<_>, _>>()?;
            m.channels[i].send.extend(whos);
        }
        if let Some((name, line)) = grant_lines.iter().find(|(n, _)| !m.roles.contains(n)) {
            return Err(at(*line, format!("`{name}` is not a declared role")));
        }
        // A cycle would make roles grant and satisfy each other, themselves included.
        if let Some((role, line)) = grant_lines.iter().find(|(r, _)| m.reachable(r, r)) {
            return Err(at(
                *line,
                format!("role `{role}` can grant itself through a cycle"),
            ));
        }
        self.end(m)
    }

    fn end(self, m: Manifest) -> Result<Manifest, Error> {
        match self.tokens.get(self.at) {
            None => Ok(m),
            Some((t, line)) => Err(at(*line, format!("unexpected {} after the app", show(t)))),
        }
    }

    /// `a, b, c` — raw names with their lines, resolved once the whole file is read.
    fn names(&mut self) -> Result<Names, Error> {
        let mut out = Vec::new();
        loop {
            let line = self.line();
            let name = self.who()?;
            out.push((name, line));
            if !self.eat(&Tok::Comma) {
                return Ok(out);
            }
        }
    }

    /// One name, or `members(<pattern>)` kept as the text `members(<pattern>)`.
    fn who(&mut self) -> Result<String, Error> {
        let w = self.word("a role, variable, `node` or `members(...)`")?;
        if w == "members" && self.eat(&Tok::LParen) {
            let pattern = self.word("a doc pattern")?;
            self.expect(Tok::RParen, "`)`")?;
            return Ok(format!("members({pattern})"));
        }
        Ok(w)
    }

    fn doc_body(
        &mut self,
        pattern: Pattern,
        line: usize,
    ) -> Result<(DocDecl, Vec<(bool, Names)>), Error> {
        self.expect(Tok::Open, "`{`")?;
        let mut decl = DocDecl {
            pattern,
            read: Vec::new(),
            write: Vec::new(),
            shard: None,
            history: None,
            validate: None,
            line,
        };
        let mut raw = Vec::new();
        loop {
            let line = self.line();
            match self.next("a clause or `}`")? {
                Tok::Close => return Ok((decl, raw)),
                Tok::Word(w) => match w.as_str() {
                    "read" => raw.push((false, self.names()?)),
                    "write" => raw.push((true, self.names()?)),
                    "shard" => {
                        let by = self.word("`by`")?;
                        if by != "by" {
                            return Err(at(line, format!("expected `by`, found `{by}`")));
                        }
                        decl.shard =
                            Some(match self.ident("`day`, `month` or a field")?.as_str() {
                                "day" => Shard::Day,
                                "month" => Shard::Month,
                                field => Shard::Field(field.to_string()),
                            });
                    }
                    "history" => {
                        decl.history = Some(match self.word("`all` or `since_join`")?.as_str() {
                            "all" => History::All,
                            "since_join" => History::SinceJoin,
                            h => {
                                return Err(at(
                                    line,
                                    format!("expected `all` or `since_join`, found `{h}`"),
                                ));
                            }
                        });
                    }
                    "validate" => decl.validate = Some(self.string("a rule name")?),
                    other => return Err(at(line, format!("unknown doc clause `{other}`"))),
                },
                t => return Err(at(line, format!("expected a clause, found {}", show(&t)))),
            }
        }
    }

    fn channel_body(&mut self, name: String, line: usize) -> Result<(Channel, Names), Error> {
        self.expect(Tok::Open, "`{`")?;
        let mut ch = Channel {
            name,
            send: Vec::new(),
            rate_per_sec: None,
            slot_sender: false,
            line,
        };
        let mut names = Vec::new();
        loop {
            let line = self.line();
            match self.next("a clause or `}`")? {
                Tok::Close => return Ok((ch, names)),
                Tok::Word(w) => match w.as_str() {
                    "send" => names.extend(self.names()?),
                    "rate" => {
                        let r = self.word("a rate like `4/s`")?;
                        let n = r
                            .strip_suffix("/s")
                            .and_then(|n| n.parse().ok())
                            .ok_or_else(|| at(line, format!("bad rate `{r}`, expected `N/s`")))?;
                        ch.rate_per_sec = Some(n);
                    }
                    "slot" => {
                        let s = self.word("`sender`")?;
                        if s != "sender" {
                            return Err(at(line, format!("expected `sender`, found `{s}`")));
                        }
                        ch.slot_sender = true;
                    }
                    other => return Err(at(line, format!("unknown channel clause `{other}`"))),
                },
                t => return Err(at(line, format!("expected a clause, found {}", show(&t)))),
            }
        }
    }
}

fn resolve_who(
    m: &Manifest,
    name: String,
    scope: Option<&Pattern>,
    line: usize,
) -> Result<Who, Error> {
    if let Some(inner) = name
        .strip_prefix("members(")
        .and_then(|r| r.strip_suffix(')'))
    {
        let target = Pattern::parse(inner).map_err(|e| at(line, e))?;
        if !m.docs.iter().any(|d| d.pattern == target) {
            return Err(at(
                line,
                format!("`members({target})` names no declared doc"),
            ));
        }
        // A channel has no variables of its own yet, so it cannot bind one.
        if let Some(v) = target.vars().find(|v| !scope.is_some_and(|s| s.has_var(v))) {
            return Err(at(
                line,
                format!("`members({target})` uses `{{{v}}}`, which is not bound here"),
            ));
        }
        return Ok(Who::Members(target));
    }
    if name == "node" {
        return Ok(Who::Node);
    }
    if scope.is_some_and(|p| p.has_var(&name)) {
        return Ok(Who::Var(name));
    }
    if name == OWNER || m.roles.contains(&name) {
        return Ok(Who::Role(name));
    }
    Err(at(
        line,
        format!("`{name}` is not a declared role or a variable of this doc"),
    ))
}

fn show(t: &Tok) -> String {
    match t {
        Tok::Word(w) => format!("`{w}`"),
        Tok::Str(s) => format!("\"{s}\""),
        Tok::Open => "`{`".into(),
        Tok::Close => "`}`".into(),
        Tok::Comma => "`,`".into(),
        Tok::LParen => "`(`".into(),
        Tok::RParen => "`)`".into(),
    }
}

#[cfg(test)]
mod tests;
