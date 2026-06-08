//! The block-kind descriptor — one [`BlockSpec`] per [`BlockKind`], the single source for how
//! a kind presents (type scale, type-tag, placeholder, lead width, slash palette, md prefixes).
//! Keeps adding a kind a one-place change. Layer split: semantic attrs (string id, `is_list`)
//! stay on [`BlockKind`] in egui-free `model.rs`; presentation lives here where egui is in scope.
//! Per-kind custom paint stays a `match` in `paint.rs` (it's paint, not data).

use egui::{Color32, FontFamily, FontId};

use crate::model::BlockKind;
use crate::theme;

/// How a kind presents in the slash palette. `None` = not insertable from the palette.
pub struct SlashSpec {
    pub group: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    /// The markdown shortcut shown on the right of the palette row (a quiet reminder).
    pub md: &'static str,
}

/// Everything presentational about one block kind. Held as raw numbers (not a built `FontId`)
/// so the table can be `const`; [`block_style`] composes the consumed [`BlockStyle`].
pub struct BlockSpec {
    pub kind: BlockKind,

    // ── type scale ──
    pub size: f32,
    /// `line_height = size * line_ratio`.
    pub line_ratio: f32,
    /// Vertical padding above and below the content within the row.
    pub py: f32,
    /// Tracking in em (`letter_spacing = size * ls_em`; negative tightens headings).
    pub ls_em: f32,
    /// JetBrains Mono when true, Inter otherwise.
    pub mono: bool,
    pub color: Color32,
    pub italics: bool,

    // ── presentation ──
    /// Mono type-tag shown in the focus margin.
    pub type_tag: &'static str,
    /// Faint prompt inside an empty, focused block.
    pub placeholder: &'static str,
    /// Width reserved at the content-column left for the lead marker.
    pub lead_w: f32,

    // ── authoring ──
    pub slash: Option<SlashSpec>,
    /// Paragraph prefixes that convert into this kind on input. `Divider` is omitted — it
    /// clears the line and spawns a paragraph, so it stays special-cased in the md handler.
    pub md_prefixes: &'static [&'static str],
}

