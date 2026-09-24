//! Audit Log Immutability — Overwrite Prevention
//!
//! `AuditContract` is append-only by construction: it exposes `append_entry`
//! but no update or delete entrypoint, and a segment can only be created once.
//! These tests pin that property down at the contract boundary, so a future
//! change that lets a caller rewrite or reset history fails here rather than
//! silently landing.
//!
//! The complementary hash-chain and Merkle-root guarantees for the in-memory
//! `MerkleLog` are covered in `tamper_resistance_test.rs`; this file is about
//! what the deployed contract's storage will and will not let a caller do.

#![allow(clippy::unwrap_used, clippy::arithmetic_side_effects)]

use audit::contract::{AuditContract, AuditContractClient, AuditLogEntry};
use soroban_sdk::{testutils::Address as _, testutils::Ledger as _, Address, Env, Symbol, Vec};

// ── Helpers ────────────────────────────────────────────────────────────────

fn setup() -> (Env, AuditContractClient<'static>, Address, Symbol) {
    let env = Env::default();
    env.mock_all_auths();

    let id = env.register(AuditContract, ());
    let client = AuditContractClient::new(&env, &id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let segment = Symbol::short("ACCESS");
    client.create_segment(&segment);

    (env, client, admin, segment)
}

fn append(
    client: &AuditContractClient,
    segment: &Symbol,
    actor: &Address,
    action: &str,
    target: &str,
    result: &str,
) -> u64 {
    client.append_entry(
        segment,
        actor,
        &Symbol::short(action),
        &Symbol::short(target),
        &Symbol::short(result),
    )
}

fn entry_at(entries: &Vec<AuditLogEntry>, index: u32) -> AuditLogEntry {
    entries.get(index).unwrap()
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. An appended entry is never mutated by later appends
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn existing_entry_is_unchanged_by_a_later_append() {
    let (env, client, _admin, segment) = setup();
    let actor = Address::generate(&env);

    append(&client, &segment, &actor, "READ", "REC1", "OK");
    let before = entry_at(&client.get_entries(&segment), 0);

    append(&client, &segment, &actor, "WRITE", "REC2", "OK");
    let after = entry_at(&client.get_entries(&segment), 0);

    assert_eq!(
        before, after,
        "the first entry must be byte-for-byte identical after a later append"
    );
}

#[test]
fn every_earlier_entry_survives_a_long_run_of_appends() {
    let (env, client, _admin, segment) = setup();
    let actor = Address::generate(&env);

    append(&client, &segment, &actor, "READ", "REC1", "OK");
    append(&client, &segment, &actor, "WRITE", "REC2", "OK");
    append(&client, &segment, &actor, "SHARE", "REC3", "DENIED");
    let snapshot = client.get_entries(&segment);

    for _ in 0..10u32 {
        append(&client, &segment, &actor, "READ", "REC9", "OK");
    }

    let current = client.get_entries(&segment);
    assert_eq!(current.len(), snapshot.len() + 10);

    for i in 0..snapshot.len() {
        assert_eq!(
            entry_at(&snapshot, i),
            entry_at(&current, i),
            "entry at index {i} was modified by subsequent appends"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Re-submitting identical content appends, it does not overwrite
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn identical_content_appends_a_new_entry_rather_than_replacing_the_first() {
    let (env, client, _admin, segment) = setup();
    let actor = Address::generate(&env);

    let first = append(&client, &segment, &actor, "READ", "REC1", "OK");
    let second = append(&client, &segment, &actor, "READ", "REC1", "OK");

    assert_ne!(
        first, second,
        "a duplicate action must receive its own sequence number"
    );
    assert_eq!(client.get_entry_count(&segment), 2);

    let entries = client.get_entries(&segment);
    assert_eq!(entry_at(&entries, 0).sequence, first);
    assert_eq!(entry_at(&entries, 1).sequence, second);
}

#[test]
fn a_later_entry_cannot_reuse_an_earlier_sequence_number() {
    let (env, client, _admin, segment) = setup();
    let actor = Address::generate(&env);

    let mut previous = 0u64;
    for _ in 0..5u32 {
        let sequence = append(&client, &segment, &actor, "READ", "REC1", "OK");
        assert!(
            sequence > previous,
            "sequence numbers must strictly increase; got {sequence} after {previous}"
        );
        previous = sequence;
    }

    // Sequence numbers are dense and start at 1, so no slot is ever revisited.
    let entries = client.get_entries(&segment);
    for i in 0..entries.len() {
        assert_eq!(entry_at(&entries, i).sequence, u64::from(i) + 1);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. A segment's history cannot be reset by re-creating it
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn recreating_a_segment_is_rejected() {
    let (_env, client, _admin, segment) = setup();

    let result = client.try_create_segment(&segment);

    assert!(
        result.is_err(),
        "re-creating an existing segment must fail — it would reset its history"
    );
}

#[test]
fn a_rejected_segment_recreation_leaves_the_existing_entries_intact() {
    let (env, client, _admin, segment) = setup();
    let actor = Address::generate(&env);

    append(&client, &segment, &actor, "READ", "REC1", "OK");
    append(&client, &segment, &actor, "WRITE", "REC2", "OK");
    let before = client.get_entries(&segment);

    assert!(client.try_create_segment(&segment).is_err());

    let after = client.get_entries(&segment);
    assert_eq!(after.len(), 2, "the segment must not have been emptied");
    assert_eq!(entry_at(&before, 0), entry_at(&after, 0));
    assert_eq!(entry_at(&before, 1), entry_at(&after, 1));

    // The next append continues the existing history rather than restarting it.
    let next = append(&client, &segment, &actor, "READ", "REC3", "OK");
    assert_eq!(next, 3);
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Segments are isolated — one cannot overwrite another
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn appending_to_one_segment_does_not_touch_another() {
    let (env, client, _admin, segment) = setup();
    let other = Symbol::short("BILLING");
    client.create_segment(&other);
    let actor = Address::generate(&env);

    append(&client, &segment, &actor, "READ", "REC1", "OK");
    let access_before = client.get_entries(&segment);

    append(&client, &other, &actor, "CHARGE", "INV1", "OK");

    assert_eq!(client.get_entries(&segment), access_before);
    assert_eq!(client.get_entry_count(&segment), 1);
    assert_eq!(client.get_entry_count(&other), 1);

    // Each segment numbers its own entries from 1; neither overwrites the other.
    assert_eq!(entry_at(&client.get_entries(&other), 0).sequence, 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Recorded timestamps are fixed at append time
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn an_entrys_timestamp_is_frozen_when_it_is_appended() {
    let (env, client, _admin, segment) = setup();
    let actor = Address::generate(&env);

    env.ledger().set_timestamp(1_700_000_000);
    append(&client, &segment, &actor, "READ", "REC1", "OK");
    let recorded = entry_at(&client.get_entries(&segment), 0).timestamp;
    assert_eq!(recorded, 1_700_000_000);

    // Advancing the ledger and appending again must not restamp the first entry.
    env.ledger().set_timestamp(1_700_009_999);
    append(&client, &segment, &actor, "READ", "REC2", "OK");

    let entries = client.get_entries(&segment);
    assert_eq!(entry_at(&entries, 0).timestamp, 1_700_000_000);
    assert_eq!(entry_at(&entries, 1).timestamp, 1_700_009_999);
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Appending to a segment that was never created is rejected
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn appending_to_an_unknown_segment_creates_nothing() {
    let (env, client, _admin, _segment) = setup();
    let actor = Address::generate(&env);
    let ghost = Symbol::short("GHOST");

    let result = client.try_append_entry(
        &ghost,
        &actor,
        &Symbol::short("READ"),
        &Symbol::short("REC1"),
        &Symbol::short("OK"),
    );
    assert!(result.is_err(), "appending to an unknown segment must fail");

    // The failed append must not have implicitly created the segment.
    assert!(client.try_get_entries(&ghost).is_err());
}
