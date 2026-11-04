//! The skill vault: saving, where skills apply, their files, importing in place,
//! and the way through a Markdown export and back.

use std::{fs, path::Path};

use bm_core::{
    CoreError, Engine, Vault, blobs::BlobStore, ids::ProjectId, import::SkipReason, model::*,
    vault::KdfParams,
};

struct Fixture {
    dir: tempfile::TempDir,
    e: Engine,
    user: Ctx,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let blobs = BlobStore::new(dir.path().join("blobs"), &keys.blob_key());
    let e = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap().with_blobs(blobs);
    let user = Ctx::new(e.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
    Fixture { dir, e, user }
}

impl Fixture {
    fn project(&self, name: &str, parent: Option<&ProjectId>) -> ProjectId {
        let input = NewProject { name: name.into(), parent: parent.cloned(), ..Default::default() };
        self.e.create_project(&self.user, input).unwrap().id
    }

    fn skill(&self, title: &str, description: &str, projects: &[&ProjectId]) -> SkillInfo {
        let input = SkillInput {
            id: None,
            title: title.into(),
            description: description.into(),
            body: format!("# {title}\n\nSchritte."),
            projects: projects.iter().map(|p| (*p).clone()).collect(),
        };
        self.e.save_skill(&self.user, input).unwrap()
    }

    fn undo_last(&self) {
        let last = self.e.change_sets(1, None).unwrap()[0].id.clone();
        self.e.revert(&self.user, &last).unwrap();
    }

