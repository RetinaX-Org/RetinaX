use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use chrono::{TimeZone, Utc};
use http_body_util::BodyExt;
use offchain_server::{
    router,
    store::{AuditEvent, AuditLogStore, MemoryStore, Page, StoreError},
    AppState, MAX_LIMIT,
};
use serde_json::Value;
use tower::ServiceExt;

const TOKEN: &str = "test-token-0123456789abcdef012345";
const PATIENT: &str = "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7";
const OTHER_PATIENT: &str = "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H";

fn event(id: i64, patient_id: &str) -> AuditEvent {
    AuditEvent {
        id,
        patient_id: patient_id.to_string(),
        sequence: id,
        actor: "GDOCTOR".to_string(),
        action: "read_record".to_string(),
        target: format!("record-{id}"),
        result: "success".to_string(),
        occurred_at: Utc.timestamp_opt(1_700_000_000 + id, 0).unwrap(),
        contract_id: "CAUDIT".to_string(),
        ledger: 1_000 + id,
        tx_hash: format!("tx{id}"),
        event_index: 0,
    }
}

fn app_with(store: impl AuditLogStore + 'static) -> Router {
    router(AppState {
        store: Arc::new(store),
        api_token: TOKEN.into(),
    })
}

/// Five events for `PATIENT` (ids 1, 2, 4, 5, 7) and two for `OTHER_PATIENT`.
fn app() -> Router {
    app_with(MemoryStore::new(vec![
        event(1, PATIENT),
        event(2, PATIENT),
        event(3, OTHER_PATIENT),
        event(4, PATIENT),
        event(5, PATIENT),
        event(6, OTHER_PATIENT),
        event(7, PATIENT),
    ]))
}

async fn get(app: Router, uri: &str, token: Option<&str>) -> (StatusCode, Value) {
    let mut request = Request::get(uri);
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn ids(body: &Value) -> Vec<i64> {
    body["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn returns_only_the_patients_events_newest_first() {
    let (status, body) = get(app(), &format!("/api/audit-logs/{PATIENT}"), Some(TOKEN)).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["patient_id"], PATIENT);
    assert_eq!(body["count"], 5);
    assert_eq!(body["next_cursor"], Value::Null);
    assert_eq!(ids(&body), vec![7, 5, 4, 2, 1]);

    let first = &body["entries"][0];
    assert_eq!(first["patient_id"], PATIENT);
    assert_eq!(first["actor"], "GDOCTOR");
    assert_eq!(first["action"], "read_record");
    assert_eq!(first["target"], "record-7");
    assert_eq!(first["result"], "success");
    assert_eq!(first["tx_hash"], "tx7");
    assert_eq!(first["occurred_at"], "2023-11-14T22:13:27Z");
}

#[tokio::test]
async fn paginates_with_before_cursor() {
    let uri = format!("/api/audit-logs/{PATIENT}?limit=2");
    let (_, page1) = get(app(), &uri, Some(TOKEN)).await;
    assert_eq!(ids(&page1), vec![7, 5]);
    assert_eq!(page1["next_cursor"], 5);

    let uri = format!("/api/audit-logs/{PATIENT}?limit=2&before=5");
    let (_, page2) = get(app(), &uri, Some(TOKEN)).await;
    assert_eq!(ids(&page2), vec![4, 2]);
    assert_eq!(page2["next_cursor"], 2);

    let uri = format!("/api/audit-logs/{PATIENT}?limit=2&before=2");
    let (_, page3) = get(app(), &uri, Some(TOKEN)).await;
    assert_eq!(ids(&page3), vec![1]);
    assert_eq!(page3["next_cursor"], Value::Null);
}

#[tokio::test]
async fn exact_page_size_has_no_next_cursor() {
    let uri = format!("/api/audit-logs/{PATIENT}?limit=5");
    let (_, body) = get(app(), &uri, Some(TOKEN)).await;
    assert_eq!(body["count"], 5);
    assert_eq!(body["next_cursor"], Value::Null);
}

#[tokio::test]
async fn unknown_patient_gets_an_empty_list() {
    let unknown = "GCEZWKCA5VLDNRLN3RPRJMRZOX3Z6G5CHCGSNFHEYVXM3XOJMDS674JZ";
    let (status, body) = get(app(), &format!("/api/audit-logs/{unknown}"), Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["count"], 0);
    assert_eq!(body["entries"], Value::Array(vec![]));
}

#[tokio::test]
async fn requires_a_valid_bearer_token() {
    let uri = format!("/api/audit-logs/{PATIENT}");

    for token in [None, Some("wrong-token"), Some("")] {
        let (status, body) = get(app(), &uri, token).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "token {token:?}");
        assert_eq!(body["error"], "missing or invalid bearer token");
    }

    // The scheme must be Bearer.
    let response = app()
        .oneshot(
            Request::get(&uri)
                .header(header::AUTHORIZATION, format!("Basic {TOKEN}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()[header::WWW_AUTHENTICATE], "Bearer");
}

#[tokio::test]
async fn rejects_invalid_patient_ids() {
    for bad in [
        "not-an-address",
        "SAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7",
    ] {
        let (status, body) = get(app(), &format!("/api/audit-logs/{bad}"), Some(TOKEN)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad}");
        assert!(body["error"].as_str().unwrap().contains("Stellar address"));
    }
}

#[tokio::test]
async fn rejects_invalid_query_parameters() {
    for query in [
        "limit=0".to_string(),
        format!("limit={}", MAX_LIMIT + 1),
        "limit=-1".to_string(),
        "limit=abc".to_string(),
        "before=0".to_string(),
        "before=-5".to_string(),
        "before=abc".to_string(),
    ] {
        let uri = format!("/api/audit-logs/{PATIENT}?{query}");
        let (status, body) = get(app(), &uri, Some(TOKEN)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
        assert!(body["error"].is_string(), "{query}");
    }

    let uri = format!("/api/audit-logs/{PATIENT}?limit={MAX_LIMIT}");
    let (status, _) = get(app(), &uri, Some(TOKEN)).await;
    assert_eq!(status, StatusCode::OK);
}

struct FailingStore;

#[async_trait]
impl AuditLogStore for FailingStore {
    async fn patient_events(&self, _: &str, _: u32, _: Option<i64>) -> Result<Page, StoreError> {
        Err(StoreError("connection refused (secret detail)".into()))
    }

    async fn ping(&self) -> Result<(), StoreError> {
        Err(StoreError("connection refused".into()))
    }
}

#[tokio::test]
async fn database_errors_return_500_without_details() {
    let uri = format!("/api/audit-logs/{PATIENT}");
    let (status, body) = get(app_with(FailingStore), &uri, Some(TOKEN)).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "failed to load audit logs");
}

#[tokio::test]
async fn health_is_public_and_reports_database_state() {
    let (status, body) = get(app(), "/health", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");

    let (status, body) = get(app_with(FailingStore), "/health", None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "unavailable");
}
