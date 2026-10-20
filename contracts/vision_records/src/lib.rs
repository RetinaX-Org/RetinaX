#![no_std]
#![allow(clippy::too_many_arguments)]

extern crate alloc;
use alloc::{
    string::{String as StdString, ToString},
    vec::Vec as StdVec,
};

pub mod appointment;
pub mod audit;
pub mod circuit_breaker;
pub mod consent_management;
pub mod emergency;
pub mod errors;
pub mod events;
pub mod examination;
pub mod insurance;
pub mod patient_profile;
pub mod prescription;
pub mod provider;
pub mod rate_limit;
pub mod rbac;
pub mod validation;

mod types;
pub use types::*;

// Contract endpoints, grouped by area. Each file adds a #[contractimpl] block.
mod access_grants;
mod admin;
mod archive;
mod multisig;
mod permissions;
mod prescriptions;
mod profile;
mod provenance;
mod records;
mod transactions;
mod user;

pub use consent_management::{ConsentGrant, ConsentType};

use access_control::AccessControlContractClient;
use key_manager::{DerivedKey, KeyManagerContractClient};
use soroban_sdk::{
    contract, contractimpl, symbol_short, Address, Bytes, BytesN, Env, String, Symbol, Vec,
};

use teye_common::lineage::{self, RelationshipKind};
use teye_common::provenance_graph::{self, LineageAccessResult, ProvenanceExport};
use teye_common::state_machine::{
    self, EntityKind, LifecycleState, TransitionContext, TransitionRecord,
};
use teye_common::{
    admin_tiers, concurrency::ConflictEntry, concurrency::FieldChange,
    concurrency::ResolutionStrategy, progressive_auth, risk_engine, session, whitelist, AdminTier,
    KeyManager, MeteringOpType, UpdateOutcome, VersionStamp,
};

/// Re-export the contract-specific error type at the crate root.
pub use errors::ContractError;

/// Re-export provider types needed by other modules (e.g. events).
pub use provider::VerificationStatus;

/// Re-export error helpers used throughout the contract.
pub use errors::{create_error_context, log_error};

/// Re-export types from submodules used directly in the contract impl.
pub use audit::{AccessAction, AccessResult};
pub use emergency::{EmergencyAccess, EmergencyAuditEntry, EmergencyCondition, EmergencyStatus};
pub use examination::{
    EyeExamination, IntraocularPressure, OptFundusPhotography, OptRetinalImaging, OptVisualField,
    SlitLampFindings, VisualAcuity,
};
pub use insurance::{InsuranceInfo, OptionalInsuranceInfo};
pub use patient_profile::{EmergencyContact, OptionalEmergencyContact, PatientProfile};
pub use prescription::{LensType, OptionalContactLensData, Prescription, PrescriptionData};

/// Storage keys for the contract
const ADMIN: Symbol = symbol_short!("ADMIN");
const PENDING_ADMIN: Symbol = symbol_short!("PEND_ADM");
const INITIALIZED: Symbol = symbol_short!("INIT");
const RATE_CFG: Symbol = symbol_short!("RL_IN_CFG");
const RATE_TRACK: Symbol = symbol_short!("RL_IN_TRK");

const TTL_THRESHOLD: u32 = 5184000;
const TTL_EXTEND_TO: u32 = 10368000;

const ENC_CUR: Symbol = symbol_short!("ENC_CUR");
const ENC_KEY: Symbol = symbol_short!("ENC_KEY");
const KEY_MGR: Symbol = symbol_short!("KEY_MGR");
const KEY_MGR_KEY: Symbol = symbol_short!("KEY_MGRK");
const ACCESS_CONTROL: Symbol = symbol_short!("ACC_CTRL");

/// Extends the time-to-live (TTL) for a storage key containing an Address.
/// This ensures the data remains accessible for the extended period.
fn extend_ttl_address_key(env: &Env, key: &(Symbol, Address)) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

/// Extends the time-to-live (TTL) for a storage key containing a u64 value.
/// This ensures the data remains accessible for the extended period.
fn extend_ttl_u64_key(env: &Env, key: &(Symbol, u64)) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

