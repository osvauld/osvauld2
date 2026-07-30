//! shell2 — the osvauld UI. It defines the screens (login, …) and their logic; the window, GPU,
//! event loop, layout, and input all live in the `runtime` crate. `main` just picks the first app.

mod login;
mod theme;
mod todo;

use login::{LoginMsg, LoginScreen};
use runtime::{App, El};
use todo::Msg as TodoMsg;

use crate::todo::TodoScreen;

#[derive(Clone)]
pub enum Msg {
    Login(LoginMsg),
    Todo(TodoMsg),
}
pub enum Screen {
    Login,
    Todo,
}
fn main() {
    // runtime::run(
    //     app_host::LuaApp::from_file(concat!(env!("CARGO_MANIFEST_DIR"), "/src/test.lua")).unwrap(),
    // );
    runtime::run(Shell::new());
}
struct Shell {
    screen: Screen,
    login: LoginScreen,
    todo: TodoScreen,
}
impl Shell {
    fn new() -> Self {
        Shell {
            screen: Screen::Todo,
            login: LoginScreen::new(),
            todo: TodoScreen::new(),
        }
    }
}

impl App for Shell {
    type Msg = Msg;

    fn view(&self) -> El<Msg> {
        match self.screen {
            Screen::Login => self.login.view().map(Msg::Login),
            Screen::Todo => self.todo.view().map(Msg::Todo),
        }
    }
    fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Login(m) => match self.login.update(m) {
                Some(login::Event::SignUpRequested) => self.screen = Screen::Todo,
                None => {}
            },
            Msg::Todo(t) => match self.todo.update(t) {
                Some(todo::Event::BackRequested) => self.screen = Screen::Login,
                None => {}
            },
        };
    }
}
