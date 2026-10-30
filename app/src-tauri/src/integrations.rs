//! One-click setup: writes Brainmatter into the MCP configuration of AI tools
//! installed on this Mac.
//!
//! These files belong to other apps, so edits are conservative: other entries
//! and (for TOML) comments stay untouched, unparseable files are refused rather
//! than replaced, a pristine backup is kept, and writes are atomic. The files
//! end up containing a token, so they are written with mode 0600.

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("{0} enthält kein gültiges Format und wurde nicht verändert: {1}")]
    Unparseable(PathBuf, String),
    #[error("Die Bridge (brainmatter-mcp) fehlt: {0}")]
    BridgeMissing(PathBuf),
    #[error("Claude Code wurde auf diesem Mac nicht gefunden.")]
    ClaudeNotFound,
    #[error("Claude Code meldet einen Fehler: {0}")]
    Cli(String),
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
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
            Some(format!("{} ist auf diesem Mac nicht installiert.", target.name()))
        } else if target.needs_bridge() && !self.bridge.is_file() {
            Some("Die Bridge (brainmatter-mcp) fehlt noch.".to_string())
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

    /// Removes our entry again. Missing files or entries are fine.
    pub fn remove(&self, target: Target) -> Result<(), SetupError> {
        match target {
            Target::ClaudeCode => match self.find_claude() {
                Some(claude) => self
                    .run_claude(&claude, &["mcp", "remove", SERVER_NAME, "--scope", "user"])
                    .map(|_| ()),
                None => Ok(()),
            },
            Target::ClaudeDesktop => set_json_server(&self.desktop_config(), None),
            Target::Cursor => set_json_server(&self.cursor_config(), None),
            Target::Codex => set_codex_server(&self.codex_config(), None),
        }
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
                    return Err(SetupError::Cli("Zeitüberschreitung".into()));
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
    let bad_shape = || SetupError::Unparseable(path.to_owned(), "unerwartete Struktur".into());
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
                    SetupError::Unparseable(path.to_owned(), "mcp_servers ist keine Tabelle".into())
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
}
