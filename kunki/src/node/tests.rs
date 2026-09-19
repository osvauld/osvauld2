use tempfile::TempDir;

use super::*;

fn dir() -> TempDir {
    TempDir::new().unwrap()
}

#[test]
fn first_boot_creates_the_account_and_later_boots_unlock_it() {
    let tmp = dir();
    let (vault, mnemonic) = open(tmp.path().to_path_buf(), "pw").unwrap();
    let created = did(&vault).unwrap();
    assert!(
        mnemonic.is_some(),
        "the mnemonic is shown once, on creation"
    );
    drop(vault);

    let (vault, mnemonic) = open(tmp.path().to_path_buf(), "pw").unwrap();
    assert_eq!(did(&vault).unwrap(), created, "the node keeps its DID");
    assert!(mnemonic.is_none(), "and is not created a second time");
}

#[test]
fn the_wrong_passphrase_does_not_unlock_the_node() {
    let tmp = dir();
    let (vault, _) = open(tmp.path().to_path_buf(), "pw").unwrap();
    drop(vault);

    assert!(matches!(
        open(tmp.path().to_path_buf(), "not-pw"),
        Err(NodeError::Vault(vault::VaultError::WrongPassphrase))
    ));
}

#[test]
fn a_second_account_in_the_node_directory_stops_the_boot() {
    let tmp = dir();
    let (vault, _) = open(tmp.path().to_path_buf(), "pw").unwrap();
    drop(vault);
    // Something else put an account here; signing as the wrong one is worse than refusing.
    let mut stray = vault::Vault::open(tmp.path().to_path_buf()).unwrap();
    stray.signup("stray", "pw").unwrap();
    drop(stray);

    assert!(matches!(
        open(tmp.path().to_path_buf(), "pw"),
        Err(NodeError::ManyAccounts(2))
    ));
}

#[test]
fn the_ticket_is_signed_by_the_node_without_the_binary_holding_the_key() {
    let tmp = dir();
    let (vault, _) = open(tmp.path().to_path_buf(), "pw").unwrap();
    let node_did = did(&vault).unwrap();

    let ticket = vault
        .with_signer(|signer| courier::issue_connection_ticket(signer, 10, "kunki"))
        .unwrap()
        .unwrap();

    assert_eq!(ticket.node_did, node_did);
    // courier re-checks the signature over the claim, so this proves the vault signed it.
    assert!(courier::desktop_start_claim(ticket, &identity::generate().0, 11).is_ok());
}
