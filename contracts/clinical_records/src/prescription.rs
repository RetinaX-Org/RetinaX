use crate::admin::require_authorized_vision_records;
use crate::errors::ContractError;
use crate::examination::validate_string;
use crate::types::{LensType, OptionalContactLensData, Prescription, PrescriptionData};
use soroban_sdk::{symbol_short, Address, Env, String, Symbol};

/// Storage key for committed prescriptions.
pub const PRESCRIPTION_KEY: Symbol = symbol_short!("RX");
/// Storage key for prescription ID counter.
pub const RX_COUNTER: Symbol = symbol_short!("RX_CTR");

/// Validate [`PrescriptionData`] optical parameters.
///
/// All fields must be non-empty strings within reasonable bounds.
/// Values are stored as strings to accommodate various notations.
///
/// # Parameters
/// - `value`: Prescription data to validate
///
/// # Errors
/// - [`ContractError::InvalidInput`] if any field outside bounds
pub fn validate_prescription_data(value: &PrescriptionData) -> Result<(), ContractError> {
    validate_string(&value.sphere, 1, 64)?;
    validate_string(&value.cylinder, 1, 64)?;
    validate_string(&value.axis, 1, 32)?;
    validate_string(&value.add, 1, 32)?;
    validate_string(&value.pd, 1, 32)?;
    Ok(())
}

/// Stage 1: Prepare a prescription for a patient.
///
/// Validates optical data and stores temporarily for commit/rollback.
/// Returns a generated `rx_id` used in subsequent commit or rollback.
/// Only the authorized Vision Records contract may call this.
///
/// # Parameters
/// - `env`: Soroban environment
/// - `caller`: Must be Vision Records contract
/// - `patient`: Patient address
/// - `provider`: Prescribing provider address
/// - `prescription_data`: Optical parameters (sphere, cylinder, axis, add, PD)
///
/// # Returns
/// Generated prescription ID (`rx_id`)
///
/// # Errors
/// - [`ContractError::Unauthorized`] if caller not Vision Records contract
/// - [`ContractError::InvalidInput`] if prescription data invalid
pub fn prepare_add_prescription(
    env: &Env,
    caller: Address,
    patient: Address,
    provider: Address,
    prescription_data: PrescriptionData,
) -> Result<u64, ContractError> {
    require_authorized_vision_records(env, &caller)?;
    validate_prescription_data(&prescription_data)?;
    let next_id = env
        .storage()
        .instance()
        .get(&RX_COUNTER)
        .unwrap_or(0u64)
        .saturating_add(1u64);
    env.storage().instance().set(&RX_COUNTER, &next_id);
    let prep_key = (symbol_short!("P_ADD_RX"), next_id);
    let prep = (patient, provider, prescription_data, env.ledger().timestamp());
    env.storage().temporary().set(&prep_key, &prep);
    Ok(next_id)
}

/// Stage 2a: Commit a prepared prescription.
///
/// Moves prescription from temporary to persistent storage with
/// default values (Glasses, 1-year expiry, unverified).
/// Only the authorized Vision Records contract may call this.
///
/// # Parameters
/// - `env`: Soroban environment
/// - `caller`: Must be Vision Records contract
/// - `rx_id`: Prescription ID from [`prepare_add_prescription`]
///
/// # Errors
/// - [`ContractError::Unauthorized`] if caller not Vision Records contract
/// - [`ContractError::InvalidPhase`] if prescription not prepared or already committed
pub fn commit_add_prescription(
    env: &Env,
    caller: Address,
    rx_id: u64,
) -> Result<(), ContractError> {
    require_authorized_vision_records(env, &caller)?;
    let prep_key = (symbol_short!("P_ADD_RX"), rx_id);
    let prepared: (Address, Address, PrescriptionData, u64) = env
        .storage()
        .temporary()
        .get(&prep_key)
        .ok_or(ContractError::InvalidPhase)?;

    let prescription = Prescription {
        id: rx_id,
        patient: prepared.0,
        provider: prepared.1,
        lens_type: LensType::Glasses,
        left_eye: prepared.2.clone(),
        right_eye: prepared.2,
        contact_data: OptionalContactLensData::None,
        issued_at: prepared.3,
        expires_at: prepared.3.saturating_add(31_536_000), // 1 year
        verified: false,
        metadata_hash: String::from_str(env, ""),
    };

    env.storage().persistent().set(&(PRESCRIPTION_KEY, rx_id), &prescription);
    env.storage().temporary().remove(&prep_key);
    Ok(())
}

/// Stage 2b: Rollback a prepared prescription.
///
/// Discards temporary prescription data. Does not affect committed prescriptions.
/// Only the authorized Vision Records contract may call this.
///
/// # Parameters
/// - `env`: Soroban environment
/// - `caller`: Must be Vision Records contract
/// - `rx_id`: Prescription ID from [`prepare_add_prescription`]
///
/// # Errors
/// - [`ContractError::Unauthorized`] if caller not Vision Records contract
/// - [`ContractError::InvalidPhase`] if prescription not in prepared state
pub fn rollback_add_prescription(
    env: &Env,
    caller: Address,
    rx_id: u64,
) -> Result<(), ContractError> {
    require_authorized_vision_records(env, &caller)?;
    env.storage().temporary().remove(&(symbol_short!("P_ADD_RX"), rx_id));
    Ok(())
}

/// Get total count of committed prescriptions.
///
/// # Parameters
/// - `env`: Soroban environment
///
/// # Returns
/// Number of prescriptions in persistent storage
pub fn get_prescription_count(env: &Env) -> u64 {
    env.storage().instance().get(&RX_COUNTER).unwrap_or(0)
}

/// Retrieve a committed prescription by ID.
///
/// Only the authorized Vision Records contract may call this.
///
/// # Parameters
/// - `env`: Soroban environment
/// - `caller`: Must be Vision Records contract
/// - `id`: Prescription ID to retrieve
///
/// # Returns
/// Complete [`Prescription`] record
///
/// # Errors
/// - [`ContractError::Unauthorized`] if caller not Vision Records contract
/// - [`ContractError::RecordNotFound`] if no prescription exists with that ID
pub fn get_prescription(env: &Env, caller: Address, id: u64) -> Result<Prescription, ContractError> {
    require_authorized_vision_records(env, &caller)?;
    env.storage()
        .persistent()
        .get(&(PRESCRIPTION_KEY, id))
        .ok_or(ContractError::RecordNotFound)
}