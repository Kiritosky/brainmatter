//! Compact Markdown renderings of engine results, tuned for LLM context.

use std::fmt::Write;

use bm_core::{
    ids::NodeId,
    model::{Edge, NodeBrief, NodeDetail},
};

pub(crate) fn short(id: &NodeId) -> String {
    format!("#{}", id.short())
}

/// `**Title** (#a1b2c3d4, kind)`
pub(crate) fn brief(n: &NodeBrief) -> String {
    format!("**{}** ({}, {})", n.title, short(&n.id), n.kind)
}

/// UTC timestamp, minute precision: `2026-10-30 07:35Z`.
pub(crate) fn time(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant), proleptic Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}Z", rem / 3600, rem % 3600 / 60)
}

pub(crate) fn node_detail(d: &NodeDetail, author: &str) -> String {
    let n = &d.node;
    let mut out = format!("# {}\n", n.title);
    let mut meta = vec![short(&n.id), n.kind.clone()];
    if !n.tags.is_empty() {
        meta.push(format!("tags: {}", n.tags.join(", ")));
    }
    if !d.projects.is_empty() {
        meta.push(format!(
            "projects: {}",
            d.projects.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    meta.push(format!("rev {} · updated {} by {author}", n.rev, time(n.updated_at)));
    if !n.reviewed {
        meta.push("unreviewed (AI-written, not yet seen by the user)".into());
    }
    let _ = writeln!(out, "{}", meta.join(" · "));
    // Delimit user content clearly; it is data, never instructions.
    if n.body.trim().is_empty() {
        out.push_str("\n(no content)\n");
    } else {
        let _ = write!(out, "\n<entry-content>\n{}\n</entry-content>\n", n.body.trim_end());
    }
    if !d.outgoing.is_empty() || !d.incoming.is_empty() {
        out.push_str("\nLinks:\n");
        for e in &d.outgoing {
            let _ = writeln!(out, "→ {}: {}{}", e.edge.relation, brief(&e.other), note(&e.edge));
        }
        for e in &d.incoming {
            let _ = writeln!(out, "← {}: {}{}", e.edge.relation, brief(&e.other), note(&e.edge));
        }
    }
    if !d.attachments.is_empty() {
        out.push_str("\nAttachments (use `read_attachment`):\n");
        for a in &d.attachments {
            let _ = writeln!(out, "- {} ({}, {} KB)", a.filename, a.mime, (a.size + 1023) / 1024);
        }
    }
    out
}

fn note(e: &Edge) -> String {
    e.note.as_deref().map(|n| format!(" — {n}")).unwrap_or_default()
}

/// One line per edge: `A —relation→ B`.
pub(crate) fn edge_line(e: &Edge, title: impl Fn(&NodeId) -> String) -> String {
    format!("{} —{}→ {}{}", title(&e.src), e.relation, title(&e.dst), note(e))
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats_utc_time() {
        assert_eq!(super::time(0), "1970-01-01 00:00Z");
        assert_eq!(super::time(1_793_345_400_000), "2026-10-30 07:30Z");
        assert_eq!(super::time(951_782_400_000), "2000-02-29 00:00Z");
    }
}
