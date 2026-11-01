use std::{fs, path::Path};

use bm_core::{Engine, Vault, blobs::BlobStore, model::*, vault::KdfParams};

struct Fixture {
    dir: tempfile::TempDir,
    engine: Engine,
    user: Ctx,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
    let blobs = BlobStore::new(dir.path().join("blobs"), &keys.blob_key());
    let engine = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap().with_blobs(blobs);
    let user = Ctx::new(engine.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
    Fixture { dir, engine, user }
}

impl Fixture {
    fn project(&self, name: &str, parent: Option<&bm_core::ids::ProjectId>) -> bm_core::ids::ProjectId {
        let input = NewProject { name: name.into(), parent: parent.cloned(), ..Default::default() };
        self.engine.create_project(&self.user, input).unwrap().id
    }

    fn note(&self, title: &str, body: &str, projects: Vec<bm_core::ids::ProjectId>) -> Node {
        let input = NewNode { title: title.into(), body: body.into(), projects, ..Default::default() };
        self.engine.create_node(&self.user, input).unwrap()
    }

    fn export(&self) -> (std::path::PathBuf, ExportSummary) {
        let into = self.dir.path().join("out");
        fs::create_dir_all(&into).unwrap();
        let summary = self.engine.export_markdown(&into).unwrap();
        (Path::new(&summary.folder).to_path_buf(), summary)
    }
}

use bm_core::engine::ExportSummary;

#[test]
fn entries_end_up_in_their_project_folders_with_their_data() {
    let f = setup();
    let parent = f.project("Arbeit", None);
    let child = f.project("Kunden", Some(&parent));
    let a = f.note("Angebot", "Siehe [[Rechnung]].", vec![child.clone()]);
    let b = f.note("Rechnung", "Offen.", vec![parent.clone()]);
    f.note("Lose Notiz", "Ohne alles.", vec![]);
    f.engine
        .link(&f.user, NewEdge { src: a.id.clone(), dst: b.id.clone(), relation: Some("depends_on".into()), weight: None, note: None })
        .unwrap();
    f.engine.create_doc_page(&f.user, &parent, "Handbuch", "# Text", None, vec!["intern".into()]).unwrap();

    let (root, summary) = f.export();
    assert_eq!(summary.entries, 4);

    let offer = fs::read_to_string(root.join("Arbeit/Kunden/Angebot.md")).unwrap();
    assert!(offer.starts_with("---\nid: \""), "{offer}");
    assert!(offer.contains("title: \"Angebot\""), "{offer}");
    assert!(offer.contains("projects: [\"Kunden\"]"), "{offer}");
    assert!(offer.contains(r#"{"relation":"depends_on","to":"Rechnung"}"#), "{offer}");
    assert!(offer.ends_with("---\n\nSiehe [[Rechnung]]."), "{offer}");

    assert!(root.join("Arbeit/Rechnung.md").is_file());
    assert!(root.join("Ohne Projekt/Lose Notiz.md").is_file());
    let page = fs::read_to_string(root.join("Arbeit/_Doku/Handbuch.md")).unwrap();
    assert!(page.contains("tags: [\"intern\"]") && page.ends_with("# Text"), "{page}");
}

#[test]
fn hostile_and_repeated_titles_stay_inside_the_export() {
    let f = setup();
    f.note("../../ausgebrochen", "x", vec![]);
    f.note("Gleich", "eins", vec![]);
    f.note("gleich", "zwei", vec![]);
    let quoted = "Zeile\neins: \"zwei\" --- ende";
    f.note(quoted, "y", vec![]);

    let (root, summary) = f.export();
    assert_eq!(summary.entries, 4);
    assert!(!f.dir.path().join("ausgebrochen.md").exists());
    let folder = root.join("Ohne Projekt");
    assert!(folder.join("ausgebrochen.md").is_file(), "{:?}", fs::read_dir(&folder).unwrap().count());
    let names: Vec<String> =
        fs::read_dir(&folder).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
    assert_eq!(names.len(), 4, "{names:?}");

    // The odd title is one quoted YAML line, so it cannot end the front matter.
    let odd = names.iter().find(|n| n.starts_with("Zeile")).unwrap();
    let text = fs::read_to_string(folder.join(odd)).unwrap();
    assert!(text.contains(r#"title: "Zeile\neins: \"zwei\" --- ende""#), "{text}");
}

#[test]
fn attachments_are_decrypted_and_the_trash_stays_out() {
    let f = setup();
    let keep = f.note("Mit Datei", "", vec![]);
    f.engine.add_attachment(&f.user, &keep.id, "bild.png", None, b"PNG-INHALT").unwrap();
    let gone = f.note("Weg", "", vec![]);
    f.engine.delete_node(&f.user, &gone.id).unwrap();

    let (root, summary) = f.export();
    assert_eq!((summary.entries, summary.attachments), (1, 1));
    assert!(summary.failed_attachments.is_empty());
    assert_eq!(fs::read(root.join("_Anhänge/bild.png")).unwrap(), b"PNG-INHALT");
    let text = fs::read_to_string(root.join("Ohne Projekt/Mit Datei.md")).unwrap();
    assert!(text.contains(r#"attachments: ["_Anhänge/bild.png"]"#), "{text}");
    assert!(!root.join("Ohne Projekt/Weg.md").exists());
}

#[test]
fn a_second_export_never_touches_the_first_and_nothing_partial_remains() {
    let f = setup();
    f.note("Eins", "alt", vec![]);
    let (first, _) = f.export();
    fs::write(first.join("Ohne Projekt/Eigene Änderung.md"), "meins").unwrap();

    let (second, _) = f.export();
    assert_ne!(first, second);
    assert!(first.join("Ohne Projekt/Eigene Änderung.md").is_file());
    let out = first.parent().unwrap();
    let left: Vec<_> = fs::read_dir(out)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".partial"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}
