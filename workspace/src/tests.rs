use super::*;

#[test]
fn parses_stable_workspace_addresses() {
    let address = ResourceAddress::parse("ws/8ab3/resource/01JORDERS/shard/2026-09").unwrap();
    assert_eq!(address.workspace_id(), "8ab3");
    assert_eq!(address.as_str(), "ws/8ab3/resource/01JORDERS/shard/2026-09");
    assert!(ResourceAddress::parse("ws/8ab3/orders/did:key:zAlice").is_ok());
    assert!(ResourceAddress::parse(&format!("ws/8ab3/{}", "a".repeat(128))).is_ok());
}

#[test]
fn rejects_non_canonical_or_ambiguous_addresses() {
    for address in [
        "",
        "ws",
        "ws/8ab3",
        "/ws/8ab3/orders",
        "ws//orders",
        "ws/8ab3/orders/",
        "ws/8ab3/../orders",
        "ws/8ab3/./orders",
        "ws/8ab3/orders/*",
        "ws/8ab3/order name",
        "ws/8ab3/order%2Fadmin",
        "ws/8ab3/order\\admin",
        "ws/8ab3/café",
    ] {
        assert!(
            ResourceAddress::parse(address).is_err(),
            "accepted {address:?}"
        );
    }
    assert!(ResourceAddress::parse(&format!("ws/8ab3/{}", "a".repeat(129))).is_err());
    assert!(ResourceAddress::parse(&"a".repeat(2048)).is_err());
}

#[test]
fn handles_are_stable_machine_names() {
    for valid in ["orders", "order_history", "shop-v2"] {
        assert_eq!(ResourceHandle::parse(valid).unwrap().as_str(), valid);
    }
    for invalid in ["", "Orders", "2orders", "order/name", "..", "order name"] {
        assert!(
            ResourceHandle::parse(invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
    assert!(ResourceHandle::parse(&format!("a{}", "b".repeat(63))).is_ok());
    assert!(ResourceHandle::parse(&format!("a{}", "b".repeat(64))).is_err());
}

#[test]
fn exact_scope_covers_only_one_address() {
    let scope = ResourceScope::parse("ws/shop/resource/orders").unwrap();
    assert!(scope.covers(&ResourceAddress::parse("ws/shop/resource/orders").unwrap()));
    assert!(!scope.covers(&ResourceAddress::parse("ws/shop/resource/orders/shard-1").unwrap()));
}

#[test]
fn subtree_scope_covers_descendants_on_segment_boundaries() {
    let scope = ResourceScope::parse("ws/shop/resource/orders/*").unwrap();
    for covered in [
        "ws/shop/resource/orders/shard-1",
        "ws/shop/resource/orders/did:key:zAlice/2026-09",
    ] {
        assert!(scope.covers(&ResourceAddress::parse(covered).unwrap()));
    }
    for excluded in [
        "ws/shop/resource/orders",
        "ws/shop/resource/orders-private",
        "ws/other/resource/orders/shard-1",
    ] {
        assert!(!scope.covers(&ResourceAddress::parse(excluded).unwrap()));
    }
}

#[test]
fn rejects_non_terminal_or_unbounded_wildcards() {
    for scope in [
        "*",
        "ws/shop/*/orders",
        "ws/shop/orders/**",
        "ws/shop/orders/*/day",
    ] {
        assert!(ResourceScope::parse(scope).is_err(), "accepted {scope:?}");
    }
}

#[test]
fn binding_resolves_a_handle_to_a_stable_address() {
    let binding = ResourceBinding::new(
        ResourceHandle::parse("orders").unwrap(),
        ResourceAddress::parse("ws/8ab3/resource/01JORDERS").unwrap(),
    );
    assert_eq!(binding.handle().as_str(), "orders");
    assert_eq!(binding.target().as_str(), "ws/8ab3/resource/01JORDERS");
}
