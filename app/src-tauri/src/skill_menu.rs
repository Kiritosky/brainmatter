//! Brainmatter's commands and skills in the skill folders of connected tools, for
//! apps that show skills but not MCP prompts.
//!
//! A skill folder belongs to its tool and to the user. Brainmatter only changes
//! folders whose name starts with its prefix and whose `SKILL.md` carries its mark.
//! A folder of such a name without the mark is the user's, and its name is skipped.
//! Files are replaced atomically, and nothing is ever written through a link.
//!
//! Every sync converges on the full set, so a missed change is caught by the next.
//! A folder that several tools write holds what each of them may see. Cursor also
//! reads the folders of Claude Code and Codex, so while one of those has the menu,
//! Cursor gets no copy of its own: it would show every entry twice.

use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use bm_core::{Engine, model::Attachment, skills};
use bm_mcp::{Client, MenuItem};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::integrations::{Integrations, SERVER_NAME, Status, Target};

/// The longest skill name the format allows.
const MAX_NAME: usize = 64;
/// Front matter is short; a huge `SKILL.md` is not read further than this.
const MAX_HEAD: u64 = 64 * 1024;
/// A skill folder with this inside loads in Claude Code as a plugin, with hooks and
/// servers of its own. That is more than a skill, so it never goes to disk.
const PLUGIN_DIR: &str = ".claude-plugin";

/// Which tools get the menu, and which folders Brainmatter made for it. Kept next to
/// the vault, like the list of clients.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Saved {
    on: Vec<Target>,
    created: Vec<PathBuf>,
}

pub struct SkillMenus {
    path: PathBuf,
    /// Also keeps two syncs from running at once.
    state: Mutex<State>,
}

struct State {
    saved: Saved,
    /// Files that match their attachment, by the blob they were written from. Spares
    /// decrypting every file again on each change in the vault.
    written: HashMap<PathBuf, String>,
}

/// Where a tool finds its menu, and whose sets go there.
struct Plan {
    folders: Vec<(PathBuf, Vec<Target>)>,
    /// The folder each tool with the menu on reads it from.
    reads: HashMap<Target, PathBuf>,
}

impl SkillMenus {
    pub fn open(path: PathBuf) -> Self {
        let saved = fs::read(&path)
            .ok()
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        Self { path, state: Mutex::new(State { saved, written: HashMap::new() }) }
    }

    /// Switches the menu for `target`; [`Self::sync`] does the rest.
    pub fn set(&self, target: Target, on: bool) -> io::Result<()> {
        let mut state = self.state.lock();
        state.saved.on.retain(|t| *t != target);
        if on {
            state.saved.on.push(target);
        }
        self.save(&state.saved)
    }

    /// Brings every skill folder in line with the vault and the connected tools. With
    /// `fresh`, files are compared with their attachments again instead of trusting
    /// what this session wrote, as after unlocking.
    pub fn sync(&self, engine: &Engine, clients: &[Client], local: &Integrations, fresh: bool) {
        let mut state = self.state.lock();
        if fresh {
            state.written.clear();
        }
        let plan = plan(&state.saved.on, clients, local);
        for (dir, targets) in &plan.folders {
            let mut items: Vec<MenuItem> = Vec::new();
            let mut complete = true;
            for target in targets {
                let Some(client) = clients.iter().find(|c| c.name == target.name()) else {
                    continue;
                };
                match bm_mcp::skill_menu(engine, client, SERVER_NAME) {
                    Ok(menu) => {
                        let known: HashSet<String> = items.iter().map(|i| i.name.clone()).collect();
                        items.extend(menu.into_iter().filter(|i| !known.contains(&i.name)));
                    }
                    Err(_) => complete = false,
                }
            }
            // Half a set would remove what is merely unreadable for a moment.
            if !complete {
                tracing::warn!(
                    "skill menu for {} skipped: the vault could not be read",
                    local.tilde(dir)
                );
                continue;
            }
            let load = |a: &Attachment| engine.attachment_data(&a.id).map(|(_, bytes)| bytes);
            if let Err(e) = converge(dir, &items, &load, &mut state) {
                tracing::warn!("skill menu in {}: {}", local.tilde(dir), e.kind());
            }
        }
        if let Err(e) = self.save(&state.saved) {
            tracing::warn!("skill menu state: {}", e.kind());
        }
    }

