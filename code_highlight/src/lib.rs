//! **`code_highlight`** — a pure syntax-highlighting leaf: `(language, source)` → a flat list
//! of [`Span`]s, each a byte range tagged with a semantic [`HlKind`].
//!
//! It is deliberately *narrow and opinionless*: no colours (the caller maps [`HlKind`] to its
//! own theme — see the `rich_text` contract: "explicit per-run colour", "theme in, no baked
//! palette"), no egui, no loro. The output is a **complete, contiguous, gap-free cover** of the
//! source in order — every byte belongs to exactly one span ([`HlKind::Text`] for the
//! un-highlighted regions) — so a consumer just walks the list and appends each slice with the
//! colour for its kind.
//!
//! Internally it drives **tree-sitter** via [`inkjet`], which is purely an implementation
//! detail behind this API (its value is a curated, mutually-compatible grammar bundle, so we
//! dodge the tree-sitter grammar/core version-matching trap). Swapping the engine later would
//! not touch a single consumer. This is the `tree-sitter for code` sibling of the math/`rich_text`
//! leaves; it depends on neither.

use std::cell::RefCell;
use std::ops::Range;

use inkjet::constants::HIGHLIGHT_NAMES;
use inkjet::tree_sitter_highlight::{Highlight, HighlightEvent};
use inkjet::{Highlighter, Language};

/// A semantic highlight category — a small, stable vocabulary the consumer maps to colours.
/// The tree-sitter capture set is much finer (dotted, hierarchical: `keyword.control.return`,
/// `constant.numeric.float`, …); [`HlKind::from_capture`] folds it down to these.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HlKind {
    Keyword,
    Function,
    Type,
    Constant,
    Number,
    String,
    Comment,
    Variable,
    /// A field / member access (`foo.bar`).
    Property,
    Operator,
    Punctuation,
    /// An attribute / annotation / macro (`#[derive]`, `@decorator`, `println!`).
    Attribute,
    /// A markup / XML-ish tag.
    Tag,
    /// A string escape (`\n`, `\u{…}`).
    Escape,
    /// Un-highlighted text (the default cover).
    Text,
}

impl HlKind {
    /// Fold a tree-sitter capture name (the helix-standard set) down to a [`HlKind`]. Matches
    /// by the most *specific* prefix first; anything unrecognised falls back to [`HlKind::Text`].
    pub fn from_capture(name: &str) -> Self {
        use HlKind::*;
        // Order matters — specific before general (e.g. `keyword.operator` before `keyword`).
        if name.starts_with("constant.numeric") {
            Number
        } else if name == "escape" || name == "constant.character.escape" {
            Escape
        } else if name.starts_with("constant") || name == "label" {
            Constant
        } else if name.starts_with("string") {
            String
        } else if name.starts_with("comment") {
            Comment
        } else if name.starts_with("variable.other.member") {
            Property
        } else if name.starts_with("variable") {
            Variable
        } else if name.starts_with("type") || name == "namespace" || name == "constructor" {
            Type
        } else if name == "attribute"
            || name.starts_with("keyword.directive")
            || name.starts_with("function.macro")
        {
            Attribute
        } else if name == "operator" || name.starts_with("keyword.operator") {
            Operator
        } else if name.starts_with("keyword") {
            Keyword
        } else if name.starts_with("function") {
            Function
        } else if name.starts_with("punctuation") {
            Punctuation
        } else if name.starts_with("tag") {
            Tag
        } else if name.starts_with("markup.heading") {
            Keyword
        } else if name.starts_with("markup.link") || name.starts_with("markup.raw") {
            String
        } else if name.starts_with("markup.quote") {
            Comment
        } else {
            Text
        }
    }
}

/// A contiguous run of source with one highlight kind. `range` is a **byte** range into the
/// source (always on UTF-8 boundaries — tree-sitter never splits a codepoint).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub kind: HlKind,
}

thread_local! {
    // One reusable highlighter per thread. `Highlighter::new()` is cheap, but reusing it
    // avoids re-allocating its parser/cursor state on every block. Each language's
    // `HighlightConfiguration` is a `&'static` inkjet lazily builds once, so repeated calls
    // only pay for parsing, not query compilation.
    static HIGHLIGHTER: RefCell<Highlighter> = RefCell::new(Highlighter::new());
}

/// A language this build can highlight: `token` is what gets stored / passed to [`highlight`];
/// `label` is the human name for a picker UI.
#[derive(Clone, Copy, Debug)]
pub struct LangInfo {
    pub token: &'static str,
    pub label: &'static str,
}

