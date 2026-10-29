use bm_core::{CoreError, Engine, Event, Vault, model::*, vault::KdfParams};

struct Fixture {
    _dir: tempfile::TempDir,
    engine: Engine,
    user: Ctx,
    ai: Ctx,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let engine = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap();
    let user = Ctx::new(engine.ensure_actor(ActorKind::User, "Lasse").unwrap().id);
    let ai = Ctx::new(engine.ensure_actor(ActorKind::Ai, "Claude Code").unwrap().id)
        .with_client("claude-code")
        .with_tool("capture");
    Fixture { _dir: dir, engine, user, ai }
}

fn note(title: &str, body: &str) -> NewNode {
    NewNode { title: title.into(), body: body.into(), ..Default::default() }
}

fn titles(hits: &[SearchHit]) -> Vec<&str> {
    hits.iter().map(|h| h.node.title.as_str()).collect()
}

#[test]
fn actors_are_idempotent() {
    let f = setup();
    let again = f.engine.ensure_actor(ActorKind::User, "Lasse").unwrap();
    assert_eq!(again.id, f.user.actor);
}

#[test]
fn create_update_delete_node() {
    let f = setup();
    let n = f
        .engine
        .create_node(
            &f.user,
            NewNode {
                title: "  Tokio Runtime ".into(),
                body: "Async für Rust".into(),
                kind: Some("Code Snippet".into()),
                tags: vec!["#Rust".into(), "async".into(), "rust".into()],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(n.title, "Tokio Runtime");
    assert_eq!(n.kind, "code_snippet");
    assert_eq!(n.tags, ["async", "rust"]);
    assert!(n.reviewed, "user-created entries are reviewed");
    assert_eq!(n.rev, 1);

    let u = f
        .engine
        .update_node(
            &f.ai,
            &n.id,
            NodePatch {
                body: Some(BodyEdit::Append("Work-Stealing-Scheduler.".into())),
                tags_add: vec!["Performance".into()],
                tags_remove: vec!["async".into()],
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(u.body, "Async für Rust\n\nWork-Stealing-Scheduler.");
    assert_eq!(u.tags, ["performance", "rust"]);
    assert_eq!(u.rev, 2);
    assert!(!u.reviewed, "AI edits mark the entry unreviewed");

    // A patch that changes nothing is a no-op without a change set.
    let before = f.engine.change_sets(100, None).unwrap().len();
    let same = f.engine.update_node(&f.ai, &n.id, NodePatch::default()).unwrap();
    assert_eq!(same.rev, 2);
    assert_eq!(f.engine.change_sets(100, None).unwrap().len(), before);

    f.engine.delete_node(&f.user, &n.id).unwrap();
    assert!(f.engine.node(&n.id).unwrap().unwrap().deleted_at.is_some());
    assert!(f.engine.search(&SearchQuery::new("tokio")).unwrap().is_empty());
    assert!(matches!(f.engine.node_detail(&n.id), Err(CoreError::NotFound { .. })));
}

#[test]
fn search_ranks_and_filters() {
    let f = setup();
    let work = f
        .engine
        .create_project(&f.user, NewProject { name: "Arbeit".into(), ..Default::default() })
        .unwrap();
    let sub = f
        .engine
        .create_project(
            &f.user,
            NewProject {
                name: "Backend".into(),
                parent: Some(work.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();

    f.engine
        .create_node(
            &f.user,
            NewNode {
                projects: vec![sub.id.clone()],
                tags: vec!["rust".into()],
                ..note("Tokio", "Laufzeit für Nebenläufigkeit")
            },
        )
        .unwrap();
    f.engine
        .create_node(&f.user, note("Brot backen", "Sauerteig braucht Zeit, nebenbei Tokio hören"))
        .unwrap();
    f.engine
        .create_node(
            &f.user,
            NewNode { kind: Some("task".into()), ..note("Nebenläufigkeit testen", "") },
        )
        .unwrap();

    let all = f.engine.search(&SearchQuery::new("tokio")).unwrap();
    assert_eq!(titles(&all), ["Tokio", "Brot backen"], "title matches rank first");
    assert!(all[1].snippet.contains("**Tokio**"));

    assert_eq!(
        titles(&f.engine.search(&SearchQuery::new("nebenlaufig")).unwrap()).len(),
        2,
        "prefix + diacritics"
    );

    let in_work = f
        .engine
        .search(&SearchQuery { project: Some(work.id.clone()), ..SearchQuery::new("tokio") })
        .unwrap();
    assert_eq!(titles(&in_work), ["Tokio"], "subprojects included");
    let only_work = f
        .engine
        .search(&SearchQuery {
            project: Some(work.id.clone()),
            include_subprojects: false,
            ..SearchQuery::new("tokio")
        })
        .unwrap();
    assert!(only_work.is_empty());

    let tasks = f
        .engine
        .search(&SearchQuery { kinds: vec!["task".into()], ..SearchQuery::new("") })
        .unwrap();
    assert_eq!(titles(&tasks), ["Nebenläufigkeit testen"]);
    let tagged = f
        .engine
        .search(&SearchQuery { tags: vec!["#Rust".into()], ..SearchQuery::new("") })
        .unwrap();
    assert_eq!(titles(&tagged), ["Tokio"]);

    // FTS operators in user input must not break the query.
    f.engine.search(&SearchQuery::new("\"unbalanced OR (* NEAR")).unwrap();

    let projects = f.engine.projects().unwrap();
    let names: Vec<_> = projects.iter().map(|p| (p.project.name.as_str(), p.node_count)).collect();
    assert_eq!(names, [("Arbeit", 0), ("Backend", 1)]);
}

#[test]
fn wikilinks_maintain_mention_edges() {
    let f = setup();
    let a = f
        .engine
        .create_node(&f.user, note("Async Rust", "Nutzt [[Tokio]] und [[Pin|Pinning]]."))
        .unwrap();
    let tokio = f.engine.create_node(&f.user, note("Tokio", "")).unwrap();

    // Backfill: creating "Tokio" connected the earlier dangling link.
    let detail = f.engine.node_detail(&tokio.id).unwrap();
    assert_eq!(detail.incoming.len(), 1);
    assert_eq!(detail.incoming[0].other.title, "Async Rust");
    assert_eq!(detail.incoming[0].edge.relation, WIKILINK_RELATION);
    assert_eq!(detail.incoming[0].edge.origin, EdgeOrigin::Wikilink);

    let pin = f.engine.create_node(&f.user, note("Pin", "")).unwrap();
    assert_eq!(f.engine.node_detail(&a.id).unwrap().outgoing.len(), 2);

    // Removing the link from the body removes the edge.
    f.engine
        .update_node(
            &f.user,
            &a.id,
            NodePatch {
                body: Some(BodyEdit::Replace("Nur noch [[pin]].".into())),
                ..Default::default()
            },
        )
        .unwrap();
    let out = f.engine.node_detail(&a.id).unwrap().outgoing;
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].other.id, pin.id);
}

#[test]
fn links_are_idempotent_and_unlink_tolerates_direction() {
    let f = setup();
    let a = f.engine.create_node(&f.user, note("A", "")).unwrap();
    let b = f.engine.create_node(&f.user, note("B", "")).unwrap();
    let edge = NewEdge {
        src: a.id.clone(),
        dst: b.id.clone(),
        relation: Some("Depends On".into()),
        weight: None,
        note: None,
    };
    let e1 = f.engine.link(&f.user, edge.clone()).unwrap();
    let e2 = f.engine.link(&f.user, edge).unwrap();
    assert_eq!(e1.id, e2.id);
    assert_eq!(e1.relation, "depends_on");
    assert_eq!(e1.origin, EdgeOrigin::Manual);

    assert!(matches!(
        f.engine.link(
            &f.user,
            NewEdge {
                src: a.id.clone(),
                dst: a.id.clone(),
                relation: None,
                weight: None,
                note: None
            }
        ),
        Err(CoreError::Invalid(_))
    ));

    let removed = f.engine.unlink(&f.user, &b.id, &a.id, None).unwrap();
    assert_eq!(removed.len(), 1);
    assert!(f.engine.node_detail(&a.id).unwrap().outgoing.is_empty());
}

#[test]
fn capture_builds_subgraph_and_revert_undoes_it() {
    let f = setup();
    let existing = f.engine.create_node(&f.user, note("Tokio", "")).unwrap();
    let project = f
        .engine
        .create_project(&f.user, NewProject { name: "Brainmatter".into(), ..Default::default() })
        .unwrap();

    let result = f
        .engine
        .capture(
            &f.ai,
            Capture {
                nodes: vec![
                    CaptureNode {
                        key: Some("mcp".into()),
                        node: note("MCP", "Model Context Protocol"),
                    },
                    CaptureNode {
                        key: Some("rmcp".into()),
                        node: note("rmcp", "Rust SDK für [[MCP]]"),
                    },
                    CaptureNode { key: None, node: note("tokio", "Duplikat?") },
                ],
                links: vec![
                    CaptureLink {
                        from: "rmcp".into(),
                        to: "MCP".into(),
                        relation: Some("implements".into()),
                        note: None,
                    },
                    CaptureLink {
                        from: "rmcp".into(),
                        to: existing.id.short().into(),
                        relation: Some("depends_on".into()),
                        note: None,
                    },
                ],
                project: Some(project.id.clone()),
            },
        )
        .unwrap();
    assert_eq!(result.nodes.len(), 3);
    assert_eq!(result.edges.len(), 2);
    assert!(result.edges.iter().all(|e| e.origin == EdgeOrigin::Ai));
    assert!(result.nodes.iter().all(|n| !n.reviewed));
    assert_eq!(result.possible_duplicates.len(), 1);
    assert_eq!(result.possible_duplicates[0].1.id, existing.id);
    assert_eq!(f.engine.projects().unwrap()[0].node_count, 3);

    let rmcp = f.engine.node_detail(&result.nodes[1].id).unwrap();
    let mut rels: Vec<_> = rmcp.outgoing.iter().map(|e| e.edge.relation.as_str()).collect();
    rels.sort();
    assert_eq!(rels, ["depends_on", "implements", "mentions"]);

    let cs = result.change_set.unwrap();
    let info = &f.engine.change_sets(10, Some(&f.ai.actor)).unwrap()[0];
    assert_eq!(info.id, cs);
    assert_eq!(info.client.as_deref(), Some("claude-code"));
    assert_eq!(info.actor_name, "Claude Code");

    let report = f.engine.revert(&f.user, &cs).unwrap();
    assert!(report.restored >= 9, "3 nodes, 3 memberships, 3 edges: {}", report.restored);
    assert!(f.engine.search(&SearchQuery::new("protocol")).unwrap().is_empty());
    assert!(f.engine.node_detail(&existing.id).unwrap().incoming.is_empty());
    assert_eq!(f.engine.projects().unwrap()[0].node_count, 0);
    assert!(matches!(f.engine.revert(&f.user, &cs), Err(CoreError::Conflict(_))), "only once");

    // Reverting the revert restores everything (redo).
    f.engine.revert(&f.user, &report.change_set).unwrap();
    assert_eq!(titles(&f.engine.search(&SearchQuery::new("protocol")).unwrap()), ["MCP"]);
    assert_eq!(f.engine.projects().unwrap()[0].node_count, 3);
}

#[test]
fn revert_restores_previous_content_and_detects_conflicts() {
    let f = setup();
    let n = f.engine.create_node(&f.user, note("Plan", "Version 1")).unwrap();
    f.engine
        .update_node(
            &f.ai,
            &n.id,
            NodePatch { body: Some(BodyEdit::Replace("Version 2".into())), ..Default::default() },
        )
        .unwrap();
    let ai_change = f.engine.change_sets(1, Some(&f.ai.actor)).unwrap().remove(0).id;

    f.engine
        .update_node(
            &f.user,
            &n.id,
            NodePatch { title: Some("Plan B".into()), ..Default::default() },
        )
        .unwrap();
    assert!(matches!(f.engine.revert(&f.user, &ai_change), Err(CoreError::Conflict(_))));

    let user_change = f.engine.change_sets(1, Some(&f.user.actor)).unwrap().remove(0).id;
    f.engine.revert(&f.user, &user_change).unwrap();
    f.engine.revert(&f.user, &ai_change).unwrap();
    let restored = f.engine.node(&n.id).unwrap().unwrap();
    assert_eq!((restored.title.as_str(), restored.body.as_str()), ("Plan", "Version 1"));

    let history = f.engine.node_history(&n.id).unwrap();
    assert_eq!(history.len(), 5);
    assert_eq!(history.last().unwrap().after.as_ref().unwrap().body, "Version 1");
    assert!(history.last().unwrap().before.is_none());
}

#[test]
fn revert_of_delete_restores_node_and_edges() {
    let f = setup();
    let a = f.engine.create_node(&f.user, note("A", "")).unwrap();
    let b = f.engine.create_node(&f.user, note("B", "")).unwrap();
    f.engine
        .link(
            &f.user,
            NewEdge {
                src: a.id.clone(),
                dst: b.id.clone(),
                relation: None,
                weight: None,
                note: None,
            },
        )
        .unwrap();

    f.engine.delete_node(&f.ai, &a.id).unwrap();
    assert!(f.engine.node_detail(&b.id).unwrap().incoming.is_empty());

    let cs = f.engine.change_sets(1, Some(&f.ai.actor)).unwrap().remove(0).id;
    f.engine.revert(&f.user, &cs).unwrap();
    let detail = f.engine.node_detail(&b.id).unwrap();
    assert_eq!(detail.incoming.len(), 1);
    assert_eq!(detail.incoming[0].other.id, a.id);
    assert_eq!(titles(&f.engine.search(&SearchQuery::new("A")).unwrap()), ["A"]);
}

#[test]
fn references_resolve_by_title_short_id_and_suggest() {
    let f = setup();
    let n = f.engine.create_node(&f.user, note("Tokio Runtime", "async")).unwrap();
    f.engine.create_node(&f.user, note("Zweimal", "")).unwrap();
    f.engine.create_node(&f.user, note("zweimal", "")).unwrap();

    assert_eq!(f.engine.resolve_node(n.id.as_str()).unwrap(), n.id);
    assert_eq!(f.engine.resolve_node("tokio runtime").unwrap(), n.id);
    assert_eq!(f.engine.resolve_node(&format!("#{}", n.id.short())).unwrap(), n.id);

    match f.engine.resolve_node("Zweimal") {
        Err(CoreError::Ambiguous { candidates, .. }) => assert_eq!(candidates.len(), 2),
        other => panic!("{other:?}"),
    }
    match f.engine.resolve_node("Tokio") {
        Err(CoreError::NotFound { suggestions, .. }) => assert_eq!(suggestions[0].id, n.id),
        other => panic!("{other:?}"),
    }
}

#[test]
fn events_are_broadcast_after_commit() {
    let f = setup();
    let mut rx = f.engine.subscribe();
    let n = f.engine.create_node(&f.ai, note("Live", "")).unwrap();

    let mut events = Vec::new();
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    assert_eq!(events.first(), Some(&Event::NodeUpserted { id: n.id.clone() }));
    assert!(
        matches!(events.last(), Some(Event::ChangeSetCommitted { actor, .. }) if *actor == f.ai.actor)
    );

    // Failed writes roll back and emit nothing.
    assert!(f.engine.create_node(&f.ai, note("   ", "")).is_err());
    assert!(rx.try_recv().is_err());
}

#[test]
fn unknown_actor_is_rejected() {
    let f = setup();
    let ghost = Ctx::new(bm_core::ids::ActorId::new());
    assert!(matches!(
        f.engine.create_node(&ghost, note("x", "")),
        Err(CoreError::NotFound { kind: "actor", .. })
    ));
}
