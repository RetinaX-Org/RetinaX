use crate::admin::require_authorized_vision_records;
use crate::errors::ContractError;
use crate::examination::validate_string;
use crate::types::{LensType, OptionalContactLensData, Prescription, PrescriptionData};
use soroban_sdk::{symbol_short, Address, Env, String, Symbol};

pub const PRESCRIPTION_KEY: Symbol = symbol_short!("RX");
pub const RX_COUNTER: Symbol = symbol_short!("RX_CTR");

/// Check the length bounds of every refraction string (sphere, cylinder, axis,
/// add, pd).
///
/// # Errors
///
/// [`InvalidInput`](ContractError::InvalidInput) when any value is out of
/// range.
pub fn validate_prescription_data(value: &PrescriptionData) -> Result<(), ContractError> {
    validate_string(&value.sphere, 1, 64)?;
    validate_string(&value.cylinder, 1, 64)?;
    validate_string(&value.axis, 1, 32)?;
    validate_string(&value.add, 1, 32)?;
    validate_string(&value.pd, 1, 32)?;
    Ok(())
}

/// Stage a new prescription and return its allocated id (see
/// [`crate::ClinicalRecordsContract::prepare_add_prescription`]).
///
/// # Errors
///
/// [`Unauthorized`](ContractError::Unauthorized) for the wrong caller or
/// [`InvalidInput`](ContractError::InvalidInput) for bad prescription strings.
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
    let prep = (
        patient,
        provider,
        prescription_data,
        env.ledger().timestamp(),
    );
    env.storage().temporary().set(&prep_key, &prep);
    Ok(next_id)
}

/// Commit staged prescription `rx_id` to persistent storage (see
/// [`crate::ClinicalRecordsContract::commit_add_prescription`]).
///
/// # Errors
///
/// [`Unauthorized`](ContractError::Unauthorized) for the wrong caller or
/// [`InvalidPhase`](ContractError::InvalidPhase) if nothing is staged.
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
        expires_at: prepared.3.saturating_add(31_536_000),
        verified: false,
        metadata_hash: String::from_str(env, ""),
    };

    env.storage()
        .persistent()
        .set(&(PRESCRIPTION_KEY, rx_id), &prescription);
    env.storage().temporary().remove(&prep_key);
    Ok(())
}

/// Discard staged prescription `rx_id` (see
/// [`crate::ClinicalRecordsContract::rollback_add_prescription`]).
///
/// # Errors
///
/// [`Unauthorized`](ContractError::Unauthorized) for callers other than the
/// vision-records contract.
pub fn rollback_add_prescription(
    env: &Env,
    caller: Address,
    rx_id: u64,
) -> Result<(), ContractError> {
    require_authorized_vision_records(env, &caller)?;
    env.storage()
        .temporary()
        .remove(&(symbol_short!("P_ADD_RX"), rx_id));
    Ok(())
}

/// Return the prescription id counter, 0 before any prepare call.
pub fn get_prescription_count(env: &Env) -> u64 {
    env.storage().instance().get(&RX_COUNTER).unwrap_or(0)
}

/// Load prescription `id` for the authorized vision-records caller (see
/// [`crate::ClinicalRecordsContract::get_prescription`]).
///
/// # Errors
///
/// [`Unauthorized`](ContractError::Unauthorized) for the wrong caller or
/// [`RecordNotFound`](ContractError::RecordNotFound) when absent.
pub fn get_prescription(
    env: &Env,
    caller: Address,
    id: u64,
) -> Result<Prescription, ContractError> {
    require_authorized_vision_records(env, &caller)?;
    env.storage()
        .persistent()
        .get(&(PRESCRIPTION_KEY, id))
        .ok_or(ContractError::RecordNotFound)
}
