//! A readable, unencrypted copy of the vault, so nobody's notes are stuck in here:
//! one Markdown file per entry, in folders by project, `[[links]]` as written.
//! Other tools that understand front matter and wikilinks can open it directly.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::Engine;
use crate::{
    error::Result,
    ids::{NodeId, ProjectId},
    model::*,
    time::now_ms,
};

const NO_PROJECT: &str = "Ohne Projekt";
const DOCS: &str = "_Doku";
const TEMPLATES: &str = "_Vorlagen";
const ATTACHMENTS: &str = "_Anhänge";
const MAX_NAME_CHARS: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSummary {
    pub folder: String,
    pub entries: usize,
    pub attachments: usize,
    /// Attachments that could not be read, by file name.
    pub failed_attachments: Vec<String>,
}

/// A name that is safe as one path component, whatever a title contains.
fn safe_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() || r#"/\:*?"<>|"#.contains(c) { ' ' } else { c })
        .collect();
    let joined = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let short: String = joined.chars().take(MAX_NAME_CHARS).collect();
    let name = short.trim_matches(|c: char| c == '.' || c.is_whitespace());
    if name.is_empty() { "Ohne Titel".to_string() } else { name.to_string() }
}

/// Hands out names unique within a folder, ignoring case as macOS does.
#[derive(Default)]
struct Names {
    taken: HashMap<PathBuf, HashSet<String>>,
}

impl Names {
    fn claim(&mut self, folder: &Path, stem: &str, extension: &str) -> PathBuf {
        let taken = self.taken.entry(folder.to_path_buf()).or_default();
        let mut number = 1;
        loop {
            let name = match number {
                1 => format!("{stem}{extension}"),
                n => format!("{stem} ({n}){extension}"),
            };
            if taken.insert(name.to_lowercase()) {
                return folder.join(name);
            }
            number += 1;
        }
    }
}

