//! Skills from GitHub: which skills a URL leads to, a preview of them, and fetching
//! those the user picked. The network sits behind [`Source`], so all of this is
//! tested with fixtures; [`crate::github`] is the real one.
//!
//! Fetching happens before the vault is asked, so a slow connection never holds the
//! unlocked core.

use std::future::Future;

use bm_core::{
    CoreError, Engine,
    ids::NodeId,
    model::{SkillImport, SkillSource},
    skills::{self, GithubUrl, SkillFilePreview, TreeEntry, TreeProblem},
    templates::slug,
};
use serde::Serialize;

use crate::{error::ApiError, i18n::german};

/// Branch names may contain `/`; a ref longer than this is not tried.
const MAX_REF_PARTS: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillImportPreview {
    pub url: String,
    /// `owner/name`
    pub repo: String,
    /// The full commit SHA the preview shows, to import exactly that.
    pub revision: String,
    pub skills: Vec<SkillCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkillCandidate {
    /// The folder of its `SKILL.md`, empty for the repository root.
    pub path: String,
    pub name: String,
    pub description: String,
    pub body: String,
    /// Relative to the skill folder, without the `SKILL.md`.
    pub files: Vec<SkillFilePreview>,
    /// All files, the `SKILL.md` included.
    pub size: u64,
    /// The skill an import would update.
    pub existing: Option<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    InvalidUrl,
    RepoNotFound,
    /// The branch, tag, commit, folder or file is not there.
    PathNotFound,
    NoSkill,
    TooMany,
    TooLarge,
    /// GitHub listed only part of the repository.
    Truncated,
    /// Unix seconds when GitHub takes requests again, if it said.
    RateLimited(Option<i64>),
    Offline,
    Timeout,
    Status(u16),
    /// Two chosen skills would get the same name.
    SameName(String),
    Unexpected(String),
}

/// Where the skills are: a repository on GitHub, or a fixture in tests.
pub trait Source: Sync {
    /// The repository's default branch.
    fn default_branch(
        &self,
        repo: &str,
    ) -> impl Future<Output = Result<String, ImportError>> + Send;

    /// The full SHA of the commit a branch, tag or SHA names; `None` if it names none.
    fn commit(
        &self,
        repo: &str,
        reference: &str,
    ) -> impl Future<Output = Result<Option<String>, ImportError>> + Send;

    /// Everything below `folder` (the whole repository for `""`), with paths from
    /// the repository root.
    fn tree(
        &self,
        repo: &str,
        revision: &str,
        folder: &str,
    ) -> impl Future<Output = Result<Vec<TreeEntry>, ImportError>> + Send;

    /// A file's content; [`ImportError::TooLarge`] beyond `limit` bytes.
    fn file(
        &self,
        repo: &str,
        revision: &str,
        path: &str,
        limit: u64,
    ) -> impl Future<Output = Result<Vec<u8>, ImportError>> + Send;
}

/// The commit and the folder a URL points at.
async fn locate<S: Source>(source: &S, url: &GithubUrl) -> Result<(String, String), ImportError> {
    let repo = url.repo_name();
    if url.rest.is_empty() {
        let branch = source.default_branch(&repo).await?;
        let revision = source.commit(&repo, &branch).await?.ok_or(ImportError::PathNotFound)?;
        return Ok((revision, String::new()));
    }
    let parts = match url.single {
        true => &url.rest[..url.rest.len() - 1],
        false => &url.rest[..],
    };
    // The ref is the shortest leading part the repository knows.
    for split in 1..=parts.len().min(MAX_REF_PARTS) {
        if let Some(revision) = source.commit(&repo, &parts[..split].join("/")).await? {
            return Ok((revision, parts[split..].join("/")));
        }
    }
    // Tells a missing repository from a missing branch.
    source.default_branch(&repo).await?;
    Err(ImportError::PathNotFound)
}

fn problem(p: TreeProblem) -> ImportError {
    match p {
        TreeProblem::NoSkill => ImportError::NoSkill,
        TreeProblem::TooMany => ImportError::TooMany,
        TreeProblem::TooLarge => ImportError::TooLarge,
    }
}

/// Name, description and text of a skill, or `None` if its `SKILL.md` is no valid one.
fn read_skill(bytes: &[u8], folder: &str, repo: &str) -> Option<(String, String, String)> {
    let text = skills::parse(&String::from_utf8_lossy(bytes));
    if text.description.is_empty() {
        return None;
    }
    let folder_name = folder.rsplit('/').next().filter(|n| !n.is_empty());
    let repo_name = repo.rsplit('/').next();
    let name = [text.name.as_deref(), folder_name, repo_name]
        .into_iter()
        .flatten()
        .find(|n| !slug(n).is_empty())?
        .to_string();
    Some((name, text.description, text.body))
}

/// The skills at `input`, fetching only their `SKILL.md` files. The rest is listed
/// with the sizes the repository gives.
pub async fn preview<S: Source>(
    source: &S,
    input: &str,
) -> Result<SkillImportPreview, ImportError> {
    let url = skills::parse_url(input).ok_or(ImportError::InvalidUrl)?;
    let repo = url.repo_name();
    let (revision, folder) = locate(source, &url).await?;
    let tree = source.tree(&repo, &revision, &folder).await?;
    let folders = skills::skill_folders(&tree, &folder, url.single).map_err(problem)?;
    let mut found = Vec::with_capacity(folders.len());
    for skill in folders {
        let bytes =
            source.file(&repo, &revision, &skill.skill_file, skills::MAX_SKILL_BYTES).await?;
        let Some((name, description, body)) = read_skill(&bytes, &skill.path, &repo) else {
            continue;
        };
        found.push(SkillCandidate {
            path: skill.path,
            name,
            description,
            body,
            files: skill.files,
            size: skill.size,
            existing: None,
        });
    }
    if found.is_empty() {
        return Err(ImportError::NoSkill);
    }
    Ok(SkillImportPreview { url: input.trim().to_string(), repo, revision, skills: found })
}

/// Names the skills in the vault an import would update.
pub fn mark_existing(preview: &mut SkillImportPreview, engine: &Engine) -> Result<(), CoreError> {
    for skill in &mut preview.skills {
        skill.existing = engine.skill_to_update(&preview.repo, &skill.path, &slug(&skill.name))?;
    }
    Ok(())
}

/// The folder all `paths` are in.
fn common_folder(paths: &[String]) -> String {
    let mut common: Vec<&str> = paths[0].split('/').filter(|p| !p.is_empty()).collect();
    for path in &paths[1..] {
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let same = common.iter().zip(&parts).take_while(|(a, b)| a == b).count();
        common.truncate(same);
    }
    common.join("/")
}

/// Fetches the skills at `paths`, as a preview of `input` at `revision` listed them,
/// with all their files.
pub async fn fetch<S: Source>(
    source: &S,
    input: &str,
    revision: &str,
    paths: &[String],
) -> Result<Vec<SkillImport>, ImportError> {
    let url = skills::parse_url(input).ok_or(ImportError::InvalidUrl)?;
    let is_sha = revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit());
    if !is_sha {
        return Err(ImportError::PathNotFound);
    }
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let repo = url.repo_name();
    let folder = common_folder(paths);
    let tree = source.tree(&repo, revision, &folder).await?;
    let folders = skills::skill_folders(&tree, &folder, false).map_err(problem)?;
    let imported_at = bm_core::time::now_ms();

    let mut out: Vec<SkillImport> = Vec::with_capacity(paths.len());
    for path in paths {
        let path = path.trim_matches('/');
        let found = folders.iter().find(|f| f.path == path).ok_or(ImportError::PathNotFound)?;
        let mut budget = skills::MAX_SKILL_BYTES;
        let text = source.file(&repo, revision, &found.skill_file, budget).await?;
        budget -= text.len() as u64;
        let (name, description, body) =
            read_skill(&text, &found.path, &repo).ok_or(ImportError::NoSkill)?;
        if out.iter().any(|s| slug(&s.title) == slug(&name)) {
            return Err(ImportError::SameName(name));
        }
        let mut files = Vec::with_capacity(found.files.len());
        for file in &found.files {
            let full = match found.path.is_empty() {
                true => file.path.clone(),
                false => format!("{}/{}", found.path, file.path),
            };
            let bytes = source.file(&repo, revision, &full, budget).await?;
            budget -= bytes.len() as u64;
            files.push((file.path.clone(), bytes));
        }
        out.push(SkillImport {
            title: name,
            description,
            body,
            files,
            source: SkillSource {
                url: input.trim().to_string(),
                repo: repo.clone(),
                path: path.to_string(),
                revision: revision.to_string(),
                imported_at,
            },
        });
    }
    Ok(out)
}

