//! RetinaX off-chain node.
//!
//! Serves audit events that an indexer has copied from the `audit` contract into PostgreSQL.
//!
//! | Route | Auth | Purpose |
//! |-------|------|---------|
//! | `GET /health` | none | Liveness and database check |
//! | `GET /api/audit-logs/{patient_id}` | bearer token | A patient's audit events, newest first |

pub mod config;
pub mod store;

use std::sync::Arc;

use axum::{
    extract::{rejection::QueryRejection, Path, Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use subtle::ConstantTimeEq;
use tower_http::trace::TraceLayer;

use store::{AuditEvent, AuditLogStore};

/// Page size when `limit` is not given.
pub const DEFAULT_LIMIT: u32 = 50;
/// Largest `limit` accepted.
pub const MAX_LIMIT: u32 = 200;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn AuditLogStore>,
    pub api_token: Arc<str>,
}

/// Builds the HTTP router.
pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/audit-logs/{patient_id}", get(get_audit_logs))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_bearer_token,
        ));

    Router::new()
        .route("/health", get(health))
        .nest("/api", api)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// JSON error body: `{"error": "..."}`.
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(json!({ "error": self.message }))).into_response();
        if self.status == StatusCode::UNAUTHORIZED {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, "Bearer".parse().unwrap());
        }
        response
    }
}

/// Rejects requests without `Authorization: Bearer <API_TOKEN>`.
async fn require_bearer_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    // Constant-time comparison, so response timing does not leak the token.
    let matches: bool = presented
        .as_bytes()
        .ct_eq(state.api_token.as_bytes())
        .into();
    if !matches {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "missing or invalid bearer token",
        ));
    }
    Ok(next.run(request).await)
}

async fn health(State(state): State<AppState>) -> Response {
    match state.store.ping().await {
        Ok(()) => Json(json!({ "status": "ok" })).into_response(),
        Err(err) => {
            tracing::error!(error = %err, "health check: database unavailable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unavailable" })),
            )
                .into_response()
        }
    }
}

/// Query parameters for `GET /api/audit-logs/{patient_id}`.
#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    /// Page size, 1 to `MAX_LIMIT`. Defaults to `DEFAULT_LIMIT`.
    pub limit: Option<u32>,
    /// Only return events with an `id` below this, taken from the previous page's `next_cursor`.
    pub before: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AuditLogResponse {
    pub patient_id: String,
    pub count: usize,
    /// Pass as `before` to fetch the next page; `null` on the last page.
    pub next_cursor: Option<i64>,
    pub entries: Vec<AuditEvent>,
}

/// Returns `patient_id`'s audit events, newest first.
async fn get_audit_logs(
    State(state): State<AppState>,
    Path(patient_id): Path<String>,
    query: Result<Query<AuditLogQuery>, QueryRejection>,
) -> Result<Json<AuditLogResponse>, ApiError> {
    if !is_stellar_address(&patient_id) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "patient_id must be a Stellar address (56 characters starting with G or C)",
        ));
    }

    let Query(query) = query.map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "limit and before must be positive integers",
        )
    })?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 || limit > MAX_LIMIT {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            format!("limit must be between 1 and {MAX_LIMIT}"),
        ));
    }
    if query.before.is_some_and(|b| b <= 0) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "before must be a positive integer",
        ));
    }

    let page = state
        .store
        .patient_events(&patient_id, limit, query.before)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "audit log query failed");
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "failed to load audit logs",
            )
        })?;

    Ok(Json(AuditLogResponse {
        patient_id,
        count: page.events.len(),
        next_cursor: page.next_cursor,
        entries: page.events,
    }))
}

/// Checks the shape of a Stellar account (`G...`) or contract (`C...`) address: 56 characters of
/// RFC 4648 base32. The checksum is not verified; an unknown address simply has no events.
pub fn is_stellar_address(value: &str) -> bool {
    value.len() == 56
        && matches!(value.as_bytes()[0], b'G' | b'C')
        && value
            .bytes()
            .all(|b| b.is_ascii_uppercase() || (b'2'..=b'7').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stellar_address_shape() {
        let account = "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7";
        let contract = "CAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7";
        assert!(is_stellar_address(account));
        assert!(is_stellar_address(contract));

        assert!(!is_stellar_address(""));
        assert!(!is_stellar_address(&account[..55]));
        assert!(!is_stellar_address(&account.replace('G', "S")));
        assert!(!is_stellar_address(&account.to_lowercase()));
        // 0, 1, 8 and 9 are not in the base32 alphabet.
        assert!(!is_stellar_address(&account.replace('2', "1")));
        assert!(!is_stellar_address(
            "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCW'1"
        ));
    }
}
