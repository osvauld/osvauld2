//! shell2 — the osvauld UI. It defines the screens (login, …) and their logic; the window, GPU,
//! event loop, layout, and input all live in the `runtime` crate. `main` just picks the first app.

mod login;
mod mnemonic;
mod signup;
mod theme;

use std::path::PathBuf;

use crate::{
    login::{LoginMsg, LoginScreen},
    mnemonic::{Mnemonic, MnemonicMsg},
};
use runtime::{App, El, EventLoopProxy};
use vault::Vault;

use crate::signup::{SignupForm, SignupMsg};

#[derive(Clone)]
pub enum Msg {
    Signup(SignupMsg),
    Mnemonic(MnemonicMsg),
    Login(LoginMsg),
}
pub enum Screen {
    Signup(SignupForm),
    Mnemonic(Mnemonic),
    Login(LoginScreen),
}

struct Shell {
    proxy: EventLoopProxy<Msg>,
    screen: Screen,
    vault: Vault,
}
fn main() {
    // runtime::run(
    //     app_host::LuaApp::from_file(concat!(env!("CARGO_MANIFEST_DIR"), "/src/kanban2.lua"))
    //         .unwrap(),
    // );

    let data_dir = std::env::var_os("OSVAULD_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(vault::default_dir);
    let vault = vault::Vault::open(data_dir).expect("failed to open the osvauld data directory");
    runtime::run_with(|proxy| Shell::new(proxy, vault));
}
impl Shell {
    fn new(proxy: EventLoopProxy<Msg>, vault: Vault) -> Self {
        let screen = if vault.is_empty() {
            Screen::Signup(SignupForm::default())
        } else {
            Screen::Login(LoginScreen::new(&vault))
        };
        let shell = Shell {
            vault,
            proxy,
            screen,
        };
        shell
    }
}

impl App for Shell {
    type Msg = Msg;

    fn view(&self) -> El<Msg> {
        match &self.screen {
            Screen::Signup(f) => f.view(),
            Screen::Mnemonic(m) => m.view(),
            Screen::Login(a) => a.view(),
        }
    }
    fn update(&mut self, msg: Msg) {
        let next = match (&mut self.screen, msg) {
            (Screen::Signup(f), Msg::Signup(m)) => f.update(m, &mut self.vault, &self.proxy),
            (Screen::Mnemonic(f), Msg::Mnemonic(m)) => f.update(m),
            (Screen::Login(f), Msg::Login(m)) => f.update(m, &mut self.vault, &self.proxy),
            _ => None,
        };
        if let Some(next) = next {
            self.screen = next;
        }
    }
}
