use super::*;

/// `docs/design/group-chat-sync.md` §3, verbatim.
const CHAT: &str = r#"
app "chat" {
  roles admin, moderator, member
  role admin { grant moderator, member }
  doc chat                   { read member  write admin }
  doc channel/{cid}/{day}    { shard by day  read member  write member  validate "rules.message" }
  doc group/{gid}/meta       { read members(group/{gid}/meta)  write member  validate "rules.group_meta" }
  doc group/{gid}/{day}      { shard by day  history all
                               read members(group/{gid}/meta)  write members(group/{gid}/meta)
                               validate "rules.message" }
  doc dm/{a}/{b}/{day}       { shard by day  read a, b  write a, b  validate "rules.message" }
  doc mod/bans               { read member  write moderator  validate "rules.bans" }
  doc user/{did}             { read did  write did }
  uses "people" read
  channel presence { send member  slot sender }
  channel typing   { send member  rate 4/s  slot sender }
}
"#;

fn chat() -> Manifest {
    Manifest::parse(CHAT).unwrap()
}

fn err(src: &str) -> Error {
    Manifest::parse(src).unwrap_err()
}

#[test]
fn the_chat_manifest_parses() {
    let m = chat();
    assert_eq!(m.name, "chat");
    assert_eq!(m.roles, ["admin", "moderator", "member"]);
    assert_eq!(m.docs.len(), 7);
    assert_eq!(
        m.uses,
        [Uses {
            namespace: "people".into(),
            write: false
        }]
    );
    assert_eq!(m.channels.len(), 2);
    let typing = &m.channels[1];
    assert_eq!(typing.name, "typing");
    assert_eq!(typing.send, [Who::Role("member".into())]);
    assert_eq!(typing.rate_per_sec, Some(4));
    assert!(typing.slot_sender);
}

#[test]
fn a_doc_declaration_keeps_every_clause() {
    let m = chat();
    let (group, _) = m.resolve("group/g1/2026-10-04").unwrap();
    assert_eq!(group.shard, Some(Shard::Day));
    assert_eq!(group.history, Some(History::All));
    assert_eq!(group.validate.as_deref(), Some("rules.message"));
    assert_eq!(
        group.read,
        [Who::Members(Pattern::parse("group/{gid}/meta").unwrap())]
    );
    let (dm, _) = m.resolve("dm/x/y/2026-10-04").unwrap();
    assert_eq!(dm.read, [Who::Var("a".into()), Who::Var("b".into())]);
}

#[test]
fn resolve_binds_path_variables() {
    let m = chat();
    let (decl, vars) = m.resolve("dm/did:key:za/did:key:zb/2026-10-04").unwrap();
    assert_eq!(decl.pattern.to_string(), "dm/{a}/{b}/{day}");
    assert_eq!(vars["a"], "did:key:za");
    assert_eq!(vars["b"], "did:key:zb");
    assert_eq!(vars["day"], "2026-10-04");
}

#[test]
fn a_literal_segment_beats_a_variable() {
    let m = chat();
    let (decl, vars) = m.resolve("group/g1/meta").unwrap();
    assert_eq!(decl.pattern.to_string(), "group/{gid}/meta");
    assert_eq!(vars["gid"], "g1");
}

#[test]
fn a_day_shard_only_matches_a_date() {
    let m = chat();
    assert!(m.resolve("channel/general/2026-10-04").is_some());
    assert!(m.resolve("channel/general/today").is_none());
    assert!(m.resolve("channel/general/2026-13-04").is_none());
}

#[test]
fn an_undeclared_doc_resolves_to_nothing() {
    let m = chat();
    assert!(m.resolve("secrets").is_none());
    assert!(m.resolve("chat/extra").is_none());
    assert!(m.resolve("").is_none());
}

#[test]
fn a_bare_manifest_declares_nothing() {
    let m = Manifest::parse("app \"chat\" {\n\n  }\n").unwrap();
    assert!(m.roles.is_empty() && m.docs.is_empty());
    assert!(m.is_bare());
    assert!(!chat().is_bare());
}

#[test]
fn a_legacy_name_only_manifest_is_bare() {
    let m = Manifest::parse("name = \"3D Model Viewer\"\n").unwrap();
    assert_eq!(m.name, "3D Model Viewer");
    assert!(m.is_bare());
}

#[test]
fn comments_and_blank_lines_are_ignored() {
    let m = Manifest::parse(
        "-- a chat\napp \"c\" {\n  roles member -- everyone\n\n  doc chat { read member write member } -- one doc\n}\n",
    )
    .unwrap();
    assert_eq!(m.docs.len(), 1);
}

