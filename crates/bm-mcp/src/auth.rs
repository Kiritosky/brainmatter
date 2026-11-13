//! Bearer-token authentication against the [`ClientRegistry`].

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{ClientRegistry, clients::Denied};

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
        .map_or(Err(Denied::Unknown), |token| registry.authenticate(token.trim()));

    let refused = match client {
        Ok(client) => {
            req.extensions_mut().insert(client);
            return next.run(req).await;
        }
        Err(Denied::Unknown) => "missing or invalid Brainmatter token",
        Err(Denied::Expired) => {
            "this Brainmatter token has expired; the user can create a new one in Brainmatter → Settings → AI clients"
        }
    };
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))],
        refused,
    )
        .into_response()
}