    /// Fills in the skill menu part of each tool's status: where a connected tool
    /// with the menu on finds it, and how many entries it holds.
    pub fn annotate(&self, statuses: &mut [Status], clients: &[Client], local: &Integrations) {
        let plan = plan(&self.state.lock().saved.on, clients, local);
        for status in statuses {
            if let Some(dir) = plan.reads.get(&status.target) {
                status.skill_menu = true;
                status.skills_dir = Some(local.tilde(dir));
                status.skills = managed(dir).len();
            }
        }
    }

    fn save(&self, saved: &Saved) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(saved).expect("state serializes"))?;
        fs::rename(tmp, &self.path)
    }
}

/// Every folder a tool reads Brainmatter's menu from, with the tools whose sets go
/// there. Folders nobody gets anything in stay listed, so they are emptied.
fn plan(on: &[Target], clients: &[Client], local: &Integrations) -> Plan {
    let wanted = |t: &Target| on.contains(t) && clients.iter().any(|c| c.name == t.name());
    let mut folders: Vec<(PathBuf, Vec<Target>)> = Vec::new();
    let mut reads = HashMap::new();
    let mut add = |dir: PathBuf, target: Option<Target>| {
        let at = match folders.iter().position(|(d, _)| *d == dir) {
            Some(at) => at,
            None => {
                folders.push((dir, Vec::new()));
                folders.len() - 1
            }
        };
        folders[at].1.extend(target);
    };
    let writers: Vec<Target> =
        Target::ALL.into_iter().filter(|t| wanted(t) && local.also_reads(*t).is_empty()).collect();
    for target in Target::ALL {
        let Some(dir) = local.skills_dir(target) else { continue };
        if writers.contains(&target) {
            reads.insert(target, dir.clone());
            add(dir, Some(target));
            continue;
        }
        let covered = local
            .also_reads(target)
            .into_iter()
            .find(|d| writers.iter().any(|w| local.skills_dir(*w).as_ref() == Some(d)));
        match covered {
            Some(other) if wanted(&target) => {
                reads.insert(target, other);
                add(dir, None);
            }
            None if wanted(&target) => {
                reads.insert(target, dir.clone());
                add(dir, Some(target));
            }
            _ => add(dir, None),
        }
    }
    Plan { folders, reads }
}

/// `brainmatter-` and the prompt's name, as a valid skill name. A name too long for
/// the format is cut, and ends in a hash of the whole, so two never become one.
fn folder_name(name: &str) -> String {
    let full = format!("{SERVER_NAME}-{name}");
    if full.len() <= MAX_NAME {
        return full;
    }
    let hash = format!("{:08x}", fnv1a(full.as_bytes()));
    let head: String = full.chars().take(MAX_NAME - hash.len() - 1).collect();
    format!("{}-{hash}", head.trim_end_matches('-'))
}

fn fnv1a(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5, |hash, b| (hash ^ u32::from(*b)).wrapping_mul(0x0100_0193))
}

fn prefix() -> String {
    format!("{SERVER_NAME}-")
}

/// The names in `dir` that Brainmatter wrote.
fn managed(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|name| name.to_ascii_lowercase().starts_with(&prefix()) && is_ours(&dir.join(name)))
        .collect()
}

/// A real folder whose `SKILL.md` says Brainmatter wrote it.
fn is_ours(folder: &Path) -> bool {
    if !fs::symlink_metadata(folder).is_ok_and(|m| m.is_dir()) {
        return false;
    }
    let file = folder.join(skills::SKILL_FILE);
    if !fs::symlink_metadata(&file).is_ok_and(|m| m.is_file()) {
        return false;
    }
    let mut head = Vec::new();
    let read = fs::File::open(&file).and_then(|f| f.take(MAX_HEAD).read_to_end(&mut head));
    // The cut may fall inside a character; the front matter comes long before it.
    read.is_ok()
        && skills::parse(&String::from_utf8_lossy(&head)).managed_by.as_deref() == Some(SERVER_NAME)
}

