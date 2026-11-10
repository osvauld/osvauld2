use identity::Identity;

use super::*;

// Any ed25519 pair stands in for the device (iroh) key here.
fn device() -> Identity {
    identity::generate().0
}

fn envelope(from: &Identity, seq: u64) -> Envelope {
    Envelope {
        ws: "ws1".into(),
        item: "jetpack".into(),
        channel: "inputs".into(),
        seq,
        from_device: from.signing_public_key(),
        body: vec![1, 2, 3],
    }
}

fn sealed(from: &Identity, seq: u64) -> Sealed {
    seal(&envelope(from, seq), |m| from.sign(m)).unwrap()
}

#[test]
fn an_envelope_opens_as_sealed() {
    let a = device();
    assert_eq!(open(&sealed(&a, 7)).unwrap(), envelope(&a, 7));
}

#[test]
fn a_touched_envelope_is_refused() {
    let a = device();
    let good = sealed(&a, 1);
    // Every field the host routes on, and the body it never reads.
    let edits: [fn(&mut Envelope); 6] = [
        |e| e.ws = "ws2".into(),
        |e| e.item = "chat".into(),
        |e| e.channel = "other".into(),
        |e| e.seq = 2,
        |e| e.from_device = [7; 32],
        |e| e.body[0] = 9,
    ];
    for edit in edits {
        let mut env = open(&good).unwrap();
        edit(&mut env);
        let forged = Sealed {
            head: bincode::serialize(&env).unwrap(),
            sig: good.sig.clone(),
        };
        assert_eq!(open(&forged), Err(CourierError::BadSignature));
    }
}

#[test]
fn another_device_cannot_speak_for_one() {
    let (a, host) = (device(), device());
    // The host signs an envelope that names A.
    let forged = seal(&envelope(&a, 1), |m| host.sign(m)).unwrap();
    assert_eq!(open(&forged), Err(CourierError::BadSignature));
}

#[test]
fn a_body_over_the_ceiling_is_refused_both_ways() {
    let a = device();
    let mut env = envelope(&a, 1);
    env.body = vec![0; manifest::CHANNEL_SIZE_MAX as usize + 1];
    assert_eq!(seal(&env, |m| a.sign(m)), Err(CourierError::TooLarge));
    let head = bincode::serialize(&env).unwrap();
    let sig = a.sign(&[DOMAIN, &head].concat()).to_vec();
    assert_eq!(open(&Sealed { head, sig }), Err(CourierError::TooLarge));
}

#[test]
fn a_replay_or_an_older_seq_is_not_fresh() {
    let (a, b) = (device(), device());
    let mut seen = Seen::default();
    assert!(seen.fresh(&envelope(&a, 1)));
    assert!(!seen.fresh(&envelope(&a, 1)), "replayed");
    assert!(seen.fresh(&envelope(&a, 5)), "a gap is fine: drops happen");
    assert!(!seen.fresh(&envelope(&a, 3)), "older than the last");
    // Independent per sender, item and channel.
    assert!(seen.fresh(&envelope(&b, 1)));
    let mut other = envelope(&a, 1);
    other.channel = "chat".into();
    assert!(seen.fresh(&other));
}

const GAME: &str = r#"
app "jetpack" {
  roles player, watcher
  channel inputs { send player  rate 2/s  size 4 }
  channel chat   { }
}
"#;

fn game() -> manifest::Manifest {
    manifest::Manifest::parse(GAME).unwrap()
}

fn roles(r: &[&str]) -> std::collections::BTreeSet<String> {
    r.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_player_may_send_on_a_declared_channel() {
    let (a, m) = (device(), game());
    let ch = may_send(&envelope(&a, 1), a.signing_public_key(), "ws1", &m, &roles(&["player"]))
        .unwrap();
    assert_eq!(ch.name, "inputs");
    // `owner` satisfies every role.
    assert!(may_send(&envelope(&a, 1), a.signing_public_key(), "ws1", &game(), &roles(&["owner"])).is_ok());
}

#[test]
fn sending_is_refused_off_the_rules() {
    let a = device();
    let me = a.signing_public_key();
    let (m, player) = (game(), roles(&["player"]));
    assert_eq!(
        may_send(&envelope(&a, 1), me, "ws1", &m, &roles(&["watcher"])).unwrap_err(),
        CourierError::NoSend
    );
    let mut env = envelope(&a, 1);
    env.channel = "voice".into();
    assert_eq!(may_send(&env, me, "ws1", &m, &player).unwrap_err(), CourierError::UndeclaredChannel);
    // Not the device the transport proved, or not this session's space.
    assert_eq!(may_send(&envelope(&a, 1), [3; 32], "ws1", &m, &player).unwrap_err(), CourierError::WrongDevice);
    assert_eq!(may_send(&envelope(&a, 1), me, "ws2", &m, &player).unwrap_err(), CourierError::WrongDevice);
    // Over this channel's own `size 4`.
    let mut env = envelope(&a, 1);
    env.body = vec![0; 5];
    assert_eq!(may_send(&env, me, "ws1", &m, &player).unwrap_err(), CourierError::TooLarge);
}

#[test]
fn a_channel_without_send_admits_every_member() {
    let a = device();
    let mut env = envelope(&a, 1);
    env.channel = "chat".into();
    assert!(may_send(&env, a.signing_public_key(), "ws1", &game(), &roles(&["watcher"])).is_ok());
    assert!(may_send(&env, a.signing_public_key(), "ws1", &game(), &roles(&["member"])).is_ok());
}

#[test]
fn the_rate_caps_each_sender_per_second() {
    let (a, b) = (device(), device());
    let mut rate = Rate::default();
    let at = |rate: &mut Rate, who: &Identity, ms: u64| rate.allow(&envelope(who, 0), Some(2), ms);
    assert!(at(&mut rate, &a, 0));
    assert!(at(&mut rate, &a, 10));
    assert!(!at(&mut rate, &a, 20), "a third inside the second");
    assert!(at(&mut rate, &b, 20), "another sender has its own");
    assert!(at(&mut rate, &a, 520), "half a second refills one");
    assert!(!at(&mut rate, &a, 530));
    // Undeclared rate: the platform's default, not unlimited.
    let mut free = Rate::default();
    let sent = (0..1000).filter(|_| free.allow(&envelope(&a, 0), None, 0)).count();
    assert_eq!(sent, DEFAULT_RATE as usize);
}
