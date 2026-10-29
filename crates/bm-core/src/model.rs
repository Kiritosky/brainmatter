//! Domain types and API inputs.

use serde::{Deserialize, Serialize};

use crate::ids::{ActorId, ChangeSetId, EdgeId, NodeId, ProjectId};

pub const DEFAULT_KIND: &str = "note";
pub const DEFAULT_RELATION: &str = "related_to";
pub const WIKILINK_RELATION: &str = "mentions";
pub const TEMPLATE_KIND: &str = "template";

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