#[test]
fn the_grant_cone_is_transitive_and_owner_is_its_root() {
    let m = Manifest::parse(
        r#"app "c" {
  roles admin, moderator, member
  role admin { grant moderator }
  role moderator { grant member }
}"#,
    )
    .unwrap();
    assert!(m.can_grant("admin", "moderator"));
    assert!(m.can_grant("admin", "member"));
    assert!(m.can_grant("moderator", "member"));
    assert!(!m.can_grant("moderator", "admin"));
    assert!(!m.can_grant("member", "member"));
    assert!(m.can_grant(OWNER, "admin"));
    assert!(!m.can_grant(OWNER, "undeclared"));
    assert!(!m.can_grant("admin", "admin"));
}

#[test]
fn a_role_satisfies_itself_and_everything_in_its_cone() {
    let m = chat();
    assert!(m.satisfies("admin", "member"));
    assert!(m.satisfies("member", "member"));
    assert!(m.satisfies(OWNER, "moderator"));
    assert!(!m.satisfies("member", "moderator"));
    assert!(!m.satisfies("moderator", "admin"));
}

#[test]
fn errors_name_the_line() {
    let e = err("app \"c\" {\n  roles member\n  doc chat { read nobody }\n}\n");
    assert_eq!(e.line, 3);
    assert!(e.message.contains("nobody"), "{e}");
}

#[test]
fn an_unknown_statement_is_an_error() {
    let e = err("app \"c\" {\n  frobnicate\n}\n");
    assert_eq!(e.line, 2);
}

#[test]
fn an_unknown_clause_is_an_error() {
    let e = err("app \"c\" {\n  roles member\n  doc chat { read member colour blue }\n}\n");
    assert_eq!(e.line, 3);
    assert!(e.message.contains("colour"), "{e}");
}

#[test]
fn a_grant_of_an_undeclared_role_is_an_error() {
    let e = err("app \"c\" {\n  roles admin\n  role admin { grant ghost }\n}\n");
    assert!(e.message.contains("ghost"), "{e}");
}

#[test]
fn a_role_named_owner_or_node_is_reserved() {
    assert!(
        err("app \"c\" {\n  roles owner\n}\n")
            .message
            .contains("reserved")
    );
    assert!(
        err("app \"c\" {\n  roles node\n}\n")
            .message
            .contains("reserved")
    );
}

#[test]
fn a_variable_rule_must_name_a_variable_of_its_pattern() {
    let e = err("app \"c\" {\n  doc user/{did} { read someone }\n}\n");
    assert!(e.message.contains("someone"), "{e}");
}

#[test]
fn members_must_name_a_declared_doc_whose_variables_are_bound() {
    let e = err("app \"c\" {\n  roles member\n  doc g/{gid}/x { read members(g/{gid}/meta) }\n}\n");
    assert!(e.message.contains("g/{gid}/meta"), "{e}");
    let e = err(
        "app \"c\" {\n  roles member\n  doc g/{gid}/meta { read member }\n  doc h/{hid} { read members(g/{gid}/meta) }\n}\n",
    );
    assert!(e.message.contains("gid"), "{e}");
}

#[test]
fn a_day_shard_needs_a_day_variable() {
    let e = err("app \"c\" {\n  roles member\n  doc log { shard by day read member }\n}\n");
    assert!(e.message.contains("{day}"), "{e}");
}

#[test]
fn two_patterns_that_match_the_same_names_are_an_error() {
    let e = err(
        "app \"c\" {\n  roles member\n  doc a/{x} { read member }\n  doc a/{y} { read member }\n}\n",
    );
    assert_eq!(e.line, 4);
}

#[test]
fn a_bad_pattern_is_an_error() {
    for bad in ["a//b", "a/{}", "a/{x", "a/{x}/{x}", "/a"] {
        let src = format!("app \"c\" {{\n  roles member\n  doc {bad} {{ read member }}\n}}\n");
        assert_eq!(err(&src).line, 3, "{bad}");
    }
}

#[test]
fn a_grant_error_names_its_line() {
    let e = err("app \"c\" {\n  roles admin\n\n  role admin { grant ghost }\n}\n");
    assert_eq!(e.line, 4);
}

#[test]
fn every_demo_app_manifest_parses() {
    let apps = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../demo_apps");
    let mut n = 0;
    for entry in std::fs::read_dir(apps).unwrap() {
        let path = entry.unwrap().path().join("manifest.osv");
        if let Ok(src) = std::fs::read_to_string(&path) {
            Manifest::parse(&src).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            n += 1;
        }
    }
    assert!(n > 10, "found only {n} manifests");
}

