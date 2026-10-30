//! Bearer-token authentication against the [`ClientRegistry`].

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::ClientRegistry;

pub(crate) async fn require_bearer(
    State(registry): State<Arc<ClientRegistry>>,
    mut req: Request,
    next: Next,
) -> Response {
    let client = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| registry.authenticate(token.trim()));

    match client {
        Some(client) => {
            req.extensions_mut().insert(client);
            next.run(req).await
        }
        None => (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))],
            "missing or invalid Brainmatter token",
        )
            .into_response(),
    }
}
