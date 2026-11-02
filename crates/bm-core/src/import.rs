//! Reads a folder of Markdown notes, as Obsidian, Logseq or any other tool leaves them,
//! or an export of Brainmatter itself, into a plan of entries and projects. Nothing is
//! written here; [`crate::engine::Engine::import_markdown`] carries the plan out.
//!
//! - Every `.md` file is an entry. Its title comes from `title` in its front matter, a
//!   Logseq `title::` property, or the file name.
//! - The folder itself and every folder inside it are projects.
//! - Front matter (or Logseq properties) give tags and the kind. A Brainmatter export also
//!   brings its typed links and further projects.
//! - Links to other files of the folder, as Markdown links or by path, become `[[Title]]`.

use std::{
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    error::Result,
    model::{DOC_KIND, TEMPLATE_KIND, WIKILINK_RELATION},
    wikilinks,
};

/// More than this is not a folder of notes.
pub const MAX_NOTES: usize = 50_000;
/// A note longer than this is most likely something else, such as a log.
pub const MAX_NOTE_BYTES: u64 = 2 * 1024 * 1024;

/// Kinds an imported note can have; anything else becomes a plain note.
const KINDS: &[&str] = &["note", "concept", "task", "person", "source", "decision", "snippet"];

/// Folders that tools keep for themselves, and the parts of a Brainmatter export that are
/// not entries of a project.
const IGNORED_FOLDERS: &[&str] =
    &["logseq", "node_modules", "_Anhänge", "_Doku", "_Vorlagen", "bak", "version-files"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    TooLarge,
    NotText,
    TooMany,
    /// Another file of the folder has the same title.
    SameTitle,
    /// The title is longer than an entry's may be.
    LongTitle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkippedFile {
    /// Relative to the folder imported.
    pub path: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFolder {
    pub name: String,
    /// The folder it is in; `None` for the folder imported, which comes first.
    pub parent: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedNote {
    /// Relative to the folder imported.
    pub path: String,
    pub folder: usize,
    pub title: String,
    pub body: String,
    pub kind: Option<String>,
    pub tags: Vec<String>,
    /// Typed links from a Brainmatter export: relation and the title linked to.
    pub links: Vec<(String, String)>,
    /// Further projects by name, from a Brainmatter export.
    pub projects: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportPlan {
    /// Whether the folder is an export of Brainmatter: then its folders are the projects
    /// it had, and the folder itself is none.
    pub brainmatter_export: bool,
    pub folders: Vec<PlannedFolder>,
    pub notes: Vec<PlannedNote>,
    pub skipped: Vec<SkippedFile>,
}

/// Reads `root` and everything in it.
pub fn plan(root: &Path) -> Result<ImportPlan> {
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Import".to_string());
    let mut plan =
        ImportPlan { folders: vec![PlannedFolder { name, parent: None }], ..Default::default() };
    let mut files = Vec::new();
    walk(root, root, 0, &mut plan, &mut files)?;

    // Titles first: links between the files are rewritten to them.
    let mut read = Vec::new();
    for (path, folder) in files {
        let relative = relative_name(root, &path);
        if read.len() >= MAX_NOTES {
            plan.skipped.push(SkippedFile { path: relative, reason: SkipReason::TooMany });
            continue;
        }
        let Ok(bytes) = fs::read(&path) else {
            plan.skipped.push(SkippedFile { path: relative, reason: SkipReason::NotText });
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            plan.skipped.push(SkippedFile { path: relative, reason: SkipReason::NotText });
            continue;
        };
        let note = parse(&text, &file_title(&path));
        read.push((path, folder, note));
    }

    let mut by_path = HashMap::new();
    let mut by_stem: HashMap<String, Option<String>> = HashMap::new();
    for (path, _, note) in &read {
        let key = wikilinks::fold(&relative_name(root, &path.with_extension("")));
        by_path.insert(key, note.title.clone());
        let stem = wikilinks::fold(&file_title(path));
        // A name two files share points at neither.
        by_stem
            .entry(stem)
            .and_modify(|t| {
                if t.as_deref() != Some(note.title.as_str()) {
                    *t = None
                }
            })
            .or_insert_with(|| Some(note.title.clone()));
    }
    let titles: std::collections::HashSet<String> =
        read.iter().map(|(_, _, n)| wikilinks::fold(&n.title)).collect();
    let links = Links { root, by_path, by_stem, titles };

    plan.brainmatter_export = !read.is_empty() && read.iter().all(|(_, _, n)| n.from_brainmatter);
    for (path, folder, note) in read {
        let dir = path.parent().unwrap_or(root);
        let body = links.rewrite(&note.body, dir);
        plan.notes.push(PlannedNote {
            path: relative_name(root, &path),
            folder,
            title: note.title,
            body,
            kind: note.kind,
            tags: note.tags,
            links: note.links,
            projects: note.projects,
        });
    }
    Ok(plan)
}

fn walk(
    root: &Path,
    dir: &Path,
    folder: usize,
    plan: &mut ImportPlan,
    files: &mut Vec<(PathBuf, usize)>,
) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        // Links are not followed: they may lead anywhere, or in a circle.
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if IGNORED_FOLDERS.iter().any(|i| i.eq_ignore_ascii_case(&name)) {
                continue;
            }
            plan.folders
                .push(PlannedFolder { name: name.trim().to_string(), parent: Some(folder) });
            let index = plan.folders.len() - 1;
            walk(root, &path, index, plan, files)?;
        } else if kind.is_file() && is_markdown(&path) {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if size > MAX_NOTE_BYTES {
                plan.skipped.push(SkippedFile {
                    path: relative_name(root, &path),
                    reason: SkipReason::TooLarge,
                });
            } else {
                files.push((path, folder));
            }
        }
    }
    Ok(())
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

fn relative_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace('\\', "/")
}

/// The title a file name stands for. Logseq writes `/` in a name as `___` or `%2F`.
fn file_title(path: &Path) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let title = percent_decode(&stem.replace("___", "/"));
    let title = title.trim();
    if title.is_empty() { "Ohne Titel".to_string() } else { title.to_string() }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = text.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(byte);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

/// One note, before links between the files are known.
#[derive(Debug, Default, PartialEq, Eq)]
struct Parsed {
    title: String,
    body: String,
    kind: Option<String>,
    tags: Vec<String>,
    links: Vec<(String, String)>,
    projects: Vec<String>,
    /// Written by Brainmatter's export, which puts these three in every file.
    from_brainmatter: bool,
}

fn parse(text: &str, fallback_title: &str) -> Parsed {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let (fields, rest) = match front_matter(text) {
        Some((fields, rest)) => (fields, rest),
        None => logseq_properties(text),
    };
    let text_of = |key: &str| fields.get(key).and_then(|v| v.as_text());
    let title = text_of("title").map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let from_brainmatter = ["id", "reviewed", "projects"].iter().all(|k| fields.contains_key(*k));
    // Brainmatter's own kinds are free, other tools' `type` often means something else.
    let kind =
        text_of("kind").or_else(|| text_of("type")).map(|k| k.trim().to_lowercase()).filter(|k| {
            match from_brainmatter {
                true => !k.is_empty() && k != TEMPLATE_KIND && k != DOC_KIND,
                false => KINDS.contains(&k.as_str()),
            }
        });

    let mut tags = Vec::new();
    for key in ["tags", "tag"] {
        if let Some(value) = fields.get(key) {
            for tag in value.items() {
                for part in tag.split([',', ' ']) {
                    let tag = part.trim().trim_start_matches('#').trim_matches(['[', ']']);
                    if !tag.is_empty() && !tags.iter().any(|t: &String| t.eq_ignore_ascii_case(tag))
                    {
                        tags.push(tag.to_string());
                    }
                }
            }
        }
    }
    let links = fields
        .get("links")
        .map(|v| v.objects())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|o| {
            let relation = o.get("relation")?.as_str()?.to_string();
            let to = o.get("to")?.as_str()?.trim().to_string();
            // Mentions come back from the `[[links]]` in the text.
            (relation != WIKILINK_RELATION && !to.is_empty()).then_some((relation, to))
        })
        .collect();
    let projects = fields.get("projects").map(|v| v.items()).unwrap_or_default();

    Parsed {
        from_brainmatter,
        title: title.unwrap_or_else(|| fallback_title.to_string()),
        body: rest.trim_start_matches(['\n', '\r']).trim_end().to_string(),
        kind,
        tags,
        links,
        projects,
    }
}

