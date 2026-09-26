use identity::Identity;

use super::*;
use crate::CourierError;
use crate::token::{self, issue_root};

fn ids() -> (Identity, Identity) {
    let (node, _) = identity::generate();
    let (desktop, _) = identity::generate();
    (node, desktop)
}

/// Stands in for the token a claim or reconnect already left the inviter holding.
fn node_token(node: &Identity, desktop: &Identity, role: &str, now: u64) -> Token {
    issue_root(
        node,
        &desktop.did(),
        role,
        Scope::Node,
        true,
        now,
        now + 1000,
    )
    .unwrap()
}

#[test]
fn an_owner_invites_a_member_into_a_workspace() {
    let (node, desktop) = ids();
    let request = InviteRequest {
        desktop_did: desktop.did().to_string(),
        token: node_token(&node, &desktop, "owner", 1),
        role: "member".to_string(),
        scope: Scope::Workspace("ws1".to_string()),
    };

    let ticket = issue_invite_ticket(&node, &request, "kunki", 2, &HashSet::new()).unwrap();
    assert_eq!(ticket.node_did, node.did());
    assert_eq!(
        InviteTicket::from_text(&ticket.to_text().unwrap()).unwrap(),
        ticket
    );
}

#[test]
fn an_owner_already_narrowed_to_a_workspace_can_still_invite_into_it() {
    let (node, desktop) = ids();
    let owner = node_token(&node, &desktop, "owner", 1);
    // Delegation only ever narrows scope, never changes role — this is still "owner", just
    // pinned to one workspace, which `platform_capabilities` grants `MemberInvite` too.
    let narrowed = token::delegate(
        &owner,
        &desktop,
        &desktop.did(),
        Scope::Workspace("ws1".to_string()),
        false,
        1,
        100,
    )
    .unwrap();
    let request = InviteRequest {
        desktop_did: desktop.did().to_string(),
        token: narrowed,
        role: "guest".to_string(),
        scope: Scope::Workspace("ws1".to_string()),
    };

    assert!(issue_invite_ticket(&node, &request, "kunki", 2, &HashSet::new()).is_ok());
}

#[test]
fn an_invite_cannot_grant_a_role_that_carries_capability() {
    let (node, desktop) = ids();
    let request = InviteRequest {
        desktop_did: desktop.did().to_string(),
        token: node_token(&node, &desktop, "owner", 1),
        // "maintainer" carries `MemberInvite`/`RoleAssign` itself — handing it out here would
        // be self-service role.assign with no rank check behind it.
        role: "maintainer".to_string(),
        scope: Scope::Workspace("ws1".to_string()),
    };

    assert_eq!(
        issue_invite_ticket(&node, &request, "kunki", 2, &HashSet::new()).unwrap_err(),
        CourierError::RoleNotInvitable
    );
}

#[test]
fn an_invite_cannot_grant_a_node_scope_role_that_carries_capability_once_narrowed_to_a_workspace() {
    let (node, desktop) = ids();
    let request = InviteRequest {
        desktop_did: desktop.did().to_string(),
        token: node_token(&node, &desktop, "owner", 1),
        // Empty at `Scope::Node`, but `token::delegate` would let the redeemer narrow this same
        // role, unchanged, straight into `(Scope::Workspace(_), "maintainer")`'s full capability
        // set — exactly the escalation `role_could_gain_capability`'s workspace lookahead exists
        // to catch.
        role: "maintainer".to_string(),
        scope: Scope::Node,
    };

    assert_eq!(
        issue_invite_ticket(&node, &request, "kunki", 2, &HashSet::new()).unwrap_err(),
        CourierError::RoleNotInvitable
    );
}

#[test]
fn a_revoked_inviter_cannot_invite() {
    let (node, desktop) = ids();
    let token = node_token(&node, &desktop, "owner", 1);
    let request = InviteRequest {
        desktop_did: desktop.did().to_string(),
        token: token.clone(),
        role: "member".to_string(),
        scope: Scope::Workspace("ws1".to_string()),
    };

    let mut revoked = HashSet::new();
    revoked.insert(token.id());
    assert_eq!(
        issue_invite_ticket(&node, &request, "kunki", 2, &revoked).unwrap_err(),
        CourierError::Revoked
    );
}

