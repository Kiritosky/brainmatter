//! `[[Wikilink]]` extraction from Markdown bodies.

/// How titles and names compare: trimmed and lowercased by Unicode, so `[[über]]`
/// finds „Über“. SQLite's `NOCASE` folds ASCII only; queries use `bm_fold` instead,
/// which is this function.
pub fn fold(text: &str) -> String {
    text.trim().to_lowercase()
}

/// Whether two titles or names count as the same.
pub fn same_title(a: &str, b: &str) -> bool {
    fold(a) == fold(b)
}

/// Distinct link targets in order of first appearance. Supports
/// `[[Title|alias]]` and `[[Title#heading]]`; ignores fenced code blocks and
/// inline code spans.
pub fn targets(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_fence = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        scan_line(line, &mut out);
    }
    out
}

fn scan_line(line: &str, out: &mut Vec<String>) {
    let mut rest = line;
    let mut in_code = false;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('`') {
            in_code = !in_code;
            rest = after;
            continue;
        }
        if !in_code
            && let Some(after) = rest.strip_prefix("[[")
            && let Some(end) = after.find("]]")
        {
            let inner = &after[..end];
            let target = inner.split(['|', '#']).next().unwrap_or("").trim();
            if !target.is_empty()
                && !target.contains('[')
                && !out.iter().any(|t| same_title(t, target))
            {
                out.push(target.to_string());
            }
            rest = &after[end + 2..];
            continue;
        }
        let mut chars = rest.chars();
        chars.next();
        rest = chars.as_str();
    }
}

#[cfg(test)]
mod tests {
    use super::targets;

    #[test]
    fn extracts_targets() {
        let body = "Siehe [[Tokio]] und [[Async Rust|async]], [[tokio]] doppelt.\n\
                    Mit Abschnitt [[Pin#Projektion]] und leer [[ ]].\n\
                    `[[kein link]]` im Code\n\
                    ```\n[[auch nicht]]\n```\n\
                    Umlaut [[Nebenläufigkeit]]";
        assert_eq!(targets(body), ["Tokio", "Async Rust", "Pin", "Nebenläufigkeit"]);
    }

    #[test]
    fn unterminated_link_is_ignored() {
        assert!(targets("[[offen und kaputt").is_empty());
    }
}