/// A value in front matter: text, a list of texts, or a list of objects (a Brainmatter
/// export writes its links so).
#[derive(Debug, Clone, PartialEq)]
enum Field {
    Text(String),
    List(Vec<Value>),
}

impl Field {
    fn as_text(&self) -> Option<&str> {
        match self {
            Field::Text(t) => Some(t),
            Field::List(_) => None,
        }
    }

    fn items(&self) -> Vec<String> {
        match self {
            Field::Text(t) => vec![t.clone()],
            Field::List(items) => items
                .iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .collect(),
        }
    }

    fn objects(&self) -> Vec<serde_json::Map<String, Value>> {
        match self {
            Field::List(items) => items.iter().filter_map(|v| v.as_object().cloned()).collect(),
            Field::Text(_) => Vec::new(),
        }
    }
}

/// `---` lines around `key: value` pairs, the common part of YAML that notes use.
fn front_matter(text: &str) -> Option<(HashMap<String, Field>, &str)> {
    let rest = text.strip_prefix("---\n").or_else(|| text.strip_prefix("---\r\n"))?;
    let mut end = None;
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        let bare = line.trim_end();
        if bare == "---" || bare == "..." {
            end = Some((at, at + line.len()));
            break;
        }
        at += line.len();
    }
    let (inner_end, after) = end?;
    let mut fields = HashMap::new();
    let mut open_list: Option<String> = None;
    for line in rest[..inner_end].lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let indented = line.starts_with([' ', '\t']);
        if let (true, Some(key)) = (indented || line.starts_with('-'), &open_list)
            && let Some(item) = line.trim_start().strip_prefix('-')
        {
            if let Some(Field::List(items)) = fields.get_mut(key) {
                items.push(scalar_value(item.trim()));
            }
            continue;
        }
        if indented {
            // Nested maps say nothing an entry has.
            continue;
        }
        open_list = None;
        let Some((key, value)) = line.split_once(':') else { continue };
        let key = key.trim().to_lowercase();
        let value = value.trim();
        if value.is_empty() {
            fields.insert(key.clone(), Field::List(Vec::new()));
            open_list = Some(key);
        } else if value.starts_with('[') {
            fields.insert(key, Field::List(flow_list(value)));
        } else {
            fields.insert(key, Field::Text(unquote(value)));
        }
    }
    Some((fields, &rest[after..]))
}

