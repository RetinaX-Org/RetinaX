use crate::admin::require_authorized_vision_records;
use crate::errors::ContractError;
use crate::types::{
    EyeExamination, IntraocularPressure, OptFundusPhotography, OptPhysicalMeasurement,
    OptRetinalImaging, OptVisualField, PhysicalMeasurement, SlitLampFindings, VisualAcuity,
};
use soroban_sdk::{symbol_short, Address, Env, String, Symbol};

/// Storage key for eye examination records.
pub const EXAM_KEY: Symbol = symbol_short!("EXAM");

/// Validate a string length is within bounds.
///
/// # Parameters
/// - `value`: String to validate
/// - `min_len`: Minimum allowed length (inclusive)
/// - `max_len`: Maximum allowed length (inclusive)
///
/// # Errors
/// - [`ContractError::InvalidInput`] if length outside bounds
pub fn validate_string(value: &String, min_len: u32, max_len: u32) -> Result<(), ContractError> {
    let len = value.len();
    if len < min_len || len > max_len {
        return Err(ContractError::InvalidInput);
    }
    Ok(())
}

/// Validate both eyes of a [`PhysicalMeasurement`] are non-empty and within 64 chars.
fn validate_physical_measurement(value: &PhysicalMeasurement) -> Result<(), ContractError> {
    validate_string(&value.left_eye, 1, 64)?;
    validate_string(&value.right_eye, 1, 64)?;
    Ok(())
}

/// Validate [`VisualAcuity`] uncorrected and corrected measurements.
fn validate_visual_acuity(value: &VisualAcuity) -> Result<(), ContractError> {
    validate_physical_measurement(&value.uncorrected)?;
    if let OptPhysicalMeasurement::Some(corrected) = &value.corrected {
        validate_physical_measurement(corrected)?;
    }
    Ok(())
}

/// Validate [`IntraocularPressure`] method string.
fn validate_iop(value: &IntraocularPressure) -> Result<(), ContractError> {
    validate_string(&value.method, 1, 64)?;
    Ok(())
}

/// Validate [`SlitLampFindings`] all four fields within 256 chars.
fn validate_slit_lamp(value: &SlitLampFindings) -> Result<(), ContractError> {
    validate_string(&value.cornea, 1, 256)?;
    validate_string(&value.anterior_chamber, 1, 256)?;
    validate_string(&value.iris, 1, 256)?;
    validate_string(&value.lens, 1, 256)?;
    Ok(())
}

/// Validate [`OptVisualField`] optional fields within bounds.
fn validate_visual_field(value: &OptVisualField) -> Result<(), ContractError> {
    if let OptVisualField::Some(field) = value {
        validate_string(&field.left_eye_reliability, 0, 128)?;
        validate_string(&field.right_eye_reliability, 0, 128)?;
        validate_string(&field.left_eye_defects, 0, 256)?;
        validate_string(&field.right_eye_defects, 0, 256)?;
    }
    Ok(())
}

/// Validate [`OptRetinalImaging`] optional fields.
fn validate_retinal_imaging(value: &OptRetinalImaging) -> Result<(), ContractError> {
    if let OptRetinalImaging::Some(image) = value {
        validate_string(&image.image_url, 1, 256)?;
        validate_string(&image.image_hash, 1, 128)?;
        validate_string(&image.findings, 0, 512)?;
    }
    Ok(())
}

/// Validate [`OptFundusPhotography`] optional fields.
fn validate_fundus_photo(value: &OptFundusPhotography) -> Result<(), ContractError> {
    if let OptFundusPhotography::Some(photo) = value {
        validate_string(&photo.image_url, 1, 256)?;
        validate_string(&photo.image_hash, 1, 128)?;
        validate_string(&photo.cup_to_disc_ratio_left, 1, 64)?;
        validate_string(&photo.cup_to_disc_ratio_right, 1, 64)?;
        validate_string(&photo.macula_status, 0, 256)?;
    }
    Ok(())
}

/// Validate all examination string fields at once.
///
/// Checks every clinical component for length and emptiness constraints.
/// Called by [`add_eye_examination`] before storage.
///
/// # Parameters
/// - All examination component structs
/// - `clinical_notes`: Free-text notes (max 2048 chars, may be empty)
///
/// # Errors
/// - [`ContractError::InvalidInput`] if any field violates constraints
pub fn validate_examination_strings(
    visual_acuity: &VisualAcuity,
    iop: &IntraocularPressure,
    slit_lamp: &SlitLampFindings,
    visual_field: &OptVisualField,
    retina_imaging: &OptRetinalImaging,
    fundus_photo: &OptFundusPhotography,
    clinical_notes: &String,
) -> Result<(), ContractError> {
    validate_visual_acuity(visual_acuity)?;
    validate_iop(iop)?;
    validate_slit_lamp(slit_lamp)?;
    validate_visual_field(visual_field)?;
    validate_retinal_imaging(retina_imaging)?;
    validate_fundus_photo(fundus_photo)?;
    validate_string(clinical_notes, 0, 2048)?;
    Ok(())
}

/// Add a complete eye examination record to persistent storage.
///
/// Called by the authorized Vision Records contract. Validates all
/// clinical components before storing under the given `record_id`.
///
/// # Parameters
/// - `env`: Soroban environment
/// - `caller`: Must be Vision Records contract (checked via [`require_authorized_vision_records`])
/// - `record_id`: Unique examination identifier
/// - `visual_acuity` through `fundus_photo`: Clinical measurement structs
/// - `clinical_notes`: Free-text observations (max 2048 chars)
///
/// # Errors
/// - [`ContractError::Unauthorized`] if caller not Vision Records contract
/// - [`ContractError::InvalidInput`] if any field fails validation
pub fn add_eye_examination(
    env: &Env,
    caller: Address,
    record_id: u64,
    visual_acuity: VisualAcuity,
    iop: IntraocularPressure,
    slit_lamp: SlitLampFindings,
    visual_field: OptVisualField,
    retina_imaging: OptRetinalImaging,
    fundus_photo: OptFundusPhotography,
    clinical_notes: String,
) -> Result<(), ContractError> {
    require_authorized_vision_records(env, &caller)?;
    validate_examination_strings(
        &visual_acuity,
        &iop,
        &slit_lamp,
        &visual_field,
        &retina_imaging,
        &fundus_photo,
        &clinical_notes,
    )?;

    let exam = EyeExamination {
        record_id,
        visual_acuity,
        iop,
        slit_lamp,
        visual_field,
        retina_imaging,
        fundus_photo,
        clinical_notes,
    };

    env.storage().persistent().set(&(EXAM_KEY, record_id), &exam);
    Ok(())
}

/// Retrieve an eye examination record from persistent storage.
///
/// Called by the authorized Vision Records contract.
///
/// # Parameters
/// - `env`: Soroban environment
/// - `caller`: Must be Vision Records contract
/// - `record_id`: Examination ID to retrieve
///
/// # Returns
/// Complete [`EyeExamination`] if found
///
/// # Errors
/// - [`ContractError::Unauthorized`] if caller not Vision Records contract
/// - [`ContractError::RecordNotFound`] if no record exists with that ID
pub fn get_eye_examination(
    env: &Env,
    caller: Address,
    record_id: u64,
) -> Result<EyeExamination, ContractError> {
    require_authorized_vision_records(env, &caller)?;
    env.storage()
        .persistent()
        .get(&(EXAM_KEY, record_id))
        .ok_or(ContractError::RecordNotFound)
}