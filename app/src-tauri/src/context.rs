//! A project as Markdown to paste into a chat app without MCP: its description,
//! its most important entries in full and its open tasks, short enough for an AI
//! to read in one go.

use std::fmt::Write;

use bm_core::{Engine, error::Result, ids::ProjectId};

use crate::i18n::{german, tr};

/// About 6,000 tokens. Entries go in whole or not at all.
pub const MAX_CHARS: usize = 24_000;

pub fn project_context(engine: &Engine, project: &ProjectId) -> Result<String> {
    let o = engine.project_overview(project)?;
    let names: Vec<&str> =
        o.path.iter().map(|p| p.name.as_str()).chain([o.project.name.as_str()]).collect();
    let today = bm_core::time::now_ms();
    let mut out = format!("# {}\n\n", names.join(" › "));
    let _ = writeln!(
        out,
        "> {}\n",
        if german() {
            format!(
                "Kontext aus Brainmatter vom {}: {} Einträge, hier die wichtigsten.",
                date(today, true),
                o.node_count
            )
        } else {
            format!(
                "Context from Brainmatter, {}: {} entries, the most important ones below.",
                date(today, false),
                o.node_count
            )
        }
    );
    if !o.project.description.trim().is_empty() {
        let _ = writeln!(out, "{}\n", o.project.description.trim());
    }
    if !o.subprojects.is_empty() {
        let subs: Vec<&str> = o.subprojects.iter().map(|s| s.project.name.as_str()).collect();
        let _ = writeln!(out, "{} {}\n", tr("Unterprojekte:", "Subprojects:"), subs.join(", "));
    }
    let pages = engine.doc_tree(project)?;
    if !pages.is_empty() {
        let titles: Vec<&str> = pages.iter().map(|p| p.node.title.as_str()).collect();
        let _ =
            writeln!(out, "{} {}\n", tr("Doku-Seiten:", "Documentation pages:"), titles.join(", "));
    }

    let mut tasks = String::new();
    if !o.tasks.is_empty() {
        let _ = writeln!(tasks, "## {}\n", tr("Offene Aufgaben", "Open tasks"));
        for t in &o.tasks {
            let _ = writeln!(tasks, "- {}", t.title);
        }
        tasks.push('\n');
    }

    // The most linked entries first, then the latest: a project with few links
    // still gets content.
    let mut candidates: Vec<&bm_core::ids::NodeId> = Vec::new();
    for id in o.hubs.iter().map(|(b, _)| &b.id).chain(o.recent.iter().map(|b| &b.id)) {
        if !candidates.contains(&id) {
            candidates.push(id);
        }
    }
    let mut left_out = 0;
    let mut entries = String::new();
    for id in candidates {
        let Some(node) = engine.node(id)? else { continue };
        let mut entry = format!("### {} ({})\n\n", node.title, node.kind);
        let body = demote_headings(node.body.trim());
        if !body.is_empty() {
            let _ = writeln!(entry, "{body}\n");
        }
        if out.len() + entries.len() + entry.len() + tasks.len() > MAX_CHARS {
            left_out += 1;
            continue;
        }
        entries.push_str(&entry);
    }
    if !entries.is_empty() {
        let _ = writeln!(out, "## {}\n", tr("Wichtigste Einträge", "Most important entries"));
        out.push_str(&entries);
    }
    out.push_str(&tasks);
    if left_out > 0 {
        let note = if german() {
            format!("_{left_out} weitere wichtige Einträge passten nicht hinein._\n")
        } else {
            format!("_{left_out} more important entries did not fit._\n")
        };
        out.push_str(&note);
    }
    Ok(out.trim_end().to_string() + "\n")
}

/// Headings inside an entry move two levels down, below its own `###` title.
fn demote_headings(body: &str) -> String {
    let mut in_fence = false;
    let mut out = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
        }
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        let heading = !in_fence && (1..=6).contains(&level) && trimmed[level..].starts_with(' ');
        if heading {
            let deeper = "#".repeat((level + 2).min(6));
            out.push(format!("{deeper}{}", &trimmed[level..]));
        } else {
            out.push(line.to_string());
        }
    }
    out.join("\n")
}

/// `13.11.2026` or `2026-11-13`, in UTC like the rest of the app's dates for AIs.
fn date(ms: i64, german: bool) -> String {
    let days = ms.div_euclid(86_400_000);
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    if german { format!("{d}.{m}.{y}") } else { format!("{y:04}-{m:02}-{d:02}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_in_entries_move_below_the_entry_title() {
        let body = "## Ablauf\nText\n```\n# kein Titel\n```\n#hashtag\n###### tief";
        assert_eq!(
            demote_headings(body),
            "#### Ablauf\nText\n```\n# kein Titel\n```\n#hashtag\n###### tief"
        );
    }

    #[test]
    fn dates_in_both_languages() {
        assert_eq!(date(1_793_345_400_000, true), "30.10.2026");
        assert_eq!(date(1_793_345_400_000, false), "2026-10-30");
    }
}
