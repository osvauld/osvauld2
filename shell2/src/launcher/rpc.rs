use super::Launcher;
use crate::{Screen, Shell};
use app_host::SourceEdit;
use osvauld_rpc::{Request, Response};
use serde_json::{Value, json};

impl Shell {
    pub(crate) fn answer_launcher(&mut self, request: Request) -> Response {
        match self.launcher_request(request) {
            Ok(value) => Response::ok(value),
            Err(error) => Response::err(error),
        }
    }

    fn selected_launcher(&self) -> Result<&Launcher, String> {
        match &self.screen {
            Screen::Items(i) => i.launcher(),
            _ => Err("select a workspace to inspect or edit its launcher".into()),
        }
    }

    fn launcher_request(&mut self, request: Request) -> Result<Value, String> {
        if self.vault.current().is_none() {
            return Err("vault is locked".into());
        }
        if self.focused != 0 && !matches!(request, Request::ResetLauncher {}) {
            return Err("focus Home to inspect or edit its launcher".into());
        }
        match request {
            Request::ReadLauncherFile { path } => {
                let file = self.selected_launcher()?.read_file(&path)?;
                Ok(json!({"content": file.content, "revision": file.revision}))
            }
            Request::EditLauncherFile {
                path,
                expected_revision,
                edits,
            } => {
                let edits: Vec<SourceEdit> = edits
                    .into_iter()
                    .map(|e| SourceEdit {
                        old_text: e.old_text,
                        new_text: e.new_text,
                    })
                    .collect();
                let revision = self.edit_launcher(None, &path, &expected_revision, &edits)?;
                Ok(json!({"revision": revision, "persisted": true, "activation": "activated"}))
            }
            Request::DumpLauncherTree {} => {
                Ok(crate::info_json(&self.selected_launcher()?.view().info()))
            }
            Request::ReadLauncherConsole { last } => {
                Ok(json!(self.selected_launcher()?.console(last)))
            }
            Request::ClickLauncher { el_id } => self.fire_launcher(&el_id, runtime::Action::Click),
            Request::TypeLauncher { el_id, content } => {
                if content.len() > 1024 * 1024 {
                    return Err("launcher input is too large".into());
                }
                self.fire_launcher(&el_id, runtime::Action::Type(&content))
            }
            Request::ResetLauncher {} => {
                self.reset_launcher()?;
                Ok(json!("reset"))
            }
            _ => Err("not a launcher request".into()),
        }
    }

    fn fire_launcher(&mut self, id: &str, action: runtime::Action<'_>) -> Result<Value, String> {
        let Screen::Items(screen) = &self.screen else {
            return Err("select a workspace".into());
        };
        let msg = screen.launcher()?.view().trigger(id, action)?;
        runtime::App::update(self, msg);
        Ok(json!("fired"))
    }

    pub(crate) fn edit_launcher(
        &mut self,
        ws_id: Option<&str>,
        path: &str,
        revision: &str,
        edits: &[SourceEdit],
    ) -> Result<String, String> {
        if self.vault.current().is_none() {
            return Err("vault is locked".into());
        }
        if self.focused != 0 {
            return Err("focus Home to edit its launcher".into());
        }
        let Screen::Items(screen) = &mut self.screen else {
            return Err("select a workspace to edit its launcher".into());
        };
        if ws_id.is_some_and(|id| id != screen.ws().id) {
            return Err("launcher workspace changed".into());
        }
        screen
            .launcher_mut()?
            .edit_file(&self.vault, path, revision, edits)
    }

    pub(crate) fn reset_launcher(&mut self) -> Result<(), String> {
        match &mut self.screen {
            Screen::Items(screen) => screen.reset_launcher(&self.vault)?,
            _ => Launcher::clear_override(&self.vault)?,
        }
        self.error = None;
        self.reset_launcher_pending = false;
        Ok(())
    }
}