/// The languages compiled into this build (mirrors the enabled inkjet `language-*` features),
/// in menu order. A picker shows these; `token` round-trips through [`highlight`] / `set_lang`.
pub const LANGUAGES: &[LangInfo] = &[
    LangInfo { token: "rust", label: "Rust" },
    LangInfo { token: "python", label: "Python" },
    LangInfo { token: "javascript", label: "JavaScript" },
    LangInfo { token: "typescript", label: "TypeScript" },
    LangInfo { token: "go", label: "Go" },
    LangInfo { token: "c", label: "C" },
    LangInfo { token: "lua", label: "Lua" },
    LangInfo { token: "json", label: "JSON" },
    LangInfo { token: "toml", label: "TOML" },
    LangInfo { token: "bash", label: "Bash" },
];

/// Whether a language tag resolves to a bundled grammar (`"rust"`, `"rs"`, `"py"`, …).
pub fn supported(lang: &str) -> bool {
    resolve(lang).is_some()
}

fn resolve(lang: &str) -> Option<Language> {
    Language::from_token(lang.trim().to_ascii_lowercase())
}

/// Highlight `source` as `lang`. Returns a flat, contiguous, gap-free cover of the whole
/// source in document order (with [`HlKind::Text`] for un-highlighted regions). Returns `None`
/// when the language is unknown or highlighting fails — the caller then renders the source
/// plain, exactly as if there were no highlighter at all.
pub fn highlight(lang: &str, source: &str) -> Option<Vec<Span>> {
    let language = resolve(lang)?;
    HIGHLIGHTER.with(|cell| {
        let mut hl = cell.borrow_mut();
        // `highlight_raw` wants `&S` where `S: Sized + AsRef<str>`, so pass `&source` (`&&str`,
        // giving `S = &str`) rather than the `&str` directly (which would make `S = str`, unsized).
        let events = hl.highlight_raw(language, &source).ok()?;
        let mut stack: Vec<usize> = Vec::new();
        let mut out: Vec<Span> = Vec::new();
        for ev in events {
            match ev.ok()? {
                HighlightEvent::HighlightStart(Highlight(i)) => stack.push(i),
                HighlightEvent::HighlightEnd => {
                    stack.pop();
                }
                HighlightEvent::Source { start, end } => {
                    if start >= end {
                        continue;
                    }
                    // The innermost active capture wins; none → plain text.
                    let kind = stack
                        .last()
                        .and_then(|&i| HIGHLIGHT_NAMES.get(i).copied())
                        .map(HlKind::from_capture)
                        .unwrap_or(HlKind::Text);
                    // Coalesce adjacent same-kind runs so the consumer builds fewer sections.
                    match out.last_mut() {
                        Some(prev) if prev.kind == kind && prev.range.end == start => {
                            prev.range.end = end;
                        }
                        _ => out.push(Span { range: start..end, kind }),
                    }
                }
            }
        }
        Some(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cover_is_complete(src: &str, spans: &[Span]) {
        assert_eq!(spans.first().unwrap().range.start, 0, "cover starts at 0");
        assert_eq!(spans.last().unwrap().range.end, src.len(), "cover ends at len");
        for w in spans.windows(2) {
            assert_eq!(w[0].range.end, w[1].range.start, "contiguous, no gaps/overlaps");
        }
    }

    #[test]
    fn rust_keywords_strings_comments() {
        let src = "// hi\nfn main() {\n    let s = \"x\";\n}";
        let spans = highlight("rust", src).expect("rust is bundled");
        cover_is_complete(src, &spans);
        assert!(spans.iter().any(|s| s.kind == HlKind::Keyword), "found `fn`/`let`");
        assert!(spans.iter().any(|s| s.kind == HlKind::String), "found \"x\"");
        assert!(spans.iter().any(|s| s.kind == HlKind::Comment), "found the // comment");
    }

    #[test]
    fn aliases_resolve() {
        assert!(supported("rs"));
        assert!(supported("RUST"));
        assert!(supported("python"));
    }

    #[test]
    fn unknown_language_is_none() {
        assert!(highlight("not-a-real-lang-zzz", "anything").is_none());
        assert!(!supported("not-a-real-lang-zzz"));
    }

    #[test]
    fn empty_source_is_empty_cover() {
        // No source events → an empty (but valid) cover; the caller renders a blank line.
        assert_eq!(highlight("rust", ""), Some(vec![]));
    }
}
