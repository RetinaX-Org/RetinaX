//! Runs the real migration and query against PostgreSQL.
//!
//! Skipped unless `TEST_DATABASE_URL` points at a database the test may write to, e.g.
//! `TEST_DATABASE_URL=postgres://postgres:postgres@localhost:5432/retinax_test cargo test`.

use offchain_server::store::{AuditLogStore, PgStore};
use sqlx::postgres::PgPoolOptions;

const PATIENT: &str = "GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN7";
const OTHER_PATIENT: &str = "GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFSHONUCEOASW7QC7OX2H";

#[tokio::test]
async fn migration_and_patient_query_work_against_postgres() {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    // Running the migrations twice is a no-op.
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query("TRUNCATE audit_events RESTART IDENTITY")
        .execute(&pool)
        .await
        .unwrap();

    for (i, patient) in [PATIENT, OTHER_PATIENT, PATIENT, PATIENT]
        .iter()
        .enumerate()
    {
        sqlx::query(
            "INSERT INTO audit_events \
               (patient_id, sequence, actor, action, target, result, occurred_at, \
                contract_id, ledger, tx_hash, event_index) \
             VALUES ($1, $2, 'GDOCTOR', 'read_record', $3, 'success', \
                     to_timestamp(1700000000 + $2), 'CAUDIT', 1000 + $2, $4, 0)",
        )
        .bind(patient)
        .bind(i as i64 + 1)
        .bind(format!("record-{}", i + 1))
        .bind(format!("tx{}", i + 1))
        .execute(&pool)
        .await
        .unwrap();
    }

    // The same on-chain event cannot be indexed twice.
    let duplicate = sqlx::query(
        "INSERT INTO audit_events \
           (patient_id, sequence, actor, action, target, result, occurred_at, \
            contract_id, ledger, tx_hash, event_index) \
         VALUES ($1, 99, 'GDOCTOR', 'read_record', 'x', 'success', now(), 'CAUDIT', 1, 'tx1', 0)",
    )
    .bind(PATIENT)
    .execute(&pool)
    .await;
    assert!(duplicate.is_err());

    let store = PgStore::new(pool);
    store.ping().await.unwrap();

    let page = store.patient_events(PATIENT, 2, None).await.unwrap();
    let ids: Vec<i64> = page.events.iter().map(|e| e.id).collect();
    assert_eq!(ids, vec![4, 3]);
    assert_eq!(page.next_cursor, Some(3));
    assert_eq!(page.events[0].target, "record-4");
    assert_eq!(page.events[0].occurred_at.timestamp(), 1_700_000_004);

    let page = store.patient_events(PATIENT, 2, Some(3)).await.unwrap();
    let ids: Vec<i64> = page.events.iter().map(|e| e.id).collect();
    assert_eq!(ids, vec![1]);
    assert_eq!(page.next_cursor, None);

    let page = store.patient_events(OTHER_PATIENT, 50, None).await.unwrap();
    assert_eq!(page.events.len(), 1);
    assert_eq!(page.events[0].id, 2);
}
