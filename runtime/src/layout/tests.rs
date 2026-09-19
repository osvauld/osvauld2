use super::*;
use crate::el::{Placement, PlacementAlign, PlacementSide, frame, rich, row, text, text_input};
use crate::frame::{Brush, Frame, Item, Path};
use std::sync::Arc;
use vello::Scene;
use vello::kurbo::PathEl;
use vello::peniko::{Color, Fill};

/// Long enough to overflow any of the widths below, with no long word to get stuck on.
const PARA: &str = "the quick brown fox jumps over the lazy dog and keeps on running";
const WHITE: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0xFF);

/// The rect of the one node carrying text.
fn text_rect(root: El<()>) -> Rect {
    solve(root, &mut TextEngine::new(), (800.0, 600.0), &Store::new())
        .iter()
        .find(|p| p.appearance.text.is_some())
        .expect("no text node")
        .rect
}

/// Widths of named nodes, from one solve. Sizing tests read children by id rather than by
/// position, so a change to the emit order can't quietly make them assert about another node.
/// One call for all of them because `El` is not `Clone` — it carries handler closures.
fn widths_of(root: El<()>, ids: &[&str]) -> Vec<f64> {
    let placed = solve(root, &mut TextEngine::new(), (800.0, 600.0), &Store::new());
    ids.iter()
        .map(|id| {
            placed
                .iter()
                .find(|p| p.id.as_deref() == Some(*id))
                .unwrap_or_else(|| panic!("no node with id {id}"))
                .rect
                .width()
        })
        .collect()
}

#[test]
fn frame_leaf_uses_intrinsic_content_size_and_paints_at_its_content_origin() {
    let path = Arc::new(
        Path::new(vec![
            PathEl::MoveTo((0.0, 0.0).into()),
            PathEl::LineTo((120.0, 0.0).into()),
            PathEl::LineTo((120.0, 80.0).into()),
            PathEl::ClosePath,
        ])
        .unwrap(),
    );
    let brush = Arc::new(Brush::solid(WHITE).unwrap());
    let visual = Arc::new(
        Frame::new(
            120.0,
            80.0,
            None,
            vec![Item::fill(path, brush, Fill::NonZero)],
        )
        .unwrap(),
    );
    let store = Store::new();
    let explicit = solve(
        frame::<()>(visual.clone()).w(60.0),
        &mut TextEngine::new(),
        (800.0, 600.0),
        &store,
    );
    assert_eq!(explicit[0].rect, Rect::new(0.0, 0.0, 60.0, 80.0));

    let placed = solve(
        frame::<()>(visual).pad(10.0),
        &mut TextEngine::new(),
        (800.0, 600.0),
        &store,
    );
    assert_eq!(placed[0].rect, Rect::new(0.0, 0.0, 140.0, 100.0));
    let mut scene = Scene::new();
    crate::paint::draw(
        &mut scene,
        &placed,
        &mut TextEngine::new(),
        Affine::IDENTITY,
        None,
        &store,
        &crate::editor::Focus::new(),
        None,
    );
    assert_eq!(scene.encoding().n_paths, 1);
    assert_eq!(scene.encoding().transforms[0].translation, [10.0, 10.0]);
}

fn bottom_start() -> Placement {
    Placement {
        side: PlacementSide::Bottom,
        align: PlacementAlign::Start,
    }
}

#[test]
fn element_overlay_anchors_to_its_stable_transformed_screen_rect() {
    let root = || {
        let panel: El<()> = col().id("popover").size(120.0, 80.0);
        let anchor = col()
            .id("anchor")
            .size(100.0, 40.0)
            .on_click(())
            .press_scale(0.8)
            .overlay(panel, None, bottom_start(), Anchor::Element);
        let content = col()
            .child(col().h(200.0))
            .child(row().child(col().w(200.0)).child(anchor));
        col().id("zoom").full().zoomable().child(content)
    };
    let mut store = Store::new();
    *store.get_or(&Id::from("zoom"), Slot::Zoom) = Zoom {
        scale: 0.5,
        pan: (30.0, 20.0),
    };
    let popover_rect = |placed: &[Placed<()>]| {
        placed
            .iter()
            .find(|p| p.id.as_deref() == Some("popover"))
            .unwrap()
            .rect
    };
    let settled = solve(root(), &mut TextEngine::new(), (800.0, 600.0), &store);
    let spring = store.get_or_with(&Id::from("anchor"), Slot::PressScale, || Spring::new(1.0));
    spring.value = 1.0;
    spring.target = 1.0;
    let pressed = solve(root(), &mut TextEngine::new(), (800.0, 600.0), &store);

    assert_eq!(
        popover_rect(&settled),
        Rect::new(130.0, 140.0, 250.0, 220.0)
    );
    assert_eq!(popover_rect(&pressed), popover_rect(&settled));
}

