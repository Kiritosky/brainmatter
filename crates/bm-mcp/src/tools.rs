//! Tool implementations. Each takes the call context and typed arguments and
//! returns Markdown for the model, or an error message it can act on.

use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    sync::Arc,
};

use bm_core::{
    CoreError, Engine,
    engine::SUGGEST_SCORE,
    ids::{ChangeSetId, NodeId, ProjectId},
    model::{
        BodyEdit, Capture, CaptureLink, CaptureNode, Ctx, NewEdge, NewNode, NewProject, NodeBrief,
        NodePatch, SearchQuery,
    },
    semantic::Embedder,
};
use rmcp::schemars::{self, JsonSchema};
use serde::Deserialize;

use crate::{Client, format};

pub(crate) struct ToolCx {
    pub engine: Arc<Engine>,
    pub ctx: Ctx,
    pub client: Client,
    /// Projects (with subprojects) this client may see; `None` = all.
    pub scope: Option<HashSet<ProjectId>>,
    pub embedder: Option<Arc<dyn Embedder>>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ToolError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Core(#[from] CoreError),
}

type Out = Result<String, ToolError>;

fn fail(msg: impl Into<String>) -> ToolError {
    ToolError::Message(msg.into())
}

/// Error text the model can act on: lists candidates and suggestions.
pub(crate) fn describe(e: &ToolError) -> String {
    match e {
        ToolError::Message(m) => m.clone(),
        ToolError::Core(CoreError::Ambiguous { reference, candidates }) => {
            let list: Vec<_> = candidates.iter().map(format::brief).collect();
            format!(
                "„{reference}“ matches several entries; use the short id instead:\n- {}",
                list.join("\n- ")
            )
        }
        ToolError::Core(CoreError::NotFound { kind, reference, suggestions }) => {
            let mut s = format!("No {kind} „{reference}“.");
            if !suggestions.is_empty() {
                let list: Vec<_> = suggestions.iter().map(format::brief).collect();
                let _ = write!(s, " Did you mean:\n- {}", list.join("\n- "));
            }
            s
        }
        ToolError::Core(CoreError::Invalid(m)) => format!("Invalid input: {m}"),
        ToolError::Core(CoreError::Conflict(m)) => format!("Conflict: {m}"),
        ToolError::Core(other) => format!("Internal error: {other}"),
    }
}

impl ToolCx {
    pub(crate) fn new(
        engine: Arc<Engine>,
        ctx: Ctx,
        client: Client,
        embedder: Option<Arc<dyn Embedder>>,
    ) -> Result<Self, ToolError> {
        let scope = if client.projects.is_empty() {
            None
        } else {
            let tree: HashMap<ProjectId, Option<ProjectId>> = engine
                .projects()?
                .into_iter()
                .map(|p| (p.project.id, p.project.parent_id))
                .collect();
            // A project is visible if it or one of its ancestors is allowed.
            let allowed: HashSet<&ProjectId> = client.projects.iter().collect();
            let visible = tree
                .keys()
                .filter(|id| {
                    let mut cur = Some(*id);
                    while let Some(c) = cur {
                        if allowed.contains(c) {
                            return true;
                        }
                        cur = tree.get(c).and_then(|p| p.as_ref());
                    }
                    false
                })
                .cloned()
                .collect();
            Some(visible)
        };
        Ok(Self { engine, ctx, client, scope, embedder })
    }

    fn visible_project(&self, p: &ProjectId) -> bool {
        self.scope.as_ref().is_none_or(|s| s.contains(p))
    }

    fn visible(&self, id: &NodeId) -> Result<bool, ToolError> {
        let Some(scope) = &self.scope else { return Ok(true) };
        Ok(self.engine.node_detail(id)?.projects.iter().any(|p| scope.contains(&p.id)))
    }

    /// Resolves a reference and hides entries outside the client's scope.
    fn entry(&self, reference: &str) -> Result<NodeId, ToolError> {
        let id = self.engine.resolve_node(reference)?;
        if !self.visible(&id)? {
            return Err(fail(format!("No entry „{reference}“.")));
        }
        Ok(id)
    }

    fn project(&self, reference: &str) -> Result<ProjectId, ToolError> {
        let id = self.engine.resolve_project(reference)?;
        if !self.visible_project(&id) {
            return Err(fail(format!("No project „{reference}“.")));
        }
        Ok(id)
    }

