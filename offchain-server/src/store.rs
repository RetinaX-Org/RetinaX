//! Read access to indexed audit events.

use std::sync::RwLock;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

/// One audit event, as stored in `audit_events` and returned by the API.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, FromRow)]
pub struct AuditEvent {
    pub id: i64,
    pub patient_id: String,
    pub sequence: i64,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub result: String,
    pub occurred_at: DateTime<Utc>,
    pub contract_id: String,
    pub ledger: i64,
    pub tx_hash: String,
    pub event_index: i32,
}

/// One page of a patient's audit events, newest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub events: Vec<AuditEvent>,
    /// Pass as `before` to fetch the next page; `None` on the last page.
    pub next_cursor: Option<i64>,
}

#[derive(Debug)]
pub struct StoreError(pub String);

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StoreError {}

impl From<sqlx::Error> for StoreError {
    fn from(err: sqlx::Error) -> Self {
        StoreError(err.to_string())
    }
}

#[async_trait]
pub trait AuditLogStore: Send + Sync {
    /// Returns up to `limit` of `patient_id`'s events with `id < before` (all when `before` is
    /// `None`), newest first.
    async fn patient_events(
        &self,
        patient_id: &str,
        limit: u32,
        before: Option<i64>,
    ) -> Result<Page, StoreError>;

    /// Succeeds when the backing store can serve queries.
    async fn ping(&self) -> Result<(), StoreError>;
}

/// Builds a page from up to `limit + 1` rows: the extra row only signals that more exist.
fn page_from(mut rows: Vec<AuditEvent>, limit: u32) -> Page {
    let limit = limit as usize;
    let has_more = rows.len() > limit;
    rows.truncate(limit);
    let next_cursor = if has_more {
        rows.last().map(|e| e.id)
    } else {
        None
    };
    Page {
        events: rows,
        next_cursor,
    }
}

/// PostgreSQL-backed store.
pub struct PgStore {
    pool: PgPool,
}

impl PgStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditLogStore for PgStore {
    async fn patient_events(
        &self,
        patient_id: &str,
        limit: u32,
        before: Option<i64>,
    ) -> Result<Page, StoreError> {
        let rows: Vec<AuditEvent> = sqlx::query_as(
            "SELECT id, patient_id, sequence, actor, action, target, result, occurred_at, \
                    contract_id, ledger, tx_hash, event_index \
             FROM audit_events \
             WHERE patient_id = $1 AND ($2::BIGINT IS NULL OR id < $2) \
             ORDER BY id DESC \
             LIMIT $3",
        )
        .bind(patient_id)
        .bind(before)
        .bind(i64::from(limit) + 1)
        .fetch_all(&self.pool)
        .await?;
        Ok(page_from(rows, limit))
    }

    async fn ping(&self) -> Result<(), StoreError> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

/// In-memory store for tests and local experiments.
#[derive(Default)]
pub struct MemoryStore {
    events: RwLock<Vec<AuditEvent>>,
}

impl MemoryStore {
    pub fn new(events: Vec<AuditEvent>) -> Self {
        Self {
            events: RwLock::new(events),
        }
    }
}

#[async_trait]
impl AuditLogStore for MemoryStore {
    async fn patient_events(
        &self,
        patient_id: &str,
        limit: u32,
        before: Option<i64>,
    ) -> Result<Page, StoreError> {
        let events = self
            .events
            .read()
            .map_err(|_| StoreError("memory store lock poisoned".into()))?;
        let mut rows: Vec<AuditEvent> = events
            .iter()
            .filter(|e| e.patient_id == patient_id && before.is_none_or(|b| e.id < b))
            .cloned()
            .collect();
        rows.sort_by_key(|e| std::cmp::Reverse(e.id));
        rows.truncate(limit as usize + 1);
        Ok(page_from(rows, limit))
    }

    async fn ping(&self) -> Result<(), StoreError> {
        Ok(())
    }
}