#[test]
fn point_overlay_anchor_is_already_in_screen_space() {
    let root: El<()> = col().overlay(
        col().id("popover").size(120.0, 80.0),
        None,
        bottom_start(),
        Anchor::Point(250.0, 180.0),
    );
    let placed = solve(root, &mut TextEngine::new(), (800.0, 600.0), &Store::new());
    let rect = placed
        .iter()
        .find(|p| p.id.as_deref() == Some("popover"))
        .unwrap()
        .rect;

    assert_eq!(rect, Rect::new(250.0, 180.0, 370.0, 260.0));
}

fn overflowing_board(cards: usize) -> El<()> {
    let body = col()
        .id("cards")
        .grow()
        .scroll_y()
        .children((0..cards).map(|i| text("card").id(format!("card:{i}")).h(50.0)));
    let column = col()
        .id("column")
        .w(240.0)
        .child(col().h(40.0))
        .child(body)
        .child(col().h(40.0));
    col().h(500.0).child(
        col().id("zoom").grow().zoomable().child(
            row()
                .id("board")
                .h_full()
                .stretch()
                .child(column)
                .child(col().id("sibling").w(240.0)),
        ),
    )
}

#[test]
fn overflowing_scroller_does_not_enlarge_a_zoomed_board() {
    let dimensions = |cards| {
        let placed = solve(
            overflowing_board(cards),
            &mut TextEngine::new(),
            (800.0, 600.0),
            &Store::new(),
        );
        let heights = ["board", "column", "sibling", "cards"].map(|id| {
            placed
                .iter()
                .find(|p| p.id.as_deref() == Some(id))
                .unwrap()
                .rect
                .height()
        });
        let cards_top = placed
            .iter()
            .find(|p| p.id.as_deref() == Some("cards"))
            .unwrap()
            .rect
            .y0;
        let content_bottom = placed
            .iter()
            .filter(|p| p.id.as_deref().is_some_and(|id| id.starts_with("card:")))
            .map(|p| p.rect.y1)
            .fold(cards_top, f64::max);
        let reported_content = placed
            .iter()
            .find(|p| p.id.as_deref() == Some("cards"))
            .unwrap()
            .content_size
            .1;
        (heights, content_bottom - cards_top, reported_content)
    };
    let short = dimensions(2);
    let overflowing = dimensions(20);

    assert_eq!(short.0, overflowing.0);
    for (actual, expected) in short.0.into_iter().zip([500.0, 500.0, 500.0, 420.0]) {
        assert!(
            (actual - expected).abs() < 1.0,
            "got {actual}, expected {expected}"
        );
    }
    assert!(overflowing.1 > short.1);
    assert!(overflowing.2 > overflowing.0[3] as f32);
    assert!(overflowing.2 > short.2);
}

/// The acceptance test for the whole hook, and the bug it was written against: before it, a
/// text leaf was shaped at max-content and frozen into `style.size` before Taffy ran, so this
/// string came out one line tall and far wider than the 200pt parent that contained it.
#[test]
fn text_wraps_to_the_width_its_parent_offers() {
    let wrapped = text_rect(col().w(200.0).child(text(PARA)));
    let loose = text_rect(col().child(text(PARA)));

    assert!(
        wrapped.width() <= 200.0,
        "text overflowed its parent: {} > 200",
        wrapped.width()
    );
    assert!(
        loose.width() > 200.0,
        "the test string is too short to prove anything: {}",
        loose.width()
    );
    assert!(
        wrapped.height() > loose.height(),
        "wrapping added no height: {} vs {}",
        wrapped.height(),
        loose.height()
    );
}

