//! One-click setup: writes Brainmatter into the MCP configuration of AI tools
//! installed on this Mac.
//!
//! These files belong to other apps, so edits are conservative: other entries
//! and (for TOML) comments stay untouched, unparseable files are refused rather
//! than replaced, a pristine backup is kept, and writes are atomic. The files
//! end up containing a token, so they are written with mode 0600.

use crate::i18n::tr;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// Debug builds register separately, so development never replaces the entry
/// of the installed app.
pub const SERVER_NAME: &str =
    if cfg!(debug_assertions) { "brainmatter-dev" } else { "brainmatter" };
const BACKUP_SUFFIX: &str = "brainmatter-backup";
const CLI_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    ClaudeCode,
    ClaudeDesktop,
    Cursor,
    Codex,
}

impl Target {
    pub const ALL: [Target; 4] =
        [Target::ClaudeCode, Target::ClaudeDesktop, Target::Cursor, Target::Codex];

    /// Also the client's name in the registry and its actor in the graph.
    pub fn name(self) -> &'static str {
        match self {
            Target::ClaudeCode => "Claude Code",
            Target::ClaudeDesktop => "Claude Desktop",
            Target::Cursor => "Cursor",
            Target::Codex => "Codex",
        }
    }

    pub fn from_name(name: &str) -> Option<Target> {
        Target::ALL.into_iter().find(|t| t.name() == name)
    }

    /// Talks stdio, so it needs the `brainmatter-mcp` bridge.
    pub fn needs_bridge(self) -> bool {
        matches!(self, Target::ClaudeDesktop | Target::Codex)
    }
}

#[derive(Debug)]
pub enum SetupError {
    Unparseable(PathBuf, String),
    BridgeMissing(PathBuf),
    ClaudeNotFound,
    Cli(String),
    Io(PathBuf, std::io::Error),
}

impl std::error::Error for SetupError {}

impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let de = crate::i18n::german();
        match self {
            Self::Unparseable(path, why) if de => write!(
                f,
                "{} enthält kein gültiges Format und wurde nicht verändert: {why}",
                path.display()
            ),
            Self::Unparseable(path, why) => {
                write!(
                    f,
                    "{} is not in a valid format and was left as it was: {why}",
                    path.display()
                )
            }
            Self::BridgeMissing(path) if de => {
                write!(f, "Die Bridge (brainmatter-mcp) fehlt: {}", path.display())
            }
            Self::BridgeMissing(path) => {
                write!(f, "The bridge (brainmatter-mcp) is missing: {}", path.display())
            }
            Self::ClaudeNotFound if de => {
                f.write_str("Claude Code wurde auf diesem Mac nicht gefunden.")
            }
            Self::ClaudeNotFound => f.write_str("Claude Code was not found on this Mac."),
            Self::Cli(why) if de => write!(f, "Claude Code meldet einen Fehler: {why}"),
            Self::Cli(why) => write!(f, "Claude Code reports an error: {why}"),
            Self::Io(path, e) => write!(f, "{}: {e}", path.display()),
        }
    }
}

/// Where to look; injectable so tests never touch real configuration.
pub struct Integrations {
    pub home: PathBuf,
    pub applications: PathBuf,
    pub url: String,
    pub bridge: PathBuf,
    /// Passed to the Claude CLI as `CLAUDE_CONFIG_DIR` when set.
    pub claude_config_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub target: Target,
    pub name: &'static str,
    pub installed: bool,
    pub configured: bool,
    /// Can be set up automatically right now.
    pub available: bool,
    pub unavailable_reason: Option<String>,
    /// Where the memory rule goes, with `~` for the home folder. `None` when the
    /// tool keeps its instructions elsewhere and the rule is pasted by hand.
    pub rule_file: Option<String>,
    /// The memory rule is in that file.
    pub rule: bool,
    /// Where it finds Brainmatter's commands and skills, with `~`. `None` when it
    /// reads no skill folder.
    pub skills_dir: Option<String>,
    /// Brainmatter keeps that folder in step with what the tool may see.
    pub skill_menu: bool,
    /// How many commands and skills of Brainmatter are there.
    pub skills: usize,
}

