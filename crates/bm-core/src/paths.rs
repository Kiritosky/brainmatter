//! Paths that entries mention, and whether they still exist on the Mac. Only a
//! file's existence and modification time are looked at, never its content.
//!
//! Paths count only inside the folders the user gave the entry's projects. Without
//! that limit, an AI client could write made-up paths and learn from the answer
//! which files exist.

use std::{
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

use crate::model::PathIssue;

/// At most this many paths are checked per entry.
const MAX_PATHS: usize = 40;

/// Characters that end a path in running text.
const DELIMITERS: &[char] =
    &['`', '(', ')', '[', ']', '"', '\'', ',', ';', '|', '“', '„', '”', '‘', '’', '«', '»'];

/// Characters that make a token a pattern, a placeholder or a URL part, not a path.
const NOT_IN_PATHS: &[char] = &['*', '?', '{', '}', '<', '>', '$', '%', '=', '@', '&', '#', '\\'];

/// Paths the Markdown mentions, as written and in order of first appearance:
/// in running text and inline code, not in fenced code blocks, which hold
/// commands and their output rather than statements about files.
pub fn mentioned(body: &str) -> Vec<String> {
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
        for token in line.split(|c: char| c.is_whitespace() || DELIMITERS.contains(&c)) {
            if let Some(path) = as_path(token)
                && !out.contains(&path)
            {
                out.push(path);
                if out.len() == MAX_PATHS {
                    return out;
                }
            }
        }
    }
    out
}

/// The path in a token, without the punctuation of the sentence around it or a
/// line number (`session.rs:311`). `None` for anything that does not look like a
/// file or folder.
fn as_path(token: &str) -> Option<String> {
    // Bold around it, and the end of a sentence after it; a leading dot stays
    // (`.github/…`).
    let token = token.trim_start_matches('*').trim_end_matches(['*', '.', ':', '!', '?']);
    let token = strip_line_number(token);
    if token.contains("://") || token.starts_with("www.") || token.contains(NOT_IN_PATHS) {
        return None;
    }
    let body = token.strip_prefix("~/").or_else(|| token.strip_prefix('/')).unwrap_or(token);
    if !token.contains('/') || body.is_empty() {
        return None;
    }
    let dir = body.ends_with('/');
    let segments: Vec<&str> = body.trim_end_matches('/').split('/').collect();
    if segments.iter().any(|s| s.is_empty() || *s == "..") {
        return None;
    }
    let names_a_file = segments.last().is_some_and(|last| has_extension(last));
    (dir || names_a_file).then(|| token.to_string())
}

fn strip_line_number(token: &str) -> &str {
    let mut rest = token;
    // `file.rs:12` and `file.rs:12:5`
    for _ in 0..2 {
        match rest.rsplit_once(':') {
            Some((head, digits))
                if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) =>
            {
                rest = head;
            }
            _ => break,
        }
    }
    rest
}

/// `name.ext` with a short extension that has a letter in it, or a dotfile.
/// Version numbers like `1.4.1` are no file names.
fn has_extension(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some(("", rest)) => !rest.is_empty(),
        Some((_, ext)) => {
            (1..=10).contains(&ext.len())
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
                && ext.chars().any(|c| c.is_ascii_alphabetic())
        }
        None => false,
    }
}

/// Checks paths an entry mentions against its project folders (`roots`).
/// A path is missing if it exists in none of them; relative paths only count
/// for folders that have their first part, so `src/x.rs` meant for a subfolder
/// is not reported. A file changed after `since`, the time the entry was last
/// edited or confirmed, counts as changed.
pub fn check(
    paths: &[String],
    roots: &[PathBuf],
    home: Option<&Path>,
    since: i64,
) -> Vec<PathIssue> {
    let mut issues = Vec::new();
    if roots.is_empty() {
        return issues;
    }
    for written in paths {
        let candidates = resolve(written, roots, home);
        if candidates.is_empty() {
            continue;
        }
        match candidates.iter().find_map(|p| std::fs::metadata(p).ok()) {
            None => issues.push(PathIssue { path: written.clone(), missing: true }),
            Some(meta) if meta.is_file() && modified_ms(&meta).is_some_and(|m| m > since) => {
                issues.push(PathIssue { path: written.clone(), missing: false });
            }
            Some(_) => {}
        }
    }
    issues
}

