use std::sync::mpsc::Receiver;

use vault::{AccountInfo, Vault};
use zeroize::Zeroize;

use crate::bridge::{self, DocRefresh};
use crate::components::Backdrop;
use crate::screens;
use crate::shell::Shell;

pub struct Sthalam {
    vault: Vault,
    screen: Screen,
    backdrop: Backdrop,
    refresh_rx: Receiver<DocRefresh>,
}

pub enum Screen {
    Signup(SignupForm),
    Mnemonic(String),
    Accounts(AccountsView),
    Unlock(UnlockForm),
    Shell(Shell),
}

impl Screen {
    pub fn shell() -> Self {
        Screen::Shell(Shell::new())
    }
}

pub struct AccountsView {
    pub accounts: Vec<AccountInfo>,
    pub selected: usize,
}

pub type SignupResult = Result<vault::PreparedAccount, vault::VaultError>;
pub type LoginResult = Result<vault::UnlockedAccount, vault::VaultError>;

#[derive(Default)]
pub struct SignupForm {
    pub label: String,
    pub passphrase: String,
    pub confirm: String,
    pub error: Option<String>,
    pub from_accounts: bool,
    pub pending: Option<std::sync::mpsc::Receiver<SignupResult>>,
}

#[derive(Default)]
pub struct UnlockForm {
    pub did: String,
    pub label: String,
    pub passphrase: String,
    pub show: bool,
    pub error: Option<String>,
    pub pending: Option<std::sync::mpsc::Receiver<LoginResult>>,
}

impl Sthalam {
    pub fn new(vault: Vault) -> Self {
        let screen = launch_screen(&vault);
        let refresh_rx = bridge::start(bridge::socket_path(), vault.clone());
        Self { vault, screen, backdrop: Backdrop::default(), refresh_rx }
    }

    fn transition(&mut self, next: Screen) {
        match &mut self.screen {
            Screen::Signup(form) => {
                form.passphrase.zeroize();
                form.confirm.zeroize();
            }
            Screen::Unlock(form) => form.passphrase.zeroize(),
            Screen::Mnemonic(words) => words.zeroize(),
            _ => {}
        }
        self.screen = next;
    }
}

impl eframe::App for Sthalam {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        // Merge any snapshots written by the bridge into open tabs.
        while let Ok(refresh) = self.refresh_rx.try_recv() {
            if let Screen::Shell(shell) = &mut self.screen {
                shell.apply_doc_refresh(&refresh.ws_id, &refresh.item_id, &refresh.snapshot);
            }
        }

        let next = match &mut self.screen {
            Screen::Signup(form) => screens::signup(ui, &mut self.vault, form, &mut self.backdrop),
            Screen::Mnemonic(words) => screens::recovery(ui, words, &mut self.backdrop),
            Screen::Accounts(view) => screens::accounts(ui, view, &mut self.backdrop),
            Screen::Unlock(form) => screens::unlock(ui, &mut self.vault, form, &mut self.backdrop),
            Screen::Shell(shell) => shell.ui(ui, &mut self.vault),
        };
        if let Some(next) = next {
            self.transition(next);
        }
    }
}

fn launch_screen(vault: &Vault) -> Screen {
    if vault.is_empty() {
        return Screen::Signup(SignupForm::default());
    }
    match vault.accounts() {
        Ok(accounts) => Screen::Accounts(AccountsView { accounts, selected: 0 }),
        Err(_) => Screen::Signup(SignupForm::default()),
    }
}
