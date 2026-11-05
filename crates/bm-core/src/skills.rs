//! Skills in the open Agent Skills format: a folder with a `SKILL.md`, whose front
//! matter gives `name` and `description`, and whatever files its instructions use.
//!
//! Nothing here touches the network. The app fetches; this decides what a skill is,
//! which URLs lead to one and which files belong to it.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

pub const SKILL_FILE: &str = "SKILL.md";
/// More than this in one preview is a collection to pick a folder from.
pub const MAX_SKILLS: usize = 50;
/// Files per skill, its `SKILL.md` included.
pub const MAX_FILES: usize = 100;
/// Bytes per skill, its `SKILL.md` included.
pub const MAX_SKILL_BYTES: u64 = 5 * 1024 * 1024;
/// The format allows no longer `name`.
const MAX_NAME_CHARS: usize = 64;
const MAX_PATH_CHARS: usize = 300;

// ------------------------------------------------------------ SKILL.md ---

/// What a `SKILL.md` says. Missing parts are empty rather than an error: whether
/// that still makes a skill is for the caller to decide.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillText {
    pub name: Option<String>,
    pub description: String,
    pub body: String,
    /// `metadata.title`, which Brainmatter's export writes for the display name.
    pub title: Option<String>,
    /// `metadata.projects`, one name per line, also from the export.
    pub projects: Vec<String>,
    /// `metadata.managed-by`: who wrote the folder and may change it; see [`render_managed`].
    pub managed_by: Option<String>,
}

/// The `metadata` key that marks a folder as written by an app.
const MANAGED_BY: &str = "managed-by";

pub fn parse(text: &str) -> SkillText {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text).replace("\r\n", "\n");
    let Some((front, body)) = split_front_matter(&text) else {
        return SkillText { body: text.trim().to_string(), ..Default::default() };
    };
    let lines: Vec<&str> = front.lines().collect();
    let mut out = SkillText { body: body.trim().to_string(), ..Default::default() };
    for (key, value, children) in fields(&lines) {
        match key.as_str() {
            "name" => out.name = Some(value).filter(|v| !v.is_empty()),
            "description" => out.description = value,
            "metadata" => {
                let inner = dedent(&children);
                let inner: Vec<&str> = inner.iter().map(String::as_str).collect();
                for (key, value, _) in fields(&inner) {
                    match key.as_str() {
                        "title" => out.title = Some(value).filter(|v| !v.is_empty()),
                        MANAGED_BY => out.managed_by = Some(value).filter(|v| !v.is_empty()),
                        "projects" => {
                            out.projects = value
                                .lines()
                                .map(str::trim)
                                .filter(|p| !p.is_empty())
                                .map(str::to_string)
                                .collect()
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// A `SKILL.md` that other tools read as well. `name` must follow the format already.
pub fn render(
    name: &str,
    description: &str,
    title: &str,
    projects: &[String],
    body: &str,
) -> String {
    let mut metadata = vec![("title", title.to_string())];
    if !projects.is_empty() {
        metadata.push(("projects", projects.join("\n")));
    }
    with_front_matter(name, description, &metadata, body)
}

/// A `SKILL.md` for a folder that `owner` keeps up to date in another app's skill
/// folder. The mark tells it which folders are its own to change or remove.
pub fn render_managed(name: &str, description: &str, owner: &str, body: &str) -> String {
    with_front_matter(name, description, &[(MANAGED_BY, owner.to_string())], body)
}

fn with_front_matter(
    name: &str,
    description: &str,
    metadata: &[(&str, String)],
    body: &str,
) -> String {
    let mut out = format!("---\nname: {name}\ndescription: {}\n", quote(description.trim()));
    out += "metadata:\n";
    for (key, value) in metadata {
        out += &format!("  {key}: {}\n", quote(value));
    }
    out += "---\n\n";
    out += body.trim();
    out.push('\n');
    out
}

/// A double-quoted YAML scalar on one line. Only printable characters stay as they
/// are; YAML forbids the others, and line separators would end the line.
fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            ' '..='~' => out.push(c),
            '\u{a0}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}'
                if !matches!(c, '\u{2028}' | '\u{2029}' | '\u{feff}') =>
            {
                out.push(c)
            }
            // Never above U+FFFF here, so four digits are enough.
            c => out.push_str(&format!("\\u{:04x}", c as u32)),
        }
    }
    out.push('"');
    out
}

/// A `name` as the format wants it: lowercase letters, digits and dashes.
pub fn name_for(title: &str) -> String {
    let slug = crate::templates::slug(title);
    let short: String = slug.chars().take(MAX_NAME_CHARS).collect();
    short.trim_end_matches('-').to_string()
}

fn split_front_matter(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n")?;
    let mut at = 0;
    for line in rest.split_inclusive('\n') {
        let bare = line.trim_end();
        if bare == "---" || bare == "..." {
            return Some((&rest[..at], &rest[at + line.len()..]));
        }
        at += line.len();
    }
    None
}

/// Top-level `key: value` pairs of a block of YAML, each with the indented lines
/// that belong to it. Values spread over several lines are joined.
fn fields<'a>(lines: &[&'a str]) -> Vec<(String, String, Vec<&'a str>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.trim().is_empty() || line.trim_start().starts_with('#') || indented(line) {
            continue;
        }
        let Some((key, rest)) = split_key(line) else { continue };
        let start = i;
        while i < lines.len() && (lines[i].trim().is_empty() || indented(lines[i])) {
            i += 1;
        }
        let children = lines[start..i].to_vec();
        out.push((key, scalar(rest, &children), children));
    }
    out
}

