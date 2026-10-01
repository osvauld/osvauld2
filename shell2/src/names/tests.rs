use super::*;

#[test]
fn bare_lowercase_names_are_names() {
    assert!(is_name("acme"));
    assert!(is_name("my-shop-2"));
}

#[test]
fn tickets_and_malformed_text_are_not_names() {
    assert!(!is_name("osvi1.abc"));
    assert!(!is_name("Acme"));
    assert!(!is_name("-acme"));
    assert!(!is_name("acme-"));
    assert!(!is_name(""));
    assert!(!is_name(&"a".repeat(64)));
}

/// Hits devnet: `cargo test -p shell2 names -- --ignored`. Needs the registry still holding
/// `acme` — devnet wipes break it.
#[test]
#[ignore]
fn a_registered_name_resolves_on_devnet() {
    assert!(resolve("acme").unwrap().starts_with("osvi1."));
    assert!(
        resolve("nobody-has-this-name")
            .unwrap_err()
            .starts_with("no name")
    );
}