/// Extends the time-to-live (TTL) for an access grant storage key.
/// This ensures access grant data remains accessible for the extended period.
fn extend_ttl_access_key(env: &Env, key: &(Symbol, Address, Address)) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn extend_ttl_record_access_key(env: &Env, key: &(Symbol, u64, Address)) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

fn rate_limit_action_hash(
    env: &Env,
    max_requests_per_window: u64,
    window_duration_seconds: u64,
) -> BytesN<32> {
    let mut payload = Bytes::new(env);
    payload.append(&Bytes::from_slice(env, b"SET_RATE"));
    payload.append(&Bytes::from_slice(
        env,
        &max_requests_per_window.to_be_bytes(),
    ));
    payload.append(&Bytes::from_slice(
        env,
        &window_duration_seconds.to_be_bytes(),
    ));
    env.crypto().sha256(&payload).into()
}

#[allow(dead_code)]
fn encryption_key_action_hash(env: &Env, version: &String, key: &String) -> BytesN<32> {
    let mut payload = Bytes::new(env);
    payload.append(&Bytes::from_slice(env, b"SET_ENC"));
    let version_std = version.to_string();
    let key_std = key.to_string();
    payload.append(&Bytes::from_slice(env, version_std.as_bytes()));
    payload.append(&Bytes::from_slice(env, key_std.as_bytes()));
    env.crypto().sha256(&payload).into()
}

fn consent_key(patient: &Address, grantee: &Address) -> (Symbol, Address, Address) {
    (symbol_short!("CONSENT"), patient.clone(), grantee.clone())
}

fn has_active_consent(env: &Env, patient: &Address, grantee: &Address) -> bool {
    let key = consent_key(patient, grantee);
    if let Some(consent) = env.storage().persistent().get::<_, ConsentGrant>(&key) {
        !consent.revoked && consent.expires_at > env.ledger().timestamp()
    } else {
        false
    }
}

pub use rbac::{
    create_access_policy, evaluate_access_policies, set_record_sensitivity, set_user_credential,
    AccessPolicy, CredentialType, Permission, PolicyContext, Role, SensitivityLevel,
    TimeRestriction,
};

#[contract]
#[allow(clippy::too_many_arguments)]
pub struct VisionRecordsContract;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl VisionRecordsContract {
    fn meter_op(_env: &Env, _caller: &Address, _op_type: MeteringOpType) {
        // Simple mock since metering is not fully implemented in teye_common yet or it was removed.
    }

    fn emit_access_violation(env: &Env, caller: &Address, action: &str, required_permission: &str) {
        events::publish_access_violation(
            env,
            caller.clone(),
            String::from_str(env, action),
            String::from_str(env, required_permission),
        );
    }

    pub(crate) fn unauthorized<T>(
        env: &Env,
        caller: &Address,
        action: &str,
        required_permission: &str,
    ) -> Result<T, ContractError> {
        Self::emit_access_violation(env, caller, action, required_permission);
        Err(ContractError::Unauthorized)
    }

    pub(crate) fn access_denied<T>(
        env: &Env,
        caller: &Address,
        action: &str,
        required_permission: &str,
    ) -> Result<T, ContractError> {
        Self::emit_access_violation(env, caller, action, required_permission);
        Err(ContractError::AccessDenied)
    }

    fn get_key_manager_config(env: &Env) -> Option<(Address, BytesN<32>)> {
        let manager: Option<Address> = env.storage().instance().get(&KEY_MGR);
        let key_id: Option<BytesN<32>> = env.storage().instance().get(&KEY_MGR_KEY);
        match (manager, key_id) {
            (Some(mgr), Some(key)) => Some((mgr, key)),
            _ => None,
        }
    }

    fn get_access_control_config(env: &Env) -> Option<Address> {
        env.storage().instance().get(&ACCESS_CONTROL)
    }

