use super::*;
use crate::tests::{assert_fits, screen_rects};

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
            let rects = screen_rects(|| screen.view(), width);
            let field = rects.iter().find(|r| r.id == id).unwrap();
            assert_fits(field, width);
            assert!((field.w - (width * 0.9).min(cap) + 24.0).abs() <= 0.5, "{field:?}");
        }
    }
}
