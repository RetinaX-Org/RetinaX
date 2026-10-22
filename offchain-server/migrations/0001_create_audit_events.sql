-- Audit events indexed from the RetinaX `audit` contract.
--
-- One row per contract event. The indexer fills it; the API only reads it.
-- Field names follow `AuditLogEntry` in contracts/audit/src/contract.rs.
CREATE TABLE IF NOT EXISTS audit_events (
    id            BIGSERIAL   PRIMARY KEY,
    -- Patient the event concerns (Stellar address, G... or C...).
    patient_id    TEXT        NOT NULL,
    -- Per-contract sequence number of the audit entry.
    sequence      BIGINT      NOT NULL,
    -- Address that performed the action.
    actor         TEXT        NOT NULL,
    -- What was done, e.g. `read_record`, `grant_access`.
    action        TEXT        NOT NULL,
    -- What it was done to, e.g. a record id.
    target        TEXT        NOT NULL,
    -- Outcome, e.g. `success`, `denied`.
    result        TEXT        NOT NULL,
    -- Ledger close time of the event.
    occurred_at   TIMESTAMPTZ NOT NULL,
    -- Where the event came from on chain.
    contract_id   TEXT        NOT NULL,
    ledger        BIGINT      NOT NULL,
    tx_hash       TEXT        NOT NULL,
    event_index   INTEGER     NOT NULL,
    indexed_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Re-indexing the same event must not create a duplicate row.
    UNIQUE (tx_hash, event_index)
);

-- Serves GET /api/audit-logs/:patient_id, newest first, with keyset pagination on id.
CREATE INDEX IF NOT EXISTS audit_events_patient_id_idx
    ON audit_events (patient_id, id DESC);
