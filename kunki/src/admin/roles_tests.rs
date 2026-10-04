//! `role.assign`, cascade revocation, app-scoped invites and fan-out re-checks — step 3 of
//! `docs/design/group-chat-sync.md`.

use courier::CourierError;
use courier::invite::{InviteRequest, desktop_start_invite_claim};
use courier::role::RoleRequest;
use courier::subscribe::desktop_start_subscribe;
use courier::sync::{SyncLayer, desktop_start_sync, item_scope};
use courier::token::{Scope, Token};
use identity::Identity;
use loro::{LoroDoc, LoroText};
use tempfile::TempDir;
use vault::Vault;

use super::*;
use crate::node;
use crate::push::MockPusher;

pub(super) const NOW: u64 = 10;

const CHAT: &str = r#"app "chat" {
  roles admin, moderator, member
  role admin { grant moderator, member }
}"#;

pub(super) struct Fixture {
    _tmp: TempDir,
    pub(super) vault: Vault,
    pub(super) admin: Admin,
    pub(super) ws: String,
    pub(super) item: String,
    pub(super) alice: Identity,
    pub(super) alice_token: Token,
}

pub(super) fn minted(byte: char) -> String {
    std::iter::repeat_n(byte, 32).collect()
}

fn src_with_manifest(manifest: &str) -> Vec<u8> {
    let doc = LoroDoc::new();
    let files = doc.get_map("files");
    for (path, text) in [("main.lua", "return {}"), ("manifest.osv", manifest)] {
        let t = files.insert_container(path, LoroText::new()).unwrap();
        t.insert(0, text).unwrap();
    }
    doc.export(loro::ExportMode::Snapshot).unwrap()
}

fn fixture() -> Fixture {
    fixture_with(CHAT)
}

/// Alice claims a fresh node (owner at node scope) and an app with `manifest` is on it.
pub(super) fn fixture_with(manifest: &str) -> Fixture {
    let tmp = TempDir::new().unwrap();
    let (vault, _) = node::open(tmp.path().to_path_buf(), "pw").unwrap();
    let admin = Admin::new(vault.clone());
    let alice = identity::generate().0;
    let ticket = vault
        .with_signer(|node| courier::issue_connection_ticket(node, NOW, "kunki"))
        .unwrap()
        .unwrap();
    let hello = courier::desktop_start_claim(ticket, &alice, NOW).unwrap();
    let alice_token = admin.accept_claim(hello, NOW).unwrap().token;
    let (ws, item) = (minted('a'), minted('b'));
    vault.put_src(&ws, &item, &src_with_manifest(manifest)).unwrap();
    Fixture {
        _tmp: tmp,
        vault,
        admin,
        ws,
        item,
        alice,
        alice_token,
    }
}

impl Fixture {
    pub(super) fn app(&self) -> Scope {
        item_scope(&self.ws, &self.item)
    }

    /// A platform member at node scope, invited by alice.
    pub(super) fn member(&self) -> (Identity, Token) {
        let who = identity::generate().0;
        let request = InviteRequest {
            desktop_did: self.alice.did().to_string(),
            token: self.alice_token.clone(),
            role: "member".to_string(),
            scope: Scope::Node,
            public: false,
        };
        let ticket = self.admin.issue_invite(&request, "kunki", NOW).unwrap();
        let hello = desktop_start_invite_claim(ticket, &who, NOW).unwrap();
        let token = self.admin.accept_invite(hello, NOW).unwrap().token;
        (who, token)
    }

    fn request(&self, caller: &Identity, token: &Token, to: &Identity, role: &str) -> RoleRequest {
        RoleRequest {
            desktop_did: caller.did().to_string(),
            token: token.clone(),
            to: to.did().to_string(),
            role: role.to_string(),
            scope: self.app(),
        }
    }

    fn roles_of(&self, who: &Identity) -> Vec<(String, Scope)> {
        self.admin
            .grants(who.did(), NOW)
            .unwrap()
            .into_iter()
            .map(|g| (g.role, g.scope))
            .collect()
    }