/// An empty folder holds nothing to lose, such as one left by an interrupted sync.
fn is_empty_dir(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
        && fs::read_dir(path).is_ok_and(|mut e| e.next().is_none())
}

type Load<'a> = dyn Fn(&Attachment) -> bm_core::error::Result<Vec<u8>> + 'a;

/// Makes `dir` hold exactly `items` of Brainmatter's, and nothing else of it.
fn converge(dir: &Path, items: &[MenuItem], load: &Load<'_>, state: &mut State) -> io::Result<()> {
    if items.is_empty() && !dir.exists() {
        return Ok(());
    }
    if !items.is_empty() && !dir.exists() {
        let mut made = Vec::new();
        let mut at = Some(dir);
        while let Some(path) = at.filter(|p| !p.exists()) {
            made.push(path.to_path_buf());
            at = path.parent();
        }
        fs::create_dir_all(dir)?;
        state.saved.created.extend(made);
    }

    let mut wanted = HashSet::new();
    for item in items {
        let name = folder_name(&item.name);
        let folder = dir.join(&name);
        let free = !fs::exists(&folder)? || is_ours(&folder) || is_empty_dir(&folder);
        if !free || !wanted.insert(name) {
            continue;
        }
        if let Err(e) = write_skill(&folder, item, load, state) {
            tracing::warn!(
                "a skill folder in {} could not be written: {}",
                dir.display(),
                e.kind()
            );
        }
    }
    for name in managed(dir) {
        if !wanted.contains(&name) {
            let folder = dir.join(&name);
            fs::remove_dir_all(&folder)?;
            state.written.retain(|path, _| !path.starts_with(&folder));
        }
    }

    // Brainmatter's own folders go once they are empty, deepest first.
    if items.is_empty() {
        let mut path = Some(dir);
        while let Some(p) = path.filter(|p| state.saved.created.iter().any(|c| c == p)) {
            if !is_empty_dir(p) {
                break;
            }
            fs::remove_dir(p)?;
            state.saved.created.retain(|c| c != p);
            path = p.parent();
        }
    }
    Ok(())
}

/// One skill folder. `SKILL.md` comes first, so an interrupted write still leaves a
/// folder that is recognisably Brainmatter's.
fn write_skill(
    folder: &Path,
    item: &MenuItem,
    load: &Load<'_>,
    state: &mut State,
) -> io::Result<()> {
    if !fs::exists(folder)? {
        fs::create_dir(folder)?;
    }
    let name = folder.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let text = skills::render_managed(name, &item.description, SERVER_NAME, &item.body);
    put(&folder.join(skills::SKILL_FILE), text.as_bytes())?;

    let mut keep: HashSet<String> = HashSet::from([skills::SKILL_FILE.to_ascii_lowercase()]);
    let files = files_to_write(&item.files);
    for (path, attachment) in &files {
        keep.insert(path.to_ascii_lowercase());
        let target = folder.join(path);
        let unchanged = state.written.get(&target) == Some(&attachment.blob_id)
            && fs::symlink_metadata(&target)
                .is_ok_and(|m| m.is_file() && m.len() == attachment.size as u64);
        if unchanged {
            continue;
        }
        let bytes = load(attachment).map_err(io::Error::other)?;
        make_dirs(folder, path)?;
        put(&target, &bytes)?;
        state.written.insert(target, attachment.blob_id.clone());
    }
    prune(folder, folder, &keep)
}

