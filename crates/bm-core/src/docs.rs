//! Project documentation: splitting pages into sections, and reading a folder of
//! Markdown files to mirror.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A heading of a page. AI clients read single sections to save tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocSection {
    /// Unique within the page.
    pub id: String,
    pub level: u8,
    pub title: String,
}

/// Headings with the byte offset of their line, skipping code blocks, where a
/// line starting with `#` is usually a comment.
fn headings(body: &str) -> Vec<(usize, u8, String)> {
    let mut out = Vec::new();
    let mut offset = 0;
    let mut fence: Option<&str> = None;
    for line in body.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let trimmed = line.trim_start();
        let marker = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m));
        match (fence, marker) {
            (None, Some(m)) => fence = Some(m),
            (Some(open), Some(m)) if open == m => fence = None,
            _ => {}
        }
        if fence.is_some() || marker.is_some() {
            continue;
        }
        let level = line.bytes().take_while(|b| *b == b'#').count();
        if (1..=6).contains(&level) && line[level..].starts_with([' ', '\t']) {
            let title = line[level..].trim().trim_end_matches('#').trim().to_string();
            if !title.is_empty() {
                out.push((start, level as u8, title));
            }
        }
    }
    out
}

fn section_id(title: &str, taken: &mut Vec<String>) -> String {
    let base = crate::templates::slug(title);
    let base = if base.is_empty() { "abschnitt".to_string() } else { base };
    let mut id = base.clone();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    taken.push(id.clone());
    id
}

pub fn sections(body: &str) -> Vec<DocSection> {
    let mut taken = Vec::new();
    headings(body)
        .into_iter()
        .map(|(_, level, title)| DocSection { id: section_id(&title, &mut taken), level, title })
        .collect()
}

/// One section with its subsections: up to the next heading of the same or a
/// higher level.
pub fn section_text<'a>(body: &'a str, id: &str) -> Option<&'a str> {
    let found = headings(body);
    let mut taken = Vec::new();
    let ids: Vec<String> = found.iter().map(|(_, _, t)| section_id(t, &mut taken)).collect();
    let index = ids.iter().position(|i| i == id)?;
    let (start, level, _) = &found[index];
    let end = found[index + 1..]
        .iter()
        .find(|(_, l, _)| l <= level)
        .map_or(body.len(), |(offset, _, _)| *offset);
    Some(body[*start..end].trim_end())
}

// --------------------------------------------------------------- mirroring ---

/// Limits, so that linking the wrong folder cannot fill the vault.
const MAX_FILES: usize = 2000;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const SKIPPED_DIRS: [&str; 6] = ["node_modules", "target", "dist", "build", "vendor", "venv"];
const INDEX_FILES: [&str; 3] = ["readme", "index", "_index"];

/// A page found in a mirrored folder or file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedPage {
    /// Relative to the source, with `/`. Identifies the page across syncs.
    pub path: String,
    /// The folder's page, if this is in a subfolder.
    pub parent: Option<String>,
    pub title: String,
    pub body: String,
    pub hash: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Scan {
    pub pages: Vec<ScannedPage>,
    /// Files passed over: too large, not UTF-8, or over the limit.
    pub skipped: usize,
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "md" | "markdown" | "mdx"))
}

fn stem(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("Seite").to_string()
}

fn read_text(path: &Path) -> Option<String> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    String::from_utf8(fs::read(path).ok()?).ok()
}

/// The first top-level heading, else the given fallback.
fn title_of(body: &str, fallback: &str) -> String {
    headings(body)
        .into_iter()
        .find(|(_, level, _)| *level == 1)
        .map_or_else(|| fallback.to_string(), |(_, _, title)| title)
}

fn page(path: String, parent: Option<String>, fallback: &str, body: String) -> ScannedPage {
    let title = title_of(&body, fallback);
    let mut digest = Sha256::new();
    digest.update(title.as_bytes());
    digest.update([0]);
    digest.update(body.as_bytes());
    let hash = digest.finalize().iter().map(|b| format!("{b:02x}")).collect();
    ScannedPage { path, parent, title, body, hash }
}

/// Reads a Markdown file, or a folder of them. A subfolder becomes a page with
/// its README as text, and the files in it become subpages. Never follows
/// symlinks, so a link inside the folder cannot pull in anything from outside.
pub fn scan(root: &Path) -> std::io::Result<Scan> {
    let mut out = Scan::default();
    let meta = fs::symlink_metadata(root)?;
    if meta.is_file() {
        if is_markdown(root)
            && let Some(body) = read_text(root)
        {
            let name = root.file_name().and_then(|n| n.to_str()).unwrap_or("Seite").to_string();
            out.pages.push(page(name, None, &stem(root), body));
        } else {
            out.skipped += 1;
        }
        return Ok(out);
    }
    scan_dir(root, "", None, &mut out)?;
    Ok(out)
}