/// The canonical kind table. **Order is load-bearing**: it's the slash-palette order, so
/// keep `Text` first and headings before lists before blocks.
pub const SPECS: &[BlockSpec] = &[
    BlockSpec {
        kind: BlockKind::Paragraph,
        size: 16.0, line_ratio: 1.70, py: 5.0, ls_em: 0.0, mono: false, color: theme::FG_1, italics: false,
        type_tag: "¶", placeholder: "Type '/' for commands", lead_w: 0.0,
        slash: Some(SlashSpec { group: "BASIC", label: "Text", hint: "Plain paragraph", md: "" }),
        md_prefixes: &[],
    },
    BlockSpec {
        kind: BlockKind::H1,
        size: 26.0, line_ratio: 1.20, py: 18.0, ls_em: -0.020, mono: false, color: theme::FG_1, italics: false,
        type_tag: "H1", placeholder: "Heading 1", lead_w: 0.0,
        slash: Some(SlashSpec { group: "BASIC", label: "Heading 1", hint: "Big section title", md: "#" }),
        md_prefixes: &["# "],
    },
    BlockSpec {
        kind: BlockKind::H2,
        size: 21.0, line_ratio: 1.25, py: 13.0, ls_em: -0.014, mono: false, color: theme::FG_1, italics: false,
        type_tag: "H2", placeholder: "Heading 2", lead_w: 0.0,
        slash: Some(SlashSpec { group: "BASIC", label: "Heading 2", hint: "Subsection", md: "##" }),
        md_prefixes: &["## "],
    },
    BlockSpec {
        kind: BlockKind::H3,
        size: 17.0, line_ratio: 1.30, py: 9.0, ls_em: -0.008, mono: false, color: theme::FG_1, italics: false,
        type_tag: "H3", placeholder: "Heading 3", lead_w: 0.0,
        slash: Some(SlashSpec { group: "BASIC", label: "Heading 3", hint: "Sub-subsection", md: "###" }),
        md_prefixes: &["### "],
    },
    BlockSpec {
        kind: BlockKind::BulletList,
        size: 16.0, line_ratio: 1.70, py: 3.0, ls_em: 0.0, mono: false, color: theme::FG_1, italics: false,
        type_tag: "•", placeholder: "List", lead_w: 22.0,
        slash: Some(SlashSpec { group: "LISTS", label: "Bulleted list", hint: "Tab to nest", md: "-" }),
        md_prefixes: &["- ", "* "],
    },
    BlockSpec {
        kind: BlockKind::NumberedList,
        size: 16.0, line_ratio: 1.70, py: 3.0, ls_em: 0.0, mono: false, color: theme::FG_1, italics: false,
        type_tag: "1.", placeholder: "List", lead_w: 22.0,
        slash: Some(SlashSpec { group: "LISTS", label: "Numbered list", hint: "Ordered", md: "1." }),
        md_prefixes: &["1. "],
    },
    BlockSpec {
        kind: BlockKind::Todo,
        size: 16.0, line_ratio: 1.70, py: 3.0, ls_em: 0.0, mono: false, color: theme::FG_1, italics: false,
        type_tag: "☐", placeholder: "To-do", lead_w: 24.0,
        slash: Some(SlashSpec { group: "LISTS", label: "To-do", hint: "Square checkbox", md: "[]" }),
        md_prefixes: &["[] ", "[ ] "],
    },
    BlockSpec {
        kind: BlockKind::Quote,
        size: 16.0, line_ratio: 1.70, py: 4.0, ls_em: 0.0, mono: false, color: theme::FG_2, italics: true,
        type_tag: "\"", placeholder: "Quote", lead_w: 16.0,
        slash: Some(SlashSpec { group: "BLOCKS", label: "Quote", hint: "Accent rule", md: ">" }),
        md_prefixes: &["> "],
    },
    BlockSpec {
        kind: BlockKind::Code,
        size: 13.5, line_ratio: 1.60, py: 0.0, ls_em: 0.0, mono: true, color: theme::FG_2, italics: false,
        type_tag: "</>", placeholder: "Code", lead_w: 14.0,
        slash: Some(SlashSpec { group: "BLOCKS", label: "Code", hint: "Monospace, lang tag", md: "```" }),
        // The fence is special-cased in the markdown handler (like Divider): ```lang␣ captures
        // the language, so it can't fire instantly on the third backtick via a plain prefix.
        md_prefixes: &[],
    },
    BlockSpec {
        kind: BlockKind::Divider,
        // Dividers carry no text; the values keep the row a stable height.
        size: 16.0, line_ratio: 1.70, py: 0.0, ls_em: 0.0, mono: false, color: theme::FAINT, italics: false,
        type_tag: "—", placeholder: "", lead_w: 0.0,
        slash: Some(SlashSpec { group: "BLOCKS", label: "Divider", hint: "Horizontal rule", md: "---" }),
        md_prefixes: &[], // special-cased in the markdown handler
    },
];

/// The descriptor for a kind.
pub fn spec(kind: BlockKind) -> &'static BlockSpec {
    SPECS.iter().find(|s| s.kind == kind).expect("every BlockKind has a BlockSpec")
}

/// How a block renders: font, colour, line height, row padding, letter spacing, italics.
pub struct BlockStyle {
    pub font: FontId,
    pub color: Color32,
    pub line_height: f32,
    pub py: f32,
    pub letter_spacing: f32,
    pub italics: bool,
    /// Whether the whole block is bold (headings). The *family* is resolved at layout time
    /// (it needs the egui context to check the bold face is registered), so this is just the
    /// intent; `layout` turns it into the bold family or a graceful regular fallback.
    pub bold: bool,
}

/// Compose the [`BlockStyle`] the layout/paint code consumes from a kind's [`BlockSpec`].
pub fn block_style(kind: BlockKind) -> BlockStyle {
    let s = spec(kind);
    // Base font stays the regular face; `layout` swaps in the bold family when available
    // (egui can't synthesise weight, so a missing bold face must fall back, not panic).
    let family = if s.mono { FontFamily::Monospace } else { FontFamily::Proportional };
    BlockStyle {
        font: FontId::new(s.size, family),
        color: s.color,
        line_height: s.size * s.line_ratio,
        py: s.py,
        letter_spacing: s.size * s.ls_em,
        italics: s.italics,
        bold: matches!(kind, BlockKind::H1 | BlockKind::H2 | BlockKind::H3),
    }
}

/// The mono type-tag shown in the focus margin.
pub fn type_tag(kind: BlockKind) -> &'static str {
    spec(kind).type_tag
}

/// The faint prompt shown inside an empty, focused block.
pub fn placeholder(kind: BlockKind) -> &'static str {
    spec(kind).placeholder
}