/// The files that can go to disk, by their path in the folder. Of two at the same
/// path, or at paths a Mac takes for the same, the newer wins.
fn files_to_write(files: &[Attachment]) -> Vec<(String, &Attachment)> {
    let mut newest: Vec<&Attachment> = files.iter().collect();
    newest.sort_by_key(|f| std::cmp::Reverse(f.created_at));
    let mut out: Vec<(String, &Attachment)> = Vec::new();
    let mut seen = HashSet::new();
    for file in newest {
        let Some(path) = skills::clean_path(&file.filename) else { continue };
        let top = path.split('/').next().unwrap_or_default();
        if top.eq_ignore_ascii_case(PLUGIN_DIR) || !seen.insert(path.to_ascii_lowercase()) {
            continue;
        }
        out.push((path, file));
    }
    // A file cannot also be a folder of another one.
    let all: HashSet<String> = out.iter().map(|(p, _)| p.to_ascii_lowercase()).collect();
    out.retain(|(path, _)| {
        let lower = path.to_ascii_lowercase();
        !lower.match_indices('/').any(|(i, _)| all.contains(&lower[..i]))
    });
    out
}

/// The folders above `path` inside `root`, real ones only: whatever else stands in
/// their place inside a folder of Brainmatter's goes.
fn make_dirs(root: &Path, path: &str) -> io::Result<()> {
    let mut at = root.to_path_buf();
    let parts: Vec<&str> = path.split('/').collect();
    for part in &parts[..parts.len() - 1] {
        at.push(part);
        match fs::symlink_metadata(&at) {
            Ok(m) if m.is_dir() => continue,
            Ok(_) => fs::remove_file(&at)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        fs::create_dir(&at)?;
    }
    Ok(())
}

/// Writes `bytes` unless the file holds them already: to a temporary file next to
/// it, then renamed over it. A link in its place is replaced, never followed.
fn put(path: &Path, bytes: &[u8]) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && m.len() == bytes.len() as u64 && fs::read(path)? == bytes => {
            return Ok(());
        }
        Ok(m) if m.is_dir() => fs::remove_dir_all(path)?,
        _ => {}
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{SERVER_NAME}-tmp"));
    let _ = fs::remove_file(&tmp);
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&tmp, path)
}

