//! Domain types and API inputs.

use serde::{Deserialize, Serialize};

use crate::ids::{
    ActorId, AttachmentId, ChangeSetId, DocSourceId, EdgeId, FileLinkId, NodeId, ProjectId,
};

pub const DEFAULT_KIND: &str = "note";
pub const DEFAULT_RELATION: &str = "related_to";
pub const WIKILINK_RELATION: &str = "mentions";
pub const TEMPLATE_KIND: &str = "template";
pub const DOC_KIND: &str = "doc";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    User,
    Ai,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Actor {
    pub id: ActorId,
    pub kind: ActorKind,
    pub name: String,
    pub color: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub title: String,
    pub body: String,
    pub kind: String,
    pub props: Option<serde_json::Value>,
    /// False for entries an AI created that the user has not looked at yet.
    pub reviewed: bool,
    pub rev: i64,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub created_by: ActorId,
    pub updated_by: ActorId,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeOrigin {
    Manual,
    Wikilink,
    Ai,
    Suggestion,
}

impl EdgeOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Wikilink => "wikilink",
            Self::Ai => "ai",
            Self::Suggestion => "suggestion",
        }
    }

    pub(crate) fn parse(s: &str) -> Self {
        match s {
            "wikilink" => Self::Wikilink,
            "ai" => Self::Ai,
            "suggestion" => Self::Suggestion,
            _ => Self::Manual,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub src: NodeId,
    pub dst: NodeId,
    pub relation: String,
    pub weight: f64,
    pub note: Option<String>,
    pub origin: EdgeOrigin,
    pub created_at: i64,
    pub created_by: ActorId,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub parent_id: Option<ProjectId>,
    pub name: String,
    pub description: String,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub sort: i64,
    pub created_at: i64,
    pub created_by: ActorId,
    pub deleted_at: Option<i64>,
}

/// Minimal node reference for lists, suggestions and edge endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeBrief {
    pub id: NodeId,
    pub title: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeView {
    pub edge: Edge,
    /// The node on the other end, from the perspective of the queried node.
    pub other: NodeBrief,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeDetail {
    pub node: Node,
    pub projects: Vec<ProjectBrief>,
    pub outgoing: Vec<EdgeView>,
    pub incoming: Vec<EdgeView>,
    pub attachments: Vec<Attachment>,
    pub file_links: Vec<FileLink>,
    /// A documentation page that mirrors a file and can only be changed there.
    pub read_only: bool,
}

/// A page in a project's documentation, in reading order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocPage {
    pub node: NodeBrief,
    pub parent: Option<NodeId>,
    pub depth: u32,
    pub read_only: bool,
    /// Relative to its source, for mirrored pages.
    pub source_path: Option<String>,
    pub reviewed: bool,
    pub updated_at: i64,
}

/// A folder or file whose Markdown a project's documentation mirrors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocSource {
    pub id: DocSourceId,
    pub project_id: ProjectId,
    pub path: String,
    pub created_at: i64,
    pub pages: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocSyncReport {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub unchanged: usize,
    /// Files passed over: too large or not text.
    pub skipped: usize,
}

/// A reference from an entry to a file or folder on the Mac.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLink {
    pub id: FileLinkId,
    pub node_id: NodeId,
    pub path: String,
    pub label: String,
    /// May AI clients read it through Brainmatter?
    pub approved_for_ai: bool,
    pub created_at: i64,
    pub created_by: ActorId,
    pub deleted_at: Option<i64>,
}

/// What deleting a project would affect.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDeletePreview {
    pub subprojects: usize,
    /// Entries in no other project, which could go to the trash with it.
    pub entries_only_here: usize,
    /// Entries also in other projects, which always stay.
    pub entries_elsewhere: usize,
    /// Documentation pages and templates, which go to the trash in any case.
    pub documents: usize,
}

/// A template in the library.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateInfo {
    pub node: NodeBrief,
    pub slug: String,
    pub body: String,
    pub arguments: Vec<String>,
    /// Empty for a template available everywhere.
    pub projects: Vec<ProjectBrief>,
    /// Has the user seen it? AIs can write templates, and only reviewed ones
    /// are offered to clients as prompts.
    pub reviewed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashedNode {
    pub node: NodeBrief,
    pub deleted_at: i64,
}

