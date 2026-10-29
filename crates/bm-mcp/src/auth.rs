//! Bearer-token authentication. Tokens are 256-bit random strings; only their
//! SHA-256 hash is kept, so a plain hash lookup is sufficient (no KDF needed).

use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};

/// The authenticated caller, injected into request extensions for tools.
#[derive(Debug, Clone)]
pub struct Actor {
    pub id: String,
    pub name: String,
}

#[derive(Default)]
pub struct TokenStore {
    by_hash: HashMap<[u8; 32], Actor>,
}

impl TokenStore {
    pub fn insert(&mut self, token: &str, actor: Actor) {
        self.by_hash.insert(hash(token), actor);
    }

    pub fn lookup(&self, token: &str) -> Option<&Actor> {
        self.by_hash.get(&hash(token))
    }
}

fn hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

pub(crate) async fn require_bearer(
    State(tokens): State<Arc<TokenStore>>,
    mut req: Request,
    next: Next,
) -> Response {
    let actor = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|token| tokens.lookup(token.trim()))
        .cloned();

    match actor {
        Some(actor) => {
            req.extensions_mut().insert(actor);
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
