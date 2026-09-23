//! Opening the node's account. The node is one `vault` account like any other, unlocked
//! non-interactively at boot instead of through a login screen.

use std::path::PathBuf;

use identity::Mnemonic;
use vault::Vault;

use crate::NodeError;

const LABEL: &str = "kunki";

/// First boot creates the account and hands back its mnemonic, once; every later boot
/// unlocks the one that is there. A second account in the same directory is ambiguous —
/// which one is the node? — so it stops rather than guessing.
pub fn open(dir: PathBuf, passphrase: &str) -> Result<(Vault, Option<Mnemonic>), NodeError> {
    let mut vault = Vault::open(dir)?;
    let accounts = vault.accounts()?;
    match accounts.as_slice() {
        [] => {
            let (_did, mnemonic) = vault.signup(LABEL, passphrase)?;
            Ok((vault, Some(mnemonic)))
        }
        [only] => {
            vault.login(&only.did, passphrase)?;
            Ok((vault, None))
        }
        many => Err(NodeError::ManyAccounts(many.len())),
    }
}

/// The node's DID — the root of every token chain it will later verify.
pub fn did(vault: &Vault) -> Result<String, NodeError> {
    vault.current().map(|a| a.did).ok_or(NodeError::Locked)
}

#[cfg(test)]
mod tests;