impl Integrations {
    pub fn status(&self, target: Target) -> Status {
        let claude = (target == Target::ClaudeCode).then(|| self.find_claude()).flatten();
        let installed = match target {
            Target::ClaudeCode => claude.is_some(),
            Target::ClaudeDesktop => {
                self.applications.join("Claude.app").exists()
                    || self.desktop_config().parent().is_some_and(Path::exists)
            }
            Target::Cursor => {
                self.applications.join("Cursor.app").exists() || self.home.join(".cursor").exists()
            }
            Target::Codex => self.home.join(".codex").exists(),
        };
        let unavailable_reason = if !installed {
            Some(match crate::i18n::german() {
                true => format!("{} ist auf diesem Mac nicht installiert.", target.name()),
                false => format!("{} is not installed on this Mac.", target.name()),
            })
        } else if target.needs_bridge() && !self.bridge.is_file() {
            Some(
                tr(
                    "Die Bridge (brainmatter-mcp) fehlt noch.",
                    "The bridge (brainmatter-mcp) is missing.",
                )
                .to_string(),
            )
        } else {
            None
        };
        Status {
            target,
            name: target.name(),
            installed,
            configured: self.is_configured(target),
            available: unavailable_reason.is_none(),
            unavailable_reason,
            rule_file: self.rule_file(target).map(|p| self.tilde(&p)),
            rule: self.has_rule(target),
            skills_dir: self.skills_dir(target).map(|p| self.tilde(&p)),
            skill_menu: false,
            skills: 0,
        }
    }

    pub fn configure(&self, target: Target, token: &str) -> Result<(), SetupError> {
        if target.needs_bridge() && !self.bridge.is_file() {
            return Err(SetupError::BridgeMissing(self.bridge.clone()));
        }
        match target {
            Target::ClaudeCode => self.configure_claude_code(token),
            Target::ClaudeDesktop => {
                let bridge = self.bridge.display().to_string();
                let entry = json!({
                    "command": bridge,
                    "env": { "BRAINMATTER_TOKEN": token, "BRAINMATTER_URL": self.url },
                });
                set_json_server(&self.desktop_config(), Some(entry))
            }
            Target::Cursor => {
                let entry = json!({
                    "url": self.url,
                    "headers": { "Authorization": format!("Bearer {token}") },
                });
                set_json_server(&self.cursor_config(), Some(entry))
            }
            Target::Codex => {
                let bridge = self.bridge.display().to_string();
                set_codex_server(&self.codex_config(), Some((&bridge, token, &self.url)))
            }
        }
    }

    /// Removes our entry and the memory rule again. Missing files or entries are fine.
    pub fn remove(&self, target: Target) -> Result<(), SetupError> {
        let rule = self.set_rule(target, None);
        let entry = match target {
            Target::ClaudeCode => match self.find_claude() {
                Some(claude) => self
                    .run_claude(&claude, &["mcp", "remove", SERVER_NAME, "--scope", "user"])
                    .map(|_| ()),
                None => Ok(()),
            },
            Target::ClaudeDesktop => set_json_server(&self.desktop_config(), None),
            Target::Cursor => set_json_server(&self.cursor_config(), None),
            Target::Codex => set_codex_server(&self.codex_config(), None),
        };
        entry.and(rule)
    }

    fn desktop_config(&self) -> PathBuf {
        self.home.join("Library/Application Support/Claude/claude_desktop_config.json")
    }

    fn cursor_config(&self) -> PathBuf {
        self.home.join(".cursor/mcp.json")
    }

    fn codex_config(&self) -> PathBuf {
        self.home.join(".codex/config.toml")
    }

    fn claude_code_config(&self) -> PathBuf {
        self.claude_config_dir.as_deref().unwrap_or(&self.home).join(".claude.json")
    }

    fn is_configured(&self, target: Target) -> bool {
        let in_json = |path: PathBuf| {
            fs::read(path)
                .ok()
                .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
                .is_some_and(|v| v.pointer(&format!("/mcpServers/{SERVER_NAME}")).is_some())
        };
        match target {
            Target::ClaudeCode => in_json(self.claude_code_config()),
            Target::ClaudeDesktop => in_json(self.desktop_config()),
            Target::Cursor => in_json(self.cursor_config()),
            Target::Codex => fs::read_to_string(self.codex_config())
                .ok()
                .and_then(|raw| raw.parse::<toml_edit::DocumentMut>().ok())
                .is_some_and(|doc| {
                    doc.get("mcp_servers").and_then(|s| s.get(SERVER_NAME)).is_some()
                }),
        }
    }

    // ------------------------------------------------------- Memory rule ---

