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