fn indented(line: &str) -> bool {
    line.starts_with([' ', '\t'])
}

fn split_key(line: &str) -> Option<(String, &str)> {
    let colon = line.char_indices().find(|&(i, c)| {
        c == ':' && line[i + 1..].chars().next().is_none_or(|n| n == ' ' || n == '\t')
    })?;
    let key = line[..colon.0].trim().trim_matches(['"', '\'']).to_lowercase();
    (!key.is_empty() && !key.starts_with('-')).then(|| (key, &line[colon.0 + 1..]))
}

fn scalar(rest: &str, children: &[&str]) -> String {
    let rest = rest.trim();
    if rest.starts_with(['|', '>']) {
        return block_scalar(children, rest.starts_with('|'));
    }
    if rest.is_empty() {
        // A map or a list follows, which is no text.
        return String::new();
    }
    // A flow value may go on in the lines below; YAML joins them with spaces.
    let mut joined = rest.to_string();
    for child in children {
        let part = child.trim();
        if part.is_empty() {
            joined.push('\n');
        } else {
            if !joined.ends_with('\n') {
                joined.push(' ');
            }
            joined.push_str(part);
        }
    }
    let joined = joined.trim_end();
    if let Some(inner) = joined.strip_prefix('"') {
        double_quoted(inner)
    } else if let Some(inner) = joined.strip_prefix('\'') {
        let end = single_quote_end(inner);
        inner[..end].replace("''", "'")
    } else {
        // ` #` starts a comment in a plain value.
        let end = joined.find(" #").unwrap_or(joined.len());
        joined[..end].trim().to_string()
    }
}

fn single_quote_end(inner: &str) -> usize {
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            if bytes.get(i + 1) == Some(&b'\'') {
                i += 2;
                continue;
            }
            return i;
        }
        i += 1;
    }
    inner.len()
}

fn double_quoted(inner: &str) -> String {
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(c);
                    }
                }
                Some(other) => out.push(other),
                None => {}
            },
            c => out.push(c),
        }
    }
    out
}

/// `|` keeps the line breaks, `>` folds lines into one paragraph.
fn block_scalar(children: &[&str], literal: bool) -> String {
    let lines = dedent(children);
    let mut out = String::new();
    for line in &lines {
        if literal {
            out.push_str(line);
            out.push('\n');
        } else if line.is_empty() {
            out.push('\n');
        } else {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push(' ');
            }
            out.push_str(line);
        }
    }
    out.trim().to_string()
}

