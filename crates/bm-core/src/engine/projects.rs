use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use super::{
    rows::{self, PROJECT_COLS, project_from_row},
    writer::{Entity, Writer},
};
use crate::{
    error::{CoreError, Result},
    events::Event,
    ids::{NodeId, ProjectId},
    model::{NewProject, Project, ProjectSummary},
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
                  WHERE m.project_id = projects.id AND n.deleted_at IS NULL)
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
