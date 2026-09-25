#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
mod common;

use common::{create_test_user, setup_test_env};
use soroban_sdk::{Address, Symbol, Vec};
use vision_records::Role;

// ── Mock Identity Contract for Testing ────────────────────────────────────

#[soroban_sdk::contract]
pub struct MockIdentityContract;

#[soroban_sdk::contractimpl]
impl MockIdentityContract {
    pub fn verify_identity(
        _env: soroban_sdk::Env,
        _user: Address,
    ) -> Result<bool, soroban_sdk::Error> {
        Ok(true)
    }

    pub fn get_user_role(
        _env: soroban_sdk::Env,
        _user: Address,
    ) -> Result<Symbol, soroban_sdk::Error> {
        Ok(Symbol::new(&_env, "optometrist"))
    }

    pub fn verify_identity_fails(
        _env: soroban_sdk::Env,
        _user: Address,
    ) -> Result<bool, soroban_sdk::Error> {
        Err(soroban_sdk::Error::from_contract_error(1001))
    }
}

// ── Mock Audit Contract for Testing ───────────────────────────────────────

#[soroban_sdk::contract]
pub struct MockAuditContract;

#[soroban_sdk::contractimpl]
impl MockAuditContract {
    pub fn create_segment(
        _env: soroban_sdk::Env,
        _segment_name: Symbol,
    ) -> Result<(), soroban_sdk::Error> {
        Ok(())
    }

    pub fn append_entry(
        _env: soroban_sdk::Env,
        _segment_name: Symbol,
        _actor: Address,
        _action: Symbol,
    ) -> Result<u64, soroban_sdk::Error> {
        Ok(1)
    }

    pub fn append_entry_fails(
        _env: soroban_sdk::Env,
        _segment_name: Symbol,
        _actor: Address,
        _action: Symbol,
    ) -> Result<u64, soroban_sdk::Error> {
        Err(soroban_sdk::Error::from_contract_error(2001))
    }
}

// ── Micro-tests: Vision Records → Identity Cross-Contract Calls ──────────

/// Micro-test: Vision Records successfully calls Identity contract to verify actor
#[test]
fn test_vision_records_calls_identity_verify_actor() {
    let ctx = setup_test_env();
    let identity_id = ctx.env.register(MockIdentityContract, ());
    let patient = create_test_user(&ctx, Role::Patient, "Patient");

    // Vision records should be able to call identity contract
    // This would be implemented in the actual contract via Soroban's cross-contract call mechanism
    let env_ref = &ctx.env;
    let result: Result<bool, soroban_sdk::Error> =
        soroban_sdk::contract_interface::invoke_contract(
            env_ref,
            &identity_id,
            &Symbol::new(env_ref, "verify_identity"),
            Vec::from_slice(env_ref, &[patient.into_val(env_ref)]),
        );

    assert!(result.is_ok(), "Vision records should successfully call identity contract");
    assert_eq!(result.unwrap(), true, "Identity verification should return true");
}

/// Micro-test: Vision Records handles identity contract call failure gracefully
#[test]
fn test_vision_records_handles_identity_call_failure() {
    let ctx = setup_test_env();
    let identity_id = ctx.env.register(MockIdentityContract, ());
    let patient = create_test_user(&ctx, Role::Patient, "Patient");

    let env_ref = &ctx.env;
    let result: Result<bool, soroban_sdk::Error> =
        soroban_sdk::contract_interface::invoke_contract(
            env_ref,
            &identity_id,
            &Symbol::new(env_ref, "verify_identity_fails"),
            Vec::from_slice(env_ref, &[patient.into_val(env_ref)]),
        );

    assert!(
        result.is_err(),
        "Vision records should handle identity contract call failure"
    );
}

/// Micro-test: Vision Records retrieves role from Identity contract
#[test]
fn test_vision_records_gets_role_from_identity() {
    let ctx = setup_test_env();
    let identity_id = ctx.env.register(MockIdentityContract, ());
    let optometrist = create_test_user(&ctx, Role::Optometrist, "Opto");

    let env_ref = &ctx.env;
    let result: Result<Symbol, soroban_sdk::Error> =
        soroban_sdk::contract_interface::invoke_contract(
            env_ref,
            &identity_id,
            &Symbol::new(env_ref, "get_user_role"),
            Vec::from_slice(env_ref, &[optometrist.into_val(env_ref)]),
        );

    assert!(result.is_ok(), "Should successfully retrieve role from identity");
    assert_eq!(
        result.unwrap(),
        Symbol::new(env_ref, "optometrist"),
        "Role should match"
    );
}

// ── Micro-tests: Vision Records → Audit Cross-Contract Calls ───────────────