/// Logseq keeps a page's properties as `key:: value` lines at its top.
fn logseq_properties(text: &str) -> (HashMap<String, Field>, &str) {
    let mut fields = HashMap::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let bare = line.trim();
        let Some((key, value)) = bare.split_once(":: ") else { break };
        if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            break;
        }
        let items: Vec<Value> = value
            .split(',')
            .map(|v| v.trim().trim_start_matches("[[").trim_end_matches("]]").trim())
            .filter(|v| !v.is_empty())
            .map(|v| Value::String(v.to_string()))
            .collect();
        let field = match key {
            "tags" | "alias" => Field::List(items),
            _ => Field::Text(value.trim().to_string()),
        };
        fields.insert(key.to_lowercase(), field);
        at += line.len();
    }
    (fields, &text[at..])
}

fn unquote(value: &str) -> String {
    if value.starts_with('"')
        && let Ok(s) = serde_json::from_str::<String>(value)
    {
        return s;
    }
    if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        return inner.replace("''", "'");
    }
    value.to_string()
}

fn scalar_value(item: &str) -> Value {
    if item.starts_with('{')
        && let Ok(v) = serde_json::from_str::<Value>(item)
    {
        return v;
    }
    Value::String(unquote(item))
}

/// `[a, "b c", d]`, with or without quotes.
fn flow_list(value: &str) -> Vec<Value> {
    if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(value) {
        return items;
    }
    let inner = value.trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| Value::String(unquote(v)))
        .collect()
}

