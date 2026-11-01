use std::collections::HashSet;

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use super::{
    nodes,
    rows::{self, PROJECT_COLS, project_from_row},
    writer::{Entity, Writer},
};
use crate::{
    error::{CoreError, Result},
    events::Event,
    ids::{NodeId, ProjectId},
    model::{DOC_KIND, NewProject, Project, ProjectDeletePreview, ProjectSummary, TEMPLATE_KIND},
};

/// Revision snapshot for a project membership; presence means "member".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Membership {
    pub project: ProjectId,
    pub node: NodeId,
}

impl Membership {
    pub(crate) fn key(&self) -> String {
        format!("{}|{}", self.project, self.node)
    }
}

pub(crate) fn create(w: &mut Writer<'_>, input: NewProject) -> Result<Project> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CoreError::Invalid("project name must not be empty".into()));
    }
    if let Some(parent) = &input.parent {
        rows::live_project(w.conn, parent)?;
    }
    let sort: i64 = w.conn.query_row(
        "SELECT coalesce(max(sort), -1) + 1 FROM projects WHERE parent_id IS ?1",
        [&input.parent],
        |r| r.get(0),
    )?;
    let id = ProjectId::new();
    w.conn.execute(
        "INSERT INTO projects (id, parent_id, name, description, color, icon, sort, created_at, created_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![id, input.parent, name, input.description, input.color, input.icon, sort, w.now, w.actor],
    )?;
    let project = rows::live_project(w.conn, &id)?;
    w.record(Entity::Project, id.as_str(), 0, None, Some(&project))?;
    w.emit(Event::ProjectUpserted { id });
    Ok(project)
}

fn store(w: &mut Writer<'_>, before: &Project, after: &Project) -> Result<()> {
    w.conn.execute(
        "UPDATE projects SET parent_id = ?2, name = ?3, description = ?4, sort = ?5, deleted_at = ?6
         WHERE id = ?1",
        params![after.id, after.parent_id, after.name, after.description, after.sort, after.deleted_at],
    )?;
    w.record(Entity::Project, after.id.as_str(), 0, Some(before), Some(after))?;
    let id = after.id.clone();
    w.emit(match after.deleted_at {
        Some(_) => Event::ProjectDeleted { id },
        None => Event::ProjectUpserted { id },
    });
    Ok(())
}

pub(crate) fn update(
    w: &mut Writer<'_>,
    id: &ProjectId,
    name: Option<&str>,
    description: Option<&str>,
) -> Result<Project> {
    let before = rows::live_project(w.conn, id)?;
    let mut after = before.clone();
    if let Some(name) = name {
        after.name = name.trim().to_string();
        if after.name.is_empty() {
            return Err(CoreError::Invalid("project name must not be empty".into()));
        }
    }
    if let Some(description) = description {
        after.description = description.to_string();
    }
    if after != before {
        store(w, &before, &after)?;
    }
    Ok(after)
}

/// Puts a project under `parent` (or at the top level) at `position` among its
/// new siblings. Its subprojects come along.
pub(crate) fn place(
    w: &mut Writer<'_>,
    id: &ProjectId,
    parent: Option<&ProjectId>,
    position: usize,
) -> Result<()> {
    let project = rows::live_project(w.conn, id)?;
    if let Some(parent) = parent {
        rows::live_project(w.conn, parent)?;
        if ancestors_and_self(w.conn, parent)?.contains(id) {
            return Err(CoreError::Invalid("a project cannot go inside itself".into()));
        }
    }
    let sql = format!(
        "SELECT {PROJECT_COLS} FROM projects
         WHERE parent_id IS ?1 AND id != ?2 AND deleted_at IS NULL ORDER BY sort, name"
    );
    let mut stmt = w.conn.prepare(&sql)?;
    let mut siblings: Vec<Project> =
        stmt.query_map(params![parent, id], project_from_row)?.collect::<rusqlite::Result<_>>()?;
    drop(stmt);
    let moved = Project { parent_id: parent.cloned(), ..project.clone() };
    siblings.insert(position.min(siblings.len()), moved);
    for (sort, after) in siblings.into_iter().enumerate() {
        let after = Project { sort: sort as i64, ..after };
        let before =
            if &after.id == id { project.clone() } else { rows::live_project(w.conn, &after.id)? };
        if before != after {
            store(w, &before, &after)?;
        }
    }
    Ok(())
}

/// What deleting a project would affect, for asking before doing it.
pub(crate) fn delete_preview(conn: &Connection, id: &ProjectId) -> Result<ProjectDeletePreview> {
    rows::live_project(conn, id)?;
    let tree: HashSet<ProjectId> = subtree(conn, id)?.into_iter().collect();
    let mut preview =
        ProjectDeletePreview { subprojects: tree.len() - 1, ..ProjectDeletePreview::default() };
    for (_, kind, exclusive) in members_of(conn, &tree)? {
        match (kind.as_str(), exclusive) {
            (TEMPLATE_KIND | DOC_KIND, true) => preview.documents += 1,
            (TEMPLATE_KIND | DOC_KIND, false) => {}
            (_, true) => preview.entries_only_here += 1,
            (_, false) => preview.entries_elsewhere += 1,
        }
    }
    Ok(preview)
}

/// Entries in these projects: `(id, kind, in no project outside them)`.
fn members_of(conn: &Connection, tree: &HashSet<ProjectId>) -> Result<Vec<(NodeId, String, bool)>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for project in tree {
        let mut stmt = conn.prepare_cached(
            "SELECT n.id, n.kind FROM project_members m JOIN nodes n ON n.id = m.node_id
             WHERE m.project_id = ?1 AND n.deleted_at IS NULL",
        )?;
        let rows: Vec<(NodeId, String)> = stmt
            .query_map([project], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        for (node, kind) in rows {
            if seen.insert(node.clone()) {
                let exclusive =
                    rows::projects_of(conn, &node)?.iter().all(|p| tree.contains(&p.id));
                out.push((node, kind, exclusive));
            }
        }
    }
    Ok(out)
}