    /// Projects for a new entry: scoped clients must place it in scope.
    fn placement(&self, requested: &[String]) -> Result<Vec<ProjectId>, ToolError> {
        let ids = requested.iter().map(|p| self.project(p)).collect::<Result<Vec<_>, _>>()?;
        if ids.is_empty()
            && let Some(first) = self.client.projects.first()
        {
            return Ok(vec![first.clone()]);
        }
        Ok(ids)
    }

    fn actor_name(&self, id: &bm_core::ids::ActorId) -> String {
        self.engine.actor(id).ok().flatten().map_or_else(|| "?".into(), |a| a.name)
    }
}

fn strings(v: &[String]) -> Vec<String> {
    v.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

// ------------------------------------------------------------------ search ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// Words to search for (full-text, prefix matching, German and English).
    /// Empty: most recently changed entries.
    #[serde(default)]
    pub query: String,
    /// Limit to a project (name or id), including its subprojects.
    pub project: Option<String>,
    /// Only these kinds, e.g. ["concept", "task"].
    #[serde(default)]
    pub kinds: Vec<String>,
    /// Only entries with any of these tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Maximum results (default 10, max 50).
    pub limit: Option<usize>,
}

pub(crate) fn search(cx: &ToolCx, a: SearchArgs) -> Out {
    let limit = a.limit.unwrap_or(10).clamp(1, 50);
    let project = a.project.as_deref().map(|p| cx.project(p)).transpose()?;
    let mut q = SearchQuery::new(a.query.clone());
    q.project = project;
    q.kinds = strings(&a.kinds);
    q.tags = strings(&a.tags);
    q.limit = if cx.scope.is_some() { limit * 4 } else { limit };

    let mut hits = Vec::new();
    // A failing model must not break search: words still work.
    let vector = cx.embedder.as_ref().and_then(|e| {
        e.embed_query(&a.query).map_err(|err| tracing::warn!("query embedding failed: {err}")).ok()
    });
    for h in cx.engine.search_hybrid(&q, vector.as_deref())? {
        if cx.visible(&h.node.id)? {
            hits.push(h);
        }
        if hits.len() == limit {
            break;
        }
    }
    if hits.is_empty() {
        return Ok(format!(
            "No entries match „{}“. Consider creating one with `capture`.",
            a.query
        ));
    }
    let mut out = if a.query.trim().is_empty() {
        format!("{} recently changed entries:\n", hits.len())
    } else {
        format!("{} results for „{}“:\n", hits.len(), a.query)
    };
    for (i, h) in hits.iter().enumerate() {
        let by_meaning = if h.semantic { " (related by meaning)" } else { "" };
        let _ = writeln!(out, "{}. {}{by_meaning}", i + 1, format::brief(&h.node));
        let snippet = h.snippet.replace('\n', " ");
        if !snippet.trim().is_empty() {
            let _ = writeln!(out, "   {}", snippet.trim());
        }
    }
    Ok(out)
}

// --------------------------------------------------------------------- get ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetArgs {
    /// The entry: exact title, short id (#a1b2c3d4) or full id.
    pub entry: String,
}

pub(crate) fn get(cx: &ToolCx, a: GetArgs) -> Out {
    let id = cx.entry(&a.entry)?;
    let mut d = cx.engine.node_detail(&id)?;
    if cx.scope.is_some() {
        let keep = |other: &NodeId| cx.visible(other).unwrap_or(false);
        d.outgoing.retain(|e| keep(&e.other.id));
        d.incoming.retain(|e| keep(&e.other.id));
    }
    let author = cx.actor_name(&d.node.updated_by);
    Ok(format::node_detail(&d, &author))
}

// ----------------------------------------------------------------- explore ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExploreArgs {
    /// Start entry: exact title, short id or full id.
    pub entry: String,
    /// Hops to follow (1–3, default 2).
    pub depth: Option<usize>,
    /// Maximum entries (default 40).
    pub limit: Option<usize>,
}