    /// Where `target` reads its standing instructions. Claude Desktop keeps them
    /// in the account, so its rule is pasted by hand.
    pub fn rule_file(&self, target: Target) -> Option<PathBuf> {
        match target {
            Target::ClaudeCode => Some(match &self.claude_config_dir {
                Some(dir) => dir.join("CLAUDE.md"),
                None => self.home.join(".claude/CLAUDE.md"),
            }),
            Target::Codex => {
                // Codex reads the override instead, as long as it says anything.
                let dir = self.home.join(".codex");
                let over = dir.join("AGENTS.override.md");
                let used = fs::read_to_string(&over).is_ok_and(|s| !s.trim().is_empty());
                Some(if used { over } else { dir.join("AGENTS.md") })
            }
            Target::Cursor => Some(self.home.join(format!(".cursor/rules/{SERVER_NAME}.mdc"))),
            Target::ClaudeDesktop => None,
        }
    }

    pub fn has_rule(&self, target: Target) -> bool {
        match (target, self.rule_file(target)) {
            (_, None) => false,
            (Target::Cursor, Some(path)) => path.is_file(),
            (_, Some(path)) => {
                fs::read_to_string(path).is_ok_and(|s| s.contains(&rule_marker("start")))
            }
        }
    }

    /// Puts the rule in, replaces it, or (with `None`) takes it out. The rest of
    /// the file stays as it was; a file that held only the rule goes away.
    pub fn set_rule(&self, target: Target, rule: Option<&str>) -> Result<(), SetupError> {
        let Some(path) = self.rule_file(target) else { return Ok(()) };
        if target == Target::Cursor {
            return match rule {
                Some(text) => write_config(&path, cursor_rule(text).as_bytes()),
                None => remove_file(&path),
            };
        }
        let old = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(SetupError::Io(path, e)),
        };
        let mut new = without_rule(&old);
        if let Some(text) = rule {
            new.truncate(new.trim_end().len());
            if !new.is_empty() {
                new.push_str("\n\n");
            }
            new.push_str(&format!("{}\n{text}{}\n", rule_marker("start"), rule_marker("end")));
        }
        if new == old {
            Ok(())
        } else if new.trim().is_empty() {
            remove_file(&path)
        } else {
            write_config(&path, new.as_bytes())
        }
    }

    /// Brings a rule an older version wrote to the current wording, where it stands,
    /// and keeps it read-only if it was. Never adds one. Returns whether it changed.
    pub fn refresh_rule(&self, target: Target) -> Result<bool, SetupError> {
        let Some(path) = self.rule_file(target) else { return Ok(false) };
        let old = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(SetupError::Io(path, e)),
        };
        let new = if target == Target::Cursor {
            cursor_rule(&rule_text(old.contains(WRITES)))
        } else {
            let Some((a, b)) = rule_block(&old) else { return Ok(false) };
            let text = rule_text(old[a..b].contains(WRITES));
            let block = format!("{}\n{text}{}", rule_marker("start"), rule_marker("end"));
            format!("{}{block}{}", &old[..a], &old[b..])
        };
        if new == old {
            return Ok(false);
        }
        write_config(&path, new.as_bytes()).map(|()| true)
    }

    // -------------------------------------------------------- Skill menu ---

    /// Where `target` looks for skills of its own. Claude Desktop keeps them in the
    /// account. Codex once used `~/.codex/skills`; Cursor reads that as well, so a
    /// copy there would show twice.
    pub fn skills_dir(&self, target: Target) -> Option<PathBuf> {
        match target {
            Target::ClaudeCode => Some(match &self.claude_config_dir {
                Some(dir) => dir.join("skills"),
                None => self.home.join(".claude/skills"),
            }),
            Target::Codex => Some(self.home.join(".agents/skills")),
            Target::Cursor => Some(self.home.join(".cursor/skills")),
            Target::ClaudeDesktop => None,
        }
    }

    /// The skill folders of other tools that `target` reads too.
    pub fn also_reads(&self, target: Target) -> Vec<PathBuf> {
        match target {
            Target::Cursor => [".claude/skills", ".agents/skills", ".codex/skills"]
                .into_iter()
                .map(|dir| self.home.join(dir))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// `~/…` for paths in the home folder, as people know them.
    pub fn tilde(&self, path: &Path) -> String {
        match path.strip_prefix(&self.home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        }
    }

    // ------------------------------------------------------- Claude Code ---

    /// Goes through the official CLI: Claude Code rewrites `~/.claude.json`
    /// constantly, so editing that file directly could lose changes.
    fn configure_claude_code(&self, token: &str) -> Result<(), SetupError> {
        let claude = self.find_claude().ok_or(SetupError::ClaudeNotFound)?;
        // `add` refuses existing names. A missing entry here is expected.
        let _ = self.run_claude(&claude, &["mcp", "remove", SERVER_NAME, "--scope", "user"]);
        let header = format!("Authorization: Bearer {token}");
        self.run_claude(
            &claude,
            &[
                "mcp",
                "add",
                "--transport",
                "http",
                "--scope",
                "user",
                SERVER_NAME,
                &self.url,
                "--header",
                &header,
            ],
        )
        .map(|_| ())
    }

    /// Apps started from the Finder do not get the shell's PATH, so check the
    /// usual install locations before asking a login shell.
    fn find_claude(&self) -> Option<PathBuf> {
        let known = [
            self.home.join(".local/bin/claude"),
            self.home.join(".claude/local/claude"),
            PathBuf::from("/opt/homebrew/bin/claude"),
            PathBuf::from("/usr/local/bin/claude"),
        ];
        if let Some(hit) = known.into_iter().find(|p| p.is_file()) {
            return Some(hit);
        }
        let out = Command::new("/bin/zsh").args(["-lc", "command -v claude"]).output().ok()?;
        let path = String::from_utf8_lossy(&out.stdout).lines().last()?.trim().to_string();
        (out.status.success() && path.starts_with('/')).then(|| PathBuf::from(path))
    }

    fn run_claude(&self, claude: &Path, args: &[&str]) -> Result<String, SetupError> {
        let mut cmd = Command::new(claude);
        cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        // npm installs are node scripts and need node on PATH.
        let path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{path}:/opt/homebrew/bin:/usr/local/bin"));
        if let Some(dir) = &self.claude_config_dir {
            cmd.env("CLAUDE_CONFIG_DIR", dir);
        }
        let mut child = cmd.spawn().map_err(|e| SetupError::Io(claude.to_owned(), e))?;
        let started = Instant::now();
        loop {
            match child.try_wait().map_err(|e| SetupError::Io(claude.to_owned(), e))? {
                Some(_) => break,
                None if started.elapsed() > CLI_TIMEOUT => {
                    let _ = child.kill();
                    return Err(SetupError::Cli(tr("Zeitüberschreitung", "Timed out").into()));
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
        let out = child.wait_with_output().map_err(|e| SetupError::Io(claude.to_owned(), e))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            // The command line holds the token; never echo it, only the output.
            Err(SetupError::Cli(String::from_utf8_lossy(&out.stderr).trim().to_string()))
        }
    }
}

/// Sets or (with `None`) removes our entry under `mcpServers`.
fn set_json_server(path: &Path, entry: Option<Value>) -> Result<(), SetupError> {
    let raw = match fs::read(path) {
        Ok(raw) => Some(raw),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(SetupError::Io(path.to_owned(), e)),
    };
    if raw.is_none() && entry.is_none() {
        return Ok(());
    }
    let is_blank = raw.as_ref().is_none_or(|r| r.iter().all(u8::is_ascii_whitespace));
    let mut root: Value = if is_blank {
        Value::Object(Map::new())
    } else {
        serde_json::from_slice(raw.as_deref().unwrap_or_default())
            .map_err(|e| SetupError::Unparseable(path.to_owned(), e.to_string()))?
    };
    let bad_shape = || {
        SetupError::Unparseable(
            path.to_owned(),
            tr("unerwartete Struktur", "unexpected structure").into(),
        )
    };
    let object = root.as_object_mut().ok_or_else(bad_shape)?;
    let servers = object
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(bad_shape)?;
    match entry {
        Some(entry) => {
            servers.insert(SERVER_NAME.to_string(), entry);
        }
        None => {
            if servers.remove(SERVER_NAME).is_none() {
                return Ok(());
            }
        }
    }
    let mut out = serde_json::to_vec_pretty(&root).expect("config serializes");
    out.push(b'\n');
    write_config(path, &out)
}

/// Sets or removes `[mcp_servers.<name>]`, keeping comments and formatting.
fn set_codex_server(path: &Path, server: Option<(&str, &str, &str)>) -> Result<(), SetupError> {
    use toml_edit::{DocumentMut, InlineTable, Item, Table, value};

    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if server.is_none() {
                return Ok(());
            }
            String::new()
        }
        Err(e) => return Err(SetupError::Io(path.to_owned(), e)),
    };
    let mut doc: DocumentMut = raw.parse().map_err(|e: toml_edit::TomlError| {
        SetupError::Unparseable(path.to_owned(), e.to_string())
    })?;

    match server {
        Some((command, token, url)) => {
            let servers = doc
                .entry("mcp_servers")
                .or_insert_with(|| {
                    let mut t = Table::new();
                    t.set_implicit(true);
                    Item::Table(t)
                })
                .as_table_mut()
                .ok_or_else(|| {
                    SetupError::Unparseable(
                        path.to_owned(),
                        tr("mcp_servers ist keine Tabelle", "mcp_servers is not a table").into(),
                    )
                })?;
            let mut env = InlineTable::new();
            env.insert("BRAINMATTER_TOKEN", token.into());
            env.insert("BRAINMATTER_URL", url.into());
            let mut table = Table::new();
            table.insert("command", value(command));
            table.insert("env", value(env));
            servers.insert(SERVER_NAME, Item::Table(table));
        }
        None => {
            let removed = doc
                .get_mut("mcp_servers")
                .and_then(Item::as_table_mut)
                .and_then(|t| t.remove(SERVER_NAME))
                .is_some();
            if !removed {
                return Ok(());
            }
        }
    }
    write_config(path, doc.to_string().as_bytes())
}

/// Standing instructions that make an AI tool use Brainmatter unasked. The
/// server sends its own instructions too, but tools weigh those less than their
/// own rule files, and some never show them to the model. Without `write`, the
/// tool may only read, so the rule does not ask it to save.
pub fn rule_text(write: bool) -> String {
    let mut text = format!(
        "## Brainmatter\n\n\
         Brainmatter (MCP server `{SERVER_NAME}`) is my knowledge graph and your long-term memory, \
         shared with my other AI tools and sessions.\n\
         - Before you start a task, look up what is already known: `project_overview` for the \
         project, `search` for the topic.\n\
         - If a task matches one of my skills (`list_skills`), load it with `get_skill` and \
         follow it.\n"
    );
    if write {
        text.push_str(
            "- Save unasked what a later session will need, as soon as it comes up: decisions with \
             their reasons, findings, conventions, my corrections, open tasks. Use `capture`, and \
             `update` an existing entry rather than adding a near-duplicate.\n\
             - No secrets, and nothing that code or git already records.\n",
        );
    }
    text
}

/// Only the rule for tools that may write mentions this.
const WRITES: &str = "`capture`";

/// Debug builds mark their own block, like their own server name.
fn rule_marker(edge: &str) -> String {
    format!("<!-- {SERVER_NAME}:{edge} -->")
}

/// Where our block starts and ends, markers included. `None` also when someone cut
/// it in half: better leave it than guess.
fn rule_block(text: &str) -> Option<(usize, usize)> {
    let (start, end) = (rule_marker("start"), rule_marker("end"));
    let a = text.find(&start)?;
    let b = text[a..].find(&end).map(|i| a + i + end.len())?;
    Some((a, b))
}

/// The text without our block and the blank line it was given.
fn without_rule(text: &str) -> String {
    let Some((a, b)) = rule_block(text) else { return text.to_owned() };
    let before = text[..a].trim_end();
    let after = text[b..].trim_start_matches(['\r', '\n']);
    match (before.is_empty(), after.is_empty()) {
        (true, _) => after.to_owned(),
        (false, true) => format!("{before}\n"),
        (false, false) => format!("{before}\n\n{after}"),
    }
}

/// A rule file of its own, applied to every chat.
fn cursor_rule(text: &str) -> String {
    format!(
        "---\ndescription: Brainmatter, long-term memory over MCP\nalwaysApply: true\n---\n\n{text}"
    )
}

fn remove_file(path: &Path) -> Result<(), SetupError> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(SetupError::Io(path.to_owned(), e))
        }
        _ => Ok(()),
    }
}

