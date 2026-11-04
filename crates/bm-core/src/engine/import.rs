//! Carries out an [`ImportPlan`] in one change set, so one undo takes it all back.
//!
//! The folder becomes a project and each folder in it a project inside, or the project
//! of that name if there is one already. An entry whose title is taken is left out: it
//! is most likely in the vault from an earlier import.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::{
    Engine, edges,
    export::{NO_PROJECT, SKILLS},
    nodes, projects, rows,
    skills::{self, StoredFile},
    writer::Writer,
};
use crate::{
    error::Result,
    ids::{ChangeSetId, NodeId, ProjectId},
    import::{self, ImportPlan, SkipReason, SkippedFile},
    model::{Ctx, EdgeOrigin, NewEdge, NewNode, NewProject},
    templates::slug,
    wikilinks::fold,
};

/// How many skipped files a summary names; the others are only counted.
const LISTED: usize = 50;
const MAX_TITLE_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportSummary {
    /// The name of the folder.
    pub folder: String,
    /// New entries, skills included.
    pub entries: usize,
    /// New projects.
    pub projects: usize,
    /// Typed links from a Brainmatter export; `[[links]]` in the text come on top.
    pub links: usize,
    /// Entries left out since the vault has one of that title, and skills since it has
    /// one of that name.
    pub existing: usize,
    /// The first files left out for another reason.
    pub skipped: Vec<SkippedFile>,
    pub skipped_count: usize,
    /// What undoes the import; `None` before it, or if it brought nothing.
    pub change_set: Option<ChangeSetId>,
}

/// Where a folder's entries go.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    /// Into no project: the folder of a Brainmatter export, its entries without a
    /// project, and folders nothing new is in.
    None,
    Existing(ProjectId),
    New,
}

/// The plan, held against what the vault has.
struct Resolved {
    targets: Vec<Target>,
    /// Per note: whether it is new.
    keep: Vec<bool>,
    /// Per skill: whether it is new.
    keep_skills: Vec<bool>,
    existing: usize,
    skipped: Vec<SkippedFile>,
    /// Folders by their name, for the projects a note of an export names.
    by_name: HashMap<String, usize>,
    links: usize,
}

fn resolve(conn: &Connection, plan: &ImportPlan) -> Result<Resolved> {
    let mut skipped = plan.skipped.clone();
    let mut keep = Vec::with_capacity(plan.notes.len());
    let mut titles = HashSet::new();
    let mut existing = 0;
    for note in &plan.notes {
        let title = fold(&note.title);
        let new = if note.title.chars().count() > MAX_TITLE_CHARS {
            skipped.push(SkippedFile { path: note.path.clone(), reason: SkipReason::LongTitle });
            false
        } else if titles.contains(&title) {
            skipped.push(SkippedFile { path: note.path.clone(), reason: SkipReason::SameTitle });
            false
        } else if !rows::nodes_titled(conn, &note.title)?.is_empty() {
            existing += 1;
            false
        } else {
            true
        };
        if new {
            titles.insert(title);
        }
        keep.push(new);
    }

    let mut keep_skills = Vec::with_capacity(plan.skills.len());
    let mut names = HashSet::new();
    for skill in &plan.skills {
        let name = slug(&skill.title);
        let new = if skill.title.chars().count() > MAX_TITLE_CHARS {
            skipped.push(SkippedFile { path: skill.path.clone(), reason: SkipReason::LongTitle });
            false
        } else if names.contains(&name) {
            skipped.push(SkippedFile { path: skill.path.clone(), reason: SkipReason::SameTitle });
            false
        } else if skills::name_taken(conn, &name)? {
            existing += 1;
            false
        } else {
            names.insert(name);
            true
        };
        keep_skills.push(new);
    }

    let export = plan.brainmatter_export;
    let outside = |i: usize| {
        let folder = &plan.folders[i];
        let top = folder.parent == Some(0)
            && [NO_PROJECT, SKILLS].iter().any(|n| fold(&folder.name) == fold(n));
        export && (i == 0 || top)
    };
    let mut by_name = HashMap::new();
    for (i, folder) in plan.folders.iter().enumerate() {
        if !outside(i) {
            by_name.entry(fold(&folder.name)).or_insert(i);
        }
    }

    // A folder is used if something new goes into it or into a folder inside it.
    let mut used = vec![false; plan.folders.len()];
    let mut mark = |mut folder: Option<usize>| {
        while let Some(i) = folder {
            if used[i] {
                break;
            }
            used[i] = true;
            folder = plan.folders[i].parent;
        }
    };
    for (note, new) in plan.notes.iter().zip(&keep) {
        if *new {
            mark(Some(note.folder));
            for name in &note.projects {
                mark(by_name.get(&fold(name)).copied());
            }
        }
    }
    for (skill, new) in plan.skills.iter().zip(&keep_skills) {
        if *new {
            mark(Some(skill.folder));
            for name in &skill.projects {
                mark(by_name.get(&fold(name)).copied());
            }
        }
    }

    let mut known: HashMap<(Option<ProjectId>, String), ProjectId> = HashMap::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, parent_id, name FROM projects WHERE deleted_at IS NULL ORDER BY sort, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, ProjectId>(0)?,
                r.get::<_, Option<ProjectId>>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (id, parent, name) = row?;
            known.entry((parent, fold(&name))).or_insert(id);
        }
    }
    let mut targets: Vec<Target> = Vec::with_capacity(plan.folders.len());
    for (i, folder) in plan.folders.iter().enumerate() {
        let target = if outside(i) || !used[i] {
            Target::None
        } else {
            let parent = match folder.parent.map(|p| &targets[p]) {
                Some(Target::New) => None,
                Some(Target::Existing(id)) => Some(Some(id.clone())),
                // The top, or the folder of an export.
                Some(Target::None) | None => Some(None),
            };
            match parent.and_then(|p| known.get(&(p, fold(&folder.name)))) {
                Some(id) => Target::Existing(id.clone()),
                None => Target::New,
            }
        };
        targets.push(target);
    }

    let mut links = 0;
    for (note, new) in plan.notes.iter().zip(&keep) {
        if !*new {
            continue;
        }
        for (_, to) in &note.links {
            let to_fold = fold(to);
            if to_fold != fold(&note.title)
                && (titles.contains(&to_fold) || !rows::nodes_titled(conn, to)?.is_empty())
            {
                links += 1;
            }
        }
    }
    Ok(Resolved { targets, keep, keep_skills, existing, skipped, by_name, links })
}