/// Turns links between the imported files into `[[Title]]`.
struct Links<'a> {
    root: &'a Path,
    /// Path without extension, relative to the root, folded.
    by_path: HashMap<String, String>,
    /// File name without extension, folded; `None` where it is not unique.
    by_stem: HashMap<String, Option<String>>,
    /// The titles of all notes, folded.
    titles: std::collections::HashSet<String>,
}

impl Links<'_> {
    fn rewrite(&self, body: &str, dir: &Path) -> String {
        let body = self.markdown_links(body, dir);
        self.wikilinks(&body)
    }

    /// `[text](other%20note.md)` to `[[Other note|text]]`.
    fn markdown_links(&self, body: &str, dir: &Path) -> String {
        let mut out = String::with_capacity(body.len());
        let mut rest = body;
        while let Some(open) = rest.find('[') {
            let (before, from) = rest.split_at(open);
            out.push_str(before);
            let image = before.ends_with('!');
            let Some(close) = from.find("](") else {
                out.push_str(from);
                return out;
            };
            let text = &from[1..close];
            let after = &from[close + 2..];
            let end = after.find(')');
            let target = end.map(|e| &after[..e]);
            let title = match target {
                Some(t) if !image && !text.contains(['\n', '[']) => self.target_of(t, dir),
                _ => None,
            };
            match (title, end) {
                (Some(title), Some(end)) => {
                    if wikilinks::same_title(text, &title) || text.trim().is_empty() {
                        out.push_str(&format!("[[{title}]]"));
                    } else {
                        out.push_str(&format!("[[{title}|{}]]", text.trim()));
                    }
                    rest = &after[end + 1..];
                }
                _ => {
                    out.push('[');
                    rest = &from[1..];
                }
            }
        }
        out.push_str(rest);
        out
    }

    fn target_of(&self, target: &str, dir: &Path) -> Option<String> {
        let target = target.trim().trim_start_matches('<').trim_end_matches('>');
        if target.contains("://") || target.starts_with('#') || target.starts_with("mailto:") {
            return None;
        }
        let path = percent_decode(target.split('#').next().unwrap_or(target));
        if !is_markdown(Path::new(&path)) {
            return None;
        }
        let joined = normalize(&dir.join(&path));
        let key = wikilinks::fold(&relative_name(self.root, &joined.with_extension("")));
        self.by_path.get(&key).cloned()
    }

    /// `[[folder/Note]]` and `[[file name]]` to the title of that note.
    fn wikilinks(&self, body: &str) -> String {
        let mut out = String::with_capacity(body.len());
        let mut rest = body;
        while let Some(open) = rest.find("[[") {
            out.push_str(&rest[..open + 2]);
            let inner_start = &rest[open + 2..];
            let Some(close) = inner_start.find("]]").filter(|c| !inner_start[..*c].contains('\n'))
            else {
                rest = inner_start;
                continue;
            };
            let inner = &inner_start[..close];
            let split = inner.find(['|', '#']).unwrap_or(inner.len());
            let (target, tail) = inner.split_at(split);
            match self.title_for(target.trim()) {
                Some(title) => {
                    out.push_str(&title);
                    out.push_str(tail);
                }
                None => out.push_str(inner),
            }
            out.push_str("]]");
            rest = &inner_start[close + 2..];
        }
        out.push_str(rest);
        out
    }

    fn title_for(&self, target: &str) -> Option<String> {
        let folded = wikilinks::fold(target.trim_end_matches(".md"));
        if folded.is_empty() || self.titles.contains(&folded) {
            return None;
        }
        if let Some(title) = self.by_path.get(&folded) {
            return Some(title.clone());
        }
        let stem = folded.rsplit('/').next().unwrap_or(&folded);
        self.by_stem.get(stem).cloned().flatten()
    }
}

