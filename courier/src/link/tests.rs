use super::*;

fn host(addrs: Vec<SocketAddr>, relay: Option<&str>) -> HostAddr {
    HostAddr {
        device: [9u8; 32],
        relay: relay.map(str::to_string),
        addrs,
    }
}

#[test]
fn a_relay_link_round_trips_and_is_short() {
    let link = Link {
        host: host(vec![], Some(KNOWN_RELAYS[2])),
        code: new_code(),
    };
    let text = link.to_text().unwrap();
    assert!(text.starts_with(PREFIX));
    assert!(text.len() <= 80, "{} chars: {text}", text.len());
    assert_eq!(Link::from_text(&text).unwrap(), link);
    assert_eq!(Link::from_text(&format!("  {text}\n")).unwrap(), link);
}

#[test]
fn a_custom_relay_is_carried_whole() {
    let link = Link {
        host: host(vec![], Some("https://relay.example.org/")),
        code: new_code(),
    };
    assert_eq!(Link::from_text(&link.to_text().unwrap()).unwrap(), link);
}

#[test]
fn a_local_link_carries_direct_addresses() {
    let link = Link {
        host: host(
            vec!["127.0.0.1:4433".parse().unwrap(), "[::1]:4433".parse().unwrap()],
            None,
        ),
        code: new_code(),
    };
    let text = link.to_text().unwrap();
    assert!(text.len() <= 120, "{} chars: {text}", text.len());
    let back = Link::from_text(&text).unwrap();
    assert_eq!(back.host.addrs.len(), 2);
    assert_eq!(back, link);
}

#[test]
fn not_a_link_is_refused_by_name() {
    assert!(!Link::is_link("osvi1.abc"));
    assert_eq!(Link::from_text("osvi1.abc"), Err(CourierError::Decode));
    assert_eq!(Link::from_text("osvauld://s/!!!"), Err(CourierError::Decode));
    assert_eq!(Link::from_text("osvauld://s/"), Err(CourierError::Decode));
    // Trailing bytes are not ignored: a link is exactly its fields.
    let link = Link { host: host(vec![], None), code: new_code() };
    let text = link.to_text().unwrap();
    assert_eq!(Link::from_text(&format!("{text}1")), Err(CourierError::Decode));
}

#[test]
fn codes_differ() {
    assert_ne!(new_code(), new_code());
}
