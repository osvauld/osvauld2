//! The one leg unit tests cannot reach: the binary itself. Everything below `main` is covered
//! in `src/`, but "copy the string kunki printed into a desktop" only exists out here, where
//! `CARGO_BIN_EXE_kunki` can run the real thing and read its real stdout.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
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

/// Kunki now serves its bridge forever instead of exiting after the ticket, so a test that
/// wants only the boot ticket spawns it, reads that one line, and kills it rather than
/// waiting for an exit that no longer comes. `socket` must be unique per test (and per call,
/// for a test that boots the same node twice) — a fixed default path would collide across
/// tests running as separate processes.
fn spawn_kunki(dir: &Path, socket: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_kunki"))
        .env("OSVAULD_KUNKI_DIR", dir)
        .env("OSVAULD_KUNKI_PASSPHRASE", "pw")
        .env("OSVAULD_KUNKI_SOCKET", socket)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

/// The ticket is always the last thing kunki prints before it blocks in the bridge loop, so
/// reading one stdout line can't block on the process now running forever.
fn read_ticket_line(child: &mut Child) -> String {
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    line
}

fn stop(mut child: Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn the_string_kunki_prints_is_one_a_desktop_can_claim_with() {
    let dir = TempDir::new().unwrap();
    let socket = dir.path().join("bridge.sock");

    let mut child = spawn_kunki(dir.path(), &socket);
    let printed = read_ticket_line(&mut child);

    // Exactly one line, so piping stdout somewhere gives a ticket and nothing else — the DID
    // and the recovery phrase go to stderr precisely so this stays true.
    assert_eq!(printed.lines().count(), 1, "stdout was: {printed:?}");

    // A fresh account writes exactly two stderr lines before the ticket — both already sat
    // in the pipe by the time the ticket line was readable, so this cannot block.
    let mut stderr = BufReader::new(child.stderr.take().unwrap());
    let mut stderr_text = String::new();
    for _ in 0..2 {
        let mut line = String::new();
        stderr.read_line(&mut line).unwrap();
        stderr_text.push_str(&line);
    }
    assert!(
        stderr_text.contains("recovery phrase"),
        "shown once, on creation"
    );

    stop(child);

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
            &desktop,
            now_secs()
        )
        .is_ok()
    );
}

#[test]
fn every_start_prints_a_fresh_ticket_for_the_same_node() {
    let dir = TempDir::new().unwrap();
    let socket = dir.path().join("bridge.sock");
    // Reused across both boots: `bind_uds` replaces a dead listener at the same path once
    // `stop` has actually killed and reaped the process ahead of it.
    let run = || {
        let mut child = spawn_kunki(dir.path(), &socket);
        let printed = read_ticket_line(&mut child);
        stop(child);
        printed
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