pub(crate) fn explore(cx: &ToolCx, a: ExploreArgs) -> Out {
    let id = cx.entry(&a.entry)?;
    let sub = cx.engine.neighborhood(
        &id,
        a.depth.unwrap_or(2).clamp(1, 3),
        a.limit.unwrap_or(40).clamp(1, 200),
    )?;
    let mut visible = HashMap::new();
    for (n, depth) in &sub.nodes {
        if cx.visible(&n.id)? {
            visible.insert(n.id.clone(), (n.clone(), *depth));
        }
    }
    let title = |id: &NodeId| {
        visible
            .get(id)
            .map_or_else(|| "?".into(), |(n, _)| format!("{} ({})", n.title, format::short(id)))
    };
    let mut out = format!(
        "Neighbourhood of {} — {} entries:\n",
        format::brief(&visible[&id].0),
        visible.len()
    );
    let mut by_depth: Vec<_> = visible.values().filter(|(_, d)| *d > 0).collect();
    by_depth.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.title.cmp(&b.0.title)));
    for (n, d) in by_depth {
        let _ = writeln!(out, "- [{d}] {}", format::brief(n));
    }
    out.push_str("\nEdges:\n");
    for e in
        sub.edges.iter().filter(|e| visible.contains_key(&e.src) && visible.contains_key(&e.dst))
    {
        let _ = writeln!(out, "{}", format::edge_line(e, title));
    }
    if sub.truncated {
        out.push_str("\n(truncated: raise `limit` or lower `depth`)\n");
    }
    Ok(out)
}

