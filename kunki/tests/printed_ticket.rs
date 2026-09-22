//! The one leg unit tests cannot reach: the binary itself. Everything below `main` is covered
//! in `src/`, but "copy the string kunki printed into a desktop" only exists out here, where
//! `CARGO_BIN_EXE_kunki` can run the real thing and read its real stdout.

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use courier::ConnectionTicket;
use kunki::admin::Admin;
use kunki::node;
use tempfile::TempDir;

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[test]
fn the_string_kunki_prints_is_one_a_desktop_can_claim_with() {
    let dir = TempDir::new().unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_kunki"))
            .env("OSVAULD_KUNKI_DIR", dir.path())
            .env("OSVAULD_KUNKI_PASSPHRASE", "pw")
            .output()
            .unwrap()
    };

    let first = run();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let printed = String::from_utf8(first.stdout).unwrap();

    // Exactly one line, so piping stdout somewhere gives a ticket and nothing else — the DID
    // and the recovery phrase go to stderr precisely so this stays true.
    assert_eq!(printed.lines().count(), 1, "stdout was: {printed:?}");
    let stderr = String::from_utf8(first.stderr).unwrap();
    assert!(
        stderr.contains("recovery phrase"),
        "shown once, on creation"
    );

    let ticket = ConnectionTicket::from_text(&printed).expect("what it printed is a ticket");

    // A desktop claims with it, against the very account the binary created.
    let desktop = identity::generate().0;
    let hello = courier::desktop_start_claim(ticket, &desktop, now_secs()).unwrap();
    let (vault, mnemonic) = node::open(dir.path().to_path_buf(), "pw").unwrap();
    assert!(mnemonic.is_none(), "the binary already created the account");
    let welcome = Admin::new(vault).accept_claim(hello, now_secs()).unwrap();
    assert!(
        courier::desktop_finish_claim(
            &ConnectionTicket::from_text(&printed).unwrap(),
            welcome,
            &desktop
        )
        .is_ok()
    );
}

#[test]
fn every_start_prints_a_fresh_ticket_for_the_same_node() {
    let dir = TempDir::new().unwrap();
    let run = || {
        let out = Command::new(env!("CARGO_BIN_EXE_kunki"))
            .env("OSVAULD_KUNKI_DIR", dir.path())
            .env("OSVAULD_KUNKI_PASSPHRASE", "pw")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    };

    let (first, second) = (run(), run());
    assert_ne!(first, second, "each ticket carries its own nonce");

    let (first, second) = (
        ConnectionTicket::from_text(&first).unwrap(),
        ConnectionTicket::from_text(&second).unwrap(),
    );
    assert_eq!(first.node_did, second.node_did, "one node, one identity");
    assert_ne!(first.claim_token, second.claim_token);
}

#[test]
fn without_a_passphrase_it_refuses_rather_than_creating_an_unlocked_node() {
    let dir = TempDir::new().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_kunki"))
        .env("OSVAULD_KUNKI_DIR", dir.path())
        .env_remove("OSVAULD_KUNKI_PASSPHRASE")
        .output()
        .unwrap();

    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("OSVAULD_KUNKI_PASSPHRASE"));
}