/// The lines without the indentation of the first one that has text.
fn dedent(lines: &[&str]) -> Vec<String> {
    let indent =
        lines.iter().find(|l| !l.trim().is_empty()).map_or(0, |l| l.len() - l.trim_start().len());
    lines
        .iter()
        .map(|l| {
            let cut = (l.len() - l.trim_start().len()).min(indent);
            l[cut..].trim_end().to_string()
        })
        .collect()
}

// --------------------------------------------------------------- files ---

/// A file's path inside a skill folder, as its attachment keeps it: plain names
/// joined by `/`. `None` for anything that could leave the folder, and for the
/// folder's own `SKILL.md`, which is the skill's text.
pub fn clean_path(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in path.split(['/', '\\']) {
        let part: String = part.chars().filter(|c| !c.is_control()).collect();
        match part.trim() {
            "" | "." => {}
            ".." => return None,
            name => parts.push(name.to_string()),
        }
    }
    let joined = parts.join("/");
    let fits = !joined.is_empty() && joined.chars().count() <= MAX_PATH_CHARS;
    (fits && !joined.eq_ignore_ascii_case(SKILL_FILE)).then_some(joined)
}

// ---------------------------------------------------------------- URLs ---

/// Where on GitHub to look for skills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubUrl {
    pub owner: String,
    pub repo: String,
    /// What follows `tree/` or `blob/`: a branch, tag or commit, then a path. Which
    /// part is the ref only the repository can tell, as branch names may contain `/`.
    pub rest: Vec<String>,
    /// A link to one `SKILL.md` rather than a folder.
    pub single: bool,
}

impl GithubUrl {
    /// `owner/name`
    pub fn repo_name(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

/// Accepts `https://github.com/owner/repo`, `…/tree/<ref>/<path>`,
/// `…/blob/<ref>/<path>/SKILL.md` and the shorthand `owner/repo`, each with a
/// trailing `.git` or `/`. Everything else is `None`.
pub fn parse_url(input: &str) -> Option<GithubUrl> {
    let input = input.trim();
    let path = if let Some(rest) = strip_prefix_ignore_case(input, "https://") {
        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        let (host, path) = rest.split_once('/')?;
        let host = host.to_ascii_lowercase();
        if host != "github.com" && host != "www.github.com" {
            return None;
        }
        path
    } else if input.contains("://") || input.matches('/').count() > 2 {
        return None;
    } else {
        // The shorthand: two parts, nothing more.
        let trimmed = input.trim_end_matches('/');
        if trimmed.matches('/').count() != 1 {
            return None;
        }
        trimmed
    };

    let mut segments: Vec<&str> = path.split('/').collect();
    while segments.last() == Some(&"") {
        segments.pop();
    }
    if segments.len() < 2 || segments.iter().any(|s| s.is_empty()) {
        return None;
    }
    let owner = segments[0];
    let mut repo = segments[1];
    if segments.len() == 2 {
        repo = repo.strip_suffix(".git").unwrap_or(repo);
    }
    let owner_ok = owner.len() <= 39
        && !owner.starts_with('-')
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let repo_ok = repo.len() <= 100
        && repo != "."
        && repo != ".."
        && !repo.ends_with(".git")
        && repo.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    if owner.is_empty() || repo.is_empty() || !owner_ok || !repo_ok {
        return None;
    }

    let mut url = GithubUrl {
        owner: owner.to_string(),
        repo: repo.to_string(),
        rest: Vec::new(),
        single: false,
    };
    if segments.len() == 2 {
        return Some(url);
    }
    url.single = match segments[2] {
        "tree" => false,
        "blob" => true,
        _ => return None,
    };
    for segment in &segments[3..] {
        let decoded = percent_decode(segment)?;
        let bad = decoded == "." || decoded == ".." || decoded.contains(['/', '\\']);
        if decoded.is_empty() || bad || decoded.chars().any(char::is_control) {
            return None;
        }
        url.rest.push(decoded);
    }
    let enough = if url.single { url.rest.len() >= 2 } else { !url.rest.is_empty() };
    let names_skill = url.rest.last().is_some_and(|f| f.eq_ignore_ascii_case(SKILL_FILE));
    (enough && (!url.single || names_skill)).then_some(url)
}

fn strip_prefix_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &text[prefix.len()..])
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

// ---------------------------------------------------------------- trees ---

/// A file or folder in a repository listing, as GitHub's tree API gives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeEntry {
    /// From the repository root.
    pub path: String,
    pub mode: String,
    /// `blob`, `tree`, or `commit` for a submodule.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub size: u64,
}

