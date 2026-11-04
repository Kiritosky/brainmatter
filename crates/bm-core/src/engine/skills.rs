//! The skill vault: entries of kind `skill`, with the description and, for imported
//! ones, their source in the props. Supporting files are attachments named by their
//! path inside the skill folder.

use std::collections::HashSet;

use rusqlite::Connection;
use serde_json::{Map, Value, json};

use super::{Engine, attachments, nodes, projects, rows, writer::Writer};
use crate::{
    error::{CoreError, Result},
    ids::{NodeId, ProjectId},
    model::*,
    templates::slug,
};

/// A skill file whose content is in the blob store already.
pub(super) struct StoredFile {
    pub path: String,
    pub mime: String,
    pub size: i64,
    pub blob: String,
}

fn props_of(node: &Node) -> Map<String, Value> {
    match &node.props {
        Some(Value::Object(map)) => map.clone(),
        _ => Map::new(),
    }
}

fn description_of(node: &Node) -> String {
    let props = props_of(node);
    props.get("description").and_then(Value::as_str).unwrap_or_default().to_string()
}

fn source_of(node: &Node) -> Option<SkillSource> {
    serde_json::from_value(props_of(node).get("source")?.clone()).ok()
}

fn info(conn: &Connection, node: Node) -> Result<SkillInfo> {
    Ok(SkillInfo {
        slug: slug(&node.title),
        description: description_of(&node),
        source: source_of(&node),
        files: attachments::of_node(conn, &node.id)?,
        projects: rows::projects_of(conn, &node.id)?,
        reviewed: node.reviewed,
        updated_at: node.updated_at,
        node: NodeBrief { id: node.id, title: node.title, kind: node.kind },
        body: node.body,
    })
}