    fn has(&self, who: &Identity, role: &str) -> bool {
        self.roles_of(who).contains(&(role.to_string(), self.app()))
    }
}

pub(super) fn refused(result: Result<impl std::fmt::Debug, NodeError>, want: CourierError) {
    match result {
        Err(NodeError::Courier(e)) => assert_eq!(e, want),
        other => panic!("expected {want:?}, got {other:?}"),
    }
}

#[test]
fn an_owner_assigns_an_app_role_and_the_cause_is_recorded() {
    let f = fixture();
    let (bob, _) = f.member();
    let token = f
        .admin
        .assign_role(f.request(&f.alice, &f.alice_token, &bob, "admin"), NOW)
        .unwrap();
    assert!(f.has(&bob, "admin"));
    let issue = f.admin.issue(&token.id()).unwrap().unwrap();
    assert_eq!(issue.cause, Cause::Under(f.alice_token.id()));
}

#[test]
fn an_app_admin_assigns_inside_its_cone_and_a_member_cannot_assign() {
    let f = fixture();
    let (bob, bob_token) = f.member();
    let (carol, carol_token) = f.member();
    let admin_token = f
        .admin
        .assign_role(f.request(&f.alice, &f.alice_token, &bob, "admin"), NOW)
        .unwrap();

    refused(
        f.admin
            .assign_role(f.request(&carol, &carol_token, &bob, "moderator"), NOW),
        CourierError::NotPermitted,
    );
    refused(
        f.admin
            .assign_role(f.request(&bob, &bob_token, &carol, "admin"), NOW),
        CourierError::NotPermitted,
    );
    // Bob presents his everyday member token; the admin grant is found in the node's records.
    let moderator = f
        .admin
        .assign_role(f.request(&bob, &bob_token, &carol, "moderator"), NOW)
        .unwrap();
    assert!(f.has(&carol, "moderator"));
    let issue = f.admin.issue(&moderator.id()).unwrap().unwrap();
    assert_eq!(issue.cause, Cause::Under(admin_token.id()));
}

#[test]
fn revoking_a_role_cascades_to_what_it_granted_and_nothing_else() {
    let f = fixture();
    let (bob, bob_token) = f.member();
    let (carol, _) = f.member();
    f.admin
        .assign_role(f.request(&f.alice, &f.alice_token, &bob, "admin"), NOW)
        .unwrap();
    f.admin
        .assign_role(f.request(&bob, &bob_token, &carol, "moderator"), NOW)
        .unwrap();

    let n = f
        .admin
        .revoke_role(f.request(&f.alice, &f.alice_token, &bob, "admin"), NOW)
        .unwrap();
    assert_eq!(n, 1, "one direct grant revoked");
    assert!(!f.has(&bob, "admin"));
    assert!(!f.has(&carol, "moderator"), "carol's came from bob's admin");
    assert!(
        f.roles_of(&carol)
            .contains(&("member".to_string(), Scope::Node)),
        "carol's own membership came from alice, not bob"
    );
    assert!(
        f.roles_of(&bob)
            .contains(&("member".to_string(), Scope::Node))
    );
}

#[test]
fn only_someone_who_could_assign_a_role_may_revoke_it() {
    let f = fixture();
    let (bob, _) = f.member();
    let (carol, carol_token) = f.member();
    f.admin
        .assign_role(f.request(&f.alice, &f.alice_token, &bob, "admin"), NOW)
        .unwrap();
    refused(
        f.admin
            .revoke_role(f.request(&carol, &carol_token, &bob, "admin"), NOW),
        CourierError::NotPermitted,
    );
    assert!(f.has(&bob, "admin"));
}