/// A supporting file of a skill, relative to its folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillFilePreview {
    pub path: String,
    pub size: u64,
}

/// A skill found in a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillFolder {
    /// From the repository root, empty for the root itself.
    pub path: String,
    /// The path of its `SKILL.md` from the repository root.
    pub skill_file: String,
    /// Without the `SKILL.md`.
    pub files: Vec<SkillFilePreview>,
    /// All files together, the `SKILL.md` included.
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeProblem {
    /// No `SKILL.md` at all.
    NoSkill,
    /// More than [`MAX_SKILLS`].
    TooMany,
    /// Every skill found is over [`MAX_FILES`] or [`MAX_SKILL_BYTES`].
    TooLarge,
}

/// The skills at or below `folder` in a recursive listing, sorted by path. With
/// `single`, only the one whose `SKILL.md` lies in `folder` itself.
///
/// A file belongs to the nearest skill folder above it, so a skill inside another
/// one stays a skill of its own. Links and submodules are passed over: they may
/// lead anywhere. Skills over the limits are left out.
pub fn skill_folders(
    tree: &[TreeEntry],
    folder: &str,
    single: bool,
) -> Result<Vec<SkillFolder>, TreeProblem> {
    let folder = folder.trim_matches('/');
    let inside = |path: &str| folder.is_empty() || path.starts_with(&format!("{folder}/"));
    let files: Vec<&TreeEntry> =
        tree.iter().filter(|e| e.kind == "blob" && e.mode != "120000" && inside(&e.path)).collect();

    let mut skills: BTreeMap<&str, SkillFolder> = BTreeMap::new();
    for file in &files {
        let (dir, name) = split_path(&file.path);
        if !name.eq_ignore_ascii_case(SKILL_FILE) {
            continue;
        }
        let skill = skills.entry(dir).or_insert_with(|| SkillFolder {
            path: dir.to_string(),
            skill_file: file.path.clone(),
            files: Vec::new(),
            size: 0,
        });
        // Two spellings in one folder: the one the format names wins.
        if name == SKILL_FILE {
            skill.skill_file = file.path.clone();
        }
    }

    let mut counts: HashMap<String, usize> = HashMap::new();
    for file in &files {
        let mut dir = split_path(&file.path).0;
        let skill = loop {
            if let Some(skill) = skills.get_mut(dir) {
                break Some(skill);
            }
            if dir.is_empty() {
                break None;
            }
            dir = split_path(dir).0;
        };
        let Some(skill) = skill else { continue };
        skill.size += file.size;
        *counts.entry(skill.path.clone()).or_default() += 1;
        if file.path != skill.skill_file {
            let relative = match skill.path.is_empty() {
                true => file.path.clone(),
                false => file.path[skill.path.len() + 1..].to_string(),
            };
            skill.files.push(SkillFilePreview { path: relative, size: file.size });
        }
    }

    let found: Vec<SkillFolder> =
        skills.into_values().filter(|s| !single || s.path == folder).collect();
    if found.is_empty() {
        return Err(TreeProblem::NoSkill);
    }
    let fits = |s: &SkillFolder| counts[&s.path] <= MAX_FILES && s.size <= MAX_SKILL_BYTES;
    let mut kept: Vec<SkillFolder> = found.into_iter().filter(fits).collect();
    for skill in &mut kept {
        skill.files.sort_by(|a, b| a.path.cmp(&b.path));
    }
    match kept.len() {
        0 => Err(TreeProblem::TooLarge),
        n if n > MAX_SKILLS => Err(TreeProblem::TooMany),
        _ => Ok(kept),
    }
}