    /// Internal permission check querying standalone access_control micro-contract if configured,
    /// falling back to local rbac storage otherwise.
    pub fn has_permission_unified(env: &Env, user: &Address, permission: &Permission) -> bool {
        if let Some(ac_addr) = Self::get_access_control_config(env) {
            let client = AccessControlContractClient::new(env, &ac_addr);
            let ac_perm = match permission {
                Permission::ReadAnyRecord => access_control::Permission::ReadAnyRecord,
                Permission::WriteRecord => access_control::Permission::WriteRecord,
                Permission::ManageAccess => access_control::Permission::ManageAccess,
                Permission::ManageUsers => access_control::Permission::ManageUsers,
                Permission::SystemAdmin => access_control::Permission::SystemAdmin,
            };
            client.has_permission(user, &ac_perm)
        } else {
            rbac::has_permission(env, user, permission)
        }
    }

    /// Internal delegated permission check querying standalone access_control micro-contract if configured.
    pub fn has_delegated_permission_unified(
        env: &Env,
        delegator: &Address,
        delegatee: &Address,
        permission: &Permission,
    ) -> bool {
        if let Some(ac_addr) = Self::get_access_control_config(env) {
            let client = AccessControlContractClient::new(env, &ac_addr);
            let ac_perm = match permission {
                Permission::ReadAnyRecord => access_control::Permission::ReadAnyRecord,
                Permission::WriteRecord => access_control::Permission::WriteRecord,
                Permission::ManageAccess => access_control::Permission::ManageAccess,
                Permission::ManageUsers => access_control::Permission::ManageUsers,
                Permission::SystemAdmin => access_control::Permission::SystemAdmin,
            };
            client.has_delegated_permission(delegator, delegatee, &ac_perm)
        } else {
            rbac::has_delegated_permission(env, delegator, delegatee, permission)
        }
    }

    #[allow(dead_code)]
    fn derive_key_manager_bytes(
        env: &Env,
        record_id: u64,
        version: Option<u32>,
    ) -> Result<Option<(StdVec<u8>, String)>, ContractError> {
        let (manager, key_id) = match Self::get_key_manager_config(env) {
            Some(cfg) => cfg,
            None => return Ok(None),
        };

        let client = KeyManagerContractClient::new(env, &manager);
        let derived: DerivedKey = match version {
            Some(ver) => client.derive_record_key_with_version(&key_id, &record_id, &ver),
            None => client.derive_record_key(&key_id, &record_id),
        };
        let bytes = derived.key.to_array().to_vec();
        let version_str = derived.version.to_string();
        Ok(Some((bytes, String::from_str(env, &version_str))))
    }

    #[allow(dead_code)]
    fn parse_key_version_u32(version: &String) -> Option<u32> {
        version.to_string().parse::<u32>().ok()
    }

    fn enforce_rate_limit(env: &Env, caller: &Address) -> Result<(), ContractError> {
        let cfg: Option<(u64, u64)> = env.storage().instance().get(&RATE_CFG);
        let (max_requests_per_window, window_duration_seconds) = match cfg {
            Some(c) => c,
            None => return Ok(()), // No config set -> unlimited
        };

        if max_requests_per_window == 0 || window_duration_seconds == 0 {
            // Explicitly disabled
            return Ok(());
        }

        let now = env.ledger().timestamp();
        let key = (RATE_TRACK, caller.clone());

        let mut state: (u64, u64) = env.storage().persistent().get(&key).unwrap_or((0, now));

        let window_end = state.1.saturating_add(window_duration_seconds);
        if now >= window_end {
            state.0 = 0;
            state.1 = now;
        }

        let next = state.0.saturating_add(1);
        if next > max_requests_per_window {
            return Err(ContractError::RateLimitExceeded);
        }

        state.0 = next;
        env.storage().persistent().set(&key, &state);

        Ok(())
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_consent_management_failures;
#[cfg(test)]
mod test_pause;
#[cfg(test)]
mod test_rbac;

#[cfg(test)]
mod test_batch;

#[cfg(test)]
mod test_admin_tiers;

#[cfg(test)]
mod test_occ;

#[cfg(test)]
mod test_diagnostic_image_metadata;

#[cfg(test)]
mod test_empty_record;

#[cfg(test)]
mod test_examination_endpoints;

#[cfg(test)]
mod test_profile;

#[cfg(test)]
mod prescription_tests;
#[cfg(test)]
mod test_archive;
#[cfg(test)]
mod test_insurance_endpoints;
#[cfg(test)]
mod test_multisig;
#[cfg(test)]
mod test_rate_limit_bypass;
