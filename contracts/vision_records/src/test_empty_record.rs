//! Micro-tests for empty clinical-record insertion attempts.
//!
//! A clinical record carries no payload on-chain — only the `data_hash`
//! pointing at the encrypted off-chain document. An empty or blank hash is
//! therefore an empty record: it would occupy a record id, consume a counter
//! slot, and emit audit and lineage entries while referencing nothing. These
//! tests pin the rejection down at the contract boundary and assert that a
//! rejected insertion leaves no trace behind.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::arithmetic_side_effects
)]

use super::{
    BatchRecordInput, ContractError, RecordType, Role, VisionRecordsContract,
    VisionRecordsContractClient,
};
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

/// A hash that satisfies `validate_data_hash` (32–64 chars of [A-Za-z0-9_-]).
const VALID_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

// ── Helpers ──────────────────────────────────────────────────────

fn setup() -> (Env, VisionRecordsContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VisionRecordsContract, ());
    let client = VisionRecordsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, client, admin)
}

fn register_provider(env: &Env, client: &VisionRecordsContractClient, admin: &Address) -> Address {
    let provider = Address::generate(env);
    client.register_user(
        admin,
        &provider,
        &Role::Optometrist,
        &String::from_str(env, "Dr. Provider"),
    );
    provider
}

fn register_patient(env: &Env, client: &VisionRecordsContractClient, admin: &Address) -> Address {
    let patient = Address::generate(env);
    client.register_user(
        admin,
        &patient,
        &Role::Patient,
        &String::from_str(env, "Alice"),
    );
    patient
}

// ── Single-record insertion ──────────────────────────────────────

#[test]
fn test_add_record_rejects_empty_data_hash() {
    let (env, client, admin) = setup();
    let provider = register_provider(&env, &client, &admin);
    let patient = register_patient(&env, &client, &admin);

    let result = client.try_add_record(
        &provider,
        &patient,
        &provider,
        &RecordType::Examination,
        &String::from_str(&env, ""),
    );

    assert_eq!(result.err().unwrap().unwrap(), ContractError::InvalidInput);
}

#[test]
fn test_rejected_empty_record_is_not_persisted() {
    let (env, client, admin) = setup();
    let provider = register_provider(&env, &client, &admin);
    let patient = register_patient(&env, &client, &admin);

    assert_eq!(client.get_record_count(), 0);

    let result = client.try_add_record(
        &provider,
        &patient,
        &provider,
        &RecordType::Examination,
        &String::from_str(&env, ""),
    );
    assert!(result.is_err());

    // No id was burned and no record row exists: the counter is untouched and
    // the id the insertion would have taken is still unused.
    assert_eq!(client.get_record_count(), 0);
    assert!(client.try_get_record(&provider, &1).is_err());
}

#[test]
fn test_record_counter_is_unbroken_after_a_rejected_empty_insert() {
    let (env, client, admin) = setup();
    let provider = register_provider(&env, &client, &admin);
    let patient = register_patient(&env, &client, &admin);

    let first = client.add_record(
        &provider,
        &patient,
        &provider,
        &RecordType::Examination,
        &String::from_str(&env, VALID_HASH),
    );
    assert_eq!(first, 1);

    let rejected = client.try_add_record(
        &provider,
        &patient,
        &provider,
        &RecordType::Examination,
        &String::from_str(&env, ""),
    );
    assert!(rejected.is_err());

    // The next valid insertion takes id 2 — the rejected attempt did not
    // consume id 2 and leave a gap in the patient's record history.
    let second = client.add_record(
        &provider,
        &patient,
        &provider,
        &RecordType::Examination,
        &String::from_str(&env, VALID_HASH),
    );
    assert_eq!(second, 2);
    assert_eq!(client.get_record_count(), 2);
}

#[test]
fn test_add_record_rejects_blank_and_undersized_hashes() {
    let (env, client, admin) = setup();
    let provider = register_provider(&env, &client, &admin);
    let patient = register_patient(&env, &client, &admin);

    // Blank-but-not-empty, a single character, and a whitespace-padded value
    // that only looks long enough are all empty records in practice.
    for blank in ["", " ", "   ", "0", "                                "] {
        let result = client.try_add_record(
            &provider,
            &patient,
            &provider,
            &RecordType::Examination,
            &String::from_str(&env, blank),
        );
        assert_eq!(
            result.err().unwrap().unwrap(),
            ContractError::InvalidInput,
            "expected an empty record to be rejected"
        );
    }

    assert_eq!(client.get_record_count(), 0);
}

// ── Batch insertion ──────────────────────────────────────────────

#[test]
fn test_add_records_rejects_an_empty_batch() {
    let (env, client, admin) = setup();
    let provider = register_provider(&env, &client, &admin);

    let inputs: Vec<BatchRecordInput> = Vec::new(&env);
    let result = client.try_add_records(&provider, &inputs);

    assert_eq!(result.err().unwrap().unwrap(), ContractError::InvalidInput);
    assert_eq!(client.get_record_count(), 0);
}

#[test]
fn test_empty_batch_leaves_the_counter_untouched() {
    let (env, client, admin) = setup();
    let provider = register_provider(&env, &client, &admin);
    let patient = register_patient(&env, &client, &admin);

    client.add_record(
        &provider,
        &patient,
        &provider,
        &RecordType::Examination,
        &String::from_str(&env, VALID_HASH),
    );
    assert_eq!(client.get_record_count(), 1);

    let inputs: Vec<BatchRecordInput> = Vec::new(&env);
    assert!(client.try_add_records(&provider, &inputs).is_err());

    assert_eq!(client.get_record_count(), 1);
}