#[test]
fn an_invitees_grant_dies_with_its_inviters() {
    let f = fixture();
    let (bob, bob_token) = f.member();
    let issue = f.admin.issue(&bob_token.id()).unwrap().unwrap();
    assert_eq!(issue.cause, Cause::Under(f.alice_token.id()));

    f.admin.revoke(&f.alice_token.id(), NOW).unwrap();
    assert!(f.admin.grants(bob.did(), NOW).unwrap().is_empty());
}

#[test]
fn an_app_scoped_invite_names_a_declared_app_role() {
    let f = fixture();
    let dave = identity::generate().0;
    let request = |role: &str| InviteRequest {
        desktop_did: f.alice.did().to_string(),
        token: f.alice_token.clone(),
        role: role.to_string(),
        scope: f.app(),
        public: false,
    };
    let ticket = f
        .admin
        .issue_invite(&request("moderator"), "kunki", NOW)
        .unwrap();
    let hello = desktop_start_invite_claim(ticket, &dave, NOW).unwrap();
    f.admin.accept_invite(hello, NOW).unwrap();
    assert!(f.has(&dave, "moderator"));

    refused(
        f.admin.issue_invite(&request("ghost"), "kunki", NOW),
        CourierError::NotPermitted,
    );
}

#[test]
fn assigning_needs_the_apps_source_on_the_node() {
    let f = fixture();
    let (bob, _) = f.member();
    let mut req = f.request(&f.alice, &f.alice_token, &bob, "admin");
    req.scope = item_scope(&f.ws, &minted('c'));
    assert!(matches!(
        f.admin.assign_role(req, NOW),
        Err(NodeError::NoManifest(_))
    ));
}

#[test]
fn a_bare_manifest_declares_no_role_to_assign() {
    let f = fixture();
    f.vault
        .put_src(&f.ws, &f.item, &src_with_manifest("app \"chat\" {\n}\n"))
        .unwrap();
    let (bob, _) = f.member();
    refused(
        f.admin
            .assign_role(f.request(&f.alice, &f.alice_token, &bob, "admin"), NOW),
        CourierError::NotPermitted,
    );
}

#[test]
fn a_push_skips_a_subscriber_whose_grant_was_revoked() {
    let f = fixture();
    let (bob, bob_token) = f.member();
    let layer = SyncLayer::Doc("board".to_string());
    let sub = desktop_start_subscribe(bob.did(), bob_token.clone(), &f.ws, &f.item, layer.clone());
    f.admin.subscribe(&sub, NOW).unwrap();
    f.admin.revoke(&bob_token.id(), NOW).unwrap();

    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "after bob left").unwrap();
    let hello = desktop_start_sync(
        f.alice.did(),
        f.alice_token.clone(),
        &f.ws,
        &f.item,
        layer,
        &doc,
        None,
    )
    .unwrap();
    let pusher = MockPusher::new();
    f.admin.accept_sync(hello, NOW, &pusher).unwrap();
    assert!(pusher.received_by(bob.did()).is_empty());
}

#[test]
fn an_app_role_alone_lets_its_holder_sync_that_item() {
    let f = fixture();
    let dave = identity::generate().0;
    let request = InviteRequest {
        desktop_did: f.alice.did().to_string(),
        token: f.alice_token.clone(),
        role: "member".to_string(),
        scope: f.app(),
        public: false,
    };
    let ticket = f.admin.issue_invite(&request, "kunki", NOW).unwrap();
    let hello = desktop_start_invite_claim(ticket, &dave, NOW).unwrap();
    let token = f.admin.accept_invite(hello, NOW).unwrap().token;

    let doc = LoroDoc::new();
    doc.get_text("t").insert(0, "from dave").unwrap();
    let layer = SyncLayer::Doc("board".to_string());
    let hello = desktop_start_sync(
        dave.did(),
        token.clone(),
        &f.ws,
        &f.item,
        layer.clone(),
        &doc,
        None,
    )
    .unwrap();
    f.admin.accept_sync(hello, NOW, &MockPusher::new()).unwrap();
    f.admin.authorize_listen(dave.did(), &token, NOW).unwrap();

    let elsewhere =
        desktop_start_sync(dave.did(), token, &f.ws, &minted('c'), layer, &doc, None).unwrap();
    refused(
        f.admin.accept_sync(elsewhere, NOW, &MockPusher::new()),
        CourierError::OutOfScope,
    );
}