/// A file attached to an entry; the content lives encrypted outside the database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: AttachmentId,
    pub node_id: NodeId,
    pub filename: String,
    pub mime: String,
    pub size: i64,
    pub blob_id: String,
    pub created_at: i64,
    pub created_by: ActorId,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectBrief {
    pub id: ProjectId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub project: Project,
    /// Live nodes directly in this project (not counting subprojects).
    pub node_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: NodeId,
    pub title: String,
    pub kind: String,
    pub reviewed: bool,
    pub updated_at: i64,
    pub created_by: ActorId,
    pub projects: Vec<ProjectId>,
    /// Number of live edges within the view.
    pub degree: i64,
    /// PageRank within the view; sums to about 1 over all nodes.
    pub rank: f32,
    /// Topic cluster within the view; 0 is the largest.
    pub cluster: u32,
    /// Saved layout position for this view, if any.
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: EdgeId,
    pub src: NodeId,
    pub dst: NodeId,
    pub relation: String,
    pub origin: EdgeOrigin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphSnapshot {
    /// `"all"` or a project id.
    pub view: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutPosition {
    pub node: NodeId,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub pinned: bool,
}

/// Breadth-first neighbourhood of a node (edges in both directions).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subgraph {
    pub root: NodeId,
    /// Nodes with their hop distance from the root (root first).
    pub nodes: Vec<(NodeBrief, usize)>,
    /// Live edges between the returned nodes.
    pub edges: Vec<Edge>,
    /// True if `limit` cut the traversal short.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathStep {
    pub node: NodeBrief,
    /// Edge that leads here from the previous step (None for the start).
    pub via: Option<Edge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectOverview {
    pub project: Project,
    /// Ancestors, root first.
    pub path: Vec<ProjectBrief>,
    pub subprojects: Vec<ProjectSummary>,
    /// Live nodes in the project and its subprojects.
    pub node_count: usize,
    pub edge_count: usize,
    /// Most important entries by PageRank, with their degree in the project.
    pub hubs: Vec<(NodeBrief, i64)>,
    /// Topic clusters (largest first), each with its most important entries.
    pub themes: Vec<Vec<NodeBrief>>,
    pub recent: Vec<NodeBrief>,
    pub kinds: Vec<(String, usize)>,
    pub tasks: Vec<NodeBrief>,
    pub unreviewed: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentChange {
    pub change_set: ChangeSetInfo,
    /// Entries the change set touched (live or not).
    pub nodes: Vec<NodeBrief>,
}

// ---------------------------------------------------------------- inputs ---

/// Who is acting, recorded on every change set.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub actor: ActorId,
    /// MCP client name (e.g. "claude-code"), if the change came through MCP.
    pub client: Option<String>,
    /// Tool or UI action that caused the change.
    pub tool: Option<String>,
}

impl Ctx {
    pub fn new(actor: ActorId) -> Self {
        Self { actor, client: None, tool: None }
    }

    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }

    pub fn with_client(mut self, client: impl Into<String>) -> Self {
        self.client = Some(client.into());
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NewNode {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub props: Option<serde_json::Value>,
    #[serde(default)]
    pub projects: Vec<ProjectId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", content = "text", rename_all = "snake_case")]
pub enum BodyEdit {
    Replace(String),
    Append(String),
    Prepend(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodePatch {
    pub title: Option<String>,
    pub body: Option<BodyEdit>,
    pub kind: Option<String>,
    pub props: Option<serde_json::Value>,
    #[serde(default)]
    pub tags_add: Vec<String>,
    #[serde(default)]
    pub tags_remove: Vec<String>,
    pub reviewed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEdge {
    pub src: NodeId,
    pub dst: NodeId,
    pub relation: Option<String>,
    pub weight: Option<f64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NewProject {
    pub name: String,
    pub parent: Option<ProjectId>,
    #[serde(default)]
    pub description: String,
    pub color: Option<String>,
    pub icon: Option<String>,
}

/// Batch creation of a small subgraph in one change set (MCP `capture`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capture {
    pub nodes: Vec<CaptureNode>,
    #[serde(default)]
    pub links: Vec<CaptureLink>,
    /// Added to every captured node in addition to its own projects.
    pub project: Option<ProjectId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureNode {
    /// Local name used by `links` within the same capture.
    pub key: Option<String>,
    #[serde(flatten)]
    pub node: NewNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureLink {
    /// A capture key, or a reference to an existing node (id, short id, title).
    pub from: String,
    pub to: String,
    pub relation: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureResult {
    pub change_set: Option<ChangeSetId>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Existing live nodes whose title equals a captured title (likely duplicates).
    pub possible_duplicates: Vec<(NodeId, NodeBrief)>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: String,
    pub project: Option<ProjectId>,
    /// Also match nodes in subprojects of `project` (default true via `new`).
    pub include_subprojects: bool,
    #[serde(default)]
    pub kinds: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub limit: usize,
}

impl SearchQuery {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), include_subprojects: true, limit: 20, ..Default::default() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    pub node: NodeBrief,
    /// Body excerpt with matches wrapped in `**`.
    pub snippet: String,
    /// Higher is better.
    pub score: f64,
    /// Found (also) by meaning rather than only by its words.
    #[serde(default)]
    pub semantic: bool,
}

/// An entry that seems related to another one, by meaning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Similar {
    pub node: NodeBrief,
    pub score: f32,
    /// Similar enough to probably say the same thing.
    pub likely_duplicate: bool,
}

/// Progress of the background embedding of entries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexStatus {
    pub indexed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeSetInfo {
    pub id: ChangeSetId,
    pub actor: ActorId,
    pub actor_name: String,
    pub client: Option<String>,
    pub tool: Option<String>,
    pub summary: String,
    pub at: i64,
    pub reverted_by: Option<ChangeSetId>,
    pub revisions: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeRevision {
    pub change_set: ChangeSetId,
    pub actor_name: String,
    pub at: i64,
    pub rev: i64,
    pub before: Option<Node>,
    pub after: Option<Node>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RevertReport {
    pub reverted: ChangeSetId,
    pub change_set: ChangeSetId,
    pub restored: usize,
}