// ----------------------------------------------------------------- capture ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CaptureEntry {
    /// Local name to reference this entry from `links` (optional).
    pub key: Option<String>,
    pub title: String,
    /// Markdown. `[[Other Title]]` links to other entries automatically.
    #[serde(default)]
    pub body: String,
    /// note (default), concept, task, person, source, decision, snippet, …
    pub kind: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Project names or ids, in addition to the capture-level `project`.
    #[serde(default)]
    pub projects: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CaptureLinkArgs {
    /// A `key` from this capture, or an existing entry (title, short id, id).
    pub from: String,
    pub to: String,
    /// e.g. related_to (default), part_of, depends_on, example_of, supports,
    /// contradicts, derived_from.
    pub relation: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CaptureArgs {
    #[serde(default)]
    pub entries: Vec<CaptureEntry>,
    #[serde(default)]
    pub links: Vec<CaptureLinkArgs>,
    /// Project (name or id) for all entries.
    pub project: Option<String>,
}

pub(crate) fn capture(cx: &ToolCx, a: CaptureArgs) -> Out {
    let project = a.project.as_deref().map(|p| cx.project(p)).transpose()?;
    let mut nodes = Vec::new();
    for e in a.entries {
        let mut requested = e.projects.clone();
        if let Some(p) = &project {
            requested.push(p.to_string());
        }
        let projects = cx.placement(&requested)?;
        nodes.push(CaptureNode {
            key: e.key,
            node: NewNode {
                title: e.title,
                body: e.body,
                kind: e.kind,
                tags: e.tags,
                props: None,
                projects,
            },
        });
    }
    let keys: HashSet<String> =
        nodes.iter().filter_map(|n| n.key.as_ref().map(|k| k.trim().to_lowercase())).collect();
    for l in &a.links {
        for end in [&l.from, &l.to] {
            if !keys.contains(&end.trim().to_lowercase()) {
                cx.entry(end)?;
            }
        }
    }
    let links = a
        .links
        .into_iter()
        .map(|l| CaptureLink { from: l.from, to: l.to, relation: l.relation, note: l.note })
        .collect();
    let r = cx.engine.capture(&cx.ctx, Capture { nodes, links, project: None })?;

    let mut out = format!("Captured {} entries and {} links", r.nodes.len(), r.edges.len());
    if let Some(cs) = &r.change_set {
        let _ = write!(out, " (change set {cs})");
    }
    out.push_str(":\n");
    for n in &r.nodes {
        let _ = writeln!(
            out,
            "- {}",
            format::brief(&NodeBrief {
                id: n.id.clone(),
                title: n.title.clone(),
                kind: n.kind.clone()
            })
        );
    }
    out.push_str(&similar_hints(cx, &r.nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()));
    if !r.possible_duplicates.is_empty() {
        out.push_str("\nPossible duplicates (same title already existed) — consider `update` on the existing entry and `delete` of the new one:\n");
        for (new, old) in &r.possible_duplicates {
            let _ = writeln!(out, "- new {} ≈ existing {}", format::short(new), format::brief(old));
        }
    }
    Ok(out)
}

/// What already exists that resembles new entries. Hints only: nothing here
/// changes the graph. A failing model never fails to write that succeeded.
fn similar_hints(cx: &ToolCx, new: &[NodeId]) -> String {
    let Some(embedder) = &cx.embedder else { return String::new() };
    if let Err(e) = cx.engine.index_now(embedder.as_ref(), new) {
        tracing::warn!("embedding new entries failed: {e}");
        return String::new();
    }
    let mut duplicates = String::new();
    let mut related = String::new();
    for id in new {
        let Ok(similar) = cx.engine.similar_nodes(id, 8, SUGGEST_SCORE) else { continue };
        let linked: HashSet<NodeId> = cx.engine.node_detail(id).map_or_else(
            |_| HashSet::new(),
            |d| d.outgoing.iter().chain(&d.incoming).map(|e| e.other.id.clone()).collect(),
        );
        for s in
            similar.iter().filter(|s| !new.contains(&s.node.id) && !linked.contains(&s.node.id))
        {
            if !cx.visible(&s.node.id).unwrap_or(false) {
                continue;
            }
            let target = if s.likely_duplicate { &mut duplicates } else { &mut related };
            let _ = writeln!(target, "- {} ≈ {}", format::short(id), format::brief(&s.node));
        }
    }
    let mut out = String::new();
    if !duplicates.is_empty() {
        let _ = write!(
            out,
            "\nLikely duplicates of existing entries — consider `merge`:\n{duplicates}"
        );
    }
    if !related.is_empty() {
        let _ =
            write!(out, "\nRelated by meaning and not linked yet — consider `link`:\n{related}");
    }
    out
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SimilarArgs {
    /// The entry: exact title, short id or full id.
    pub entry: String,
    /// Maximum results (default 8).
    pub limit: Option<usize>,
}

pub(crate) fn similar(cx: &ToolCx, a: SimilarArgs) -> Out {
    let Some(embedder) = &cx.embedder else {
        return Err(fail(
            "Similarity needs the embedding model. The user can set it up in Brainmatter → Einstellungen → Suche.",
        ));
    };
    let id = cx.entry(&a.entry)?;
    cx.engine.index_now(embedder.as_ref(), std::slice::from_ref(&id))?;
    let limit = a.limit.unwrap_or(8).clamp(1, 30);
    let mut lines = Vec::new();
    for s in cx.engine.similar_nodes(&id, limit * 2, SUGGEST_SCORE)? {
        if cx.visible(&s.node.id)? && lines.len() < limit {
            let mark = if s.likely_duplicate { " — likely duplicate" } else { "" };
            lines.push(format!("- {} ({:.2}){mark}", format::brief(&s.node), s.score));
        }
    }
    Ok(if lines.is_empty() {
        format!("Nothing clearly similar to „{}“.", a.entry)
    } else {
        format!("Similar to „{}“ (hints; 1.0 = identical):\n{}", a.entry, lines.join("\n"))
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MergeArgs {
    /// The entry that remains (title, short id or id).
    pub keep: String,
    /// Entries to merge into it; they end up in the trash.
    pub merge: Vec<String>,
}

pub(crate) fn merge(cx: &ToolCx, a: MergeArgs) -> Out {
    let keep = cx.entry(&a.keep)?;
    let others = a.merge.iter().map(|m| cx.entry(m)).collect::<Result<Vec<_>, _>>()?;
    if others.is_empty() {
        return Err(fail("`merge` needs at least one entry."));
    }
    let n = cx.engine.merge_nodes(&cx.ctx, &keep, &others)?;
    Ok(format!(
        "Merged {} into **{}** ({}). The user can undo this.",
        a.merge.join(", "),
        n.title,
        format::short(&n.id)
    ))
}

/// More than a model should get in one tool result.
const MAX_ATTACHMENT_CHARS: usize = 100_000;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadAttachmentArgs {
    /// The entry: exact title, short id or full id.
    pub entry: String,
    /// The attachment's file name, as listed by `get`.
    pub file: String,
}

pub(crate) fn read_attachment(cx: &ToolCx, a: ReadAttachmentArgs) -> Out {
    let id = cx.entry(&a.entry)?;
    let detail = cx.engine.node_detail(&id)?;
    let Some(found) = detail.attachments.iter().find(|x| x.filename == a.file) else {
        let names: Vec<_> = detail.attachments.iter().map(|x| x.filename.as_str()).collect();
        return Err(fail(format!(
            "„{}“ has no attachment „{}“. It has: {}",
            detail.node.title,
            a.file,
            if names.is_empty() { "none".to_string() } else { names.join(", ") }
        )));
    };
    let (_, bytes) = cx.engine.attachment_data(&found.id)?;
    let Ok(text) = String::from_utf8(bytes) else {
        return Err(fail(format!(
            "„{}“ is a binary file ({}), which cannot be read as text.",
            found.filename, found.mime
        )));
    };
    let shown: String = text.chars().take(MAX_ATTACHMENT_CHARS).collect();
    let cut = if shown.len() < text.len() {
        format!("\n\n(Cut off after {MAX_ATTACHMENT_CHARS} characters.)")
    } else {
        String::new()
    };
    // Delimited like entries: the content is data, never instructions.
    Ok(format!("<entry-content>\n{shown}\n</entry-content>{cut}"))
}

// ------------------------------------------------------------------ update ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateArgs {
    /// The entry: exact title, short id or full id.
    pub entry: String,
    pub title: Option<String>,
    /// New Markdown content, applied according to `body_mode`.
    pub body: Option<String>,
    /// append (default, safest), prepend or replace.
    pub body_mode: Option<String>,
    pub kind: Option<String>,
    #[serde(default)]
    pub tags_add: Vec<String>,
    #[serde(default)]
    pub tags_remove: Vec<String>,
}

pub(crate) fn update(cx: &ToolCx, a: UpdateArgs) -> Out {
    let id = cx.entry(&a.entry)?;
    let body = match (a.body, a.body_mode.as_deref().unwrap_or("append")) {
        (None, _) => None,
        (Some(t), "append") => Some(BodyEdit::Append(t)),
        (Some(t), "prepend") => Some(BodyEdit::Prepend(t)),
        (Some(t), "replace") => Some(BodyEdit::Replace(t)),
        (Some(_), other) => {
            return Err(fail(format!(
                "body_mode must be append, prepend or replace, not „{other}“"
            )));
        }
    };
    let n = cx.engine.update_node(
        &cx.ctx,
        &id,
        NodePatch {
            title: a.title,
            body,
            kind: a.kind,
            props: None,
            tags_add: a.tags_add,
            tags_remove: a.tags_remove,
            reviewed: None,
        },
    )?;
    Ok(format!("Updated **{}** ({}), now rev {}.", n.title, format::short(&n.id), n.rev))
}

// ------------------------------------------------------------- link/unlink ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LinkArgs {
    pub from: String,
    pub to: String,
    /// related_to (default), part_of, depends_on, example_of, supports,
    /// contradicts, derived_from, …
    pub relation: Option<String>,
    pub note: Option<String>,
}

pub(crate) fn link(cx: &ToolCx, a: LinkArgs) -> Out {
    let (src, dst) = (cx.entry(&a.from)?, cx.entry(&a.to)?);
    let e = cx
        .engine
        .link(&cx.ctx, NewEdge { src, dst, relation: a.relation, weight: None, note: a.note })?;
    Ok(format!("Linked {} —{}→ {}.", a.from, e.relation, a.to))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnlinkArgs {
    pub from: String,
    pub to: String,
    /// Only this relation; omit to remove all links between the two.
    pub relation: Option<String>,
}

pub(crate) fn unlink(cx: &ToolCx, a: UnlinkArgs) -> Out {
    let (src, dst) = (cx.entry(&a.from)?, cx.entry(&a.to)?);
    let removed = cx.engine.unlink(&cx.ctx, &src, &dst, a.relation.as_deref())?;
    Ok(match removed.len() {
        0 => format!("No link between {} and {}.", a.from, a.to),
        n => format!("Removed {n} link(s) between {} and {}.", a.from, a.to),
    })
}

// ---------------------------------------------------------------- projects ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoArgs {}

pub(crate) fn list_projects(cx: &ToolCx, _a: NoArgs) -> Out {
    let projects: Vec<_> =
        cx.engine.projects()?.into_iter().filter(|p| cx.visible_project(&p.project.id)).collect();
    if projects.is_empty() {
        return Ok("No projects yet. Create one with `create_project`.".into());
    }
    let depth = |p: &bm_core::model::Project| {
        let mut d = 0;
        let mut cur = p.parent_id.clone();
        while let Some(id) = cur {
            d += 1;
            cur = projects
                .iter()
                .find(|x| x.project.id == id)
                .and_then(|x| x.project.parent_id.clone());
        }
        d
    };
    let mut out = String::from("Projects (entries directly in each):\n");
    for p in &projects {
        let _ = writeln!(
            out,
            "{}- {} ({} entries)",
            "  ".repeat(depth(&p.project)),
            p.project.name,
            p.node_count
        );
    }
    Ok(out)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateProjectArgs {
    pub name: String,
    /// Parent project (name or id) to create a subproject.
    pub parent: Option<String>,
    #[serde(default)]
    pub description: String,
}

pub(crate) fn create_project(cx: &ToolCx, a: CreateProjectArgs) -> Out {
    let parent = match a.parent.as_deref() {
        Some(p) => Some(cx.project(p)?),
        None if cx.scope.is_some() => {
            return Err(fail(
                "This token is limited to certain projects; pass `parent` to create a subproject.",
            ));
        }
        None => None,
    };
    let p = cx.engine.create_project(
        &cx.ctx,
        NewProject { name: a.name, parent, description: a.description, color: None, icon: None },
    )?;
    Ok(format!("Created project **{}**.", p.name))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MembershipArgs {
    /// Entries (titles, short ids or ids).
    pub entries: Vec<String>,
    /// Project name or id.
    pub project: String,
}

pub(crate) fn add_to_project(cx: &ToolCx, a: MembershipArgs) -> Out {
    let project = cx.project(&a.project)?;
    let ids = a.entries.iter().map(|e| cx.entry(e)).collect::<Result<Vec<_>, _>>()?;
    let n = cx.engine.add_to_project(&cx.ctx, &project, &ids)?;
    Ok(format!("Added {n} entries to {}.", a.project))
}

pub(crate) fn remove_from_project(cx: &ToolCx, a: MembershipArgs) -> Out {
    let project = cx.project(&a.project)?;
    let ids = a.entries.iter().map(|e| cx.entry(e)).collect::<Result<Vec<_>, _>>()?;
    let n = cx.engine.remove_from_project(&cx.ctx, &project, &ids)?;
    Ok(format!("Removed {n} entries from {}.", a.project))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectArgs {
    /// Project name or id.
    pub project: String,
}

pub(crate) fn project_overview(cx: &ToolCx, a: ProjectArgs) -> Out {
    let id = cx.project(&a.project)?;
    let o = cx.engine.project_overview(&id)?;
    let mut path: Vec<_> = o.path.iter().map(|p| p.name.as_str()).collect();
    path.push(&o.project.name);
    let mut out = format!(
        "# Project {}\n{} entries · {} links",
        path.join(" › "),
        o.node_count,
        o.edge_count
    );
    if o.unreviewed > 0 {
        let _ = write!(out, " · {} unreviewed", o.unreviewed);
    }
    out.push('\n');
    if !o.project.description.trim().is_empty() {
        let _ = writeln!(out, "\n{}", o.project.description.trim());
    }
    let section = |out: &mut String, title: &str, items: Vec<String>| {
        if !items.is_empty() {
            let _ = writeln!(out, "\n{title}:\n- {}", items.join("\n- "));
        }
    };
    section(
        &mut out,
        "Subprojects",
        o.subprojects
            .iter()
            .map(|s| format!("{} ({} entries)", s.project.name, s.node_count))
            .collect(),
    );
    section(
        &mut out,
        "Most connected",
        o.hubs.iter().map(|(n, d)| format!("{} — {d} links", format::brief(n))).collect(),
    );
    let themes: Vec<String> = o
        .themes
        .iter()
        .map(|members| members.iter().map(|n| n.title.as_str()).collect::<Vec<_>>().join(", "))
        .collect();
    section(&mut out, "Themes (densely linked groups)", themes);
    section(&mut out, "Recently changed", o.recent.iter().map(format::brief).collect());
    section(&mut out, "Tasks", o.tasks.iter().map(format::brief).collect());
    section(&mut out, "Kinds", o.kinds.iter().map(|(k, n)| format!("{k}: {n}")).collect());
    Ok(out)
}

// ----------------------------------------------------------------- history ---

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecentArgs {
    /// Maximum change sets (default 15).
    pub limit: Option<usize>,
    /// Only changes after this Unix time in milliseconds.
    pub since: Option<i64>,
}

pub(crate) fn recent_changes(cx: &ToolCx, a: RecentArgs) -> Out {
    let changes = cx.engine.recent_changes(a.limit.unwrap_or(15).clamp(1, 100), a.since)?;
    let mut out = String::new();
    for c in changes {
        let nodes: Vec<_> = c.nodes.iter().filter(|n| cx.visible(&n.id).unwrap_or(false)).collect();
        if cx.scope.is_some() && nodes.is_empty() {
            continue;
        }
        let cs = &c.change_set;
        let who = match &cs.client {
            Some(client) => format!("{} via {client}", cs.actor_name),
            None => cs.actor_name.clone(),
        };
        let reverted = if cs.reverted_by.is_some() { " (reverted)" } else { "" };
        let _ = writeln!(
            out,
            "- {} · {who}: {}{reverted} [change set {}]",
            format::time(cs.at),
            cs.summary,
            cs.id
        );
        for n in nodes.iter().take(5) {
            let _ = writeln!(out, "  · {}", format::brief(n));
        }
    }
    Ok(if out.is_empty() {
        "No changes yet.".into()
    } else {
        format!("Recent changes (newest first):\n{out}")
    })
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PathArgs {
    pub from: String,
    pub to: String,
    /// Maximum hops (default 5).
    pub max_depth: Option<usize>,
}

pub(crate) fn find_path(cx: &ToolCx, a: PathArgs) -> Out {
    let (from, to) = (cx.entry(&a.from)?, cx.entry(&a.to)?);
    let Some(path) = cx.engine.shortest_path(&from, &to, a.max_depth.unwrap_or(5))? else {
        return Ok(format!(
            "{} and {} are not connected within {} hops.",
            a.from,
            a.to,
            a.max_depth.unwrap_or(5)
        ));
    };
    if path.iter().any(|s| !cx.visible(&s.node.id).unwrap_or(false)) {
        return Ok(format!("{} and {} are not connected within your projects.", a.from, a.to));
    }
    let mut out = format!("Path ({} hops):\n", path.len() - 1);
    for step in &path {
        match &step.via {
            None => {
                let _ = writeln!(out, "{}", format::brief(&step.node));
            }
            Some(e) => {
                let dir = if e.dst == step.node.id { "→" } else { "←" };
                let _ = writeln!(out, "  {dir} {} {dir} {}", e.relation, format::brief(&step.node));
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteArgs {
    /// The entry: exact title, short id or full id. Moves it to the trash.
    pub entry: String,
}

pub(crate) fn delete(cx: &ToolCx, a: DeleteArgs) -> Out {
    let id = cx.entry(&a.entry)?;
    let title = cx.engine.node(&id)?.map(|n| n.title).unwrap_or_default();
    cx.engine.delete_node(&cx.ctx, &id)?;
    Ok(format!("Moved **{title}** ({}) to the trash. The user can restore it.", format::short(&id)))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RevertArgs {
    /// Id of one of your own change sets (from `capture` or `recent_changes`).
    pub change_set: String,
}

pub(crate) fn revert(cx: &ToolCx, a: RevertArgs) -> Out {
    let id = ChangeSetId::from_raw(a.change_set.trim().to_string());
    let cs = cx
        .engine
        .change_set(&id)?
        .ok_or_else(|| fail(format!("No change set {}.", a.change_set)))?;
    if cs.actor != cx.ctx.actor {
        return Err(fail(
            "You can only revert your own change sets; ask the user to undo others in the app.",
        ));
    }
    let r = cx.engine.revert(&cx.ctx, &id)?;
    Ok(format!("Reverted change set {} ({} changes undone).", r.reverted, r.restored))
}