impl From<ImportError> for ApiError {
    fn from(e: ImportError) -> Self {
        let de = german();
        let text = |german: &str, english: &str| match de {
            true => german.to_string(),
            false => english.to_string(),
        };
        match e {
            ImportError::InvalidUrl => Self::new(
                "invalid_url",
                text(
                    "Diese Adresse kann Brainmatter nicht lesen. Möglich sind github.com/Besitzer/Repository, ein Ordner darin (…/tree/…) oder eine SKILL.md darin (…/blob/…).",
                    "Brainmatter cannot read this address. Use github.com/owner/repository, a folder in it (…/tree/…) or a SKILL.md in it (…/blob/…).",
                ),
            ),
            ImportError::RepoNotFound => Self::new(
                "not_found",
                text(
                    "Dieses Repository gibt es nicht, oder es ist privat. Brainmatter kann nur öffentliche Repositories lesen.",
                    "This repository does not exist or is private. Brainmatter can only read public repositories.",
                ),
            ),
            ImportError::PathNotFound => Self::new(
                "not_found",
                text(
                    "Diesen Branch oder Ordner gibt es in dem Repository nicht.",
                    "This branch or folder does not exist in the repository.",
                ),
            ),
            ImportError::NoSkill => Self::new(
                "no_skill",
                text(
                    "Dort liegt kein Skill: keine SKILL.md mit name und description.",
                    "There is no skill there: no SKILL.md with a name and a description.",
                ),
            ),
            ImportError::TooMany => Self::new(
                "too_large",
                match de {
                    true => format!(
                        "Dort liegen mehr als {} Skills. Bitte die Adresse eines Ordners darin angeben.",
                        skills::MAX_SKILLS
                    ),
                    false => format!(
                        "There are more than {} skills there. Please use the address of a folder inside.",
                        skills::MAX_SKILLS
                    ),
                },
            ),
            ImportError::TooLarge => Self::new(
                "too_large",
                match de {
                    true => format!(
                        "Die Skills dort sind zu groß. Ein Skill darf höchstens {} Dateien und {} MB haben.",
                        skills::MAX_FILES,
                        skills::MAX_SKILL_BYTES / (1024 * 1024)
                    ),
                    false => format!(
                        "The skills there are too large. A skill may have at most {} files and {} MB.",
                        skills::MAX_FILES,
                        skills::MAX_SKILL_BYTES / (1024 * 1024)
                    ),
                },
            ),
            ImportError::Truncated => Self::new(
                "too_large",
                text(
                    "Das Repository ist zu groß, um es ganz zu lesen. Bitte die Adresse des Ordners mit den Skills angeben.",
                    "The repository is too large to read as a whole. Please use the address of the folder with the skills.",
                ),
            ),
            ImportError::RateLimited(reset) => {
                let minutes = reset.map(|at| {
                    let seconds = at - bm_core::time::now_ms() / 1000;
                    (seconds.max(1) + 59) / 60
                });
                let message = match (de, minutes) {
                    (true, Some(1)) => "GitHub nimmt ohne Anmeldung gerade keine Anfragen mehr an. In etwa einer Minute geht es wieder.".to_string(),
                    (true, Some(n)) => format!("GitHub nimmt ohne Anmeldung gerade keine Anfragen mehr an. In etwa {n} Minuten geht es wieder."),
                    (true, None) => "GitHub nimmt ohne Anmeldung gerade keine Anfragen mehr an. Bitte später noch einmal versuchen.".to_string(),
                    (false, Some(1)) => "GitHub takes no more requests without signing in right now. Try again in about a minute.".to_string(),
                    (false, Some(n)) => format!("GitHub takes no more requests without signing in right now. Try again in about {n} minutes."),
                    (false, None) => "GitHub takes no more requests without signing in right now. Please try again later.".to_string(),
                };
                Self::new("rate_limited", message)
            }
            ImportError::Offline => Self::new(
                "offline",
                text(
                    "GitHub ist nicht erreichbar. Besteht eine Internetverbindung?",
                    "GitHub cannot be reached. Are you online?",
                ),
            ),
            ImportError::Timeout => Self::new(
                "offline",
                text(
                    "GitHub hat nicht rechtzeitig geantwortet. Bitte noch einmal versuchen.",
                    "GitHub did not answer in time. Please try again.",
                ),
            ),
            ImportError::Status(status) => Self::new(
                "github",
                match de {
                    true => format!("GitHub hat mit einem Fehler geantwortet ({status})."),
                    false => format!("GitHub answered with an error ({status})."),
                },
            ),
            ImportError::SameName(name) => Self::new(
                "conflict",
                match de {
                    true => format!(
                        "Zwei der gewählten Skills heißen „{name}“. Bitte nur einen davon wählen."
                    ),
                    false => format!(
                        "Two of the chosen skills are named “{name}”. Please choose only one of them."
                    ),
                },
            ),
            ImportError::Unexpected(detail) => {
                tracing::warn!("unexpected answer from GitHub: {detail}");
                Self::new(
                    "github",
                    text("GitHub hat unerwartet geantwortet.", "GitHub gave an unexpected answer."),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Mutex};

    use bm_core::{
        Vault,
        model::{ActorKind, Ctx, SkillInput},
        vault::KdfParams,
    };

    use super::*;

    const MAIN: &str = "1111111111111111111111111111111111111111";
    const FEATURE: &str = "2222222222222222222222222222222222222222";

    /// A repository in memory, which counts what is asked of it.
    #[derive(Default)]
    struct Repo {
        name: &'static str,
        refs: HashMap<&'static str, &'static str>,
        files: Vec<(String, Vec<u8>)>,
        /// Sizes the listing gives, where they differ from the content.
        listed: HashMap<String, u64>,
        truncated: bool,
        fetched: Mutex<Vec<String>>,
    }

    impl Repo {
        fn new(files: &[(&str, &str)]) -> Self {
            let files = files.iter().map(|(p, c)| (p.to_string(), c.as_bytes().to_vec())).collect();
            let refs = HashMap::from([("main", MAIN), ("feature/x", FEATURE), (MAIN, MAIN)]);
            Repo { name: "o/r", refs, files, ..Default::default() }
        }

        fn known(&self, repo: &str) -> Result<(), ImportError> {
            if repo == self.name { Ok(()) } else { Err(ImportError::RepoNotFound) }
        }

        fn fetched(&self) -> Vec<String> {
            self.fetched.lock().unwrap().clone()
        }
    }

    impl Source for Repo {
        async fn default_branch(&self, repo: &str) -> Result<String, ImportError> {
            self.known(repo).map(|_| "main".to_string())
        }

        async fn commit(&self, repo: &str, reference: &str) -> Result<Option<String>, ImportError> {
            Ok(self.known(repo).ok().and_then(|_| self.refs.get(reference)).map(|s| s.to_string()))
        }

        async fn tree(
            &self,
            repo: &str,
            _revision: &str,
            folder: &str,
        ) -> Result<Vec<TreeEntry>, ImportError> {
            self.known(repo)?;
            if self.truncated {
                return Err(ImportError::Truncated);
            }
            let inside = |p: &str| folder.is_empty() || p.starts_with(&format!("{folder}/"));
            let entries: Vec<TreeEntry> = self
                .files
                .iter()
                .filter(|(p, _)| inside(p))
                .map(|(path, content)| TreeEntry {
                    path: path.clone(),
                    mode: "100644".into(),
                    kind: "blob".into(),
                    size: self.listed.get(path).copied().unwrap_or(content.len() as u64),
                })
                .collect();
            if entries.is_empty() && !folder.is_empty() {
                return Err(ImportError::PathNotFound);
            }
            Ok(entries)
        }

        async fn file(
            &self,
            repo: &str,
            _revision: &str,
            path: &str,
            limit: u64,
        ) -> Result<Vec<u8>, ImportError> {
            self.known(repo)?;
            self.fetched.lock().unwrap().push(path.to_string());
            let (_, content) =
                self.files.iter().find(|(p, _)| p == path).ok_or(ImportError::PathNotFound)?;
            if content.len() as u64 > limit {
                return Err(ImportError::TooLarge);
            }
            Ok(content.clone())
        }
    }

    fn skill_md(name: &str, description: &str) -> String {
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n")
    }

    fn sample() -> Repo {
        Repo::new(&[
            ("README.md", "readme"),
            ("skills/pdf/SKILL.md", &skill_md("pdf", "Fill PDF forms.")),
            ("skills/pdf/scripts/fill.py", "print(1)"),
            ("skills/docx/SKILL.md", &skill_md("docx", "Edit Word files.")),
            ("skills/broken/SKILL.md", "no front matter"),
        ])
    }

    fn run<T>(future: impl Future<Output = T>) -> T {
        tauri::async_runtime::block_on(future)
    }

    fn paths(p: &SkillImportPreview) -> Vec<&str> {
        p.skills.iter().map(|s| s.path.as_str()).collect()
    }

    #[test]
    fn a_preview_reads_only_the_skill_files() {
        let repo = sample();
        let p = run(preview(&repo, " https://github.com/o/r ")).unwrap();
        assert_eq!(
            (p.url.as_str(), p.repo.as_str(), p.revision.as_str()),
            ("https://github.com/o/r", "o/r", MAIN)
        );
        assert_eq!(
            paths(&p),
            ["skills/docx", "skills/pdf"],
            "a SKILL.md without a description is none"
        );
        let pdf = &p.skills[1];
        assert_eq!(
            (pdf.name.as_str(), pdf.description.as_str(), pdf.body.as_str()),
            ("pdf", "Fill PDF forms.", "# pdf")
        );
        assert_eq!(pdf.files, [SkillFilePreview { path: "scripts/fill.py".into(), size: 8 }]);
        assert_eq!(pdf.size, 8 + skill_md("pdf", "Fill PDF forms.").len() as u64);
        assert!(repo.fetched().iter().all(|f| f.ends_with("SKILL.md")), "{:?}", repo.fetched());
    }

    #[test]
    fn refs_may_contain_slashes_and_a_link_may_name_one_skill() {
        let repo = sample();
        let p = run(preview(&repo, "https://github.com/o/r/tree/feature/x/skills/pdf")).unwrap();
        assert_eq!((p.revision.as_str(), paths(&p)), (FEATURE, vec!["skills/pdf"]));
        let p =
            run(preview(&repo, "https://github.com/o/r/blob/main/skills/docx/SKILL.md")).unwrap();
        assert_eq!(paths(&p), ["skills/docx"]);
        let p =
            run(preview(&repo, &format!("https://github.com/o/r/tree/{MAIN}/skills/"))).unwrap();
        assert_eq!(paths(&p), ["skills/docx", "skills/pdf"]);
    }

    #[test]
    fn says_what_went_wrong() {
        let repo = sample();
        let err = |url: &str| run(preview(&repo, url)).unwrap_err();
        assert_eq!(err("https://gitlab.com/o/r"), ImportError::InvalidUrl);
        assert_eq!(err("o/missing"), ImportError::RepoNotFound);
        assert_eq!(err("https://github.com/o/missing/tree/main/x"), ImportError::RepoNotFound);
        assert_eq!(err("https://github.com/o/r/tree/nope/skills"), ImportError::PathNotFound);
        assert_eq!(err("https://github.com/o/r/tree/main/nowhere"), ImportError::PathNotFound);
        assert_eq!(err("https://github.com/o/r/tree/main/skills/broken"), ImportError::NoSkill);
        let empty = Repo::new(&[("README.md", "x")]);
        assert_eq!(run(preview(&empty, "o/r")).unwrap_err(), ImportError::NoSkill);
        let big = Repo { truncated: true, ..sample() };
        assert_eq!(run(preview(&big, "o/r")).unwrap_err(), ImportError::Truncated);
        let mut huge = sample();
        huge.files.retain(|(p, _)| !p.contains("broken"));
        huge.listed.insert("skills/docx/SKILL.md".into(), skills::MAX_SKILL_BYTES + 1);
        huge.listed.insert("skills/pdf/scripts/fill.py".into(), skills::MAX_SKILL_BYTES);
        assert_eq!(run(preview(&huge, "o/r")).unwrap_err(), ImportError::TooLarge);
    }

    fn engine() -> (tempfile::TempDir, Engine, Ctx) {
        let dir = tempfile::tempdir().unwrap();
        let (vault, keys, _) = Vault::create(dir.path(), "pw", KdfParams::insecure_fast()).unwrap();
        let blobs = bm_core::blobs::BlobStore::new(dir.path().join("blobs"), &keys.blob_key());
        let engine = Engine::open(&vault.db_path(), &keys.db_key(), 2).unwrap().with_blobs(blobs);
        let user = Ctx::new(engine.ensure_actor(ActorKind::User, "Nutzer").unwrap().id);
        (dir, engine, user)
    }

    #[test]
    fn an_import_fetches_what_was_chosen_and_updates_in_place() {
        let (_dir, engine, user) = engine();
        let mine = SkillInput {
            title: "docx".into(),
            description: "Meine Version".into(),
            ..Default::default()
        };
        let mine = engine.save_skill(&user, mine).unwrap();

        let repo = sample();
        let url = "https://github.com/o/r";
        let mut p = run(preview(&repo, url)).unwrap();
        mark_existing(&mut p, &engine).unwrap();
        assert_eq!(p.skills[0].existing, Some(mine.node.id.clone()), "the same name");
        assert_eq!(p.skills[1].existing, None);

        let chosen = vec!["skills/pdf".to_string(), "skills/docx/".to_string()];
        let fetched = run(fetch(&repo, url, &p.revision, &chosen)).unwrap();
        assert_eq!(fetched.len(), 2);
        assert_eq!(fetched[0].files, [("scripts/fill.py".to_string(), b"print(1)".to_vec())]);
        assert_eq!(fetched[0].source.path, "skills/pdf");
        assert_eq!(
            (fetched[1].source.path.as_str(), fetched[1].source.revision.as_str()),
            ("skills/docx", MAIN)
        );
        let imported = engine.import_skills(&user, fetched, &[]).unwrap();
        assert_eq!(imported[1].node.id, mine.node.id);
        assert_eq!(imported[1].description, "Edit Word files.");
        assert_eq!(imported[0].files[0].filename, "scripts/fill.py");

        // Later, from the same place, it is found by where it came from.
        let mut p = run(preview(&repo, url)).unwrap();
        mark_existing(&mut p, &engine).unwrap();
        assert_eq!(p.skills[1].existing, Some(imported[0].node.id.clone()));

        let err = |paths: &[&str]| {
            let paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
            run(fetch(&repo, url, MAIN, &paths)).unwrap_err()
        };
        assert_eq!(err(&["skills/nope"]), ImportError::PathNotFound);
        assert_eq!(err(&["skills/broken"]), ImportError::NoSkill);
        assert_eq!(run(fetch(&repo, url, "main", &chosen)).unwrap_err(), ImportError::PathNotFound);
        assert!(run(fetch(&repo, url, MAIN, &[])).unwrap().is_empty());

        let twice = Repo::new(&[
            ("a/SKILL.md", &skill_md("same", "one")),
            ("b/SKILL.md", &skill_md("Same", "two")),
        ]);
        let both = vec!["a".to_string(), "b".to_string()];
        let err = run(fetch(&twice, url, MAIN, &both)).unwrap_err();
        assert_eq!(err, ImportError::SameName("Same".into()));

        // A file larger than the listing said is still held to the limit.
        let mut liar = sample();
        liar.files.push(("skills/pdf/big.bin".into(), vec![0; skills::MAX_SKILL_BYTES as usize]));
        liar.listed.insert("skills/pdf/big.bin".into(), 1);
        let pdf = vec!["skills/pdf".to_string()];
        assert_eq!(run(fetch(&liar, url, MAIN, &pdf)).unwrap_err(), ImportError::TooLarge);
    }

    #[test]
    fn the_common_folder_is_fetched_once() {
        let paths = |p: &[&str]| p.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(common_folder(&paths(&["skills/a", "skills/b/c"])), "skills");
        assert_eq!(common_folder(&paths(&["skills/a"])), "skills/a");
        assert_eq!(common_folder(&paths(&["a", "b"])), "");
        assert_eq!(common_folder(&paths(&["", "skills/a"])), "");
    }

    #[test]
    fn every_problem_has_a_message() {
        let all = [
            ImportError::InvalidUrl,
            ImportError::RepoNotFound,
            ImportError::PathNotFound,
            ImportError::NoSkill,
            ImportError::TooMany,
            ImportError::TooLarge,
            ImportError::Truncated,
            ImportError::RateLimited(Some(bm_core::time::now_ms() / 1000 + 600)),
            ImportError::RateLimited(None),
            ImportError::Offline,
            ImportError::Timeout,
            ImportError::Status(500),
            ImportError::SameName("x".into()),
            ImportError::Unexpected("x".into()),
        ];
        for e in all {
            let api = ApiError::from(e.clone());
            assert!(!api.message.is_empty() && api.code != "internal", "{e:?}");
        }
        let soon =
            ApiError::from(ImportError::RateLimited(Some(bm_core::time::now_ms() / 1000 + 600)));
        assert!(soon.message.contains("10"), "{}", soon.message);
    }
}