    fn titles(&self, project: Option<&ProjectId>) -> Vec<String> {
        self.e.skills(project).unwrap().into_iter().map(|s| s.node.title).collect()
    }
}

fn names(projects: &[ProjectBrief]) -> Vec<&str> {
    projects.iter().map(|p| p.name.as_str()).collect()
}

#[test]
fn saving_sets_exactly_the_projects_as_one_undoable_change() {
    let f = setup();
    let a = f.project("A", None);
    let b = f.project("B", None);
    let created = f.skill("Code Review", "Prüft Änderungen.", &[&a]);
    assert_eq!(created.slug, "code-review");
    assert_eq!(created.description, "Prüft Änderungen.");
    assert_eq!((created.node.kind.as_str(), created.reviewed), (SKILL_KIND, true));
    assert_eq!(names(&created.projects), ["A"]);
    assert_eq!(created.source, None);

    let change = SkillInput {
        id: Some(created.node.id.clone()),
        title: "Code Review".into(),
        description: "  Prüft Diffs.  ".into(),
        body: "Neu".into(),
        projects: vec![b.clone()],
    };
    let changed = f.e.save_skill(&f.user, change).unwrap();
    assert_eq!((changed.description.as_str(), changed.body.as_str()), ("Prüft Diffs.", "Neu"));
    assert_eq!(names(&changed.projects), ["B"]);

    f.undo_last();
    let back = f.e.skill(&created.node.id).unwrap();
    assert_eq!(
        (back.description.as_str(), names(&back.projects)),
        ("Prüft Änderungen.", vec!["A"])
    );

    let input = |title: &str, description: &str| SkillInput {
        title: title.into(),
        description: description.into(),
        ..Default::default()
    };
    let refused = |i: SkillInput| f.e.save_skill(&f.user, i).unwrap_err();
    assert!(matches!(refused(input(" ", "x")), CoreError::Invalid(_)));
    assert!(matches!(refused(input("Neu", " ")), CoreError::Invalid(_)));
    assert!(matches!(refused(input("!!!", "x")), CoreError::Invalid(_)));
    assert!(matches!(refused(input("code review!", "x")), CoreError::Conflict(_)));
    let other = f.skill("Andere", "x", &[]);
    let rename = SkillInput { id: Some(other.node.id), ..input("Code-Review", "x") };
    assert!(matches!(refused(rename), CoreError::Conflict(_)));
    // Saving under its own name is no conflict.
    let same = SkillInput { id: Some(created.node.id.clone()), ..input("Code Review", "x") };
    f.e.save_skill(&f.user, same).unwrap();
    let note = f.e.create_node(&f.user, NewNode { title: "Notiz".into(), ..Default::default() });
    let not_a_skill = SkillInput { id: Some(note.unwrap().id), ..input("Notiz", "x") };
    assert!(matches!(refused(not_a_skill), CoreError::Invalid(_)));
}

#[test]
fn skills_apply_like_templates_and_stay_out_of_the_graph() {
    let f = setup();
    let parent = f.project("Eltern", None);
    let child = f.project("Kind", Some(&parent));
    let other = f.project("Fremd", None);
    f.skill("Global", "überall", &[]);
    f.skill("Elternskill", "oben", &[&parent]);
    f.skill("Fremder Skill", "anderswo", &[&other]);

    assert_eq!(f.titles(None), ["Elternskill", "Fremder Skill", "Global"]);
    assert_eq!(f.titles(Some(&child)), ["Elternskill", "Global"]);
    assert!(f.e.graph_snapshot(None).unwrap().nodes.is_empty());
    let counts: Vec<i64> = f.e.projects().unwrap().iter().map(|p| p.node_count).collect();
    assert_eq!(counts, [0, 0, 0]);
    assert!(f.e.orphans(10).unwrap().is_empty());

    assert_eq!(f.e.project_delete_preview(&parent).unwrap().documents, 1);
    // A project's skill must not turn into a global one.
    f.e.delete_project(&f.user, &parent, false).unwrap();
    assert_eq!(f.titles(None), ["Fremder Skill", "Global"]);
}

#[test]
fn files_keep_their_paths_and_an_ai_changing_them_needs_a_review() {
    let f = setup();
    let skill = f.skill("PDF", "Formulare", &[]);
    let id = &skill.node.id;
    let add = |ctx: &Ctx, name: &str| f.e.add_attachment(ctx, id, name, None, b"print(1)");
    assert_eq!(add(&f.user, "scripts/fill.py").unwrap().filename, "scripts/fill.py");
    assert!(add(&f.user, "../fill.py").is_err());
    assert!(add(&f.user, "SKILL.md").is_err());
    let note = f.e.create_node(&f.user, NewNode { title: "Notiz".into(), ..Default::default() });
    let plain = f.e.add_attachment(&f.user, &note.unwrap().id, "a/b.txt", None, b"x").unwrap();
    assert_eq!(plain.filename, "b.txt", "other entries keep plain file names");
    assert!(f.e.skill(id).unwrap().reviewed);

    let ai = Ctx::new(f.e.ensure_actor(ActorKind::Ai, "KI").unwrap().id);
    add(&ai, "scripts/evil.sh").unwrap();
    let skill = f.e.skill(id).unwrap();
    assert!(!skill.reviewed, "a file an AI added must be looked at first");
    let paths: Vec<&str> = skill.files.iter().map(|a| a.filename.as_str()).collect();
    assert_eq!(paths, ["scripts/fill.py", "scripts/evil.sh"]);

    let seen = NodePatch { reviewed: Some(true), ..Default::default() };
    f.e.update_node(&f.user, id, seen).unwrap();
    f.e.delete_attachment(&ai, &skill.files[0].id).unwrap();
    assert!(!f.e.skill(id).unwrap().reviewed);
}

fn source(repo: &str, path: &str, revision: &str) -> SkillSource {
    SkillSource {
        url: format!("https://github.com/{repo}"),
        repo: repo.into(),
        path: path.into(),
        revision: revision.into(),
        imported_at: 1,
    }
}

fn fetched(
    title: &str,
    description: &str,
    files: &[(&str, &str)],
    from: SkillSource,
) -> SkillImport {
    SkillImport {
        title: title.into(),
        description: description.into(),
        body: format!("Anleitung für {title}"),
        files: files.iter().map(|(p, c)| (p.to_string(), c.as_bytes().to_vec())).collect(),
        source: from,
    }
}

#[test]
fn an_import_updates_in_place_and_goes_with_one_undo() {
    let f = setup();
    let work = f.project("Arbeit", None);
    let first = vec![
        fetched(
            "pdf",
            "PDFs",
            &[("scripts/a.py", "a"), ("ref.md", "r")],
            source("o/r", "skills/pdf", "1"),
        ),
        fetched("docx", "Word", &[], source("o/r", "skills/docx", "1")),
    ];
    let imported = f.e.import_skills(&f.user, first, std::slice::from_ref(&work)).unwrap();
    assert_eq!(imported.len(), 2);
    let pdf = &imported[0];
    assert_eq!((pdf.slug.as_str(), names(&pdf.projects)), ("pdf", vec!["Arbeit"]));
    assert!(pdf.reviewed);
    assert_eq!(pdf.source.as_ref().unwrap().path, "skills/pdf");
    assert_eq!(pdf.files.len(), 2);
    assert_eq!(
        f.e.skill_to_update("O/R", "skills/pdf", "anything").unwrap(),
        Some(pdf.node.id.clone())
    );
    assert_eq!(f.e.skill_to_update("x/y", "", "docx").unwrap(), Some(imported[1].node.id.clone()));
    assert_eq!(f.e.skill_to_update("x/y", "", "neu").unwrap(), None);

    // The user renames it and makes it global; an update keeps both.
    let renamed = SkillInput {
        id: Some(pdf.node.id.clone()),
        title: "PDF Formulare".into(),
        description: "PDFs".into(),
        body: pdf.body.clone(),
        projects: vec![],
    };
    f.e.save_skill(&f.user, renamed).unwrap();
    let ai = Ctx::new(f.e.ensure_actor(ActorKind::Ai, "KI").unwrap().id);
    f.e.update_node(
        &ai,
        &imported[1].node.id,
        NodePatch { body: Some(BodyEdit::Append("KI".into())), ..Default::default() },
    )
    .unwrap();

    let second = vec![
        fetched(
            "pdf",
            "PDFs, neu",
            &[("scripts/a.py", "a"), ("scripts/b.py", "b")],
            source("o/r", "skills/pdf", "2"),
        ),
        fetched("docx", "Word, neu", &[], source("andere/quelle", "", "9")),
    ];
    let updated = f.e.import_skills(&f.user, second, &[]).unwrap();
    let pdf = f.e.skill(&updated[0].node.id).unwrap();
    assert_eq!(pdf.node.id, imported[0].node.id, "updated in place");
    assert_eq!((pdf.node.title.as_str(), pdf.description.as_str()), ("PDF Formulare", "PDFs, neu"));
    assert!(pdf.projects.is_empty());
    let paths: Vec<&str> = pdf.files.iter().map(|a| a.filename.as_str()).collect();
    assert_eq!(paths, ["scripts/a.py", "scripts/b.py"]);
    assert_eq!(pdf.files[0].id, imported[0].files[0].id, "an unchanged file stays as it was");
    assert_eq!(pdf.source.as_ref().unwrap().revision, "2");
    let docx = f.e.skill(&imported[1].node.id).unwrap();
    assert_eq!(docx.source.as_ref().unwrap().repo, "andere/quelle", "found by its name");
    assert!(docx.reviewed, "an import is the user's own");
    assert_eq!(names(&docx.projects), ["Arbeit"]);

    f.undo_last();
    let docx = f.e.skill(&imported[1].node.id).unwrap();
    assert_eq!(docx.description, "Word");
    assert_eq!(f.e.skill(&imported[0].node.id).unwrap().files.len(), 2);
    assert_eq!(f.titles(None).len(), 2);

    let twice = vec![
        fetched("neu", "eins", &[], source("o/r", "a/neu", "3")),
        fetched("Neu", "zwei", &[], source("o/r", "b/neu", "3")),
    ];
    assert!(matches!(f.e.import_skills(&f.user, twice, &[]), Err(CoreError::Conflict(_))));
    assert_eq!(f.titles(None).len(), 2, "nothing of a failed import stays");
    let bad = vec![fetched("x", "y", &[("../x", "z")], source("o/r", "x", "3"))];
    assert!(matches!(f.e.import_skills(&f.user, bad, &[]), Err(CoreError::Invalid(_))));
}

#[test]
fn skills_go_through_an_export_and_come_back() {
    let from = setup();
    let work = from.project("Arbeit", None);
    let review = from.skill("Code Review", "Prüft „Code“: \"gründlich\".", &[&work]);
    from.e
        .add_attachment(&from.user, &review.node.id, "scripts/check.py", None, b"print(1)")
        .unwrap();
    from.e.add_attachment(&from.user, &review.node.id, "reference.md", None, b"# Mehr").unwrap();
    from.skill("Notizen schreiben", "Kurz und klar.", &[]);
    let note =
        NewNode { title: "Notiz".into(), projects: vec![work.clone()], ..Default::default() };
    from.e.create_node(&from.user, note).unwrap();

    let into = from.dir.path().join("out");
    fs::create_dir_all(&into).unwrap();
    let export = from.e.export_markdown(&into).unwrap();
    assert_eq!((export.entries, export.attachments), (3, 2));
    let root = Path::new(&export.folder);
    let text = fs::read_to_string(root.join("skills/code-review/SKILL.md")).unwrap();
    assert!(text.starts_with("---\nname: code-review\ndescription: "), "{text}");
    assert!(text.ends_with("# Code Review\n\nSchritte.\n"), "{text}");
    assert_eq!(fs::read(root.join("skills/code-review/scripts/check.py")).unwrap(), b"print(1)");
    assert!(root.join("skills/notizen-schreiben/SKILL.md").exists());
    assert!(!root.join("_Anhänge").exists(), "a skill's files stay in its folder");

    let to = setup();
    let summary = to.e.import_markdown(&to.user, root).unwrap();
    assert_eq!((summary.entries, summary.projects, summary.existing), (3, 1, 0));
    let skills = to.e.skills(None).unwrap();
    let titles: Vec<&str> = skills.iter().map(|s| s.node.title.as_str()).collect();
    assert_eq!(titles, ["Code Review", "Notizen schreiben"]);
    let review = &skills[0];
    assert_eq!(review.description, "Prüft „Code“: \"gründlich\".");
    assert_eq!(review.body, "# Code Review\n\nSchritte.");
    assert_eq!(names(&review.projects), ["Arbeit"]);
    assert!(skills[1].projects.is_empty());
    let mut files: Vec<(String, Vec<u8>)> = review
        .files
        .iter()
        .map(|a| (a.filename.clone(), to.e.attachment_data(&a.id).unwrap().1))
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            ("reference.md".to_string(), b"# Mehr".to_vec()),
            ("scripts/check.py".to_string(), b"print(1)".to_vec())
        ]
    );
    let projects: Vec<String> =
        to.e.projects().unwrap().into_iter().map(|p| p.project.name).collect();
    assert_eq!(projects, ["Arbeit"], "the skills folder is no project");

    let again = to.e.import_markdown(&to.user, root).unwrap();
    assert_eq!((again.entries, again.existing), (0, 3));
}

