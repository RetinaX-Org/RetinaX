//! Permission / auth tests for the orchestrator (#421).
//!
//! Verifies that each mutating entry point refuses to run when it should:
//! before `initialize()` at all, and (for admin-gated entry points) when
//! the caller is not the configured admin. The orchestrator's
//! authorization model is:
//!
//! * `initialize`         — once; re-init errors with `TransactionExists`.
//! * `start_transaction`  — any initiator, but the contract must be
//!                          initialised first.
//! * `rollback_transaction`, `update_timeout_config` — admin-only.
//! * `process_timeouts`, `get_transaction*`, `get_timeout_config` —
//!                          not mutating in a way that needs admin, but
//!                          still require the contract to be initialised.

#![cfg(test)]

use common::transaction::{TransactionError, TransactionOperation, TransactionTimeoutConfig};
use orchestrator::{OrchestratorContract, OrchestratorContractClient};
use soroban_sdk::{testutils::Address as _, vec, Address, Env, String};

fn setup_initialized() -> (Env, OrchestratorContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(OrchestratorContract, ());
    let client = OrchestratorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    client.initialize(&admin, &None);

    (env, client, admin, non_admin)
}

fn fresh_uninitialized() -> (Env, OrchestratorContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(OrchestratorContract, ());
    let client = OrchestratorContractClient::new(&env, &contract_id);
    (env, client)
}

// ── start_transaction ────────────────────────────────────────────────────────

#[test]
fn start_transaction_refuses_before_initialize() {
    let (env, client) = fresh_uninitialized();
    let initiator = Address::generate(&env);

    let result = client.try_start_transaction(
        &initiator,
        &vec![&env],
        &None,
        &vec![&env],
    );

    assert_eq!(result, Err(Ok(TransactionError::Unauthorized)));
}

// ── rollback_transaction ─────────────────────────────────────────────────────

#[test]
fn rollback_transaction_refuses_before_initialize() {
    let (env, client) = fresh_uninitialized();
    let who = Address::generate(&env);

    let result = client.try_rollback_transaction(&who, &0u64, &String::from_str(&env, "r"));

    // Admin check runs first; before init there is no admin, so the auth
    // check already rejects the call.
    assert_eq!(result, Err(Ok(TransactionError::Unauthorized)));
}

#[test]
fn rollback_transaction_refuses_non_admin() {
    let (env, client, _admin, non_admin) = setup_initialized();

    let result = client.try_rollback_transaction(
        &non_admin,
        &0u64,
        &String::from_str(&env, "no-reason"),
    );

    assert_eq!(result, Err(Ok(TransactionError::Unauthorized)));
}

// ── update_timeout_config ────────────────────────────────────────────────────

#[test]
fn update_timeout_config_refuses_before_initialize() {
    let (env, client) = fresh_uninitialized();
    let who = Address::generate(&env);
    let cfg = TransactionTimeoutConfig {
        default_timeout: 60,
        max_timeout: 120,
        warning_threshold: 10,
    };

    let result = client.try_update_timeout_config(&who, &cfg);

    assert_eq!(result, Err(Ok(TransactionError::Unauthorized)));
    let _ = env; // silence unused-var warning if the env is not touched further.
}

#[test]
fn update_timeout_config_refuses_non_admin() {
    let (_env, client, _admin, non_admin) = setup_initialized();
    let cfg = TransactionTimeoutConfig {
        default_timeout: 60,
        max_timeout: 120,
        warning_threshold: 10,
    };

    let result = client.try_update_timeout_config(&non_admin, &cfg);

    assert_eq!(result, Err(Ok(TransactionError::Unauthorized)));
}

#[test]
fn update_timeout_config_accepts_admin() {
    let (_env, client, admin, _non_admin) = setup_initialized();
    let cfg = TransactionTimeoutConfig {
        default_timeout: 60,
        max_timeout: 120,
        warning_threshold: 10,
    };

    client.update_timeout_config(&admin, &cfg);

    let stored = client.get_timeout_config();
    assert_eq!(stored.default_timeout, 60);
    assert_eq!(stored.max_timeout, 120);
}

// ── process_timeouts / get_* ─────────────────────────────────────────────────

#[test]
fn process_timeouts_refuses_before_initialize() {
    let (_env, client) = fresh_uninitialized();
    assert_eq!(
        client.try_process_timeouts(),
        Err(Ok(TransactionError::Unauthorized)),
    );
}

#[test]
fn get_transaction_refuses_before_initialize() {
    let (_env, client) = fresh_uninitialized();
    assert_eq!(
        client.try_get_transaction(&0u64),
        Err(Ok(TransactionError::Unauthorized)),
    );
}

#[test]
fn get_active_transactions_refuses_before_initialize() {
    let (_env, client) = fresh_uninitialized();
    assert_eq!(
        client.try_get_active_transactions(),
        Err(Ok(TransactionError::Unauthorized)),
    );
}

#[test]
fn get_timeout_config_refuses_before_initialize() {
    let (_env, client) = fresh_uninitialized();
    assert_eq!(
        client.try_get_timeout_config(),
        Err(Ok(TransactionError::Unauthorized)),
    );
}

// ── initialize ──────────────────────────────────────────────────────────────

#[test]
fn initialize_is_idempotent_only_once() {
    let (env, client, _admin, _non_admin) = setup_initialized();
    let second_admin = Address::generate(&env);

    // A second call with a different admin must fail; the issue's spec
    // reports it as `TransactionExists` (same slot as "already set").
    let result = client.try_initialize(&second_admin, &None);
    assert_eq!(result, Err(Ok(TransactionError::TransactionExists)));
}