/// Removes what is in `dir` but not in `keep`, which holds lowercase paths relative
/// to `root`, and folders left empty.
fn prune(root: &Path, dir: &Path, keep: &HashSet<String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let relative =
            path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_ascii_lowercase();
        let meta = fs::symlink_metadata(&path)?;
        if meta.is_dir() {
            let below = format!("{relative}/");
            if keep.iter().any(|k| k.starts_with(&below)) {
                prune(root, &path, keep)?;
            } else {
                fs::remove_dir_all(&path)?;
            }
        } else if !keep.contains(&relative) {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_core::{
        Vault,
        blobs::BlobStore,
        ids::NodeId,
        model::{ActorKind, Ctx, NewNode, NewProject, SkillInput},
        vault::KdfParams,
    };
    use bm_mcp::{ClientRegistry, Scope};

    struct Fixture {
        _dir: tempfile::TempDir,
        engine: Engine,
        user: Ctx,
        registry: ClientRegistry,
        local: Integrations,
        menus: SkillMenus,
    }

    fn setup() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let (vault, keys, _) =
            Vault::create(&dir.path().join("vault"), "pw", KdfParams::insecure_fast()).unwrap();
        let blobs = BlobStore::new(dir.path().join("blobs"), &keys.blob_key());
        let engine = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap().with_blobs(blobs);
        let user = Ctx::new(engine.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
        let local = Integrations {
            home: dir.path().join("home"),
            applications: dir.path().join("Applications"),
            url: "http://127.0.0.1:47474/mcp".into(),
            bridge: dir.path().join("brainmatter-mcp"),
            claude_config_dir: None,
        };
        fs::create_dir_all(&local.home).unwrap();
        let menus = SkillMenus::open(dir.path().join("vault/skill-menus.json"));
        Fixture { _dir: dir, engine, user, registry: ClientRegistry::in_memory(), local, menus }
    }

    impl Fixture {
        fn connect(&self, target: Target, scopes: &[Scope], projects: &[bm_core::ids::ProjectId]) {
            self.registry.create(target.name(), scopes, projects).unwrap();
            self.menus.set(target, true).unwrap();
        }

        fn sync(&self) {
            self.menus.sync(&self.engine, &self.registry.list(), &self.local, false);
        }

        fn dir(&self, target: Target) -> PathBuf {
            self.local.skills_dir(target).unwrap()
        }

        fn names(&self, target: Target) -> Vec<String> {
            let mut names = managed(&self.dir(target));
            names.sort();
            names
        }

        fn skill(&self, title: &str, projects: Vec<bm_core::ids::ProjectId>) -> NodeId {
            let input = SkillInput {
                id: None,
                title: title.into(),
                description: format!("Use for {title}"),
                body: format!("Steps for {title}"),
                projects,
            };
            self.engine.save_skill(&self.user, input).unwrap().node.id
        }

        fn attach(&self, id: &NodeId, path: &str, content: &str) {
            self.engine.add_attachment(&self.user, id, path, None, content.as_bytes()).unwrap();
        }

        fn status(&self, target: Target) -> Status {
            let mut all: Vec<Status> =
                Target::ALL.into_iter().map(|t| self.local.status(t)).collect();
            self.menus.annotate(&mut all, &self.registry.list(), &self.local);
            all.into_iter().find(|s| s.target == target).unwrap()
        }
    }

    fn named(name: &str) -> String {
        format!("{SERVER_NAME}-{name}")
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn a_client_gets_what_it_may_see_and_nothing_of_anyone_else_is_touched() {
        let f = setup();
        let work = f
            .engine
            .create_project(&f.user, NewProject { name: "Work".into(), ..Default::default() })
            .unwrap()
            .id;
        let own = f.skill("Release Notes", vec![]);
        f.attach(&own, "template.md", "secret");
        f.skill("Only Work", vec![work.clone()]);
        let input = NewNode {
            title: "Weekly".into(),
            body: "Week {{week}}".into(),
            kind: Some("template".into()),
            ..Default::default()
        };
        f.engine.create_node(&f.user, input).unwrap();

        let dir = f.dir(Target::ClaudeCode);
        fs::create_dir_all(dir.join("mine")).unwrap();
        fs::write(dir.join("mine/SKILL.md"), "---\nname: mine\n---\n").unwrap();
        // Named like one of Brainmatter's, but not marked: the user's.
        fs::create_dir_all(dir.join(named("recall"))).unwrap();
        fs::write(dir.join(named("recall")).join("SKILL.md"), "mine too").unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), dir.join(named("tidy-project"))).unwrap();

        f.connect(Target::ClaudeCode, &[Scope::Read, Scope::Write], &[]);
        f.sync();
        let mut expected: Vec<String> = [
            "setup-project",
            "update-docs",
            "capture-session",
            "weekly",
            "skill-release-notes",
            "skill-only-work",
        ]
        .iter()
        .map(|n| named(n))
        .collect();
        expected.sort();
        assert_eq!(f.names(Target::ClaudeCode), expected);
        assert_eq!(read(&dir.join("mine/SKILL.md")), "---\nname: mine\n---\n");
        assert_eq!(read(&dir.join(named("recall")).join("SKILL.md")), "mine too");
        assert!(fs::read_dir(elsewhere.path()).unwrap().next().is_none(), "never through a link");

        let release = read(&dir.join(named("skill-release-notes")).join("SKILL.md"));
        let parsed = skills::parse(&release);
        assert_eq!(parsed.name.as_deref(), Some(named("skill-release-notes").as_str()));
        assert_eq!(parsed.description, "Use for Release Notes");
        assert!(parsed.body.contains("`get_skill`") && parsed.body.contains("`release-notes`"));
        assert!(
            !release.contains("Steps for")
                && !dir.join(named("skill-release-notes")).join("template.md").exists()
        );
        let weekly = skills::parse(&read(&dir.join(named("weekly")).join("SKILL.md")));
        assert!(weekly.body.contains("`get_template`") && weekly.body.contains("`week`"));

        let status = f.status(Target::ClaudeCode);
        assert!(status.skill_menu);
        assert_eq!((status.skills_dir.as_deref(), status.skills), (Some("~/.claude/skills"), 6));
        assert!(!f.status(Target::Codex).skill_menu);

        // Switched off: Brainmatter's folders go, the others stay.
        f.menus.set(Target::ClaudeCode, false).unwrap();
        f.sync();
        assert!(f.names(Target::ClaudeCode).is_empty());
        let left: HashSet<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(left, HashSet::from(["mine".into(), named("recall"), named("tidy-project")]));
    }

    #[test]
    fn a_skill_on_disk_comes_with_its_files_and_follows_every_change() {
        let f = setup();
        let id = f.skill("PDF", vec![]);
        f.attach(&id, "scripts/fill.py", "print(1)");
        f.attach(&id, "ref/forms.md", "forms");
        f.attach(&id, "Übersicht.md", "overview");
        f.attach(&id, ".claude-plugin/plugin.json", "{}");
        f.engine.set_skill_on_disk(&f.user, &id, true).unwrap();
        f.connect(Target::Codex, &[Scope::Read], &[]);
        f.sync();

        let folder = f.dir(Target::Codex).join(named("skill-pdf"));
        let text = read(&folder.join("SKILL.md"));
        assert!(text.contains("Steps for PDF") && text.contains("managed-by"), "{text}");
        assert_eq!(read(&folder.join("scripts/fill.py")), "print(1)");
        assert_eq!(read(&folder.join("ref/forms.md")), "forms");
        assert_eq!(read(&folder.join("Übersicht.md")), "overview");
        assert!(!folder.join(".claude-plugin").exists(), "no plugin through the back door");

        // Edited, a file swapped, one removed, and something stray inside.
        let skill = f.engine.skill(&id).unwrap();
        let edit = SkillInput {
            id: Some(id.clone()),
            title: "PDF".into(),
            description: "Fill: \"forms\"\n---".into(),
            body: "New steps".into(),
            projects: vec![],
        };
        f.engine.save_skill(&f.user, edit).unwrap();
        for file in &skill.files {
            if file.filename != "ref/forms.md" {
                f.engine.delete_attachment(&f.user, &file.id).unwrap();
            }
        }
        f.attach(&id, "scripts/fill.py", "print(2)");
        let forms = skill.files.iter().find(|a| a.filename == "ref/forms.md").unwrap();
        f.engine.delete_attachment(&f.user, &forms.id).unwrap();
        fs::write(folder.join("stray.txt"), "x").unwrap();
        let linked = tempfile::tempdir().unwrap();
        fs::remove_file(folder.join("scripts/fill.py")).unwrap();
        std::os::unix::fs::symlink(linked.path().join("target.py"), folder.join("scripts/fill.py"))
            .unwrap();
        f.sync();

        let parsed = skills::parse(&read(&folder.join("SKILL.md")));
        assert_eq!(
            (parsed.description.as_str(), parsed.body.as_str()),
            ("Fill: \"forms\"\n---", "New steps")
        );
        assert!(fs::symlink_metadata(folder.join("scripts/fill.py")).unwrap().is_file());
        assert_eq!(read(&folder.join("scripts/fill.py")), "print(2)");
        assert!(!linked.path().join("target.py").exists(), "the link was replaced, not followed");
        assert!(!folder.join("ref").exists() && !folder.join("stray.txt").exists());

        // Back into the vault: only a pointer stays on disk.
        f.engine.set_skill_on_disk(&f.user, &id, false).unwrap();
        f.sync();
        assert!(!folder.join("scripts").exists());
        assert!(read(&folder.join("SKILL.md")).contains("`get_skill`"));

        // Trashed, it leaves; restored, it is back.
        f.engine.delete_node(&f.user, &id).unwrap();
        f.sync();
        assert!(!folder.exists());
        f.engine.restore_node(&f.user, &id).unwrap();
        f.sync();
        assert!(folder.exists());
    }

    #[test]
    fn cursor_reads_the_folder_of_claude_code_and_gets_no_second_copy() {
        let f = setup();
        let work = f
            .engine
            .create_project(&f.user, NewProject { name: "Work".into(), ..Default::default() })
            .unwrap()
            .id;
        let other = f
            .engine
            .create_project(&f.user, NewProject { name: "Other".into(), ..Default::default() })
            .unwrap()
            .id;
        f.skill("At Work", vec![work.clone()]);
        f.skill("Elsewhere", vec![other.clone()]);

        // Cursor alone writes into its own folder.
        f.connect(Target::Cursor, &[Scope::Read], std::slice::from_ref(&other));
        f.sync();
        assert_eq!(f.names(Target::Cursor), [named("recall"), named("skill-elsewhere")]);
        assert_eq!(f.status(Target::Cursor).skills_dir.as_deref(), Some("~/.cursor/skills"));

        // Claude Code's folder is read by Cursor too, so Cursor's copy goes. The
        // folder holds what Claude Code may see, nothing of Cursor's on top.
        f.connect(Target::ClaudeCode, &[Scope::Read, Scope::Write], std::slice::from_ref(&work));
        f.sync();
        assert!(f.names(Target::Cursor).is_empty());
        let shared = f.names(Target::ClaudeCode);
        assert!(
            shared.contains(&named("skill-at-work")) && !shared.contains(&named("skill-elsewhere"))
        );
        let cursor = f.status(Target::Cursor);
        assert!(cursor.skill_menu);
        assert_eq!(
            (cursor.skills_dir.as_deref(), cursor.skills),
            (Some("~/.claude/skills"), shared.len())
        );

        // Syncing again changes nothing: the two do not fight.
        let file = f.dir(Target::ClaudeCode).join(named("skill-at-work")).join("SKILL.md");
        let before = fs::metadata(&file).unwrap().modified().unwrap();
        f.sync();
        assert_eq!(f.names(Target::ClaudeCode), shared);
        assert!(f.names(Target::Cursor).is_empty());
        assert_eq!(fs::metadata(&file).unwrap().modified().unwrap(), before);

        // Claude Code switched off: Cursor gets its own folder back.
        f.menus.set(Target::ClaudeCode, false).unwrap();
        f.sync();
        assert!(f.names(Target::ClaudeCode).is_empty());
        assert_eq!(f.names(Target::Cursor), [named("recall"), named("skill-elsewhere")]);
    }

    #[test]
    fn folders_brainmatter_made_go_again_when_empty() {
        let f = setup();
        f.connect(Target::Codex, &[Scope::Read], &[]);
        f.sync();
        let dir = f.dir(Target::Codex);
        assert_eq!(f.names(Target::Codex), [named("recall")]);

        // The state survives a restart of the app.
        let menus = SkillMenus::open(f.menus.path.clone());
        assert_eq!(menus.state.lock().saved.on, [Target::Codex]);
        menus.set(Target::Codex, false).unwrap();
        menus.sync(&f.engine, &f.registry.list(), &f.local, true);
        assert!(!dir.exists() && !f.local.home.join(".agents").exists());

        // A folder that was there before stays, even empty.
        fs::create_dir_all(f.dir(Target::Cursor)).unwrap();
        f.connect(Target::Cursor, &[Scope::Read], &[]);
        f.sync();
        f.menus.set(Target::Cursor, false).unwrap();
        f.sync();
        assert!(f.dir(Target::Cursor).is_dir());
        assert!(f.names(Target::Cursor).is_empty());
    }

    #[test]
    fn long_names_are_cut_without_meeting() {
        let a = folder_name(&format!("skill-{}-one", "x".repeat(70)));
        let b = folder_name(&format!("skill-{}-two", "x".repeat(70)));
        for name in [&a, &b] {
            assert!(name.len() <= MAX_NAME && name.starts_with(&prefix()), "{name}");
            assert!(!name.contains("--") && !name.ends_with('-'), "{name}");
        }
        assert_ne!(a, b);
        assert_eq!(folder_name("recall"), named("recall"));
        let longest = format!("skill-{}", "y".repeat(MAX_NAME - prefix().len() - 6));
        assert_eq!(folder_name(&longest), named(&longest));
    }
}
