use rmcp::{
    ErrorData, ServerHandler,
    handler::server::{router::tool::ToolRouter, tool::Extension},
    model::{CallToolResult, ContentBlock},
    tool, tool_handler, tool_router,
};

use crate::Actor;

#[derive(Clone)]
pub struct BrainmatterMcp {
    tool_router: ToolRouter<Self>,
}

impl BrainmatterMcp {
    pub fn new() -> Self {
        Self { tool_router: Self::tool_router() }
    }
}

impl Default for BrainmatterMcp {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl BrainmatterMcp {
    #[tool(description = "Check connectivity. Returns who you are authenticated as.")]
    async fn ping(
        &self,
        Extension(parts): Extension<axum::http::request::Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let actor = parts.extensions.get::<Actor>().ok_or_else(|| {
            ErrorData::internal_error("request reached tool without an actor", None)
        })?;
        Ok(CallToolResult::success(vec![ContentBlock::text(format!(
            "pong — authenticated as {}",
            actor.name
        ))]))
    }
}

#[tool_handler(
    router = self.tool_router,
    name = "brainmatter",
    instructions = "Brainmatter is the user's personal knowledge graph. \
Search before you create, extend existing entries instead of duplicating them, \
keep one idea per entry, link entries generously and assign them to a project."
)]
impl ServerHandler for BrainmatterMcp {}
