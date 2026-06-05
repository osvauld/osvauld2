use doc_editor::{Doc, DocEditor};
use eframe::egui;
use vault::{AccountInfo, Vault};
use zeroize::Zeroize;

use compositor::Workspace;

use crate::components::Backdrop;
use crate::screens;

pub struct Sthalam {
    vault: Vault,
    screen: Screen,
    backdrop: Backdrop,
}

// The shell is a small state machine of full-window screens; each variant carries the
// in-progress input it needs (egui is immediate-mode, so the buffers live here).
pub enum Screen {
    Signup(SignupForm),
    Mnemonic(String),       // the recovery phrase, shown exactly once
    Accounts(AccountsView), // cached on entry — never re-read per frame
    Unlock(UnlockForm),
    Home {
        /// The `.doc` editor's view state (caret, focus). The document itself is owned
        /// separately so it can also be fed by the network and persisted.
        editor: DocEditor,
        /// The home document, loaded from the vault on this screen's first frame (where
        /// the vault is unlocked) — see `screens::home`.
        doc: Option<Doc>,
    },
    /// A workspace of sandboxed wasm app-cells — the intro app, for the POC.
    Workspace(Workspace),
}

impl Screen {
    /// Enter the home screen with a fresh editor; the document loads lazily on first frame.
    pub fn home() -> Self {
        Screen::Home { editor: DocEditor::new(), doc: None }
    }
}

// The login picker's state: the accounts (cached on entry) and which row is highlighted for
// keyboard nav (↑↓ to move, ↩ to unlock).
pub struct AccountsView {
    pub accounts: Vec<AccountInfo>,
    pub selected: usize,
}

// Results of the off-thread hashing, delivered back to the UI thread.
pub type SignupResult = Result<vault::PreparedAccount, vault::VaultError>;
pub type LoginResult = Result<vault::UnlockedAccount, vault::VaultError>;

#[derive(Default)]
pub struct SignupForm {
    pub label: String,
    pub passphrase: String,
    pub confirm: String,
    pub error: Option<String>,
    pub from_accounts: bool,
    // Some while the Argon2 hashing runs on a worker thread.
    pub pending: Option<std::sync::mpsc::Receiver<SignupResult>>,
}

#[derive(Default)]
pub struct UnlockForm {
    pub did: String,
    pub label: String,
    pub passphrase: String,
    pub show: bool, // reveal the passphrase (the field's show/hide toggle)
    pub error: Option<String>,
    // Some while the Argon2 decrypt runs on a worker thread.
    pub pending: Option<std::sync::mpsc::Receiver<LoginResult>>,
}

impl Sthalam {
    pub fn new(vault: Vault) -> Self {
        let screen = launch_screen(&vault);
        Self { vault, screen, backdrop: Backdrop::default() }
    }

    // Wipe any passphrase (or the recovery phrase) the outgoing screen held — the only
    // lingering copy — before moving on.
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
    // eframe 0.34 wraps this in a CentralPanel and hands us the `ui` directly.
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // The workspace screen composites wasm app-cells, which needs the wgpu
        // RenderState from eframe — handled here rather than via a plain screen fn.
        if matches!(self.screen, Screen::Workspace(_)) {
            let mut back = false;
            ui.horizontal(|ui| {
                if ui.button("← back").clicked() {
                    back = true;
                }
                ui.add_space(6.0);
                ui.weak("intro");
            });
            if let (Screen::Workspace(ws), Some(rs)) = (&mut self.screen, frame.wgpu_render_state()) {
                ws.ui(ui, rs);
            }
            if back {
                self.screen = Screen::home();
            }
            return;
        }

        // Disjoint borrows: the match holds `self.screen`, the arms take `self.vault`.
        let next = match &mut self.screen {
            Screen::Signup(form) => screens::signup(ui, &mut self.vault, form, &mut self.backdrop),
            Screen::Mnemonic(words) => screens::recovery(ui, words, &mut self.backdrop),
            Screen::Accounts(view) => screens::accounts(ui, view, &mut self.backdrop),
            Screen::Unlock(form) => screens::unlock(ui, &mut self.vault, form, &mut self.backdrop),
            Screen::Home { editor, doc } => screens::home(ui, editor, doc, &mut self.vault),
            Screen::Workspace(_) => None, // handled above
        };
        if let Some(next) = next {
            self.transition(next);
        }
    }
}

// No accounts → signup only; otherwise the account picker.
fn launch_screen(vault: &Vault) -> Screen {
    if vault.is_empty() {
        return Screen::Signup(SignupForm::default());
    }
    match vault.accounts() {
        Ok(accounts) => Screen::Accounts(AccountsView { accounts, selected: 0 }),
        Err(_) => Screen::Signup(SignupForm::default()),
    }
}