/// `a/./b/../c` to `a/c`, without asking the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_gives_title_kind_tags_and_typed_links() {
        let text = "---\nid: \"x\"\ntitle: \"Über uns\"\nkind: \"Concept\"\ntags: [\"a\", \"#b\"]\nprojects: [\"P\", \"Q\"]\nlinks:\n  - {\"relation\":\"supports\",\"to\":\"Ziel\"}\n  - {\"relation\":\"mentions\",\"to\":\"Egal\"}\n---\n\nText [[Ziel]]\n";
        let p = parse(text, "datei");
        assert_eq!(p.title, "Über uns");
        assert_eq!(p.kind.as_deref(), Some("concept"));
        assert_eq!(p.tags, ["a", "b"]);
        assert_eq!(p.projects, ["P", "Q"]);
        assert_eq!(p.links, [("supports".to_string(), "Ziel".to_string())]);
        assert_eq!(p.body, "Text [[Ziel]]");
    }

    #[test]
    fn obsidian_and_logseq_styles() {
        let p = parse(
            "---\ntags:\n  - rust\n  - lernen\naliases: [x]\ntype: unbekannt\n---\nBody",
            "Name",
        );
        assert_eq!(
            (p.title.as_str(), p.kind, p.tags.clone()),
            ("Name", None, vec!["rust".into(), "lernen".into()])
        );
        let p = parse("tags:: [[eins]], zwei\ntitle:: Seite/Unter\n\n- Punkt\n", "datei");
        assert_eq!(p.title, "Seite/Unter");
        assert_eq!(p.tags, ["eins", "zwei"]);
        assert_eq!(p.body, "- Punkt");
        let p = parse("Kein: Front Matter\n---\n", "Datei");
        assert_eq!((p.title.as_str(), p.body.as_str()), ("Datei", "Kein: Front Matter\n---"));
        // A comma-separated string, as some tools write tags.
        assert_eq!(parse("---\ntags: a, b c\n---\n", "x").tags, ["a", "b", "c"]);
    }

    #[test]
    fn names_from_logseq_files() {
        assert_eq!(file_title(Path::new("pages/Projekt___Plan.md")), "Projekt/Plan");
        assert_eq!(file_title(Path::new("pages/a%2Fb%20c.md")), "a/b c");
        assert_eq!(file_title(Path::new("pages/ .md")), "Ohne Titel");
    }

    #[test]
    fn a_folder_becomes_projects_and_links_between_its_files_become_titles() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Notizen");
        fs::create_dir_all(root.join("Arbeit/Kunden")).unwrap();
        fs::create_dir_all(root.join(".obsidian")).unwrap();
        fs::create_dir_all(root.join("_Anhänge")).unwrap();
        fs::write(root.join(".obsidian/app.md"), "nicht").unwrap();
        fs::write(root.join("_Anhänge/a.md"), "nicht").unwrap();
        fs::write(root.join("Start.md"), "Siehe [Kunde A](Arbeit/Kunden/Kunde%20A.md), [[Kunden/Kunde A#Kontakt|den Kunden]], [[Plan]] und [Web](https://x.de/a.md) ![](bild.png)").unwrap();
        fs::write(root.join("Arbeit/Plan.md"), "---\ntitle: Der Plan\n---\nZurück zu [[Start]]")
            .unwrap();
        fs::write(root.join("Arbeit/Kunden/Kunde A.md"), "[hoch](../../Start.md)").unwrap();
        fs::write(root.join("Arbeit/bild.png"), [0xff, 0xd8]).unwrap();
        fs::write(root.join("Arbeit/kaputt.md"), [0xff, 0xfe, 0x00]).unwrap();

        let plan = plan(&root).unwrap();
        let folders: Vec<_> = plan.folders.iter().map(|f| (f.name.as_str(), f.parent)).collect();
        assert_eq!(folders, [("Notizen", None), ("Arbeit", Some(0)), ("Kunden", Some(1))]);
        let notes: HashMap<_, _> = plan.notes.iter().map(|n| (n.title.as_str(), n)).collect();
        assert_eq!(notes.len(), 3);
        assert_eq!(
            notes["Start"].body,
            "Siehe [[Kunde A]], [[Kunde A#Kontakt|den Kunden]], [[Der Plan]] und [Web](https://x.de/a.md) ![](bild.png)"
        );
        assert_eq!(notes["Start"].folder, 0);
        assert_eq!(notes["Der Plan"].folder, 1);
        assert_eq!(notes["Kunde A"].body, "[[Start|hoch]]");
        assert_eq!(notes["Kunde A"].folder, 2);
        assert_eq!(
            plan.skipped,
            [SkippedFile { path: "Arbeit/kaputt.md".into(), reason: SkipReason::NotText }]
        );
    }
}