fn summary(plan: &ImportPlan, resolved: &Resolved) -> ImportSummary {
    ImportSummary {
        folder: plan.folders[0].name.clone(),
        entries: resolved.keep.iter().chain(&resolved.keep_skills).filter(|k| **k).count(),
        projects: resolved.targets.iter().filter(|t| **t == Target::New).count(),
        links: resolved.links,
        existing: resolved.existing,
        skipped: resolved.skipped.iter().take(LISTED).cloned().collect(),
        skipped_count: resolved.skipped.len(),
        change_set: None,
    }
}

/// `skill_files` holds the files of each planned skill, stored already.
fn carry_out(
    w: &mut Writer<'_>,
    plan: &ImportPlan,
    skill_files: &[Vec<StoredFile>],
) -> Result<ImportSummary> {
    let resolved = resolve(w.conn, plan)?;
    let mut folder_projects: Vec<Option<ProjectId>> = Vec::with_capacity(plan.folders.len());
    for (folder, target) in plan.folders.iter().zip(&resolved.targets) {
        let project = match target {
            Target::None => None,
            Target::Existing(id) => Some(id.clone()),
            Target::New => {
                let parent = folder.parent.and_then(|p| folder_projects[p].clone());
                let input = NewProject { name: folder.name.clone(), parent, ..Default::default() };
                Some(projects::create(w, input)?.id)
            }
        };
        folder_projects.push(project);
    }

    let members = |folder: usize, named: &[String]| {
        let mut members: Vec<ProjectId> = folder_projects[folder].iter().cloned().collect();
        for name in named {
            if let Some(Some(project)) =
                resolved.by_name.get(&fold(name)).map(|f| &folder_projects[*f])
                && !members.contains(project)
            {
                members.push(project.clone());
            }
        }
        members
    };

    for ((skill, new), files) in plan.skills.iter().zip(&resolved.keep_skills).zip(skill_files) {
        if *new {
            let projects = members(skill.folder, &skill.projects);
            skills::create(w, &skill.title, &skill.description, &skill.body, projects, files)?;
        }
    }

    let mut created: HashMap<String, NodeId> = HashMap::new();
    for (note, new) in plan.notes.iter().zip(&resolved.keep) {
        if !*new {
            continue;
        }
        let members = members(note.folder, &note.projects);
        let node = nodes::create(
            w,
            NewNode {
                title: note.title.clone(),
                body: note.body.clone(),
                kind: note.kind.clone(),
                tags: note.tags.clone(),
                props: None,
                projects: members,
            },
        )?;
        created.insert(fold(&note.title), node.id);
    }

    // Links last: they may lead to any of the entries.
    for (note, new) in plan.notes.iter().zip(&resolved.keep) {
        if !*new || note.links.is_empty() {
            continue;
        }
        let src = created[&fold(&note.title)].clone();
        for (relation, to) in &note.links {
            let dst = match created.get(&fold(to)) {
                Some(id) => Some(id.clone()),
                None => rows::nodes_titled(w.conn, to)?.into_iter().next().map(|b| b.id),
            };
            if let Some(dst) = dst.filter(|d| *d != src) {
                let edge = NewEdge {
                    src: src.clone(),
                    dst,
                    relation: Some(relation.clone()),
                    weight: None,
                    note: None,
                };
                edges::link(w, &edge, EdgeOrigin::Manual)?;
            }
        }
    }
    Ok(summary(plan, &resolved))
}

impl Engine {
    /// What [`Self::import_markdown`] would bring in from `root`.
    pub fn import_preview(&self, root: &Path) -> Result<ImportSummary> {
        let plan = import::plan(root)?;
        self.read(|c| Ok(summary(&plan, &resolve(c, &plan)?)))
    }

    /// Brings in a folder of Markdown notes, as Obsidian, Logseq or Brainmatter's own
    /// export leave them; see [`crate::import`].
    pub fn import_markdown(&self, ctx: &Ctx, root: &Path) -> Result<ImportSummary> {
        let plan = import::plan(root)?;
        let skill_files = plan
            .skills
            .iter()
            .map(|s| skills::store_files(self, &s.files))
            .collect::<Result<Vec<_>>>()?;
        let text = format!("„{}“ importiert", plan.folders[0].name);
        let (mut summary, change_set) =
            self.write(ctx, text, |w| carry_out(w, &plan, &skill_files))?;
        summary.change_set = change_set;
        Ok(summary)
    }
}
