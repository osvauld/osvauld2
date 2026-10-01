#[test_only]
module xnet::names_tests {
    use std::signer;
    use std::string::utf8;
    use xnet::names;

    #[test(publisher = @xnet, alice = @0xA)]
    fun a_registered_name_resolves_to_its_owner_and_ticket(publisher: &signer, alice: &signer) {
        names::init_for_test(publisher);
        names::register(alice, utf8(b"acme"), utf8(b"osvi1.abc"));
        let (owner, ticket) = names::resolve(utf8(b"acme"));
        assert!(owner == signer::address_of(alice), 0);
        assert!(ticket == utf8(b"osvi1.abc"), 1);
    }

    #[test(publisher = @xnet, alice = @0xA, bob = @0xB)]
    #[expected_failure(abort_code = 1, location = xnet::names)]
    fun a_taken_name_cannot_be_registered_again(publisher: &signer, alice: &signer, bob: &signer) {
        names::init_for_test(publisher);
        names::register(alice, utf8(b"acme"), utf8(b"osvi1.abc"));
        names::register(bob, utf8(b"acme"), utf8(b"osvi1.evil"));
    }

    #[test(publisher = @xnet, alice = @0xA)]
    fun the_owner_rotates_the_ticket(publisher: &signer, alice: &signer) {
        names::init_for_test(publisher);
        names::register(alice, utf8(b"acme"), utf8(b"osvi1.old"));
        names::update(alice, utf8(b"acme"), utf8(b"osvi1.new"));
        let (_, ticket) = names::resolve(utf8(b"acme"));
        assert!(ticket == utf8(b"osvi1.new"), 0);
    }

    #[test(publisher = @xnet, alice = @0xA, bob = @0xB)]
    #[expected_failure(abort_code = 3, location = xnet::names)]
    fun a_stranger_cannot_repoint_a_name(publisher: &signer, alice: &signer, bob: &signer) {
        names::init_for_test(publisher);
        names::register(alice, utf8(b"acme"), utf8(b"osvi1.abc"));
        names::update(bob, utf8(b"acme"), utf8(b"osvi1.evil"));
    }

    #[test(publisher = @xnet, alice = @0xA, bob = @0xB)]
    fun after_transfer_only_the_new_owner_can_update(publisher: &signer, alice: &signer, bob: &signer) {
        names::init_for_test(publisher);
        names::register(alice, utf8(b"acme"), utf8(b"osvi1.abc"));
        names::transfer(alice, utf8(b"acme"), signer::address_of(bob));
        names::update(bob, utf8(b"acme"), utf8(b"osvi1.bob"));
        let (owner, _) = names::resolve(utf8(b"acme"));
        assert!(owner == signer::address_of(bob), 0);
    }

    #[test(publisher = @xnet, alice = @0xA)]
    #[expected_failure(abort_code = 4, location = xnet::names)]
    fun a_name_with_a_dot_is_refused(publisher: &signer, alice: &signer) {
        names::init_for_test(publisher);
        names::register(alice, utf8(b"osv1.x"), utf8(b"osvi1.abc"));
    }

    #[test(publisher = @xnet, alice = @0xA)]
    #[expected_failure(abort_code = 4, location = xnet::names)]
    fun an_uppercase_name_is_refused(publisher: &signer, alice: &signer) {
        names::init_for_test(publisher);
        names::register(alice, utf8(b"Acme"), utf8(b"osvi1.abc"));
    }

    #[test(publisher = @xnet)]
    #[expected_failure(abort_code = 2, location = xnet::names)]
    fun an_unknown_name_does_not_resolve(publisher: &signer) {
        names::init_for_test(publisher);
        names::resolve(utf8(b"nobody"));
    }
}