/// A narrower box is a taller one, all the way down. Guards the direction of the constraint:
/// passing Taffy's available width through unchanged is easy to get inverted or dropped, and
/// either mistake still produces *a* layout.
#[test]
fn a_narrower_parent_makes_taller_text() {
    let heights: Vec<f64> = [400.0, 200.0, 100.0]
        .iter()
        .map(|w| text_rect(col().w(*w).child(text(PARA))).height())
        .collect();

    assert!(
        heights.windows(2).all(|p| p[1] > p[0]),
        "height did not grow as the parent shrank: {heights:?}"
    );
}

/// Padding is added exactly once. It used to be added by hand here *and* by Taffy at
/// `leaf.rs:146`; the hand-written half is gone, so this is what proves the survivor is
/// counted and not the ghost.
#[test]
fn padding_is_added_once_around_the_measured_text() {
    let bare = text_rect(text("hello"));
    let padded = text_rect(text("hello").pad(10.0));

    assert!(
        (padded.width() - bare.width() - 20.0).abs() < 1.0,
        "padded {} vs bare {} — expected exactly 20 more",
        padded.width(),
        bare.width()
    );
    assert!((padded.height() - bare.height() - 20.0).abs() < 1.0);
}

/// An explicit size still wins. Taffy clamps the measured size against `style.size`, which is
/// the mechanism that replaced the old `is_auto()` guards — worth pinning, because losing it
/// would silently make every `.w()` on a text leaf advisory.
#[test]
fn an_explicit_width_beats_the_measurement() {
    let r = text_rect(text("hi").w(300.0));
    assert!((r.width() - 300.0).abs() < 1.0, "got {}", r.width());
}

/// The layout pass and the paint pass have to shape the same way — same width, same plain/rich
/// routing — and only the layout pass is told either. Getting it wrong is invisible to every
/// test above: the box is reserved correctly and the glyphs land somewhere else, which is
/// exactly what shipped for one commit here.
///
/// Asserted as the height coming *back*, not as a bound, because the two ways to get it wrong
/// point opposite ways. Shaping unwrapped overflows the box; shaping a rich leaf through the
/// plain path underfills one sized for runs it then ignored. Only equality catches both.
#[test]
fn paint_shapes_text_to_the_box_the_layout_reserved() {
    let runs = vec![
        Run::new(0..20, 15.0, WHITE).bold(),
        Run::new(20..PARA.len(), 34.0, WHITE),
    ];
    for (name, leaf) in [("plain", text(PARA)), ("rich", rich(PARA, runs))] {
        let placed = solve::<()>(
            col().w(200.0).pad(12.0).child(leaf),
            &mut TextEngine::new(),
            (800.0, 600.0),
            &Store::new(),
        );
        let p = placed
            .iter()
            .find(|p| p.appearance.text.is_some())
            .expect("no text node");
        let ts = p.appearance.text.as_ref().unwrap();

        let (w, h) = crate::paint::measure_placed(&mut TextEngine::new(), ts, p.rect, p.pad);
        let (box_w, box_h) = (
            p.rect.width() - p.pad.x0 - p.pad.x1,
            p.rect.height() - p.pad.y0 - p.pad.y1,
        );

        assert!(
            w as f64 <= box_w + 1.0,
            "{name}: glyphs run {w} wide out of a {box_w} box"
        );
        assert!(
            (h as f64 - box_h).abs() < 1.0,
            "{name}: paint shapes {h} tall into a box reserved for {box_h}"
        );
    }
}

/// A rich leaf measures through the run list, which is the whole point of carrying it into
/// `TextCtx`. Pinned by size rather than by inspecting the tree, because a run that never
/// reaches the measure hook produces a perfectly plausible box — just the plain one.
#[test]
fn a_rich_leaf_is_measured_from_its_runs() {
    let s = "small BIG";
    let plain = text_rect(text(s));
    let mixed = text_rect(rich(
        s,
        vec![
            Run::new(0..6, 15.0, WHITE),
            Run::new(6..s.len(), 40.0, WHITE),
        ],
    ));

    assert!(
        mixed.height() > plain.height(),
        "the large run did not make the leaf taller: {} vs {}",
        mixed.height(),
        plain.height()
    );
    assert!(
        mixed.width() > plain.width(),
        "the large run did not make the leaf wider: {} vs {}",
        mixed.width(),
        plain.width()
    );
}

