//! Render styled text runs → an egui [`Galley`] — the one renderer every consumer shares. The
//! galley is the seam: the public output is always a galley (or the [`LayoutJob`] behind it),
//! never a private draw stream.
//!
//! Design contract:
//! - a [`Run`] carries open, string-keyed [`Marks`] *and* an explicit `color` — tree-sitter/math
//!   inject arbitrary foregrounds, so colour can't be implied by booleans alone;
//! - mark vocabulary is open: known keys (`bold`/`italic`/`strike`/`code`/`link`) map to a
//!   `TextFormat`, unknown keys are ignored — new marks never change the type;
//! - theme in, no baked palette: code/link colours come from [`Theme`];
//! - no loro/mlua: this crate only ever sees already-resolved runs;
//! - owns the bold face (egui can't synthesise weight) with a registered-or-fallback guard, so a
//!   font-less context (tests) renders un-bolded instead of panicking.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Galley, Stroke};

/// The font-family name for bold runs. Matches `doc_editor::theme::BOLD_FAMILY` so a single bold
/// face serves both.
pub const BOLD_FAMILY: &str = "sans_sb";

const SANS_SEMIBOLD: &[u8] = include_bytes!("../assets/NotoSans-SemiBold.ttf");

fn bold_family() -> FontFamily {
    FontFamily::Name(BOLD_FAMILY.into())
}

/// Register the bundled bold face on `ctx`. Call once per context. Rendering works without it
/// (bold falls back to the proportional face), so tests need not call it.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(BOLD_FAMILY.to_owned(), Arc::new(FontData::from_static(SANS_SEMIBOLD)));
    fonts.families.insert(bold_family(), vec![BOLD_FAMILY.to_owned()]);
    ctx.set_fonts(fonts);
}

/// The bold family if the consumer registered it ([`install_fonts`]), else the proportional
/// face. egui panics on an unbound `FontFamily::Name`; this guard keeps font-less tests alive.
/// Public so consumers building their own `LayoutJob` (rather than going through [`galley`]) —
/// e.g. doc_editor's per-block layout — resolve the bold face the same way.
pub fn bold_or_fallback(ctx: &egui::Context) -> FontFamily {
    let want = bold_family();
    if ctx.fonts(|f| f.definitions().families.contains_key(&want)) {
        want
    } else {
        FontFamily::Proportional
    }
}

/// The block-level base every run inherits before its own marks: the text format a whole block
/// contributes. A run's marks *add* to this (never remove) — a heading sets `bold` so every run
/// is bold, an inline `bold` mark bolds just one run; both resolve to the same bold face. This is
/// the shared input both consumers feed: doc_editor maps a block kind onto it, a Lua `ui.text`
/// node maps its style table onto it.
#[derive(Clone, Copy)]
pub struct Style {
    pub size: f32,
    pub color: Color32,
    /// Monospace base — a code block. Inline `code` *marks* force mono per-run regardless.
    pub mono: bool,
    /// Bold every run — a heading. (A `bold` mark bolds a single run.)
    pub bold: bool,
    /// Italicise every run — a quote. (An `italic` mark italicises a single run.)
    pub italic: bool,
    /// Strike every run, drawn faint in [`Theme::strike_color`] — a done to-do. A run's own
    /// `strike` mark instead strikes in that run's text colour.
    pub strike: bool,
    /// Line height; `None` leaves the font's natural height.
    pub line_height: Option<f32>,
    /// Extra tracking in points (negative tightens — headings).
    pub letter_spacing: f32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            size: 16.0,
            color: Color32::GRAY,
            mono: false,
            bold: false,
            italic: false,
            strike: false,
            line_height: None,
            letter_spacing: 0.0,
        }
    }
}

impl Style {
    /// The common case — size + colour, proportional, no block-level marks. Other fields default.
    pub fn new(size: f32, color: Color32) -> Self {
        Style { size, color, ..Self::default() }
    }
}

/// Mark styling this crate does not hardcode — consumers pass their palette.
#[derive(Clone, Copy)]
pub struct Theme {
    pub code_color: Color32,
    pub code_bg: Color32,
    pub link_color: Color32,
    pub link_underline: Color32,
    pub strike_color: Color32,
}

/// Open, string-keyed marks. A flag mark (`"bold"`) carries value `""`; a valued mark (`"link"`)
/// carries its value. Known keys map to a `TextFormat`; unknown are ignored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Marks(Vec<(String, String)>);

impl Marks {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has(&self, key: &str) -> bool {
        self.0.iter().any(|(k, _)| k == key)
    }

