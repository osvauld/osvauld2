use super::{Launcher, source};
use app_host::{SourceEdit, SourceFile, edit_source_file};
use vault::Vault;

impl Launcher {
    pub fn read_file(&self, path: &str) -> Result<SourceFile, String> {
        crate::valid_source_path(path)?;
        self.app.read_source_file_versioned(path)
    }

    pub fn edit_file(
        &mut self,
        vault: &Vault,
        path: &str,
        revision: &str,
        edits: &[SourceEdit],
    ) -> Result<String, String> {
        crate::valid_source_path(path)?;
        let size = edits.iter().fold(0usize, |n, e| {
            n.saturating_add(e.old_text.len())
                .saturating_add(e.new_text.len())
        });
        if edits.len() > 128 || size > 1024 * 1024 {
            return Err("source edit batch is too large".into());
        }
        // LoroDoc::clone shares the document. Export/import makes an independent candidate.
        let candidate_source = source::import(&self.app.source_snapshot()?)?;
        let edited = edit_source_file(&candidate_source, path, revision, edits)?;
        let candidate = self
            .app
            .stage_source(candidate_source.clone())
            .map_err(|error| {
                self.app.note(format!("launcher edit rejected: {error}"));
                error
            })?;
        vault
            .put_entry(source::ENTRY, &edited.snapshot)
            .map_err(|e| e.to_string())?;
        self.source.borrow_mut().replace(candidate_source);
        self.app = candidate;
        self.warning = None;
        self.commands.borrow_mut().clear();
        Ok(edited.revision)
    }

    pub fn reset(&mut self, vault: &Vault) -> Result<(), String> {
        // Recovery must not execute the old VM, even to export its scratch state.
        let mut replacement = Self::open(self.context.clone(), None)?;
        replacement.app = replacement.app.stage_source(source::bundled()?)?;
        Self::clear_override(vault)?;
        *self = replacement;
        Ok(())
    }

    pub fn clear_override(vault: &Vault) -> Result<(), String> {
        vault.delete_entry(source::ENTRY).map_err(|e| e.to_string())
    }

    pub fn console(&self, limit: usize) -> Vec<String> {
        self.app.console(limit.min(1000))
    }
}
