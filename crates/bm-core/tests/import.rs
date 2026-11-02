use std::{fs, path::Path};

use bm_core::{
    Engine, Vault, engine::ImportSummary, ids::ProjectId, import::SkipReason, model::*,
    vault::KdfParams,
};

struct Fixture {
    dir: tempfile::TempDir,
    engine: Engine,
    user: Ctx,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let engine = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap();
    let user = Ctx::new(engine.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
    Fixture { dir, engine, user }
}

impl Fixture {
    fn import(&self, root: &Path) -> ImportSummary {
        self.engine.import_markdown(&self.user, root).unwrap()
    }

    /// Projects as `parent/name`, sorted.
    fn project_paths(&self) -> Vec<String> {
        let projects = self.engine.projects().unwrap();
        let name = |id: &ProjectId| {
            projects.iter().find(|p| &p.project.id == id).map(|p| p.project.name.clone()).unwrap()
        };
        let mut paths: Vec<String> = projects
            .iter()
            .map(|p| match &p.project.parent_id {
                Some(parent) => format!("{}/{}", name(parent), p.project.name),
                None => p.project.name.clone(),
            })
            .collect();
        paths.sort();
        paths
    }

    fn detail(&self, title: &str) -> NodeDetail {
        let id = self.engine.resolve_node(title).unwrap();
        self.engine.node_detail(&id).unwrap()
    }
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn obsidian_vault(dir: &Path) -> std::path::PathBuf {
    let root = dir.join("Zweites Hirn");
    write(&root, ".obsidian/workspace.md", "nicht");
    write(
        &root,
        "Start.md",
        "---\ntags: [anfang]\n---\nSiehe [[Rust]] und [Plan](Projekte/Plan.md).",
    );
    write(&root, "Projekte/Plan.md", "---\ntype: decision\n---\nZurück zu [[Start]].");
    write(&root, "Projekte/Lernen/Rust.md", "#lernen\n\nAsync.");
    write(&root, "Bilder/foto.png", "");
    root
}

#[test]
fn a_folder_comes_in_with_its_projects_and_links_and_goes_with_one_undo() {
    let f = setup();
    let root = obsidian_vault(f.dir.path());

    let preview = f.engine.import_preview(&root).unwrap();
    assert_eq!((preview.entries, preview.projects, preview.existing), (3, 3, 0));
    assert_eq!(preview.change_set, None);
    assert!(f.engine.projects().unwrap().is_empty(), "a preview writes nothing");

    let summary = f.import(&root);
    assert_eq!((summary.entries, summary.projects), (3, 3));
    // The folder of pictures has no notes and is no project.
    assert_eq!(f.project_paths(), ["Projekte/Lernen", "Zweites Hirn", "Zweites Hirn/Projekte"]);

    let start = f.detail("Start");
    assert_eq!(start.node.tags, ["anfang"]);
    assert_eq!(start.node.body, "Siehe [[Rust]] und [[Plan]].");
    assert_eq!(
        start.projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["Zweites Hirn"]
    );
    let mut out: Vec<_> = start.outgoing.iter().map(|e| e.other.title.as_str()).collect();
    out.sort();
    assert_eq!(out, ["Plan", "Rust"], "links between the files are links in the graph");
    assert_eq!(f.detail("Plan").node.kind, "decision");
    assert_eq!(f.detail("Rust").projects[0].name, "Lernen");

    // A second time, everything is there already.
    let again = f.engine.import_preview(&root).unwrap();
    assert_eq!((again.entries, again.projects, again.existing), (0, 0, 3));

    f.engine.revert(&f.user, &summary.change_set.unwrap()).unwrap();
    assert!(f.engine.projects().unwrap().is_empty());
    assert!(f.engine.resolve_node("Start").is_err());
}

#[test]
fn projects_of_the_same_name_are_used_and_taken_titles_left_out() {
    let f = setup();
    let root = obsidian_vault(f.dir.path());
    let top = f
        .engine
        .create_project(&f.user, NewProject { name: "zweites hirn".into(), ..Default::default() })
        .unwrap();
    f.engine.create_node(&f.user, NewNode { title: "rust".into(), ..Default::default() }).unwrap();
    write(&root, "Projekte/Zweiter Plan.md", "---\ntitle: Plan\n---\nDoppelt.");

    let summary = f.import(&root);
    assert_eq!((summary.entries, summary.projects, summary.existing), (2, 1, 1));
    assert_eq!(summary.skipped_count, 1);
    assert_eq!(summary.skipped[0].path, "Projekte/Zweiter Plan.md");
    assert_eq!(summary.skipped[0].reason, SkipReason::SameTitle);
    // „Lernen“ would only hold „Rust“, which is there already.
    assert_eq!(f.project_paths(), ["zweites hirn", "zweites hirn/Projekte"]);
    assert_eq!(f.detail("Start").projects[0].id, top.id);
    assert_eq!(f.detail("Plan").node.body, "Zurück zu [[Start]].");
    // The link to the entry that was there already holds too.
    assert!(f.detail("Start").outgoing.iter().any(|e| e.other.title == "rust"));
}

#[test]
fn an_export_comes_back_as_it_was() {
    let from = setup();
    let work = from
        .engine
        .create_project(&from.user, NewProject { name: "Arbeit".into(), ..Default::default() })
        .unwrap();
    let clients = from
        .engine
        .create_project(
            &from.user,
            NewProject {
                name: "Kunden".into(),
                parent: Some(work.id.clone()),
                ..Default::default()
            },
        )
        .unwrap();
    let offer = from
        .engine
        .create_node(
            &from.user,
            NewNode {
                title: "Angebot".into(),
                body: "Siehe [[Rechnung]].".into(),
                kind: Some("vertrag".into()),
                tags: vec!["geld".into()],
                projects: vec![clients.id.clone(), work.id.clone()],
                ..Default::default()
            },
        )
        .unwrap();
    let bill = from
        .engine
        .create_node(
            &from.user,
            NewNode {
                title: "Rechnung".into(),
                projects: vec![work.id.clone()],
                ..Default::default()
            },
        )
        .unwrap();
    from.engine
        .link(
            &from.user,
            NewEdge {
                src: bill.id.clone(),
                dst: offer.id.clone(),
                relation: Some("depends_on".into()),
                weight: None,
                note: None,
            },
        )
        .unwrap();
    from.engine
        .create_node(&from.user, NewNode { title: "Lose Notiz".into(), ..Default::default() })
        .unwrap();
    from.engine.create_doc_page(&from.user, &work.id, "Handbuch", "# Text", None, vec![]).unwrap();
    let into = from.dir.path().join("out");
    fs::create_dir_all(&into).unwrap();
    let export = from.engine.export_markdown(&into).unwrap();

    let to = setup();
    let summary = to.import(Path::new(&export.folder));
    assert_eq!((summary.entries, summary.projects, summary.links), (3, 2, 1));
    // The export's own folder and „Ohne Projekt“ are none.
    assert_eq!(to.project_paths(), ["Arbeit", "Arbeit/Kunden"]);
    let offer = to.detail("Angebot");
    assert_eq!(offer.node.kind, "vertrag");
    assert_eq!(offer.node.tags, ["geld"]);
    let mut projects: Vec<_> = offer.projects.iter().map(|p| p.name.as_str()).collect();
    projects.sort();
    assert_eq!(projects, ["Arbeit", "Kunden"]);
    let bill = to.detail("Rechnung");
    let links: Vec<_> =
        bill.outgoing.iter().map(|e| (e.edge.relation.as_str(), e.other.title.as_str())).collect();
    assert_eq!(links, [("depends_on", "Angebot")]);
    assert!(bill.incoming.iter().any(|e| e.edge.relation == "mentions"));
    assert!(to.detail("Lose Notiz").projects.is_empty());
    assert!(to.engine.resolve_node("Handbuch").is_err(), "documentation stays out");
}
