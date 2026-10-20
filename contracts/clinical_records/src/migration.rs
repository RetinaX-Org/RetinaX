//! Contract upgrade and schema versioning.
//!
//! The contract schema version lives in instance storage. Each record's schema
//! version lives in its own persistent entry beside the record, so record
//! layouts written before versioning existed stay readable unchanged. A record
//! or contract with no stored version is treated as `INITIAL_SCHEMA_VERSION`.
//!
//! Every function here touches a constant number of storage entries: O(1).

use crate::admin::{require_admin, require_authorized_vision_records};
use crate::errors::ContractError;
use core::cmp::Ordering;
use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol};

const SCHEMA_VERSION: Symbol = symbol_short!("SCHEMA_V");

/// Version of contracts and records written before versioning existed.
pub const INITIAL_SCHEMA_VERSION: u32 = 1;

/// Storage key of a record, or of that record's schema version.
pub type RecordKey = (Symbol, u64);

/// Replaces the contract's Wasm with an already-uploaded build. Storage is
/// kept; the new code runs after this invocation completes.
pub fn upgrade(env: &Env, admin: Address, new_wasm_hash: BytesN<32>) -> Result<(), ContractError> {
    require_admin(env, &admin)?;
    env.deployer().update_current_contract_wasm(new_wasm_hash);
    Ok(())
}

pub fn get_schema_version(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&SCHEMA_VERSION)
        .unwrap_or(INITIAL_SCHEMA_VERSION)
}

/// Moves the contract schema forward to `target_version`.
pub fn migrate(env: &Env, admin: Address, target_version: u32) -> Result<(), ContractError> {
    require_admin(env, &admin)?;
    ensure_forward(get_schema_version(env), target_version)?;
    env.storage()
        .instance()
        .set(&SCHEMA_VERSION, &target_version);
    Ok(())
}

/// Tags a newly written record with the current schema version.
pub fn stamp_record(env: &Env, version_key: &RecordKey) {
    env.storage()
        .persistent()
        .set(version_key, &get_schema_version(env));
}

/// Lazy migration: brings a record read under an older schema up to the
/// current one. Writes only when the version actually changes.
pub fn upgrade_on_read(env: &Env, version_key: &RecordKey) {
    let current = get_schema_version(env);
    if record_version(env, version_key) < current {
        env.storage().persistent().set(version_key, &current);
    }
}

pub fn get_record_version(
    env: &Env,
    caller: &Address,
    record_key: &RecordKey,
    version_key: &RecordKey,
) -> Result<u32, ContractError> {
    require_authorized_vision_records(env, caller)?;
    require_record(env, record_key)?;
    Ok(record_version(env, version_key))
}

/// Explicitly migrates one record. The target may not exceed the contract
/// schema version, so a record never claims a schema the contract lacks.
pub fn migrate_record(
    env: &Env,
    caller: &Address,
    record_key: &RecordKey,
    version_key: &RecordKey,
    target_version: u32,
) -> Result<u32, ContractError> {
    require_authorized_vision_records(env, caller)?;
    require_record(env, record_key)?;
    if target_version > get_schema_version(env) {
        return Err(ContractError::InvalidVersion);
    }
    ensure_forward(record_version(env, version_key), target_version)?;
    env.storage().persistent().set(version_key, &target_version);
    Ok(target_version)
}

fn record_version(env: &Env, version_key: &RecordKey) -> u32 {
    env.storage()
        .persistent()
        .get(version_key)
        .unwrap_or(INITIAL_SCHEMA_VERSION)
}

fn require_record(env: &Env, record_key: &RecordKey) -> Result<(), ContractError> {
    if env.storage().persistent().has(record_key) {
        Ok(())
    } else {
        Err(ContractError::RecordNotFound)
    }
}

fn ensure_forward(current: u32, target: u32) -> Result<(), ContractError> {
    match target.cmp(&current) {
        Ordering::Greater => Ok(()),
        Ordering::Equal => Err(ContractError::AlreadyMigrated),
        Ordering::Less => Err(ContractError::InvalidVersion),
    }
}
