//! A kept subtree (`docs/design/ui-reuse.md` step 2): `El` clones whole — closures, input maps,
//! overlays — so a screen can keep what it built and hand out a copy next frame.
use super::*;

fn tree() -> El<String> {
    col()
        .id("root")
        .child(
            col()
                .id("button")
                .on_click("clicked".to_string())
                .on_hover("button", |h| format!("hover {}", h.phase.as_str()))
                .child(text("label")),
        )
        .child(text_input("typed", "field", |s| format!("typed {s}")))
        .child(col().id("host").overlay(
            col().id("panel").on_click("panel".to_string()),
            Some("dismissed".to_string()),
            Placement { side: PlacementSide::Bottom, align: PlacementAlign::Start },
            Anchor::Element,
        ))
}

#[test]
fn a_clone_reports_the_same_tree() {
    let original = tree();
    let copy = original.clone();
    assert_eq!(format!("{:?}", original.info()), format!("{:?}", copy.info()));
}

#[test]
fn a_clone_fires_every_kind_of_handler_and_the_original_still_can() {
    let original = tree();
    let mut copy = original.clone();
    assert_eq!(copy.trigger("button", Action::Click), Ok("clicked".into()));
    assert_eq!(copy.trigger("button", Action::Hover(HoverPhase::Enter, (1.0, 1.0))), Ok("hover enter".into()));
    assert_eq!(copy.trigger("field", Action::Type("x")), Ok("typed x".into()));
    assert_eq!(copy.trigger("panel", Action::Click), Ok("panel".into()));

    drop(copy);
    let mut original = original;
    assert_eq!(
        original.trigger("button", Action::Click),
        Ok("clicked".into()),
        "firing the copy's click consumed the original's"
    );
}