    /// The value of a mark (e.g. a `"link"`'s url), if present.
    pub fn value(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    /// Add a flag mark (no value), builder-style.
    pub fn flag(mut self, key: impl Into<String>) -> Self {
        self.0.push((key.into(), String::new()));
        self
    }

    /// Add a valued mark (e.g. `("link", url)`), builder-style.
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.0.push((key.into(), value.into()));
        self
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One run of text: its string, its marks, and an optional explicit foreground that overrides
/// both the theme and the base colour (the seam tree-sitter/math feed through).
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: String,
    pub marks: Marks,
    pub color: Option<Color32>,
}

impl Run {
    /// An unmarked run that inherits the base style.
    pub fn plain(text: impl Into<String>) -> Self {
        Run { text: text.into(), marks: Marks::new(), color: None }
    }
}

/// The block-level base `TextFormat` from a [`Style`] — the starting point every run inherits
/// before its own marks. `bold_fam` is the resolved bold family (or `Proportional` font-less).
fn base_format(style: Style, bold_fam: &FontFamily) -> TextFormat {
    let family = if style.mono {
        FontFamily::Monospace
    } else if style.bold {
        bold_fam.clone()
    } else {
        FontFamily::Proportional
    };
    TextFormat {
        font_id: FontId::new(style.size, family),
        color: style.color,
        line_height: style.line_height,
        extra_letter_spacing: style.letter_spacing,
        italics: style.italic,
        ..Default::default()
    }
}

/// Build a [`LayoutJob`] from runs over a block-level [`Style`] (known marks → `TextFormat`).
/// `bold_fam` is the resolved bold family (via [`galley`], or `FontFamily::Proportional` in
/// font-less tests).
pub fn layout_job(runs: &[Run], style: Style, theme: Theme, bold_fam: FontFamily, wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;

    // An empty run list still needs one (empty) section so the galley has a caret row — at the
    // block's own metrics (an empty heading row is heading-tall).
    if runs.is_empty() {
        job.append("", 0.0, base_format(style, &bold_fam));
        return job;
    }

    for run in runs {
        let code = run.marks.has("code");
        let bold = run.marks.has("bold");
        let italic = run.marks.has("italic");
        let link = run.marks.has("link");
        let strike = run.marks.has("strike");

        // Start from the block-level base, then layer the run's own marks on top.
        let mut fmt = base_format(style, &bold_fam);
        // Font: inline code wins (mono, slightly smaller); else a bold *mark* forces the bold
        // face; else keep the base family (which already encodes the block's mono/bold).
        if code {
            fmt.font_id = FontId::new(style.size * 0.92, FontFamily::Monospace);
        } else if bold {
            fmt.font_id = FontId::new(style.size, bold_fam.clone());
        }
        if italic {
            fmt.italics = true; // egui slant (fake-shear) — mirrored in the PDF backend.
        }

        // Colour precedence: explicit run colour > code/link theme colour > base colour.
        fmt.color = run.color.unwrap_or(if code {
            theme.code_color
        } else if link {
            theme.link_color
        } else {
            fmt.color
        });
        if code {
            fmt.background = theme.code_bg;
        }
        if link {
            fmt.underline = Stroke::new(1.0, theme.link_underline);
        }
        // Strike: a block-level strike (done to-do) is faint (`theme.strike_color`) and wins; an
        // inline `strike` mark strikes in the run's own (resolved) text colour.
        if style.strike {
            fmt.strikethrough = Stroke::new(1.0, theme.strike_color);
        } else if strike {
            fmt.strikethrough = Stroke::new(1.0, fmt.color);
        }

        job.append(&run.text, 0.0, fmt);
    }
    job
}

/// Render runs to a galley — the public output. Resolves the bold family with the proportional
/// fallback so a font-less context never panics.
pub fn galley(ctx: &egui::Context, runs: &[Run], style: Style, theme: Theme, wrap_width: f32) -> Arc<Galley> {
    let bold_fam = bold_or_fallback(ctx);
    let job = layout_job(runs, style, theme, bold_fam, wrap_width);
    ctx.fonts_mut(|f| f.layout_job(job))
}

/// A content+marks+style fingerprint for the galley cache: changes whenever any run's
/// text/marks/colour (or style/theme/wrap) changes — even when the total length doesn't.
pub fn fingerprint(runs: &[Run], style: Style, theme: Theme, wrap_width: f32) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for run in runs {
        run.text.hash(&mut h);
        run.marks.hash(&mut h);
        run.color.map(|c| c.to_array()).hash(&mut h);
    }
    style.size.to_bits().hash(&mut h);
    style.color.to_array().hash(&mut h);
    (style.mono, style.bold, style.italic, style.strike).hash(&mut h);
    style.line_height.map(|lh| lh.to_bits()).hash(&mut h);
    style.letter_spacing.to_bits().hash(&mut h);
    for c in [theme.code_color, theme.code_bg, theme.link_color, theme.link_underline, theme.strike_color] {
        c.to_array().hash(&mut h);
    }
    wrap_width.to_bits().hash(&mut h);
    h.finish()
}

/// A single-entry galley cache keyed by [`fingerprint`] — a clean run is an `Arc` clone instead
/// of a reshape.
#[derive(Default)]
pub struct Cache {
    key: Option<u64>,
    galley: Option<Arc<Galley>>,
}

impl Cache {
    pub fn galley(
        &mut self,
        ctx: &egui::Context,
        runs: &[Run],
        style: Style,
        theme: Theme,
        wrap_width: f32,
    ) -> Arc<Galley> {
        let fp = fingerprint(runs, style, theme, wrap_width);
        if self.key != Some(fp) {
            self.key = Some(fp);
            self.galley = Some(galley(ctx, runs, style, theme, wrap_width));
        }
        self.galley.clone().expect("just populated")
    }
}

#[cfg(test)]
mod tests;
