use courier::sync::SyncLayer;

use super::*;

fn push(item_id: &str) -> Push {
    Push {
        ws_id: "ws1".to_string(),
        item_id: item_id.to_string(),
        layer: SyncLayer::Doc("board".to_string()),
        snapshot: b"snapshot-bytes".to_vec(),
    }
}

#[test]
fn a_push_is_recorded_against_the_subscriber_it_was_sent_to() {
    let pusher = MockPusher::new();
    pusher.push("did:bob", &push("item1"));

    let received = pusher.received_by("did:bob");
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].item_id, "item1");
}

#[test]
fn a_push_to_one_subscriber_is_not_seen_by_another() {
    let pusher = MockPusher::new();
    pusher.push("did:bob", &push("item1"));

    assert!(pusher.received_by("did:alice").is_empty());
}

#[test]
fn two_pushes_to_the_same_subscriber_both_land() {
    let pusher = MockPusher::new();
    pusher.push("did:bob", &push("item1"));
    pusher.push("did:bob", &push("item2"));

    let received = pusher.received_by("did:bob");
    assert_eq!(received.len(), 2);
}

#[test]
fn a_registered_desktop_receives_pushes_on_its_channel() {
    let registry = LiveRegistry::new();
    let rx = registry.register("did:bob");

    registry.push("did:bob", &push("item1"));

    let received = rx.try_recv().unwrap();
    assert_eq!(received.item_id, "item1");
}

#[test]
fn pushing_to_an_unregistered_desktop_is_a_silent_no_op() {
    let registry = LiveRegistry::new();
    // No panic, no error — the same "offline is not a caller-visible error" contract
    // `Pusher::push`'s own doc already promises.
    registry.push("did:nobody", &push("item1"));
}

#[test]
fn re_registering_replaces_the_earlier_channel() {
    let registry = LiveRegistry::new();
    let first = registry.register("did:bob");
    let second = registry.register("did:bob");

    registry.push("did:bob", &push("item1"));

    assert!(first.try_recv().is_err(), "the old channel gets nothing");
    assert_eq!(second.try_recv().unwrap().item_id, "item1");
}

#[test]
fn unregistering_stops_delivery() {
    let registry = LiveRegistry::new();
    let rx = registry.register("did:bob");
    registry.unregister("did:bob");

    registry.push("did:bob", &push("item1"));

    assert!(rx.try_recv().is_err());
}

#[test]
fn a_full_channel_drops_the_push_instead_of_blocking() {
    let registry = LiveRegistry::new();
    let rx = registry.register("did:bob");

    // One more than the channel can hold — `push` must return instead of blocking, and the
    // overflow one is simply gone.
    for i in 0..(CHANNEL_CAPACITY + 1) {
        registry.push("did:bob", &push(&i.to_string()));
    }

    assert_eq!(rx.try_iter().count(), CHANNEL_CAPACITY);
}
