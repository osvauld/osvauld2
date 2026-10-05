/// Xnet names: a bare name → the public invite ticket a node published under it
/// (`docs/design/xnet-names.md`). The chain holds where and how to join, never authority.
/// Writes are owner-signed; reads (`resolve`) are view calls and free.
module xnet::names {
    use std::signer;
    use std::string::{Self, String};
    use aptos_std::table::{Self, Table};

    const E_TAKEN: u64 = 1;
    const E_NOT_FOUND: u64 = 2;
    const E_NOT_OWNER: u64 = 3;
    const E_BAD_NAME: u64 = 4;
    const E_TICKET_TOO_LONG: u64 = 5;

    const MAX_NAME: u64 = 63;
    /// Bounds the storage deposit a sponsored write can cost; a ticket is a few hundred bytes.
    const MAX_TICKET: u64 = 2048;

    struct Record has store, drop {
        owner: address,
        ticket: String,
    }

    struct Registry has key {
        names: Table<String, Record>,
    }

    fun init_module(publisher: &signer) {
        move_to(publisher, Registry { names: table::new() });
    }

    public entry fun register(owner: &signer, name: String, ticket: String) acquires Registry {
        assert!(valid_name(&name), E_BAD_NAME);
        assert!(string::length(&ticket) <= MAX_TICKET, E_TICKET_TOO_LONG);
        let names = &mut borrow_global_mut<Registry>(@xnet).names;
        assert!(!table::contains(names, name), E_TAKEN);
        table::add(names, name, Record { owner: signer::address_of(owner), ticket });
    }

    /// How a revoked public invite is rotated.
    public entry fun update(owner: &signer, name: String, ticket: String) acquires Registry {
        assert!(string::length(&ticket) <= MAX_TICKET, E_TICKET_TOO_LONG);
        let record = owned(owner, name);
        record.ticket = ticket;
    }

    public entry fun transfer(owner: &signer, name: String, new_owner: address) acquires Registry {
        owned(owner, name).owner = new_owner;
    }

    #[view]
    public fun resolve(name: String): (address, String) acquires Registry {
        let names = &borrow_global<Registry>(@xnet).names;
        assert!(table::contains(names, name), E_NOT_FOUND);
        let record = table::borrow(names, name);
        (record.owner, record.ticket)
    }

    /// Inline: the verifier won't let a plain function return a reference into global storage.
    inline fun owned(owner: &signer, name: String): &mut Record {
        let names = &mut borrow_global_mut<Registry>(@xnet).names;
        assert!(table::contains(names, name), E_NOT_FOUND);
        let record = table::borrow_mut(names, name);
        assert!(record.owner == signer::address_of(owner), E_NOT_OWNER);
        record
    }

    /// `[a-z0-9-]`, no leading/trailing `-`. No `.`, so a name never collides with a ticket's
    /// `osv1.`/`osvi1.` prefix in the bar; lowercase only, so `Acme` and `acme` can't both exist.
    fun valid_name(name: &String): bool {
        let bytes = string::bytes(name);
        let n = std::vector::length(bytes);
        if (n == 0 || n > MAX_NAME) return false;
        if (*std::vector::borrow(bytes, 0) == 0x2d || *std::vector::borrow(bytes, n - 1) == 0x2d) {
            return false
        };
        let i = 0;
        while (i < n) {
            let c = *std::vector::borrow(bytes, i);
            let ok = (c >= 0x61 && c <= 0x7a) || (c >= 0x30 && c <= 0x39) || c == 0x2d;
            if (!ok) return false;
            i = i + 1;
        };
        true
    }

    #[test_only]
    public fun init_for_test(publisher: &signer) {
        init_module(publisher);
    }
}
