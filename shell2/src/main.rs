//! shell2 — the osvauld UI. It defines the screens (login, …) and their logic; the window, GPU,
//! event loop, layout, and input all live in the `runtime` crate. `main` just picks the first app.

mod login;
mod theme;

use login::LoginScreen;

fn main() {
    runtime::run(LoginScreen::new());
}