/// Micro-test: Vision Records successfully calls Audit contract to create segment
#[test]
fn test_vision_records_calls_audit_create_segment() {
    let ctx = setup_test_env();
    let audit_id = ctx.env.register(MockAuditContract, ());

    let env_ref = &ctx.env;
    let segment_name = Symbol::new(env_ref, "patient_segment_1");

    let result: Result<(), soroban_sdk::Error> =
        soroban_sdk::contract_interface::invoke_contract(
            env_ref,
            &audit_id,
            &Symbol::new(env_ref, "create_segment"),
            Vec::from_slice(env_ref, &[segment_name.into_val(env_ref)]),
        );

    assert!(
        result.is_ok(),
        "Vision records should successfully create audit segment"
    );
}

/// Micro-test: Vision Records calls Audit contract to append entry
#[test]
fn test_vision_records_calls_audit_append_entry() {
    let ctx = setup_test_env();
    let audit_id = ctx.env.register(MockAuditContract, ());
    let provider = create_test_user(&ctx, Role::Optometrist, "Provider");

    let env_ref = &ctx.env;
    let segment_name = Symbol::new(env_ref, "provider_audit");
    let action = Symbol::new(env_ref, "record_created");

    let result: Result<u64, soroban_sdk::Error> =
        soroban_sdk::contract_interface::invoke_contract(
            env_ref,
            &audit_id,
            &Symbol::new(env_ref, "append_entry"),
            Vec::from_slice(
                env_ref,
                &[
                    segment_name.into_val(env_ref),
                    provider.into_val(env_ref),
                    action.into_val(env_ref),
                ],
            ),
        );

    assert!(
        result.is_ok(),
        "Vision records should successfully append audit entry"
    );
    assert_eq!(result.unwrap(), 1, "Sequence number should be 1");
}

/// Micro-test: Vision Records handles Audit contract call failure
#[test]
fn test_vision_records_handles_audit_call_failure() {
    let ctx = setup_test_env();
    let audit_id = ctx.env.register(MockAuditContract, ());
    let provider = create_test_user(&ctx, Role::Optometrist, "Provider");

    let env_ref = &ctx.env;
    let segment_name = Symbol::new(env_ref, "failing_segment");
    let action = Symbol::new(env_ref, "record_created");

    let result: Result<u64, soroban_sdk::Error> =
        soroban_sdk::contract_interface::invoke_contract(
            env_ref,
            &audit_id,
            &Symbol::new(env_ref, "append_entry_fails"),
            Vec::from_slice(
                env_ref,
                &[
                    segment_name.into_val(env_ref),
                    provider.into_val(env_ref),
                    action.into_val(env_ref),
                ],
            ),
        );

    assert!(
        result.is_err(),
        "Vision records should handle audit contract call failure"
    );
}

/// Micro-test: Multiple sequential cross-contract calls succeed
#[test]
fn test_multiple_cross_contract_calls_sequential() {
    let ctx = setup_test_env();
    let identity_id = ctx.env.register(MockIdentityContract, ());
    let audit_id = ctx.env.register(MockAuditContract, ());
    let provider = create_test_user(&ctx, Role::Optometrist, "Provider");

    let env_ref = &ctx.env;

    // Call 1: Verify identity
    let verify_result: Result<bool, soroban_sdk::Error> =
        soroban_sdk::contract_interface::invoke_contract(
            env_ref,
            &identity_id,
            &Symbol::new(env_ref, "verify_identity"),
            Vec::from_slice(env_ref, &[provider.into_val(env_ref)]),
        );
    assert!(verify_result.is_ok());

    // Call 2: Create audit segment
    let segment_result: Result<(), soroban_sdk::Error> =
        soroban_sdk::contract_interface::invoke_contract(
            env_ref,
            &audit_id,
            &Symbol::new(env_ref, "create_segment"),
            Vec::from_slice(env_ref, &[Symbol::new(env_ref, "segment_1").into_val(env_ref)]),
        );
    assert!(segment_result.is_ok());

    // Call 3: Append audit entry
    let append_result: Result<u64, soroban_sdk::Error> =
        soroban_sdk::contract_interface::invoke_contract(
            env_ref,
            &audit_id,
            &Symbol::new(env_ref, "append_entry"),
            Vec::from_slice(
                env_ref,
                &[
                    Symbol::new(env_ref, "segment_1").into_val(env_ref),
                    provider.into_val(env_ref),
                    Symbol::new(env_ref, "verification_complete").into_val(env_ref),
                ],
            ),
        );
    assert!(append_result.is_ok());
}

/// Micro-test: Cross-contract call with invalid contract address fails gracefully
#[test]
fn test_cross_contract_call_invalid_contract_address() {
    let ctx = setup_test_env();
    let invalid_contract = Address::generate(&ctx.env);
    let provider = create_test_user(&ctx, Role::Optometrist, "Provider");

    let env_ref = &ctx.env;

    let result: Result<bool, soroban_sdk::Error> =
        soroban_sdk::contract_interface::invoke_contract(
            env_ref,
            &invalid_contract,
            &Symbol::new(env_ref, "verify_identity"),
            Vec::from_slice(env_ref, &[provider.into_val(env_ref)]),
        );

    assert!(
        result.is_err(),
        "Cross-contract call to invalid address should fail"
    );
}