/// All live skills, by title.
fn all(conn: &Connection) -> Result<Vec<Node>> {
    let sql = format!(
        "SELECT {} FROM nodes WHERE kind = ?1 AND deleted_at IS NULL ORDER BY title COLLATE NOCASE",
        rows::NODE_COLS
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let found = stmt.query_map([SKILL_KIND], rows::node_from_row)?;
    Ok(found.collect::<rusqlite::Result<_>>()?)
}

fn live_skill(conn: &Connection, id: &NodeId) -> Result<Node> {
    let node = rows::live_node(conn, id)?;
    if node.kind != SKILL_KIND {
        return Err(CoreError::Invalid(format!("„{}“ is not a skill", node.title)));
    }
    Ok(node)
}

/// A slug is how clients call a skill, so two skills must not share one.
fn ensure_free(conn: &Connection, name: &str, except: Option<&NodeId>) -> Result<()> {
    if name.is_empty() {
        return Err(CoreError::Invalid("a skill's name needs letters or digits".into()));
    }
    let taken = all(conn)?.into_iter().find(|n| slug(&n.title) == name && Some(&n.id) != except);
    match taken {
        Some(other) => Err(CoreError::Conflict(format!(
            "the skill „{}“ already has the name {name}",
            other.title
        ))),
        None => Ok(()),
    }
}

fn clean_description(description: &str) -> Result<String> {
    let description = description.trim();
    if description.is_empty() {
        return Err(CoreError::Invalid("a skill needs a description".into()));
    }
    Ok(description.to_string())
}

/// The skill an import updates: one imported from the same place before, else one
/// with the same name.
fn to_update(conn: &Connection, repo: &str, path: &str, name: &str) -> Result<Option<NodeId>> {
    let skills = all(conn)?;
    let same_place = skills.iter().find(|n| {
        source_of(n).is_some_and(|s| s.repo.eq_ignore_ascii_case(repo) && s.path == path)
    });
    let found = same_place.or_else(|| skills.iter().find(|n| slug(&n.title) == name));
    Ok(found.map(|n| n.id.clone()))
}

/// Makes the skill's projects exactly `wanted`.
fn set_projects(w: &mut Writer<'_>, id: &NodeId, wanted: &[ProjectId]) -> Result<()> {
    let current: Vec<ProjectId> =
        rows::projects_of(w.conn, id)?.into_iter().map(|p| p.id).collect();
    for project in current.iter().filter(|p| !wanted.contains(p)) {
        projects::remove_member(w, project, id)?;
    }
    for project in wanted.iter().filter(|p| !current.contains(p)) {
        projects::add_member(w, project, id)?;
    }
    Ok(())
}

/// Replaces the skill's files, keeping those that stay the same so the history
/// shows only what changed.
fn replace_files(w: &mut Writer<'_>, id: &NodeId, files: &[StoredFile]) -> Result<()> {
    let current = attachments::of_node(w.conn, id)?;
    let same = |a: &Attachment, f: &StoredFile| a.filename == f.path && a.blob_id == f.blob;
    for old in current.iter().filter(|a| !files.iter().any(|f| same(a, f))) {
        attachments::remove(w, old)?;
    }
    for file in files.iter().filter(|f| !current.iter().any(|a| same(a, f))) {
        attachments::insert(w, id, &file.path, &file.mime, file.size, &file.blob)?;
    }
    Ok(())
}

/// What an AI changes about a skill's files needs the user's look again, like its text.
pub(super) fn after_file_change(w: &mut Writer<'_>, id: &NodeId) -> Result<()> {
    let node = rows::live_node(w.conn, id)?;
    if w.actor_is_ai && node.kind == SKILL_KIND && node.reviewed {
        let patch = NodePatch { reviewed: Some(false), ..Default::default() };
        nodes::update(w, id, patch)?;
    }
    Ok(())
}

impl Engine {
    /// The skill vault, by title. With a project: what applies there, which is
    /// global skills plus those of the project and its ancestors.
    pub fn skills(&self, project: Option<&ProjectId>) -> Result<Vec<SkillInfo>> {
        self.read(|c| {
            let applicable: Option<HashSet<ProjectId>> =
                project.map(|p| projects::ancestors_and_self(c, p)).transpose()?;
            let mut out = Vec::new();
            for node in all(c)? {
                let skill = info(c, node)?;
                let applies = skill.projects.is_empty()
                    || applicable
                        .as_ref()
                        .is_none_or(|a| skill.projects.iter().any(|p| a.contains(&p.id)));
                if applies {
                    out.push(skill);
                }
            }
            Ok(out)
        })
    }

    pub fn skill(&self, id: &NodeId) -> Result<SkillInfo> {
        self.read(|c| info(c, live_skill(c, id)?))
    }

    /// Creates a skill, or changes one together with its projects, as one change.
    pub fn save_skill(&self, ctx: &Ctx, input: SkillInput) -> Result<SkillInfo> {
        let title = rows::clean_title(&input.title)?;
        let description = clean_description(&input.description)?;
        let name = slug(&title);
        let id = match &input.id {
            None => {
                let summary = format!("Skill „{title}“ erstellt");
                self.write(ctx, summary, |w| {
                    ensure_free(w.conn, &name, None)?;
                    let node = NewNode {
                        title,
                        body: input.body,
                        kind: Some(SKILL_KIND.to_string()),
                        props: Some(json!({ "description": description })),
                        projects: input.projects,
                        ..Default::default()
                    };
                    Ok(nodes::create(w, node)?.id)
                })?
                .0
            }
            Some(id) => {
                let before = self.read(|c| live_skill(c, id))?;
                let mut props = props_of(&before);
                props.insert("description".into(), Value::String(description));
                let patch = NodePatch {
                    title: Some(title),
                    body: Some(BodyEdit::Replace(input.body)),
                    props: Some(Value::Object(props)),
                    ..Default::default()
                };
                let summary = format!("Skill „{}“ bearbeitet", before.title);
                let key = format!("update:{id}");
                self.write_coalesced(ctx, summary, Some(&key), |w| {
                    ensure_free(w.conn, &name, Some(id))?;
                    nodes::update(w, id, patch)?;
                    set_projects(w, id, &input.projects)?;
                    Ok(id.clone())
                })?
                .0
            }
        };
        self.skill(&id)
    }

    /// The skill an import from `repo` at `path` would update, if any; see
    /// [`Self::import_skills`].
    pub fn skill_to_update(&self, repo: &str, path: &str, name: &str) -> Result<Option<NodeId>> {
        self.read(|c| to_update(c, repo, path, name))
    }

    /// Brings fetched skills into the vault as one change. A skill imported from the
    /// same place before, else one with the same name, is updated in place: its
    /// description, text and files are replaced, its title and projects stay. New
    /// skills go into `projects`. Either way they count as the user's own.
    pub fn import_skills(
        &self,
        ctx: &Ctx,
        skills: Vec<SkillImport>,
        projects: &[ProjectId],
    ) -> Result<Vec<SkillInfo>> {
        let mut stored = Vec::with_capacity(skills.len());
        for skill in &skills {
            rows::clean_title(&skill.title)?;
            clean_description(&skill.description)?;
            stored.push(store_files(self, &skill.files)?);
        }
        let summary = match skills.as_slice() {
            [one] => format!("Skill „{}“ importiert", one.title.trim()),
            many => format!("{} Skills importiert", many.len()),
        };
        let (ids, _) = self.write(ctx, summary, |w| {
            let mut ids: Vec<NodeId> = Vec::with_capacity(skills.len());
            for (skill, files) in skills.into_iter().zip(&stored) {
                let name = slug(&skill.title);
                let existing = to_update(w.conn, &skill.source.repo, &skill.source.path, &name)?;
                let description = skill.description.trim().to_string();
                let id = match existing {
                    Some(id) if ids.contains(&id) => {
                        return Err(CoreError::Conflict(format!(
                            "two of these skills have the name {name}"
                        )));
                    }
                    Some(id) => {
                        let mut props = props_of(&rows::live_node(w.conn, &id)?);
                        props.insert("description".into(), Value::String(description));
                        props.insert("source".into(), serde_json::to_value(&skill.source)?);
                        let patch = NodePatch {
                            body: Some(BodyEdit::Replace(skill.body)),
                            props: Some(Value::Object(props)),
                            reviewed: Some(!w.actor_is_ai),
                            ..Default::default()
                        };
                        nodes::update(w, &id, patch)?;
                        id
                    }
                    None => {
                        let props = json!({ "description": description, "source": skill.source });
                        let node = NewNode {
                            title: skill.title,
                            body: skill.body,
                            kind: Some(SKILL_KIND.to_string()),
                            props: Some(props),
                            projects: projects.to_vec(),
                            ..Default::default()
                        };
                        nodes::create(w, node)?.id
                    }
                };
                replace_files(w, &id, files)?;
                ids.push(id);
            }
            Ok(ids)
        })?;
        ids.iter().map(|id| self.skill(id)).collect()
    }
}

/// Creates a skill inside a larger change, as the Markdown import does.
pub(super) fn create(
    w: &mut Writer<'_>,
    title: &str,
    description: &str,
    body: &str,
    projects: Vec<ProjectId>,
    files: &[StoredFile],
) -> Result<Node> {
    let node = NewNode {
        title: title.to_string(),
        body: body.to_string(),
        kind: Some(SKILL_KIND.to_string()),
        props: Some(json!({ "description": description.trim() })),
        projects,
        ..Default::default()
    };
    let node = nodes::create(w, node)?;
    replace_files(w, &node.id, files)?;
    Ok(node)
}

/// Whether a live skill has this name already.
pub(super) fn name_taken(conn: &Connection, name: &str) -> Result<bool> {
    Ok(all(conn)?.iter().any(|n| slug(&n.title) == name))
}

/// Puts files into the blob store before a change uses them: outside the
/// transaction, since encrypting files must not hold up writes.
pub(super) fn store_files(engine: &Engine, files: &[(String, Vec<u8>)]) -> Result<Vec<StoredFile>> {
    let mut out = Vec::with_capacity(files.len());
    for (path, bytes) in files {
        let path = crate::skills::clean_path(path)
            .ok_or_else(|| CoreError::Invalid(format!("„{path}“ is no file path")))?;
        let mime = attachments::clean_mime(None, &path);
        let blob = engine.blob_store()?.put(bytes)?;
        out.push(StoredFile { path, mime, size: bytes.len() as i64, blob });
    }
    Ok(out)
}
