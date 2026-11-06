use super::*;
use crate::tests::{assert_fits, screen_rects};

#[test]
fn the_join_box_tells_a_share_link_from_a_node_ticket() {
    assert!(is_share_link("osvauld://s/3QfJ7u9kx"));
    assert!(is_share_link("  osvauld://s/3QfJ7u9kx\n"));
    assert!(!is_share_link("eyJ0aWNrZXQiOiJ..."));
    assert!(!is_share_link("osvauld://x/3Qf"));
    assert!(!is_share_link(""));
}

#[test]
fn a_failed_join_keeps_the_box_open_with_the_reason() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.signup("joiner", "test-only").unwrap();
    let mut screen = SpaceScreen::new(&vault);
    screen.show_claim = true;
    screen.joining = true;
    screen.join_answered(Err("not a share link".into()));
    assert!(screen.show_claim && !screen.joining);
    assert_eq!(screen.claim_error.as_deref(), Some("not a share link"));
    screen.claim_error = None;
    screen.join_answered(Ok(()));
    assert!(!screen.show_claim && screen.claim_error.is_none() && screen.claim_ticket.is_empty());
}

#[test]
fn workspace_popovers_fit_phones_and_keep_desktop_widths() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.signup("layout", "test-only").unwrap();
    vault.create_workspace("layout").unwrap();
    let mut screen = SpaceScreen::new(&vault);
    for width in [320.0_f32, 360.0, 393.0, 900.0] {
        for (claim, id, cap) in [(false, "workspace_name", 320.0), (true, "claim_ticket", 360.0)] {
            screen.show_claim = claim;
            screen.add_space = !claim;
            let rects = screen_rects(|| screen.view(None, None), width);
            let field = rects.iter().find(|r| r.id == id).unwrap();
            assert_fits(field, width);
            assert!((field.w - (width * 0.9).min(cap) + 24.0).abs() <= 0.5, "{field:?}");
        }
    }
}

#[test]
fn an_account_that_hosts_itself_still_has_the_join_button() {
    let tmp = tempfile::TempDir::new().unwrap();
    let mut vault = Vault::open(tmp.path().to_path_buf()).unwrap();
    vault.signup("host", "test-only").unwrap();
    vault.create_workspace("team").unwrap();
    // What every unlock does since step 4: the account's own owner grant, as its node record.
    crate::node::self_claim(&vault, crate::node::now_secs()).unwrap();
    let screen = SpaceScreen::new(&vault);
    assert!(screen.node.is_some() && !screen.foreign_node);
    let rects = screen_rects(|| screen.view(None, None), 393.0);
    let ids: Vec<_> = rects.iter().map(|r| r.id.to_string()).collect();
    assert!(ids.iter().any(|id| id == "claim_node"), "no join button for a self-hosting account: {ids:?}");
}
