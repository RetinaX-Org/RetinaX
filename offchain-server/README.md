# RetinaX Off-chain Server

The off-chain node that runs next to the RetinaX Soroban contracts. It is the service the project calls the **prover node**: the home for work that should not run on chain, such as serving indexed data and, later, generating zero-knowledge proofs.

Today it does one job: it serves the audit trail that an indexer copies from the `audit` contract into PostgreSQL, so clients can read a patient's access history without scanning the ledger.

| Route | Auth | Purpose |
|-------|------|---------|
| `GET /health` | none | Liveness check, including the database connection |
| `GET /api/audit-logs/{patient_id}` | bearer token | A patient's audit events, newest first |

> **Proof generation is not served yet.** The prover logic lives in [`sdk/zk_prover`](../sdk/zk_prover) as a Rust library, and it currently produces mock proofs shaped for the `zk_verifier` contract's checks. An HTTP proving endpoint (`POST /api/prove`) is planned but not part of this server.

---

## Contents

- [Requirements](#requirements)
- [Quick start](#quick-start)
- [Configuration](#configuration)
- [API](#api)
- [Database](#database)
- [Testing](#testing)
- [Running in production](#running-in-production)
- [Troubleshooting](#troubleshooting)

---

## Requirements

- **Rust**, stable 1.88 or newer (`rustup update stable`)
- **PostgreSQL 12 or newer**. The included `docker-compose.yml` starts PostgreSQL 16 if you have Docker.

This directory is its own Cargo project, separate from the contracts workspace, so building it does not touch the contracts' `Cargo.lock` and needs no `wasm32` target.

---

## Quick start

All commands run from this directory:

```bash
cd offchain-server
```

**1. Start PostgreSQL.**

With Docker:

```bash
docker compose up -d postgres
```

Or use an existing server and create a database for it:

```bash
createdb retinax
```

**2. Configure the node.**

```bash
cp .env.example .env
# Set API_TOKEN to a random value, for example:
sed -i "s/^API_TOKEN=.*/API_TOKEN=$(openssl rand -hex 32)/" .env
# If you are not using docker-compose, also edit DATABASE_URL.
```

**3. Run it.**

The server reads its settings from environment variables. It does not load `.env` by itself, so export the file first:

```bash
set -a; source .env; set +a
cargo run --release
```

On startup the node applies the database migrations in [`migrations/`](migrations), then listens on `BIND_ADDR`:

```text
INFO offchain_server: database migrations applied
INFO offchain_server: offchain-server listening addr=127.0.0.1:8080
```

**4. Check it.**

```bash
curl http://127.0.0.1:8080/health
# {"status":"ok"}

curl -H "Authorization: Bearer $API_TOKEN" \
  http://127.0.0.1:8080/api/audit-logs/GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7
# {"patient_id":"GAAZ...","count":0,"next_cursor":null,"entries":[]}
```

The list is empty until events are indexed. To try the API with data, insert a sample event:

```bash
psql "$DATABASE_URL" <<'SQL'
INSERT INTO audit_events
  (patient_id, sequence, actor, action, target, result, occurred_at,
   contract_id, ledger, tx_hash, event_index)
VALUES
  ('GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7', 1,
   'GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H', 'read_record',
   'record-42', 'success', now(), 'CAUDIT', 5000, 'sample-tx', 0);
SQL
```

Stop the node with `Ctrl+C`. It finishes in-flight requests before exiting.

---

## Configuration

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | yes | | PostgreSQL connection string |
| `API_TOKEN` | yes | | Bearer token clients must send. At least 32 characters. |
| `BIND_ADDR` | no | `127.0.0.1:8080` | Address and port to listen on. Use `0.0.0.0:8080` to accept remote connections. |
| `DB_MAX_CONNECTIONS` | no | `5` | Connection pool size |
| `RUST_LOG` | no | `offchain_server=info,tower_http=info` | Log filter, [`EnvFilter`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) syntax. `tower_http=debug` logs every request. |

The node refuses to start if a required variable is missing or invalid, and says which one.

---

## API

### `GET /health`

No authentication. Returns `200 {"status":"ok"}`, or `503 {"status":"unavailable"}` if the database cannot be reached. Use it for load balancer and container health checks.

### `GET /api/audit-logs/{patient_id}`

Returns the audit events for one patient, newest first.

**Headers**

```http
Authorization: Bearer <API_TOKEN>
```

**Path**

- `patient_id`: the patient's Stellar address, 56 characters starting with `G` (account) or `C` (contract).

**Query parameters**

| Name | Default | Description |
|------|---------|-------------|
| `limit` | `50` | Page size, 1 to 200 |
| `before` | | Return only events with an `id` below this. Pass the previous page's `next_cursor`. |

**Response `200`**

```json
{
  "patient_id": "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7",
  "count": 1,
  "next_cursor": null,
  "entries": [
    {
      "id": 1,
      "patient_id": "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7",
      "sequence": 1,
      "actor": "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H",
      "action": "read_record",
      "target": "record-42",
      "result": "success",
      "occurred_at": "2026-10-21T12:00:00Z",
      "contract_id": "CAUDIT",
      "ledger": 5000,
      "tx_hash": "sample-tx",
      "event_index": 0
    }
  ]
}
```

A patient with no events gets `200` with an empty `entries` list.

**Pagination**

When `next_cursor` is a number, more events exist. Request the next page with `?before=<next_cursor>`, keeping the same `limit`. `null` means you have reached the oldest event. Paging is by `id`, so events indexed while you page do not shift or repeat results.

```bash
curl -H "Authorization: Bearer $API_TOKEN" "http://127.0.0.1:8080/api/audit-logs/$PATIENT?limit=20"
curl -H "Authorization: Bearer $API_TOKEN" "http://127.0.0.1:8080/api/audit-logs/$PATIENT?limit=20&before=181"
```

**Errors** are JSON `{"error": "..."}`:

| Status | Cause |
|--------|-------|
| `400` | `patient_id` is not a Stellar address, or `limit` / `before` is out of range |
| `401` | Missing, malformed or wrong bearer token |
| `500` | Database query failed. Details go to the server log, not the response. |

---

## Database

Migrations live in [`migrations/`](migrations) and run automatically at startup. They are tracked in the `_sqlx_migrations` table, so each runs once. Add a change as a new numbered file, never by editing an applied one.

`audit_events` holds one row per event from the `audit` contract. Its fields follow `AuditLogEntry` in [`contracts/audit/src/contract.rs`](../contracts/audit/src/contract.rs), plus where the event came from on chain (`contract_id`, `ledger`, `tx_hash`, `event_index`). `(tx_hash, event_index)` is unique, so re-indexing the same ledger range cannot create duplicates. Index `(patient_id, id DESC)` serves the audit-log query.

This server only reads `audit_events`. Filling it is the indexer's job: anything that follows the `audit` contract's events, for example through Horizon or Stellar RPC `getEvents`, can insert rows with `INSERT ... ON CONFLICT (tx_hash, event_index) DO NOTHING`.

---

## Testing

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The API tests use an in-memory store and need no database. The PostgreSQL test runs the real migrations and query, and is skipped unless `TEST_DATABASE_URL` is set:

```bash
createdb retinax_test   # it truncates audit_events, so do not point it at real data
TEST_DATABASE_URL=postgres://retinax:retinax@localhost:5432/retinax_test cargo test
```

CI ([`.github/workflows/offchain-server-ci.yml`](../.github/workflows/offchain-server-ci.yml)) runs all of these, with the PostgreSQL test against a PostgreSQL 16 service container.

---

## Running in production

- **Build:** `cargo build --release` produces a single binary, `target/release/offchain-server`. Migrations are compiled into it.
- **Secrets:** generate `API_TOKEN` with `openssl rand -hex 32` and keep it in your secret store. Rotating it means restarting the node with the new value and updating clients.
- **TLS:** the node speaks plain HTTP. Put it behind a reverse proxy or load balancer that terminates TLS, and keep `BIND_ADDR` on a private interface.
- **Database:** use a dedicated role. The node needs `CREATE` on its schema for migrations at startup, then only `SELECT` on `audit_events`.
- **Health checks:** point them at `GET /health`.
- **Logs:** written to stdout. Request logs never include the bearer token.
- **Data sensitivity:** audit logs show who accessed which patient's records. Treat the API and database as holding protected health information: restrict network access, and give the token only to services that need it.

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `DATABASE_URL is not set` / `API_TOKEN is not set` | Export the variables, for example `set -a; source .env; set +a`. |
| `API_TOKEN must be at least 32 characters` | Generate one with `openssl rand -hex 32`. |
| `cannot connect to the database at DATABASE_URL` at startup | PostgreSQL is not running, is unreachable, or `DATABASE_URL` points elsewhere. With Docker, check `docker compose ps`. The node gives up after 10 seconds. |
| `Address already in use` | Something else is on that port. Change `BIND_ADDR`. |
| `401` from the API | Send `Authorization: Bearer <API_TOKEN>` with the exact token the node was started with. |
| Always an empty `entries` list | Nothing has been indexed for that patient. Check with `SELECT count(*) FROM audit_events WHERE patient_id = '...'`. |
