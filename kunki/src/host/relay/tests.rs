use std::collections::BTreeSet;

use courier::net::{Envelope, Sealed, seal};
use identity::Identity;
use manifest::Manifest;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::*;

const GAME: &str = "app \"jet\" {\n  roles player\n  channel inputs { send player  rate 2/s }\n}\n";

fn peer(sid: u64, device: [u8; 32]) -> (Peer, UnboundedReceiver<NetFrame>) {
    let (tx, rx) = unbounded_channel();
    let player: BTreeSet<String> = ["player".to_string()].into();
    let peer = Peer {
        sid,
        device,
        attestation: format!("att-{sid}"),
        tx,
        rules: std::sync::Arc::new((Manifest::parse(GAME).unwrap(), player)),
    };
    (peer, rx)
}

fn drain(rx: &mut UnboundedReceiver<NetFrame>) -> Vec<NetFrame> {
    let mut out = Vec::new();
    while let Ok(f) = rx.try_recv() {
        out.push(f);
    }
    out
}

fn sealed_by(who: &Identity, item: &str, channel: &str, seq: u64) -> Sealed {
    let env = Envelope {
        ws: "ws".into(),
        item: item.into(),
        channel: channel.into(),
        seq,
        from_device: who.signing_public_key(),
        body: vec![1],
    };
    seal(&env, |m| who.sign(m)).unwrap()
}

#[test]
fn a_message_reaches_devices_here_on_its_item_only_not_its_sender() {
    let nets = Nets::default();
    let (a, mut a_rx) = peer(1, [1; 32]);
    let (b, mut b_rx) = peer(2, [2; 32]);
    let (c, mut c_rx) = peer(3, [3; 32]);
    let (d, mut d_rx) = peer(4, [4; 32]);
    let (e, mut e_rx) = peer(5, [5; 32]);
    nets.open("ws", "jet", a);
    nets.open("ws", "jet", b);
    nets.open("ws", "jet", c);
    nets.open("ws", "chat", d);
    nets.open("ws2", "jet", e);
    for rx in [&mut a_rx, &mut b_rx, &mut c_rx, &mut d_rx, &mut e_rx] {
        drain(rx);
    }
    let who = identity::generate().0;
    let msg = sealed_by(&who, "jet", "inputs", 1);
    nets.relay("ws", "jet", 1, &msg);
    assert!(drain(&mut a_rx).is_empty(), "the sender");
    assert_eq!(drain(&mut b_rx), [NetFrame::Msg(msg.clone())]);
    assert_eq!(drain(&mut c_rx), [NetFrame::Msg(msg)]);
    assert!(drain(&mut d_rx).is_empty(), "another item");
    assert!(drain(&mut e_rx).is_empty(), "another space");
}

#[test]
fn presence_is_told_both_ways_and_on_leaving() {
    let nets = Nets::default();
    let (a, mut a_rx) = peer(1, [1; 32]);
    let (b, mut b_rx) = peer(2, [2; 32]);
    let (b2, _) = peer(2, [2; 32]);
    nets.open("ws", "jet", a);
    assert!(drain(&mut a_rx).is_empty(), "alone");
    nets.open("ws", "jet", b);
    let here = |sid: u64| NetFrame::Here {
        item_id: "jet".into(),
        attestation: format!("att-{sid}"),
    };
    assert_eq!(drain(&mut a_rx), [here(2)]);
    assert_eq!(drain(&mut b_rx), [here(1)]);
    nets.open("ws", "jet", b2);
    assert!(drain(&mut a_rx).is_empty(), "opening twice is once");
    assert!(nets.is_open("ws", "jet", 2));

    nets.close("ws", "jet", 2);
    let gone = |d: u8, item: &str| NetFrame::Gone {
        item_id: item.into(),
        device: [d; 32],
    };
    assert_eq!(drain(&mut a_rx), [gone(2, "jet")]);
    assert!(!nets.is_open("ws", "jet", 2));

    // A dropped stream leaves every item it was here on.
    let (b, mut b_rx) = peer(2, [2; 32]);
    let (c, mut c_rx) = peer(3, [3; 32]);
    nets.open("ws", "jet", b);
    nets.open("ws", "chat", c);
    let (a_again, _) = peer(1, [1; 32]);
    nets.open("ws", "chat", a_again);
    drain(&mut a_rx);
    drain(&mut b_rx);
    drain(&mut c_rx);
    nets.close_all(1);
    assert_eq!(drain(&mut b_rx), [gone(1, "jet")]);
    assert_eq!(drain(&mut c_rx), [gone(1, "chat")]);
}

#[test]
fn a_message_is_judged_before_it_is_relayed() {
    let nets = Nets::default();
    let a = identity::generate().0;
    let me = a.signing_public_key();
    let (p, _rx) = peer(1, me);
    nets.open("ws", "jet", p);
    let judge = |s: &Sealed, sid: u64, now: u64| nets.judge(s, sid, me, "ws", now);

    assert!(judge(&sealed_by(&a, "jet", "inputs", 1), 1, 0).is_ok());
    let err = judge(&sealed_by(&a, "jet", "inputs", 1), 1, 1).unwrap_err();
    assert!(err.contains("replay"), "{err}");
    let err = judge(&sealed_by(&a, "chat", "inputs", 2), 1, 2).unwrap_err();
    assert!(err.contains("not open"), "{err}");
    let err = judge(&sealed_by(&a, "jet", "voice", 3), 1, 3).unwrap_err();
    assert!(err.contains("channel"), "{err}");
    let mut touched = sealed_by(&a, "jet", "inputs", 4);
    touched.sig[0] ^= 1;
    assert!(judge(&touched, 1, 4).unwrap_err().contains("signature"));
    // `rate 2/s`: the second passes, the third in the same second is over.
    assert!(judge(&sealed_by(&a, "jet", "inputs", 5), 1, 5).is_ok());
    let err = judge(&sealed_by(&a, "jet", "inputs", 6), 1, 6).unwrap_err();
    assert!(err.contains("over rate"), "{err}");
}
