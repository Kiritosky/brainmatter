//! Domain events, broadcast after each committed change set. The UI uses them
//! for live updates; the MCP server for resource notifications.

use serde::Serialize;

use crate::ids::{ActorId, ChangeSetId, EdgeId, NodeId, ProjectId};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    NodeUpserted { id: NodeId },
    NodeDeleted { id: NodeId },
    EdgeUpserted { id: EdgeId, src: NodeId, dst: NodeId },
    EdgeDeleted { id: EdgeId, src: NodeId, dst: NodeId },
    ProjectUpserted { id: ProjectId },
    ProjectDeleted { id: ProjectId },
    MembershipChanged { project: ProjectId, node: NodeId, member: bool },
    ChangeSetCommitted { id: ChangeSetId, actor: ActorId, summary: String },
}