/// Deletes a project with its subprojects, as one undoable step. Entries stay in
/// the graph unless `with_entries`. Documentation and templates that belong only
/// here go to the trash either way: pages have no place without their project,
/// and a project's template must not turn into a global one.
pub(crate) fn delete(w: &mut Writer<'_>, id: &ProjectId, with_entries: bool) -> Result<usize> {
    rows::live_project(w.conn, id)?;
    let ids = subtree(w.conn, id)?;
    let tree: HashSet<ProjectId> = ids.iter().cloned().collect();
    let mut trashed = 0;
    for (node, kind, exclusive) in members_of(w.conn, &tree)? {
        let always = matches!(kind.as_str(), TEMPLATE_KIND | DOC_KIND);
        if exclusive && (always || with_entries) {
            nodes::delete(w, &node)?;
            trashed += 1;
        }
    }
    for project in &ids {
        let before = rows::live_project(w.conn, project)?;
        let after = Project { deleted_at: Some(w.now), ..before.clone() };
        store(w, &before, &after)?;
    }
    Ok(trashed)
}

/// Returns false if the node already was a member.
pub(crate) fn add_member(w: &mut Writer<'_>, project: &ProjectId, node: &NodeId) -> Result<bool> {
    rows::live_project(w.conn, project)?;
    rows::live_node(w.conn, node)?;
    let added = w.conn.execute(
        "INSERT OR IGNORE INTO project_members (project_id, node_id, added_at, added_by)
         VALUES (?1, ?2, ?3, ?4)",
        params![project, node, w.now, w.actor],
    )? == 1;
    if added {
        let m = Membership { project: project.clone(), node: node.clone() };
        w.record(Entity::Membership, &m.key(), 0, None, Some(&m))?;
        w.emit(Event::MembershipChanged { project: m.project, node: m.node, member: true });
    }
    Ok(added)
}

pub(crate) fn remove_member(
    w: &mut Writer<'_>,
    project: &ProjectId,
    node: &NodeId,
) -> Result<bool> {
    let removed = w.conn.execute(
        "DELETE FROM project_members WHERE project_id = ?1 AND node_id = ?2",
        params![project, node],
    )? == 1;
    if removed {
        let m = Membership { project: project.clone(), node: node.clone() };
        w.record(Entity::Membership, &m.key(), 0, Some(&m), None)?;
        w.emit(Event::MembershipChanged { project: m.project, node: m.node, member: false });
    }
    Ok(removed)
}

pub(crate) fn is_member(conn: &Connection, project: &ProjectId, node: &NodeId) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM project_members WHERE project_id = ?1 AND node_id = ?2)",
        params![project, node],
        |r| r.get(0),
    )?)
}

/// All live projects, parents before children, siblings by `sort`.
pub(crate) fn list(conn: &Connection) -> Result<Vec<ProjectSummary>> {
    let sql = format!(
        "SELECT {PROJECT_COLS},
                (SELECT count(*) FROM project_members m JOIN nodes n ON n.id = m.node_id
                  WHERE m.project_id = projects.id AND n.deleted_at IS NULL
                    AND n.kind NOT IN ('template', 'doc'))
         FROM projects WHERE deleted_at IS NULL ORDER BY sort, name"
    );
    let mut stmt = conn.prepare(&sql)?;
    let all: Vec<ProjectSummary> = stmt
        .query_map([], |r| {
            Ok(ProjectSummary { project: project_from_row(r)?, node_count: r.get(10)? })
        })?
        .collect::<rusqlite::Result<_>>()?;

    // Depth-first order so a UI can render the tree in one pass.
    fn visit(parent: Option<&ProjectId>, all: &[ProjectSummary], out: &mut Vec<ProjectSummary>) {
        for p in all.iter().filter(|p| p.project.parent_id.as_ref() == parent) {
            out.push(p.clone());
            visit(Some(&p.project.id), all, out);
        }
    }
    let mut ordered = Vec::with_capacity(all.len());
    visit(None, &all, &mut ordered);
    // Orphans (parent in the trash) go last rather than disappearing.
    for p in &all {
        if !ordered.iter().any(|o| o.project.id == p.project.id) {
            ordered.push(p.clone());
        }
    }
    Ok(ordered)
}

/// The project and everything above it.
pub(crate) fn ancestors_and_self(conn: &Connection, id: &ProjectId) -> Result<HashSet<ProjectId>> {
    let mut stmt = conn.prepare_cached(
        "WITH RECURSIVE chain(id, parent_id) AS (
             SELECT id, parent_id FROM projects WHERE id = ?1
             UNION SELECT p.id, p.parent_id FROM projects p JOIN chain c ON p.id = c.parent_id)
         SELECT id FROM chain",
    )?;
    let ids = stmt.query_map([id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

/// The project and all its live descendants.
pub(crate) fn subtree(conn: &Connection, root: &ProjectId) -> Result<Vec<ProjectId>> {
    let mut stmt = conn.prepare_cached(
        "WITH RECURSIVE tree(id) AS (
             SELECT id FROM projects WHERE id = ?1 AND deleted_at IS NULL
             UNION SELECT p.id FROM projects p JOIN tree t ON p.parent_id = t.id
                   WHERE p.deleted_at IS NULL)
         SELECT id FROM tree",
    )?;
    let ids = stmt.query_map([root], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}