/// Returns whether anything was found, since empty folders get no page.
fn scan_dir(dir: &Path, rel: &str, parent: Option<&str>, out: &mut Scan) -> std::io::Result<bool> {
    let mut entries: Vec<PathBuf> =
        fs::read_dir(dir)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    let join = |name: &str| if rel.is_empty() { name.to_string() } else { format!("{rel}/{name}") };

    let index = entries
        .iter()
        .find(|p| is_markdown(p) && INDEX_FILES.contains(&stem(p).to_ascii_lowercase().as_str()));
    let mut found = false;
    // In the root the README is simply the first page; there is no folder page.
    let own_page = !rel.is_empty();
    let position = out.pages.len();
    if let Some(path) = index {
        if let Some(body) = read_text(path) {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("README.md");
            let (key, fallback) = if own_page {
                (rel.to_string(), rel.rsplit('/').next().unwrap_or(rel).to_string())
            } else {
                (name.to_string(), stem(path))
            };
            out.pages.push(page(key, parent.map(str::to_string), &fallback, body));
            found = true;
        } else {
            out.skipped += 1;
        }
    }

    for path in &entries {
        if Some(path) == index {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        let Ok(meta) = fs::symlink_metadata(path) else { continue };
        if name.starts_with('.') {
            continue;
        }
        let this = if own_page { Some(rel.to_string()) } else { None };
        if meta.is_dir() {
            if !SKIPPED_DIRS.contains(&name) {
                found |= scan_dir(path, &join(name), this.as_deref(), out)?;
            }
        } else if meta.is_file() && is_markdown(path) {
            if out.pages.len() >= MAX_FILES {
                out.skipped += 1;
            } else if let Some(body) = read_text(path) {
                out.pages.push(page(join(name), this, &stem(path), body));
                found = true;
            } else {
                out.skipped += 1;
            }
        }
    }

    // A folder without a README still gets a page, to hold its subpages.
    if found && own_page && !out.pages[position..].iter().any(|p| p.path == rel) {
        let fallback = rel.rsplit('/').next().unwrap_or(rel).to_string();
        let folder = page(rel.to_string(), parent.map(str::to_string), &fallback, String::new());
        out.pages.insert(position, folder);
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "# Titel\nEinleitung\n\n## Installation\nSchritt 1\n\n### Mac\nbrew\n\n## Nutzung\nLos\n\n## Nutzung\nNochmal\n\n```sh\n# kein Abschnitt\n```\n";

    #[test]
    fn finds_sections_and_skips_code() {
        let found = sections(PAGE);
        let ids: Vec<_> = found.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["titel", "installation", "mac", "nutzung", "nutzung-2"]);
        assert_eq!(found[2].level, 3);
    }

    #[test]
    fn a_section_includes_its_subsections_and_ends_at_the_next_peer() {
        let text = section_text(PAGE, "installation").unwrap();
        assert!(text.starts_with("## Installation") && text.ends_with("brew"), "{text}");
        assert_eq!(section_text(PAGE, "mac").unwrap(), "### Mac\nbrew");
        assert!(section_text(PAGE, "nutzung-2").unwrap().contains("# kein Abschnitt"));
        assert_eq!(section_text(PAGE, "gibt-es-nicht"), None);
    }

    #[test]
    fn not_every_hash_starts_a_heading() {
        assert!(sections("#kein-titel\n####### zu tief\n#").is_empty());
    }

    fn write(root: &Path, path: &str, text: &str) {
        let full = root.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }

    #[test]
    fn a_folder_becomes_a_tree_of_pages() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "README.md", "# Mein Projekt\nStart");
        write(root, "setup.md", "Ohne Überschrift");
        write(root, "guide/README.md", "# Anleitung\nText");
        write(root, "guide/erste-schritte.md", "# Erste Schritte");
        write(root, "api/tief/referenz.md", "# Referenz");
        write(root, "bild.png", "keine Doku");
        write(root, ".git/config.md", "# versteckt");
        write(root, "node_modules/x/README.md", "# fremd");

        let found = scan(root).unwrap();
        let summary: Vec<_> = found
            .pages
            .iter()
            .map(|p| (p.path.as_str(), p.parent.as_deref(), p.title.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                ("README.md", None, "Mein Projekt"),
                ("api", None, "api"),
                ("api/tief", Some("api"), "tief"),
                ("api/tief/referenz.md", Some("api/tief"), "Referenz"),
                ("guide", None, "Anleitung"),
                ("guide/erste-schritte.md", Some("guide"), "Erste Schritte"),
                ("setup.md", None, "setup"),
            ]
        );
    }

    #[test]
    fn a_single_file_can_be_mirrored() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "README.md", "# Nur das");
        let found = scan(&dir.path().join("README.md")).unwrap();
        assert_eq!(found.pages[0].title, "Nur das");
        assert_eq!(found.pages[0].path, "README.md");
    }

    #[test]
    fn links_out_of_the_folder_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(outside.path(), "geheim.md", "# Geheim");
        write(dir.path(), "a.md", "# A");
        std::os::unix::fs::symlink(outside.path().join("geheim.md"), dir.path().join("link.md"))
            .unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("ordner")).unwrap();
        let found = scan(dir.path()).unwrap();
        assert_eq!(found.pages.len(), 1, "{:?}", found.pages);
        assert_eq!(found.pages[0].title, "A");
    }

    #[test]
    fn unreadable_and_huge_files_are_counted_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "gut.md", "# Gut");
        fs::write(dir.path().join("kaputt.md"), [0xff, 0xfe]).unwrap();
        fs::write(dir.path().join("riesig.md"), vec![b'x'; MAX_FILE_BYTES as usize + 1]).unwrap();
        let found = scan(dir.path()).unwrap();
        assert_eq!(found.pages.len(), 1);
        assert_eq!(found.skipped, 2);
    }

    #[test]
    fn hashes_change_with_the_content() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "a.md", "# A\neins");
        let first = scan(dir.path()).unwrap().pages[0].hash.clone();
        assert_eq!(scan(dir.path()).unwrap().pages[0].hash, first);
        write(dir.path(), "a.md", "# A\nzwei");
        assert_ne!(scan(dir.path()).unwrap().pages[0].hash, first);
    }
}