#[test]
fn a_name_with_an_unsafe_segment_resolves_to_nothing() {
    let m = chat();
    for bad in [
        "user/..", "user/.", "user/a b", "user/é", "user/a\0", "user/", "/user/x", "user//x",
    ] {
        assert!(m.resolve(bad).is_none(), "{bad:?}");
    }
    assert!(m.resolve(&format!("user/{}", "a".repeat(129))).is_none());
    assert!(m.resolve("user/did:key:z6Mk-a_b").is_some());
    assert!(m.resolve("user/a.b").is_none());
}

#[test]
fn a_name_longer_than_any_address_resolves_to_nothing() {
    let m = Manifest::parse(
        "app \"c\" {\n  roles member\n  doc {a}/{b}/{c}/{d}/{e}/{f}/{g}/{h} { read member }\n}\n",
    )
    .unwrap();
    let name = |len| vec!["c".repeat(len); 8].join("/");
    let fits = name((workspace::MAX_DOC_NAME_LEN - 7) / 8);
    assert!(m.resolve(&fits).is_some());
    assert!(m.resolve(&name(128)).is_none());
}

#[test]
fn a_pattern_literal_must_be_a_safe_segment() {
    for bad in ["a/..", "a/b c", "a/é"] {
        let src = format!("app \"c\" {{\n  roles member\n  doc {bad} {{ read member }}\n}}\n");
        assert_eq!(err(&src).line, 3, "{bad}");
    }
}

#[test]
fn a_variable_named_node_or_owner_is_reserved() {
    let e = err("app \"c\" {\n  doc user/{node} { read node }\n}\n");
    assert!(e.message.contains("reserved"), "{e}");
    let e = err("app \"c\" {\n  doc user/{owner} { read owner }\n}\n");
    assert!(e.message.contains("reserved"), "{e}");
}

#[test]
fn a_day_shard_only_matches_a_real_calendar_day() {
    let m = chat();
    assert!(m.resolve("channel/c/2024-02-29").is_some());
    assert!(m.resolve("channel/c/2026-02-29").is_none());
    assert!(m.resolve("channel/c/2026-02-31").is_none());
    assert!(m.resolve("channel/c/2026-04-31").is_none());
    assert!(m.resolve("channel/c/2026-00-10").is_none());
}

#[test]
fn a_channel_cannot_bind_a_members_variable() {
    let e = err(
        "app \"c\" {\n  roles member\n  doc g/{gid}/meta { read member }\n  channel typing { send members(g/{gid}/meta) }\n}\n",
    );
    assert_eq!(e.line, 4);
    assert!(e.message.contains("gid"), "{e}");
}

#[test]
fn a_members_error_names_the_clause_line() {
    let e = err(
        "app \"c\" {\n  roles member\n  doc g/{gid}/x { read member\n    write members(g/{gid}/meta) }\n}\n",
    );
    assert_eq!(e.line, 4);
}

#[test]
fn a_grant_cycle_is_an_error() {
    let e = err("app \"c\" {\n  roles a, b\n  role a { grant b }\n  role b { grant a }\n}\n");
    assert!(e.message.contains("cycle"), "{e}");
    let e = err("app \"c\" {\n  roles a\n  role a { grant a }\n}\n");
    assert!(e.message.contains("cycle"), "{e}");
}

#[test]
fn a_role_or_role_block_declared_twice_is_an_error() {
    let e = err("app \"c\" {\n  roles a, b, a\n}\n");
    assert!(e.message.contains("twice"), "{e}");
    let e = err("app \"c\" {\n  roles a, b, c\n  role a { grant b }\n  role a { grant c }\n}\n");
    assert_eq!(e.line, 4);
}

#[test]
fn designed_but_unbuilt_statements_say_so() {
    for kw in ["derive", "local", "sim"] {
        let e = err(&format!("app \"c\" {{\n  {kw} \"x\"\n}}\n"));
        assert!(e.message.contains("not built"), "{kw}: {e}");
    }
}

#[test]
fn where_patterns_overlap_the_first_literal_wins() {
    let m = Manifest::parse(
        "app \"c\" {\n  roles member\n  doc a/{x}/c { read member }\n  doc a/b/{y} { read member }\n}\n",
    )
    .unwrap();
    let (decl, _) = m.resolve("a/b/c").unwrap();
    assert_eq!(decl.pattern.to_string(), "a/b/{y}");
    let (decl, _) = m.resolve("a/z/c").unwrap();
    assert_eq!(decl.pattern.to_string(), "a/{x}/c");
}

#[test]
fn an_unclosed_block_is_an_error() {
    let e = err("app \"c\" {\n  roles member\n");
    assert!(e.message.contains("}"), "{e}");
}