#[test]
fn a_member_cannot_rewrite_the_manifest_to_widen_its_own_cone() {
    let f = fixture();
    let (bob, bob_token) = f.member();
    let src = LoroDoc::new();
    src.import(&f.vault.get_src(&f.ws, &f.item).unwrap().unwrap())
        .unwrap();
    let forged =
        "app \"chat\" {\n  roles admin, moderator, member\n  role member { grant admin }\n}\n";
    let files = src.get_map("files");
    let text = files
        .insert_container("manifest.osv", LoroText::new())
        .unwrap();
    text.insert(0, forged).unwrap();
    let hello = desktop_start_sync(
        bob.did(),
        bob_token,
        &f.ws,
        &f.item,
        SyncLayer::Src,
        &src,
        None,
    )
    .unwrap();
    refused(
        f.admin.accept_sync(hello, NOW, &MockPusher::new()),
        CourierError::NotPermitted,
    );
    assert!(
        !f.admin
            .manifest(&f.app())
            .unwrap()
            .can_grant("member", "admin")
    );
}

#[test]
fn a_reconnect_reissue_dies_with_the_token_it_replaced() {
    let f = fixture();
    let record = courier::DesktopNodeRecord {
        node_did: node::did(&f.vault).unwrap(),
        node_id: String::new(),
        node_encryption_key: String::new(),
        token: f.alice_token.clone(),
    };
    let mut challenges = Vec::new();
    let challenge = f
        .vault
        .with_signer(|node| courier::node_issue_reconnect_challenge(node, &mut challenges))
        .unwrap();
    let hello = courier::desktop_start_reconnect(&record, &f.alice, challenge).unwrap();
    let reissued = f
        .admin
        .accept_reconnect(hello, &mut challenges, NOW)
        .unwrap();

    f.admin.revoke(&f.alice_token.id(), NOW).unwrap();
    let live: Vec<_> = f.admin.grants(f.alice.did(), NOW).unwrap();
    assert!(
        live.iter().all(|g| g.id != reissued.id()),
        "the reissue outlived the grant it replaced"
    );
}

#[test]
fn an_invite_redeemed_after_its_inviter_was_revoked_is_refused() {
    let f = fixture();
    let request = InviteRequest {
        desktop_did: f.alice.did().to_string(),
        token: f.alice_token.clone(),
        role: "member".to_string(),
        scope: Scope::Node,
        public: false,
    };
    let ticket = f.admin.issue_invite(&request, "kunki", NOW).unwrap();
    f.admin.revoke(&f.alice_token.id(), NOW).unwrap();

    let dave = identity::generate().0;
    let hello = desktop_start_invite_claim(ticket, &dave, NOW).unwrap();
    refused(f.admin.accept_invite(hello, NOW), CourierError::Revoked);
    assert!(f.admin.grants(dave.did(), NOW).unwrap().is_empty());
}

#[test]
fn an_invite_with_no_recorded_inviter_is_refused() {
    let f = fixture();
    let request = InviteRequest {
        desktop_did: f.alice.did().to_string(),
        token: f.alice_token.clone(),
        role: "member".to_string(),
        scope: Scope::Node,
        public: false,
    };
    // Minted by courier directly, the way a ticket from before inviter tracking was.
    let ticket = f
        .vault
        .with_signer(|node| {
            courier::invite::issue_invite_ticket(node, &request, "kunki", NOW, &Default::default())
        })
        .unwrap()
        .unwrap();
    let dave = identity::generate().0;
    let hello = desktop_start_invite_claim(ticket, &dave, NOW).unwrap();
    assert!(matches!(
        f.admin.accept_invite(hello, NOW),
        Err(NodeError::UntrackedInvite)
    ));
}