/// `2026-11-01T09:30:00Z`, from milliseconds since the epoch.
fn iso_date(ms: i64) -> String {
    let seconds = ms.div_euclid(1000);
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    // Days to a civil date, after Howard Hinnant's `civil_from_days`.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// JSON strings and lists are valid YAML, and take care of all quoting.
fn front_matter(detail: &NodeDetail, attachments: &[String]) -> Result<String> {
    let node = &detail.node;
    let quote = |s: &str| serde_json::to_string(s);
    let projects: Vec<&str> = detail.projects.iter().map(|p| p.name.as_str()).collect();
    let mut out = String::from("---\n");
    out += &format!("id: {}\n", quote(node.id.as_str())?);
    out += &format!("title: {}\n", quote(&node.title)?);
    out += &format!("kind: {}\n", quote(&node.kind)?);
    out += &format!("created: {}\n", iso_date(node.created_at));
    out += &format!("updated: {}\n", iso_date(node.updated_at));
    out += &format!("reviewed: {}\n", node.reviewed);
    out += &format!("tags: {}\n", serde_json::to_string(&node.tags)?);
    out += &format!("projects: {}\n", serde_json::to_string(&projects)?);
    if !detail.outgoing.is_empty() {
        out += "links:\n";
        for e in &detail.outgoing {
            let link = json!({"relation": e.edge.relation, "to": e.other.title});
            out += &format!("  - {link}\n");
        }
    }
    if !attachments.is_empty() {
        out += &format!("attachments: {}\n", serde_json::to_string(attachments)?);
    }
    if !detail.file_links.is_empty() {
        let paths: Vec<&str> = detail.file_links.iter().map(|f| f.path.as_str()).collect();
        out += &format!("files: {}\n", serde_json::to_string(&paths)?);
    }
    out += "---\n\n";
    Ok(out)
}

fn project_folders(
    projects: &[ProjectSummary],
    root: &Path,
    names: &mut Names,
) -> HashMap<ProjectId, PathBuf> {
    let mut folders: HashMap<ProjectId, PathBuf> = HashMap::new();
    let mut waiting: Vec<&Project> = projects.iter().map(|p| &p.project).collect();
    // Parents first, however the list is sorted. Whatever is left when nothing moves
    // anymore has a parent outside the list, and goes to the top.
    while !waiting.is_empty() {
        let before = waiting.len();
        waiting.retain(|p| {
            let parent = match &p.parent_id {
                None => root.to_path_buf(),
                Some(id) => match folders.get(id) {
                    Some(folder) => folder.clone(),
                    None => return true,
                },
            };
            folders.insert(p.id.clone(), names.claim(&parent, &safe_name(&p.name), ""));
            false
        });
        if waiting.len() == before {
            for p in waiting.drain(..) {
                folders.insert(p.id.clone(), names.claim(root, &safe_name(&p.name), ""));
            }
        }
    }
    folders
}

impl Engine {
    /// Writes everything but the trash to a new folder inside `into`. The folder
    /// appears only when complete, and no plain copy is left behind on failure.
    pub fn export_markdown(&self, into: &Path) -> Result<ExportSummary> {
        let date = &iso_date(now_ms())[..10];
        let base = format!("Brainmatter-Export-{date}");
        let mut name = base.clone();
        let mut number = 1;
        while into.join(&name).exists() {
            number += 1;
            name = format!("{base} ({number})");
        }
        let partial = into.join(format!(".{name}.partial"));
        if partial.exists() {
            fs::remove_dir_all(&partial)?;
        }
        fs::create_dir_all(&partial)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&partial, fs::Permissions::from_mode(0o700))?;
        }

        let written = self.write_export(&partial);
        let finished = written.and_then(|mut summary| {
            let target = into.join(&name);
            fs::rename(&partial, &target)?;
            summary.folder = target.to_string_lossy().into_owned();
            Ok(summary)
        });
        if finished.is_err() {
            let _ = fs::remove_dir_all(&partial);
        }
        finished
    }

    fn write_export(&self, root: &Path) -> Result<ExportSummary> {
        let ids: Vec<NodeId> = self.read(|c| {
            let mut stmt =
                c.prepare("SELECT id FROM nodes WHERE deleted_at IS NULL ORDER BY id")?;
            let rows = stmt.query_map([], |r| r.get::<_, NodeId>(0))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })?;
        let mut names = Names::default();
        let projects = self.projects()?;
        let folders = project_folders(&projects, root, &mut names);
        for folder in folders.values() {
            fs::create_dir_all(folder)?;
        }

        let mut summary = ExportSummary {
            folder: String::new(),
            entries: 0,
            attachments: 0,
            failed_attachments: Vec::new(),
        };
        let attachments_dir = root.join(ATTACHMENTS);
        for id in &ids {
            let detail = self.node_detail(id)?;
            let home = detail
                .projects
                .first()
                .and_then(|p| folders.get(&p.id))
                .cloned()
                .unwrap_or_else(|| root.join(NO_PROJECT));
            let folder = match detail.node.kind.as_str() {
                "doc" => home.join(DOCS),
                "template" => root.join(TEMPLATES),
                _ => home,
            };
            fs::create_dir_all(&folder)?;

            let mut saved = Vec::new();
            for attachment in &detail.attachments {
                match self.attachment_data(&attachment.id) {
                    Ok((_, bytes)) => {
                        fs::create_dir_all(&attachments_dir)?;
                        let path =
                            names.claim(&attachments_dir, &safe_name(&attachment.filename), "");
                        fs::write(&path, bytes)?;
                        let relative = path.strip_prefix(root).unwrap_or(&path);
                        saved.push(relative.to_string_lossy().into_owned());
                        summary.attachments += 1;
                    }
                    Err(_) => summary.failed_attachments.push(attachment.filename.clone()),
                }
            }

            let path = names.claim(&folder, &safe_name(&detail.node.title), ".md");
            fs::write(path, front_matter(&detail, &saved)? + &detail.node.body)?;
            summary.entries += 1;
        }
        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_cannot_leave_their_folder() {
        assert_eq!(safe_name("../../etc/passwd"), "etc passwd");
        assert_eq!(safe_name(".."), "Ohne Titel");
        assert_eq!(safe_name("  a:b\n\tc "), "a b c");
        assert_eq!(safe_name(&"x".repeat(500)).chars().count(), MAX_NAME_CHARS);
    }

    #[test]
    fn names_stay_unique_ignoring_case() {
        let mut names = Names::default();
        let folder = Path::new("f");
        assert_eq!(names.claim(folder, "Notiz", ".md"), folder.join("Notiz.md"));
        assert_eq!(names.claim(folder, "notiz", ".md"), folder.join("notiz (2).md"));
        assert_eq!(names.claim(Path::new("g"), "Notiz", ".md"), Path::new("g").join("Notiz.md"));
    }

    #[test]
    fn dates_are_right_across_leap_years() {
        assert_eq!(iso_date(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_date(1_709_210_096_000), "2024-02-29T12:34:56Z");
        assert_eq!(iso_date(1_793_491_200_000), "2026-11-01T00:00:00Z");
    }
}
