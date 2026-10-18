//! Launcher owner for the account session; screens supply metadata, never own its VM.
use super::{Launcher, context::Context, source};
use crate::shell_api::Snapshot;
use std::{cell::RefCell, rc::Rc};
use vault::Vault;

pub(crate) type Inventory = Rc<RefCell<Snapshot>>;

#[derive(Default)]
pub(crate) struct Session {
    account: Option<String>,
    inventory: Option<Context>,
    launcher: Option<Result<Launcher, String>>,
}

impl Session {
    pub fn bind(&mut self, vault: &Vault, inventory: Option<Inventory>) {
        self.bind_context(vault, inventory.map(Context::Workspace));
    }

    pub fn bind_context(&mut self, vault: &Vault, inventory: Option<Context>) {
        let account = vault.current().map(|account| account.did);
        let inventory = account.as_ref().and(inventory);
        let same = match (&self.inventory, &inventory) {
            (Some(old), Some(new)) => old.same(new),
            (None, None) => true,
            _ => false,
        };
        if self.account == account && same {
            return;
        }
        let launcher = inventory
            .as_ref()
            .map(|inventory| Launcher::new_in_context(vault, inventory.clone()));
        *self = Self {
            account,
            inventory,
            launcher,
        };
    }

    pub fn reset(&mut self, vault: &Vault) -> Result<(), String> {
        if let Some(Ok(launcher)) = &mut self.launcher {
            return launcher.reset(vault);
        }
        if let Some(inventory) = &self.inventory {
            let mut replacement = Launcher::open(inventory.clone(), None)?;
            replacement.app = replacement.app.stage_source(source::bundled()?)?;
            Launcher::clear_override(vault)?;
            self.launcher = Some(Ok(replacement));
        } else {
            Launcher::clear_override(vault)?;
        }
        Ok(())
    }

    pub fn warning(&self) -> Option<&str> {
        match &self.launcher {
            Some(Ok(launcher)) => launcher.warning(),
            Some(Err(error)) => Some(error),
            None => None,
        }
    }

    pub fn get(&self) -> Result<&Launcher, String> {
        self.launcher
            .as_ref()
            .ok_or("launcher is unavailable; reset it")?
            .as_ref()
            .map_err(Clone::clone)
    }

    pub fn get_mut(&mut self) -> Result<&mut Launcher, String> {
        self.launcher
            .as_mut()
            .ok_or("launcher is unavailable; reset it")?
            .as_mut()
            .map_err(|error| error.clone())
    }
}

#[cfg(test)]
mod tests;