#[test]
fn a_folder_of_skills_from_elsewhere_comes_in_too() {
    let f = setup();
    let root = f.dir.path().join("Meine Skills");
    let write = |path: &str, text: &str| {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    };
    write(
        "pdf/SKILL.md",
        "---\r\nname: pdf\r\ndescription: >-\r\n  Fill PDF\r\n  forms.\r\n---\r\nSteps\r\n",
    );
    write("pdf/scripts/fill.py", "print(1)");
    write("pdf/.DS_Store", "x");
    write("notes.md", "Eine Notiz");
    write("ohne/SKILL.md", "---\nname: ohne\n---\nKeine Beschreibung");
    write("big/SKILL.md", "---\nname: big\ndescription: zu viel\n---\n");
    for i in 0..bm_core::skills::MAX_FILES {
        write(&format!("big/f{i}.txt"), "x");
    }

    let summary = f.e.import_markdown(&f.user, &root).unwrap();
    let skills = f.e.skills(None).unwrap();
    assert_eq!(skills.len(), 1);
    let pdf = &skills[0];
    assert_eq!((pdf.node.title.as_str(), pdf.description.as_str()), ("pdf", "Fill PDF forms."));
    assert_eq!(pdf.body, "Steps");
    assert_eq!(names(&pdf.projects), ["Meine Skills"]);
    let paths: Vec<&str> = pdf.files.iter().map(|a| a.filename.as_str()).collect();
    assert_eq!(paths, ["scripts/fill.py"]);
    // Without a description it is no skill, just a folder of notes.
    assert!(f.e.resolve_node("SKILL").is_ok());
    assert!(
        summary
            .skipped
            .iter()
            .any(|s| s.path == "big/SKILL.md" && s.reason == SkipReason::TooLarge)
    );
}
