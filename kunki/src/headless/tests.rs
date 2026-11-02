//! The binary's pieces without a network: the argument forms, and what the sink answers
//! from a vault that joined a space.

use std::path::PathBuf;

use courier::link::HostAddr;
use courier::session::HostEntry;
use tempfile::TempDir;
use vault::Vault;

use super::*;
use crate::node;
use crate::session::{Joined, save_joined};

fn args(line: &str) -> Result<Args, String> {
    parse_args(line.split_whitespace().map(String::from), None)
}

#[test]
fn no_subcommand_is_the_node_in_the_env_dir_or_dot_kunki() {
    let node = parse_args(std::iter::empty(), Some(PathBuf::from("/env"))).unwrap();
    assert_eq!(
        node,
        Args {
            data: PathBuf::from("/env"),
            command: Command::Node
        }
    );
    assert_eq!(args("").unwrap().data, PathBuf::from(".kunki"));
}

#[test]
fn data_beats_the_env() {
    let a = parse_args(
        ["--data", "/flag"].map(String::from),
        Some(PathBuf::from("/env")),
    )
    .unwrap();
    assert_eq!(a.data, PathBuf::from("/flag"));
}

#[test]
fn host_takes_a_link_and_the_space_to_accept() {
    let a = args("--data d host --join osvauld://s/x --accept ws9").unwrap();
    assert_eq!(
        a,
        Args {
            data: PathBuf::from("d"),
            command: Command::Host {
                link: "osvauld://s/x".into(),
                accept: "ws9".into()
            }
        }
    );
    // `--data` after the subcommand is the same flag.
    assert_eq!(
        args("host --join l --accept w --data d").unwrap().data,
        PathBuf::from("d")
    );
}

#[test]
fn host_without_its_flags_is_an_error() {
    assert!(args("host --join l").unwrap_err().contains("--accept"));
    assert!(args("host --accept w").unwrap_err().contains("--join"));
    assert!(args("host --join").unwrap_err().contains("--join"));
}

#[test]
fn an_unknown_argument_is_an_error() {
    assert!(args("--port 1").unwrap_err().contains("--port"));
    assert!(
        args("--join l").unwrap_err().contains("--join"),
        "host's flags mean nothing outside host"
    );
    assert!(args("host host").unwrap_err().contains("host"));
    assert!(args("--data").unwrap_err().contains("--data"));
}

fn addr(n: u8) -> HostAddr {
    HostAddr {
        device: [n; 32],
        relay: None,
        addrs: Vec::new(),
    }
}

/// A node vault that joined `team` from its owner, told of the owner alone.
fn joined_vault() -> (TempDir, Vault, String, HostEntry) {
    let tmp = TempDir::new().unwrap();
    let (vault, _) = node::open(tmp.path().to_path_buf(), "pw").unwrap();
    let owner = HostEntry {
        did: identity::generate().0.did().to_string(),
        addr: addr(1),
    };
    let ws = "team".to_string();
    let joined = Joined {
        host_did: owner.did.clone(),
        host: owner.addr.clone(),
        token: None,
        hosts: vec![owner.clone()],
    };
    save_joined(&vault, &ws, &joined).unwrap();
    (tmp, vault, ws, owner)
}

#[test]
fn the_admin_for_a_joined_space_is_the_owners_and_none_elsewhere() {
    let (_tmp, vault, ws, owner) = joined_vault();
    let sink = HeadlessSink::new(vault);
    assert_eq!(sink.admin_for(&ws).unwrap().root().unwrap(), owner.did);
    assert!(sink.admin_for("not_joined").is_none());
    assert!(
        sink.fetch_admin().is_none(),
        "links are the owner's to answer"
    );
}

#[test]
fn hosts_are_the_owners_last_word_seeded_from_the_join() {
    let (_tmp, vault, ws, owner) = joined_vault();
    let sink = HeadlessSink::new(vault);
    assert_eq!(sink.hosts(&ws), vec![owner.clone()]);
    let announced = vec![
        owner,
        HostEntry {
            did: "did:key:z6MkanotherHost".into(),
            addr: addr(2),
        },
    ];
    sink.set_hosts(&ws, announced.clone());
    assert_eq!(sink.hosts(&ws), announced);
    assert!(sink.hosts("not_joined").is_empty());
}