/// Atomic write with mode 0600; the first change keeps a pristine backup.
/// Follows symlinks, so dotfile managers keep working.
fn write_config(path: &Path, content: &[u8]) -> Result<(), SetupError> {
    let io = |e| SetupError::Io(path.to_owned(), e);
    let target = fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir).map_err(io)?;
    }
    if target.exists() {
        let backup = with_suffix(&target, BACKUP_SUFFIX);
        if !backup.exists() {
            fs::copy(&target, &backup).map_err(io)?;
        }
    }
    let tmp = with_suffix(&target, "brainmatter-tmp");
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        let mut f = opts.open(&tmp).map_err(io)?;
        f.write_all(content).map_err(io)?;
        f.sync_all().map_err(io)?;
    }
    fs::rename(&tmp, &target).map_err(io)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "bm_secret";

    fn setup() -> (tempfile::TempDir, Integrations) {
        let dir = tempfile::tempdir().unwrap();
        let bridge = dir.path().join("brainmatter-mcp");
        fs::write(&bridge, "").unwrap();
        let i = Integrations {
            home: dir.path().join("home"),
            applications: dir.path().join("Applications"),
            url: "http://127.0.0.1:47474/mcp".into(),
            bridge,
            claude_config_dir: None,
        };
        fs::create_dir_all(&i.home).unwrap();
        (dir, i)
    }

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn json_keeps_other_servers_and_settings_and_can_be_undone() {
        let (_d, i) = setup();
        let path = i.cursor_config();
        let original = r#"{"theme":"dark","mcpServers":{"github":{"url":"https://x"}}}"#;
        write(&path, original);

        i.configure(Target::Cursor, TOKEN).unwrap();
        let v: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["mcpServers"]["github"]["url"], "https://x");
        assert_eq!(v["mcpServers"][SERVER_NAME]["headers"]["Authorization"], "Bearer bm_secret");
        assert!(i.is_configured(Target::Cursor));

        assert_eq!(fs::read_to_string(with_suffix(&path, BACKUP_SUFFIX)).unwrap(), original);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);

        // Reconnecting keeps the pristine backup, not the one holding a token.
        i.configure(Target::Cursor, "bm_other").unwrap();
        assert_eq!(fs::read_to_string(with_suffix(&path, BACKUP_SUFFIX)).unwrap(), original);

        i.remove(Target::Cursor).unwrap();
        let v: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(v["mcpServers"].get(SERVER_NAME).is_none());
        assert_eq!(v["mcpServers"]["github"]["url"], "https://x");
        assert!(!i.is_configured(Target::Cursor));
    }

    #[test]
    fn broken_files_are_refused_and_left_alone() {
        let (_d, i) = setup();
        for broken in ["{ not json", "[1, 2]", r#"{"mcpServers": 5}"#] {
            let path = i.cursor_config();
            write(&path, broken);
            assert!(matches!(i.configure(Target::Cursor, TOKEN), Err(SetupError::Unparseable(..))));
            assert_eq!(fs::read_to_string(&path).unwrap(), broken);
        }
        let path = i.codex_config();
        write(&path, "not = = toml");
        assert!(matches!(i.configure(Target::Codex, TOKEN), Err(SetupError::Unparseable(..))));
        assert_eq!(fs::read_to_string(&path).unwrap(), "not = = toml");
    }

    #[test]
    fn creates_missing_and_blank_files() {
        let (_d, i) = setup();
        i.configure(Target::ClaudeDesktop, TOKEN).unwrap();
        let v: Value = serde_json::from_slice(&fs::read(i.desktop_config()).unwrap()).unwrap();
        assert_eq!(v["mcpServers"][SERVER_NAME]["env"]["BRAINMATTER_TOKEN"], TOKEN);
        assert_eq!(v["mcpServers"][SERVER_NAME]["command"], i.bridge.display().to_string());

        write(&i.cursor_config(), "  \n");
        i.configure(Target::Cursor, TOKEN).unwrap();
        assert!(i.is_configured(Target::Cursor));
        // Removing from files that never existed is not an error.
        i.remove(Target::Codex).unwrap();
        assert!(!i.codex_config().exists());
    }

    #[test]
    fn toml_keeps_comments_and_other_servers() {
        let (_d, i) = setup();
        let path = i.codex_config();
        write(
            &path,
            "# meine Einstellungen\nmodel = \"o4\"\n\n[mcp_servers.other]\ncommand = \"x\" # wichtig\n",
        );

        i.configure(Target::Codex, TOKEN).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# meine Einstellungen") && text.contains("# wichtig"), "{text}");
        let doc: toml_edit::DocumentMut = text.parse().unwrap();
        assert_eq!(
            doc["mcp_servers"][SERVER_NAME]["env"]["BRAINMATTER_TOKEN"].as_str(),
            Some(TOKEN)
        );
        assert_eq!(doc["mcp_servers"]["other"]["command"].as_str(), Some("x"));
        assert!(i.is_configured(Target::Codex));

        i.remove(Target::Codex).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains(TOKEN)
                && text.contains("# meine Einstellungen")
                && text.contains("[mcp_servers.other]"),
            "{text}"
        );
    }

    #[test]
    fn stdio_targets_need_the_bridge() {
        let (_d, mut i) = setup();
        fs::create_dir_all(i.home.join(".codex")).unwrap();
        assert!(i.status(Target::Codex).available);
        i.bridge = PathBuf::from("/does/not/exist");
        let status = i.status(Target::Codex);
        assert!(status.installed && !status.available);
        assert!(matches!(i.configure(Target::Codex, TOKEN), Err(SetupError::BridgeMissing(_))));
        assert!(!i.codex_config().exists());
    }

    #[test]
    fn detects_what_is_installed() {
        let (_d, i) = setup();
        assert!(!i.status(Target::Cursor).installed);
        fs::create_dir_all(i.applications.join("Cursor.app")).unwrap();
        assert!(i.status(Target::Cursor).installed);
    }

    /// Uses the real `claude` binary against a throwaway config directory:
    /// `cargo test -p brainmatter claude_code_roundtrip -- --ignored`
    #[test]
    #[ignore = "needs the claude CLI"]
    fn claude_code_roundtrip() {
        let (d, mut i) = setup();
        i.home = dirs_home();
        let config = d.path().join("claude-config");
        fs::create_dir_all(&config).unwrap();
        i.claude_config_dir = Some(config.clone());

        i.configure(Target::ClaudeCode, TOKEN).unwrap();
        assert!(i.is_configured(Target::ClaudeCode), "not found in {}", config.display());
        // Reconnecting replaces the entry instead of failing on the name.
        i.configure(Target::ClaudeCode, "bm_second").unwrap();
        let raw = fs::read_to_string(i.claude_code_config()).unwrap();
        assert!(raw.contains("bm_second") && !raw.contains(TOKEN), "{raw}");

        i.remove(Target::ClaudeCode).unwrap();
        assert!(!i.is_configured(Target::ClaudeCode));
    }

    fn dirs_home() -> PathBuf {
        PathBuf::from(std::env::var("HOME").unwrap())
    }

    #[test]
    fn the_memory_rule_comes_and_goes_without_touching_the_rest() {
        let (_d, i) = setup();
        let path = i.rule_file(Target::ClaudeCode).unwrap();
        let mine = "# My rules\n\nAlways answer in German.\n";
        write(&path, mine);

        i.set_rule(Target::ClaudeCode, Some(&rule_text(true))).unwrap();
        let with = fs::read_to_string(&path).unwrap();
        assert!(with.starts_with(mine), "{with}");
        assert!(with.contains("`capture`") && with.contains(&format!("`{SERVER_NAME}`")));
        assert!(i.has_rule(Target::ClaudeCode));
        assert!(i.status(Target::ClaudeCode).rule);

        // Again, and read-only: replaced, not added twice.
        i.set_rule(Target::ClaudeCode, Some(&rule_text(false))).unwrap();
        let again = fs::read_to_string(&path).unwrap();
        assert_eq!(again.matches(&rule_marker("start")).count(), 1);
        assert!(!again.contains("`capture`"));

        i.set_rule(Target::ClaudeCode, None).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), mine);
        assert!(!i.has_rule(Target::ClaudeCode));
    }

    #[test]
    fn both_rules_point_to_the_skills_right_after_looking_things_up() {
        let skills = "- If a task matches one of my skills (`list_skills`), load it with \
                      `get_skill` and follow it.\n";
        for write in [true, false] {
            let rule = rule_text(write);
            let look_up = rule.find("- Before you start a task").unwrap();
            let next = look_up + rule[look_up..].find('\n').unwrap() + 1;
            assert!(rule[next..].starts_with(skills), "{rule}");
        }
    }

    #[test]
    fn a_rule_file_made_for_the_rule_goes_away_with_it() {
        let (_d, i) = setup();
        for target in [Target::ClaudeCode, Target::Codex, Target::Cursor] {
            let path = i.rule_file(target).unwrap();
            i.set_rule(target, Some(&rule_text(true))).unwrap();
            assert!(i.has_rule(target), "{target:?}");
            // Not `remove`: for Claude Code that would run the real CLI.
            i.set_rule(target, None).unwrap();
            assert!(!path.exists(), "{}", path.display());
        }
        // Disconnecting takes the rule along.
        i.set_rule(Target::Cursor, Some("x")).unwrap();
        i.remove(Target::Cursor).unwrap();
        assert!(!i.has_rule(Target::Cursor));
        assert_eq!(i.rule_file(Target::ClaudeDesktop), None);
        assert!(i.set_rule(Target::ClaudeDesktop, Some("x")).is_ok());
    }

    #[test]
    fn cursor_gets_a_rule_for_every_chat() {
        let (_d, i) = setup();
        i.set_rule(Target::Cursor, Some(&rule_text(true))).unwrap();
        let raw = fs::read_to_string(i.rule_file(Target::Cursor).unwrap()).unwrap();
        assert!(raw.starts_with("---\n") && raw.contains("alwaysApply: true"), "{raw}");
        assert_eq!(
            i.status(Target::Cursor).rule_file.unwrap(),
            format!("~/.cursor/rules/{SERVER_NAME}.mdc")
        );
    }

    #[test]
    fn codex_gets_the_rule_where_it_reads() {
        let (_d, i) = setup();
        assert!(i.rule_file(Target::Codex).unwrap().ends_with(".codex/AGENTS.md"));
        // An empty override is skipped by Codex, so it is skipped here too.
        write(&i.home.join(".codex/AGENTS.override.md"), "\n");
        assert!(i.rule_file(Target::Codex).unwrap().ends_with(".codex/AGENTS.md"));
        write(&i.home.join(".codex/AGENTS.override.md"), "Be brief.\n");
        assert!(i.rule_file(Target::Codex).unwrap().ends_with(".codex/AGENTS.override.md"));
    }

    #[test]
    fn an_old_rule_gets_the_new_wording_where_it_stands() {
        let (_d, i) = setup();
        assert!(rule_text(true).contains(WRITES) && !rule_text(false).contains(WRITES));
        let block = |text: &str| format!("{}\n{text}{}", rule_marker("start"), rule_marker("end"));
        let old_write = "## Brainmatter\n\nOld wording. Use `capture` for decisions.\n";
        let old_read = "## Brainmatter\n\nOld wording, just look things up.\n";

        let claude = i.rule_file(Target::ClaudeCode).unwrap();
        write(&claude, &format!("# Mine\n\n{}\n\n## After\n\nStays below.\n", block(old_write)));
        let codex = i.rule_file(Target::Codex).unwrap();
        write(&codex, &format!("{}\n", block(old_read)));
        let cursor = i.rule_file(Target::Cursor).unwrap();
        write(&cursor, &cursor_rule(old_write));

        for target in [Target::ClaudeCode, Target::Codex, Target::Cursor] {
            assert!(i.refresh_rule(target).unwrap(), "{target:?}");
            assert!(!i.refresh_rule(target).unwrap(), "{target:?} is current now");
        }
        let text = fs::read_to_string(&claude).unwrap();
        let expected =
            format!("# Mine\n\n{}\n\n## After\n\nStays below.\n", block(&rule_text(true)));
        assert_eq!(text, expected, "in place, the rest untouched");
        assert_eq!(fs::read_to_string(&codex).unwrap(), format!("{}\n", block(&rule_text(false))));
        assert_eq!(fs::read_to_string(&cursor).unwrap(), cursor_rule(&rule_text(true)));

        // No rule, no new one; and half a block stays as it is.
        let (_other_dir, other) = setup();
        write(&other.rule_file(Target::ClaudeCode).unwrap(), "# Mine\n");
        let half = format!("Mine\n\n{}\nhalf a rule\n", rule_marker("start"));
        write(&other.rule_file(Target::Codex).unwrap(), &half);
        for target in Target::ALL {
            assert!(!other.refresh_rule(target).unwrap(), "{target:?}");
        }
        assert!(!other.rule_file(Target::Cursor).unwrap().exists());
        assert_eq!(fs::read_to_string(other.rule_file(Target::Codex).unwrap()).unwrap(), half);
        assert_eq!(
            fs::read_to_string(other.rule_file(Target::ClaudeCode).unwrap()).unwrap(),
            "# Mine\n"
        );
    }

    #[test]
    fn skill_folders_are_where_each_tool_reads_them() {
        let (d, mut i) = setup();
        let dir = |t| i.skills_dir(t).map(|p| i.tilde(&p));
        assert_eq!(dir(Target::ClaudeCode).as_deref(), Some("~/.claude/skills"));
        assert_eq!(dir(Target::Codex).as_deref(), Some("~/.agents/skills"));
        assert_eq!(dir(Target::Cursor).as_deref(), Some("~/.cursor/skills"));
        assert_eq!(dir(Target::ClaudeDesktop), None);
        assert_eq!(i.status(Target::ClaudeDesktop).skills_dir, None);
        let cursor = i.also_reads(Target::Cursor);
        assert!(cursor.contains(&i.skills_dir(Target::ClaudeCode).unwrap()));
        assert!(cursor.contains(&i.skills_dir(Target::Codex).unwrap()));
        assert!(i.also_reads(Target::ClaudeCode).is_empty());

        i.claude_config_dir = Some(d.path().join("config"));
        assert_eq!(i.skills_dir(Target::ClaudeCode), Some(d.path().join("config/skills")));
    }

    #[test]
    fn a_broken_block_is_left_alone() {
        let half = format!("Mine\n\n{}\nhalf a rule\n", rule_marker("start"));
        assert_eq!(without_rule(&half), half);
        let between = format!("A\n\n{}\nx\n{}\n\nB\n", rule_marker("start"), rule_marker("end"));
        assert_eq!(without_rule(&between), "A\n\nB\n");
    }
}
