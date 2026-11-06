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

// ── owned-entries §2.3: rules as data ────────────────────────────────────────

const BOARD: &str = r#"
app "board" {
  roles admin, editor, member
  doc board/{id} {
    read member  write member
    own cards/* override admin
    rule cards/* {
      forbid set, delete, move  unless actor is entry.creator or actor has admin
      forbid set where changed state
             unless state from todo to doing, doing to done, doing to todo
      forbid insert, set where after.title is missing or len after.title above 200
      forbid set unless changed only title, state, body
    }
    rule tree/** { forbid set, delete, move unless actor is entry.creator or actor has editor }
  }
  doc sheet/{id} { write member }
}
"#;

fn lit(s: &str) -> Lit {
    Lit::Str(s.into())
}

#[test]
fn the_board_rules_parse_as_data() {
    let m = Manifest::parse(BOARD).unwrap();
    let board = &m.docs[0];
    assert_eq!(board.rules.len(), 3);
    assert_eq!(m.docs[1].rules, []);

    // `own … override admin` is the first forbid of the rule block, spelled out.
    let own = &board.rules[0];
    assert_eq!(own.path, RulePath::parse("cards/*").unwrap());
    assert_eq!(own.forbids.len(), 1);
    assert_eq!(own.forbids[0], board.rules[1].forbids[0].clone_with_line(own.forbids[0].line));

    let block = &board.rules[1];
    assert_eq!(block.forbids.len(), 4);
    assert_eq!(
        block.forbids[0],
        Forbid {
            ops: vec![OpKind::Set, OpKind::Delete, OpKind::Move],
            when: None,
            unless: Some(Pred::Or(
                Box::new(Pred::Is(Operand::Actor, Operand::Creator)),
                Box::new(Pred::Has("admin".into())),
            )),
            line: block.forbids[0].line,
        }
    );
    assert_eq!(block.forbids[1].when, Some(Pred::Changed("state".into())));
    assert_eq!(
        block.forbids[1].unless,
        Some(Pred::From(
            "state".into(),
            vec![
                (lit("todo"), lit("doing")),
                (lit("doing"), lit("done")),
                (lit("doing"), lit("todo")),
            ]
        ))
    );
    assert_eq!(block.forbids[2].ops, [OpKind::Insert, OpKind::Set]);
    assert_eq!(
        block.forbids[2].when,
        Some(Pred::Or(
            Box::new(Pred::Missing(Operand::After("title".into()))),
            Box::new(Pred::Above(
                Operand::Len(Box::new(Operand::After("title".into()))),
                Lit::Int(200)
            )),
        ))
    );
    assert_eq!(
        block.forbids[3].unless,
        Some(Pred::ChangedOnly(vec!["title".into(), "state".into(), "body".into()]))
    );
    assert_eq!(
        board.rules[2].path,
        RulePath(vec![RuleSeg::Lit("tree".into()), RuleSeg::Rest])
    );
}

impl Forbid {
    fn clone_with_line(&self, line: usize) -> Self {
        Self { line, ..self.clone() }
    }
}

#[test]
fn a_rule_path_covers_the_entry_and_anything_inside_it() {
    let p = |s: &str| RulePath::parse(s).unwrap();
    let path = |s: &str| s.split('/').map(String::from).collect::<Vec<_>>();
    assert!(p("cards/*").covers(&path("cards/c1")));
    assert!(p("cards/*").covers(&path("cards/c1/title")));
    assert!(!p("cards/*").covers(&path("cards")));
    assert!(!p("cards/*").covers(&path("notes/c1")));
    assert!(p("tree/**").covers(&path("tree/a/b/c")));
    assert!(!p("tree/**").covers(&path("tree")));
    assert_eq!(p("cards/*").depth(&path("cards/c1/title")), 2);
    assert_eq!(p("tree/**").depth(&path("tree/a/b")), 3);
    assert_eq!(RulePath::parse("a/**/b").unwrap_err(), "`**` must end `a/**/b`");
}

#[test]
fn precedence_is_not_then_and_then_or_and_parentheses_group() {
    let src = |pred: &str| {
        format!("app \"x\" {{ roles a\n doc d {{ write a  rule e/* {{ forbid set where {pred} }} }} }}")
    };
    let parse = |pred: &str| {
        Manifest::parse(&src(pred)).unwrap().docs[0].rules[0].forbids[0]
            .when
            .clone()
            .unwrap()
    };
    let t = |s: &str| Operand::Lit(lit(s));
    let is = |a: &str, b: &str| Pred::Is(t(a), t(b));
    assert_eq!(
        parse("x is p or y is q and not z is r"),
        Pred::Or(
            Box::new(is("x", "p")),
            Box::new(Pred::And(
                Box::new(is("y", "q")),
                Box::new(Pred::Not(Box::new(is("z", "r"))))
            ))
        )
    );
    assert_eq!(
        parse("(x is p or y is q) and z is r"),
        Pred::And(
            Box::new(Pred::Or(Box::new(is("x", "p")), Box::new(is("y", "q")))),
            Box::new(is("z", "r"))
        )
    );
}

#[test]
fn a_rule_that_is_not_data_is_refused() {
    let src = |body: &str| format!("app \"x\" {{ roles a\n doc d {{ write a  {body} }} }}");
    let e = |body: &str| Manifest::parse(&src(body)).unwrap_err().message;
    assert_eq!(e("rule e/* { }"), "a rule needs at least one `forbid`");
    assert_eq!(e("rule e/* { forbid drop }"), "expected `insert`, `set`, `delete` or `move`, found `drop`");
    assert_eq!(e("own e/* override mod"), "`mod` is not a declared role");
    assert_eq!(e("rule e/* { forbid set where actor has mod }"), "`mod` is not a declared role");
    assert_eq!(e("rule e/* { forbid set where after.x is forbid }"), "`forbid` is a keyword, not a value");
    assert_eq!(e("rule e/**/f { forbid set }"), "`**` must end `e/**/f`");
    assert!(e("rule e/* { forbid set where after.x }").starts_with("expected `is`, `in`, `above` or `below`"));
}

#[test]
fn owner_may_be_named_as_an_override_without_being_declared() {
    let src = "app \"x\" { roles a\n doc d { write a  own e/* override owner } }";
    Manifest::parse(src).unwrap();
}