/// Where a written path may be, within the roots only.
fn resolve(written: &str, roots: &[PathBuf], home: Option<&Path>) -> Vec<PathBuf> {
    let absolute = match written.strip_prefix("~/") {
        Some(rest) => match home {
            Some(home) => Some(home.join(rest)),
            None => return Vec::new(),
        },
        None => written.starts_with('/').then(|| PathBuf::from(written)),
    };
    match absolute {
        Some(path) => {
            if roots.iter().any(|root| path.starts_with(root)) {
                vec![path]
            } else {
                Vec::new()
            }
        }
        None => {
            let relative = Path::new(written);
            let Some(Component::Normal(first)) = relative.components().next() else {
                return Vec::new();
            };
            roots
                .iter()
                .filter(|root| root.join(first).exists())
                .map(|root| root.join(relative))
                .collect()
        }
    }
}

fn modified_ms(meta: &std::fs::Metadata) -> Option<i64> {
    let since_epoch = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    i64::try_from(since_epoch.as_millis()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_paths_in_text_and_inline_code() {
        let body = "Die Logik steht in `app/src-tauri/src/session.rs:311`, das Skript in \
            scripts/release-notes.sh. Siehe auch (.github/workflows/ci.yml) und ~/brainmatter/README.md.\n\
            Ordner: `app/src-tauri/capabilities/`.";
        assert_eq!(
            mentioned(body),
            [
                "app/src-tauri/src/session.rs",
                "scripts/release-notes.sh",
                ".github/workflows/ci.yml",
                "~/brainmatter/README.md",
                "app/src-tauri/capabilities/",
            ]
        );
    }

    #[test]
    fn ignores_what_only_looks_like_a_path() {
        let body = "Kiritosky/homebrew-tap, refs/heads/main, und/oder, 1.4.0/1.4.1, \
            https://github.com/Kiritosky/brainmatter/blob/main/README.md, \
            `target/release/bundle/dmg/Brainmatter_*_aarch64.dmg`, \
            `docs/specs/YYYY-<topic>.md`, `binaries/brainmatter-mcp-$TARGET`, \
            ../../etc/hosts.txt, a//b.rs, Cargo.toml\n\
            ```\ncp target/release/x.rs out/\n```";
        assert_eq!(mentioned(body), Vec::<String>::new());
    }

    #[test]
    fn reports_missing_and_changed_files_inside_the_folder_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/kept.rs"), "").unwrap();
        let outside = dir.path().join("secret.txt");
        std::fs::write(&outside, "").unwrap();

        let paths = vec![
            "src/kept.rs".to_string(),
            "src/gone.rs".to_string(),
            // Not in the folder: neither reported nor looked at.
            "lib/other.rs".to_string(),
            outside.to_str().unwrap().to_string(),
            format!("{}/src/gone_too.rs", root.display()),
        ];
        let roots = vec![root.clone()];

        let before = check(&paths, &roots, None, 0);
        assert_eq!(
            before,
            [
                PathIssue { path: "src/kept.rs".into(), missing: false },
                PathIssue { path: "src/gone.rs".into(), missing: true },
                PathIssue { path: format!("{}/src/gone_too.rs", root.display()), missing: true },
            ]
        );

        let later = check(&paths, &roots, None, i64::MAX);
        assert!(later.iter().all(|i| i.missing), "unchanged since: {later:?}");
        assert!(check(&paths, &[], None, 0).is_empty());
    }

    #[test]
    fn home_paths_resolve_only_inside_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        let root = home.join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let paths = vec!["~/repo/missing.md".to_string(), "~/elsewhere/missing.md".to_string()];
        assert_eq!(
            check(&paths, &[root], Some(home), 0),
            [PathIssue { path: "~/repo/missing.md".into(), missing: true }]
        );
    }
}