/// An empty run list is exactly `text`, so `rich` can be the only builder a caller reaches for
/// without paying for it when there is nothing to style.
#[test]
fn rich_with_no_runs_is_plain_text() {
    assert_eq!(text_rect(rich(PARA, Vec::new())), text_rect(text(PARA)));
}

/// Rich leaves wrap like plain ones — the run list rides through the measure hook rather than
/// round it, so the width constraint still reaches parley.
#[test]
fn a_rich_leaf_still_wraps_to_its_parent() {
    let runs = vec![
        Run::new(0..20, 15.0, WHITE).bold(),
        Run::new(20..PARA.len(), 15.0, WHITE),
    ];
    let r = text_rect(col().w(200.0).child(rich(PARA, runs)));

    assert!(r.width() <= 200.0, "rich text overflowed: {}", r.width());
    assert!(r.height() > 20.0, "rich text did not wrap: {}", r.height());
}

/// An input is designed sized, never text sized — the one exception `build` carves out, and
/// the reason it hands Taffy no context for one. Stated as "its box does not move when its
/// value does", which holds whatever the designed width happens to be.
#[test]
fn an_input_does_not_grow_with_its_value() {
    let short = text_rect(text_input("hi", "field", |_| ()));
    let long = text_rect(text_input(PARA, "field", |_| ()));

    assert_eq!(
        (short.width(), short.height()),
        (long.width(), long.height()),
        "an input resized itself to fit its value"
    );
}

// ── elastic sizing ───────────────────────────────────────────────────────
// What a splitter drag has to be able to write. The `grow`↔`grow` row of the resize table in
// docs/design/code-as-tree.md §11 was inexpressible while `grow` set `flex_grow = 1.0` flatly:
// two elastic siblings were permanently 50/50, so a boundary between them had nowhere to put
// the drag. These say the knobs exist and that Taffy honours them.

/// A ratio, not a flag. 2:1 of 600 is 400/200.
#[test]
fn two_grow_siblings_split_by_their_ratio() {
    let root = row()
        .w(600.0)
        .child(col().id("a").grow_by(2.0))
        .child(col().id("b").grow_by(1.0));
    let w = widths_of(root, &["a", "b"]);

    assert!((w[0] - 400.0).abs() < 1.0, "a was {}, expected 400", w[0]);
    assert!((w[1] - 200.0).abs() < 1.0, "b was {}, expected 200", w[1]);
}

/// The old spelling still means what it meant, so no app has to change. `grow = true` reaches
/// this path through `props.rs` as `grow_by(1.0)`.
#[test]
fn plain_grow_is_still_an_even_split() {
    let root = row()
        .w(600.0)
        .child(col().id("a").grow())
        .child(col().id("b").grow());
    let w = widths_of(root, &["a", "b"]);

    assert!((w[0] - 300.0).abs() < 1.0, "a was {}", w[0]);
    assert!((w[1] - 300.0).abs() < 1.0, "b was {}", w[1]);
}

/// Elastic without a floor collapses. The sidebar dragged shut that cannot be dragged back is
/// the failure this prevents, so the floor has to beat the ratio rather than lose to it.
#[test]
fn min_w_outranks_the_grow_ratio() {
    let root = row()
        .w(600.0)
        .child(col().id("a").grow_by(1.0).min_w(500.0))
        .child(col().id("b").grow_by(5.0));
    let w = widths_of(root, &["a"]);

    assert!(
        (w[0] - 500.0).abs() < 1.0,
        "a was {}, expected its 500 floor",
        w[0]
    );
}

/// `shell2`'s header, small enough to reproduce the bug: back button, title, `grow` spacer,
/// action button. Narrow it and the spacer collapses first, then every remaining item shrinks
/// together — a flex item's floor being its *min-content* width, which for a text leaf is the
/// longest word. So the button does not clip, it folds "+ Add item" into stacked words inside
/// a 36pt box. Measured at 260pt before the fix: 28 wide and 53 tall, three lines.
fn header(action: El<()>) -> El<()> {
    let page = col().full().pad(40.0).gap(24.0).child(
        row()
            .gap(12.0)
            .align_center()
            .child(text("My workspace").font_size(15.0))
            .child(col().grow())
            .child(action),
    );
    page
}