#[test]
fn a_desktop_redeems_an_invite_and_gets_the_role_it_named() {
    let (node, inviter) = ids();
    let (invitee, _) = identity::generate();
    let request = InviteRequest {
        desktop_did: inviter.did().to_string(),
        token: node_token(&node, &inviter, "owner", 1),
        role: "member".to_string(),
        scope: Scope::Workspace("ws1".to_string()),
    };
    let ticket = issue_invite_ticket(&node, &request, "kunki", 2, &HashSet::new()).unwrap();

    let hello = desktop_start_invite_claim(ticket, &invitee, 3).unwrap();
    let welcome = node_accept_invite(hello, &node, 4, &HashSet::new()).unwrap();

    assert_eq!(welcome.node_did, node.did());
    let claims = token::verify_chain(
        &welcome.token,
        &node.did(),
        invitee.did(),
        5,
        &HashSet::new(),
    )
    .unwrap();
    assert_eq!(claims.role, "member");
    assert_eq!(claims.scope, Scope::Workspace("ws1".to_string()));
}

#[test]
fn a_redeemed_invite_cannot_be_redeemed_again() {
    let (node, inviter) = ids();
    let (invitee, _) = identity::generate();
    let request = InviteRequest {
        desktop_did: inviter.did().to_string(),
        token: node_token(&node, &inviter, "owner", 1),
        role: "member".to_string(),
        scope: Scope::Workspace("ws1".to_string()),
    };
    let ticket = issue_invite_ticket(&node, &request, "kunki", 2, &HashSet::new()).unwrap();
    let hello = desktop_start_invite_claim(ticket, &invitee, 3).unwrap();

    let welcome = node_accept_invite(hello.clone(), &node, 4, &HashSet::new()).unwrap();
    let mut redeemed = HashSet::new();
    redeemed.insert(welcome.redeemed_nonce);

    assert_eq!(
        node_accept_invite(hello, &node, 4, &redeemed).unwrap_err(),
        CourierError::InviteAlreadyRedeemed
    );
}

#[test]
fn a_ticket_minted_by_a_different_node_is_refused() {
    let (node, inviter) = ids();
    let (other_node, _) = identity::generate();
    let (invitee, _) = identity::generate();
    let request = InviteRequest {
        desktop_did: inviter.did().to_string(),
        token: node_token(&node, &inviter, "owner", 1),
        role: "member".to_string(),
        scope: Scope::Workspace("ws1".to_string()),
    };
    let ticket = issue_invite_ticket(&node, &request, "kunki", 2, &HashSet::new()).unwrap();
    let hello = desktop_start_invite_claim(ticket, &invitee, 3).unwrap();

    assert_eq!(
        node_accept_invite(hello, &other_node, 4, &HashSet::new()).unwrap_err(),
        CourierError::NodeMismatch
    );
}

#[test]
fn a_tampered_invite_ticket_is_rejected() {
    let (node, inviter) = ids();
    let (invitee, _) = identity::generate();
    let request = InviteRequest {
        desktop_did: inviter.did().to_string(),
        token: node_token(&node, &inviter, "owner", 1),
        role: "member".to_string(),
        scope: Scope::Workspace("ws1".to_string()),
    };
    let mut ticket = issue_invite_ticket(&node, &request, "kunki", 2, &HashSet::new()).unwrap();
    ticket.node_id.push('x');

    assert_eq!(
        desktop_start_invite_claim(ticket, &invitee, 3).unwrap_err(),
        CourierError::BadAttestation
    );
}

#[test]
fn an_invite_request_that_lies_about_its_holder_is_refused() {
    let (node, desktop) = ids();
    let (someone_else, _) = identity::generate();
    let request = InviteRequest {
        desktop_did: someone_else.did().to_string(),
        token: node_token(&node, &desktop, "owner", 1),
        role: "member".to_string(),
        scope: Scope::Workspace("ws1".to_string()),
    };

    assert_eq!(
        issue_invite_ticket(&node, &request, "kunki", 2, &HashSet::new()).unwrap_err(),
        CourierError::WrongHolder
    );
}