/// `a/b/SKILL.md` to `("a/b", "SKILL.md")`, and `SKILL.md` to `("", "SKILL.md")`.
fn split_path(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_front_matter_and_body() {
        let s =
            parse("---\nname: pdf-forms\ndescription: Fill PDF forms.\n---\n\n# Steps\n\nDo it.\n");
        assert_eq!(s.name.as_deref(), Some("pdf-forms"));
        assert_eq!(s.description, "Fill PDF forms.");
        assert_eq!(s.body, "# Steps\n\nDo it.");
        assert_eq!((s.title, s.projects.len()), (None, 0));
    }

    #[test]
    fn tolerates_quotes_folding_crlf_and_other_keys() {
        let text = "\u{feff}---\r\nname: 'it''s'\r\ndescription: >\r\n  Use this when\r\n  the task needs it.\r\n\r\n  Second line.\r\nlicense: MIT\r\nallowed-tools:\r\n  - Read\r\n  - Bash\r\nmetadata:\r\n  version: \"1.0\"\r\n---\r\nBody\r\n";
        let s = parse(text);
        assert_eq!(s.name.as_deref(), Some("it's"));
        assert_eq!(s.description, "Use this when the task needs it.\nSecond line.");
        assert_eq!(s.body, "Body");

        let s = parse("---\ndescription: \"Says \\\"hi\\\" \\u00e4\" # comment\nname: x\n---\n");
        assert_eq!(s.description, "Says \"hi\" ä");
        let s = parse("---\ndescription: |-\n  one\n  two\nname: x # note\n---\n");
        assert_eq!((s.description.as_str(), s.name.as_deref()), ("one\ntwo", Some("x")));
        let s = parse("---\ndescription: a plain value\n  that goes on\n---\n");
        assert_eq!(s.description, "a plain value that goes on");
        let s = parse("---\ndescription: Handles C#: well\n---\n");
        assert_eq!(s.description, "Handles C#: well");
    }

    #[test]
    fn without_front_matter_everything_is_body() {
        let s = parse("# Only text\n\n---\nname: no\n");
        assert_eq!((s.name, s.description.as_str()), (None, ""));
        assert!(s.body.starts_with("# Only text"));
        assert_eq!(parse("---\nname: open\nnever closed").body, "---\nname: open\nnever closed");
    }

    #[test]
    fn rendered_text_reads_back_the_same() {
        let projects = vec!["Arbeit".to_string(), "Kunde: A, B".to_string()];
        let text = render(
            "code-review",
            "Prüft „Code“: \"gründlich\"\n",
            "Code Review",
            &projects,
            "\n# Schritte\n",
        );
        assert!(text.starts_with("---\nname: code-review\ndescription: \"Prüft"), "{text}");
        let s = parse(&text);
        assert_eq!(s.name.as_deref(), Some("code-review"));
        assert_eq!(s.description, "Prüft „Code“: \"gründlich\"");
        assert_eq!(s.title.as_deref(), Some("Code Review"));
        assert_eq!(s.projects, projects);
        assert_eq!(s.body, "# Schritte");
        assert!(!render("x", "d", "x", &[], "b").contains("projects"));
    }

    #[test]
    fn hostile_descriptions_stay_one_quoted_line() {
        let hostile = [
            "key: value: more",
            "\"quoted\" and 'single' and \\back\\slash",
            "line one\nline two\r\nthree",
            "---\nname: other\n---",
            "# not a comment",
            "- not a list",
            "{flow: map} [and, list] & *alias !tag | > %",
            "control \u{0}\u{7}\u{7f}\u{85}\u{9f} and separators \u{2028}\u{2029}\u{feff}",
            "Prüft „Code“ 😀",
        ];
        for description in hostile {
            let text = render_managed("brainmatter-x", description, "brainmatter", "Body");
            let front = text.split("\n---\n").next().unwrap();
            assert_eq!(front.lines().count(), 5, "{text}");
            let line = front.lines().find(|l| l.starts_with("description: ")).unwrap();
            assert!(line.ends_with('"') && line.chars().all(|c| !c.is_control()), "{line}");

            let s = parse(&text);
            assert_eq!(s.name.as_deref(), Some("brainmatter-x"));
            assert_eq!(s.description, description.trim(), "{text}");
            assert_eq!(s.managed_by.as_deref(), Some("brainmatter"));
            assert_eq!(s.body, "Body");
        }
        assert_eq!(parse(&render("x", "d", "T", &[], "b")).managed_by, None);
    }

    #[test]
    fn names_follow_the_format() {
        assert_eq!(name_for("Code Review für Größen"), "code-review-fuer-groessen");
        let long = name_for(&"ab ".repeat(40));
        assert!(long.len() <= 64 && !long.ends_with('-'), "{long}");
    }

    #[test]
    fn file_paths_stay_inside_the_folder() {
        assert_eq!(clean_path("scripts/fill_form.py").as_deref(), Some("scripts/fill_form.py"));
        assert_eq!(clean_path("./a\\b//c.txt").as_deref(), Some("a/b/c.txt"));
        assert_eq!(clean_path("a/\u{0}b.md").as_deref(), Some("a/b.md"));
        assert_eq!(clean_path("../etc/passwd"), None);
        assert_eq!(clean_path("a/../../b"), None);
        assert_eq!(clean_path("skill.md"), None);
        assert_eq!(clean_path("docs/SKILL.md").as_deref(), Some("docs/SKILL.md"));
        assert_eq!(clean_path(" a / b.txt ").as_deref(), Some("a/b.txt"));
        assert_eq!(clean_path(" / "), None);
        assert_eq!(clean_path("//"), None);
    }

    fn url(owner: &str, repo: &str, rest: &[&str], single: bool) -> Option<GithubUrl> {
        let rest = rest.iter().map(|s| s.to_string()).collect();
        Some(GithubUrl { owner: owner.into(), repo: repo.into(), rest, single })
    }

    #[test]
    fn accepts_the_four_url_forms() {
        assert_eq!(parse_url("https://github.com/acme/skills"), url("acme", "skills", &[], false));
        assert_eq!(parse_url(" https://github.com/a/b.git "), url("a", "b", &[], false));
        assert_eq!(parse_url("https://www.GitHub.com/a/b/"), url("a", "b", &[], false));
        assert_eq!(parse_url("a-b/c.d_e"), url("a-b", "c.d_e", &[], false));
        assert_eq!(parse_url("a/b/"), url("a", "b", &[], false));
        assert_eq!(parse_url("a/b.git"), url("a", "b", &[], false));
        assert_eq!(
            parse_url("https://github.com/a/b/tree/main/skills/pdf/?tab=readme#top"),
            url("a", "b", &["main", "skills", "pdf"], false)
        );
        assert_eq!(
            parse_url("https://github.com/a/b/tree/feature/x/My%20Skill"),
            url("a", "b", &["feature", "x", "My Skill"], false)
        );
        assert_eq!(
            parse_url("https://github.com/a/b/blob/v1.0/skills/pdf/SKILL.md"),
            url("a", "b", &["v1.0", "skills", "pdf", "SKILL.md"], true)
        );
        assert_eq!(
            parse_url("https://github.com/a/b/blob/main/SKILL.md"),
            url("a", "b", &["main", "SKILL.md"], true)
        );
    }

    #[test]
    fn rejects_everything_else() {
        for bad in [
            "",
            "a",
            "a/b/c",
            "http://github.com/a/b",
            "ftp://github.com/a/b",
            "https://gitlab.com/a/b",
            "https://github.com.evil.com/a/b",
            "https://evil.com/github.com/a/b",
            "https://github.com/a",
            "https://github.com/a/b/issues/1",
            "https://github.com/a/b/tree",
            "https://github.com/a/b/tree/",
            "https://github.com/a/b/tree/main/../secret",
            "https://github.com/a/b/tree/main/%2e%2e/x",
            "https://github.com/a/b/tree/main/a%2Fb",
            "https://github.com/a/b/tree/main//x",
            "https://github.com/a/b/blob/main/README.md",
            "https://github.com/a/b/blob/SKILL.md",
            "https://github.com/-a/b",
            "https://github.com/a/../b",
            "https://github.com/a/b c",
            "https://github.com/a/b.git/tree/main",
            "github.com/a/b",
            "a/b/../c",
            "https://github.com/a/b/tree/main/%zz",
        ] {
            assert_eq!(parse_url(bad), None, "{bad}");
        }
    }

    fn blob(path: &str, size: u64) -> TreeEntry {
        TreeEntry { path: path.into(), mode: "100644".into(), kind: "blob".into(), size }
    }

    fn paths(found: &[SkillFolder]) -> Vec<&str> {
        found.iter().map(|s| s.path.as_str()).collect()
    }

    #[test]
    fn finds_skill_folders_and_their_files() {
        let tree = vec![
            blob("README.md", 10),
            TreeEntry {
                path: "skills".into(),
                mode: "040000".into(),
                kind: "tree".into(),
                size: 0,
            },
            blob("skills/pdf/SKILL.md", 100),
            blob("skills/pdf/scripts/fill.py", 50),
            blob("skills/pdf/forms/SKILL.md", 20),
            blob("skills/pdf/forms/a.txt", 5),
            blob("skills/docx/skill.md", 30),
            TreeEntry {
                path: "skills/docx/link".into(),
                mode: "120000".into(),
                kind: "blob".into(),
                size: 9,
            },
            TreeEntry {
                path: "skills/docx/sub".into(),
                mode: "160000".into(),
                kind: "commit".into(),
                size: 0,
            },
            blob("other/x.md", 1),
        ];
        let found = skill_folders(&tree, "", false).unwrap();
        assert_eq!(paths(&found), ["skills/docx", "skills/pdf", "skills/pdf/forms"]);
        let pdf = &found[1];
        assert_eq!(pdf.skill_file, "skills/pdf/SKILL.md");
        assert_eq!(pdf.files, [SkillFilePreview { path: "scripts/fill.py".into(), size: 50 }]);
        assert_eq!(pdf.size, 150);
        assert_eq!(found[2].files[0].path, "a.txt");
        assert!(found[0].files.is_empty(), "links and submodules are left out");

        assert_eq!(
            paths(&skill_folders(&tree, "skills/pdf", false).unwrap()),
            ["skills/pdf", "skills/pdf/forms"]
        );
        let one = skill_folders(&tree, "skills/pdf/", true).unwrap();
        assert_eq!(paths(&one), ["skills/pdf"]);
        assert_eq!(one[0].files.len(), 1, "the skill inside keeps its files");
        assert_eq!(skill_folders(&tree, "other", false), Err(TreeProblem::NoSkill));
        assert_eq!(skill_folders(&tree, "skills/pd", false), Err(TreeProblem::NoSkill));

        let root = vec![blob("SKILL.md", 1), blob("LICENSE", 2)];
        let found = skill_folders(&root, "", false).unwrap();
        assert_eq!((found[0].path.as_str(), found[0].files[0].path.as_str()), ("", "LICENSE"));
    }

    #[test]
    fn applies_the_limits() {
        let mut tree = vec![blob("big/SKILL.md", 1), blob("big/data.bin", MAX_SKILL_BYTES)];
        tree.push(blob("many/SKILL.md", 1));
        for i in 0..MAX_FILES {
            tree.push(blob(&format!("many/f{i}"), 1));
        }
        tree.push(blob("fits/SKILL.md", 1));
        for i in 0..MAX_FILES - 1 {
            tree.push(blob(&format!("fits/f{i}"), 1));
        }
        assert_eq!(paths(&skill_folders(&tree, "", false).unwrap()), ["fits"]);
        assert_eq!(skill_folders(&tree, "big", false), Err(TreeProblem::TooLarge));

        let crowd: Vec<TreeEntry> =
            (0..=MAX_SKILLS).map(|i| blob(&format!("s{i:02}/SKILL.md"), 1)).collect();
        assert_eq!(skill_folders(&crowd, "", false), Err(TreeProblem::TooMany));
        assert_eq!(skill_folders(&crowd[1..], "", false).unwrap().len(), MAX_SKILLS);
    }
}