/// The label's height, which is the only portable way to say "it wrapped" — font metrics are
/// the OS's, so no absolute pixel count here would travel.
fn label_height(root: El<()>) -> f64 {
    solve(root, &mut TextEngine::new(), (260.0, 600.0), &Store::new())
        .iter()
        .find(|p| {
            p.appearance
                .text
                .as_ref()
                .is_some_and(|t| t.text == "+ Add item")
        })
        .expect("no label")
        .rect
        .height()
}

/// Paint re-shapes a string into the box the layout reserved, so the two have to agree about
/// how many lines that is. `paint_shapes_text_to_the_box_the_layout_reserved` above covers a
/// paragraph — and a paragraph is where this bug hides, because both paths wrap it identically
/// and the heights match. The failure needs a label that fits on *one* line and whose natural
/// width is fractional.
///
/// "+ new workspace" at 13pt measures 105.0010. Taffy rounds the reserved box to 105, and paint,
/// handed one thousandth of a point less than the string that sized it, breaks the line: a
/// one-line label painted as two, in a box tall enough for one, at every window size. Reported
/// as "same everywhere, it has space it is not using", which is exactly right.
#[test]
fn paint_does_not_fold_a_label_the_layout_fitted_on_one_line() {
    for label in [
        "+ new workspace",
        "+ Add another account",
        "+ Add item",
        "Add card",
        "Create Identity",
    ] {
        let placed = solve::<()>(
            row()
                .h(36.0)
                .px(14.0)
                .center()
                .child(text(label).font_size(13.0)),
            &mut TextEngine::new(),
            (800.0, 600.0),
            &Store::new(),
        );
        let p = placed
            .iter()
            .find(|p| p.appearance.text.is_some())
            .expect("no text node");
        let ts = p.appearance.text.as_ref().unwrap();
        let (_, painted) = crate::paint::measure_placed(&mut TextEngine::new(), ts, p.rect, p.pad);

        assert!(
            (painted as f64 - p.rect.height()).abs() < 1.0,
            "{label:?}: layout reserved {:.2} tall, paint shaped {painted:.2} — it folded",
            p.rect.height()
        );
    }
}

/// `no_wrap` outranks a definite width, which is the case `no_shrink` cannot reach: a label
/// centred in a *column* has its width set on the cross axis, where `flex_shrink` does nothing
/// at all. This is the shape of `login.rs`'s "+ Add another account", measured at 139pt.
#[test]
fn a_label_does_not_fold_however_narrow_the_box() {
    let boxed = |el: El<()>| col().w(80.0).center().child(el);
    let folded = text_rect(boxed(text("+ Add another account").font_size(13.0)));
    let kept = text_rect(boxed(
        text("+ Add another account").font_size(13.0).no_wrap(),
    ));

    assert!(
        folded.height() > kept.height(),
        "the 80pt box did not fold the wrapping label: {folded:?}"
    );
    assert!(
        kept.width() > 80.0,
        "no_wrap must overflow the box, not shrink into it: {kept:?}"
    );
}

/// And prose still wraps — the parley hook is the point of the whole measure path, and
/// `no_wrap` is opt-in precisely so this keeps working.
#[test]
fn no_wrap_is_opt_in_and_prose_still_wraps() {
    let r = text_rect(col().w(200.0).child(text(PARA)));
    assert!(r.width() <= 200.0, "prose stopped wrapping: {r:?}");
}

#[test]
fn a_squeezed_row_folds_a_button_label_without_no_shrink() {
    let bare = row()
        .h(36.0)
        .px(14.0)
        .center()
        .child(text("+ Add item").font_size(13.0));
    let held = row()
        .h(36.0)
        .px(14.0)
        .center()
        .no_shrink()
        .child(text("+ Add item").font_size(13.0));

    let (folded, kept) = (label_height(header(bare)), label_height(header(held)));
    assert!(
        kept < folded,
        "no_shrink changed nothing: {kept} vs {folded} — is the row wide enough to prove it?"
    );
    assert!(
        folded > kept * 1.5,
        "the label did not actually fold, so this test proves nothing: {folded} vs {kept}"
    );
}

/// And a ceiling holds against a child that would otherwise take everything.
#[test]
fn max_w_caps_a_full_width_child() {
    let root = row().w(600.0).child(col().id("a").w_full().max_w(150.0));
    let w = widths_of(root, &["a"]);

    assert!(
        (w[0] - 150.0).abs() < 1.0,
        "a was {}, expected its 150 cap",
        w[0]
    );
}
