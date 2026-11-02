use courier::link::HostAddr;
use courier::session::HostEntry;

use super::Joined;

fn addr(byte: u8) -> HostAddr {
    HostAddr {
        device: [byte; 32],
        relay: None,
        addrs: Vec::new(),
    }
}

#[test]
fn a_joined_record_with_no_hosts_list_dials_its_owner() {
    let joined = Joined {
        host_did: "did:key:owner".into(),
        host: addr(1),
        token: None,
        hosts: Vec::new(),
    };
    let list = joined.host_list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].did, "did:key:owner");
    assert_eq!(list[0].addr, addr(1));
}

#[test]
fn the_hosts_list_is_the_failover_order_as_told() {
    let hosts = vec![
        HostEntry { did: "did:key:owner".into(), addr: addr(1) },
        HostEntry { did: "did:key:kunki".into(), addr: addr(2) },
    ];
    let joined = Joined {
        host_did: "did:key:owner".into(),
        host: addr(1),
        token: None,
        hosts: hosts.clone(),
    };
    assert_eq!(joined.host_list(), hosts);
}
