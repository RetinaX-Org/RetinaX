use crate::admin::require_authorized_vision_records;
use crate::errors::ContractError;
use crate::types::{
    EyeExamination, IntraocularPressure, OptFundusPhotography, OptPhysicalMeasurement,
    OptRetinalImaging, OptVisualField, PhysicalMeasurement, SlitLampFindings, VisualAcuity,
};
use soroban_sdk::{symbol_short, Address, Env, String, Symbol};

pub const EXAM_KEY: Symbol = symbol_short!("EXAM");

/// Check that `value.len()` is within `min_len..=max_len`.
///
/// # Errors
///
/// [`InvalidInput`](ContractError::InvalidInput) when out of range.
pub fn validate_string(value: &String, min_len: u32, max_len: u32) -> Result<(), ContractError> {
    let len = value.len();
    if len < min_len || len > max_len {
        return Err(ContractError::InvalidInput);
    }
    Ok(())
}

fn validate_physical_measurement(value: &PhysicalMeasurement) -> Result<(), ContractError> {
    validate_string(&value.left_eye, 1, 64)?;
    validate_string(&value.right_eye, 1, 64)?;
    Ok(())
}

fn validate_visual_acuity(value: &VisualAcuity) -> Result<(), ContractError> {
    validate_physical_measurement(&value.uncorrected)?;
    if let OptPhysicalMeasurement::Some(corrected) = &value.corrected {
        validate_physical_measurement(corrected)?;
    }
    Ok(())
}

fn validate_iop(value: &IntraocularPressure) -> Result<(), ContractError> {
    validate_string(&value.method, 1, 64)?;
    Ok(())
}

fn validate_slit_lamp(value: &SlitLampFindings) -> Result<(), ContractError> {
    validate_string(&value.cornea, 1, 256)?;
    validate_string(&value.anterior_chamber, 1, 256)?;
    validate_string(&value.iris, 1, 256)?;
    validate_string(&value.lens, 1, 256)?;
    Ok(())
}

fn validate_visual_field(value: &OptVisualField) -> Result<(), ContractError> {
    if let OptVisualField::Some(field) = value {
        validate_string(&field.left_eye_reliability, 0, 128)?;
        validate_string(&field.right_eye_reliability, 0, 128)?;
        validate_string(&field.left_eye_defects, 0, 256)?;
        validate_string(&field.right_eye_defects, 0, 256)?;
    }
    Ok(())
}

fn validate_retinal_imaging(value: &OptRetinalImaging) -> Result<(), ContractError> {
    if let OptRetinalImaging::Some(image) = value {
        validate_string(&image.image_url, 1, 256)?;
        validate_string(&image.image_hash, 1, 128)?;
        validate_string(&image.findings, 0, 512)?;
    }
    Ok(())
}

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

/// Run every string-length rule over an incoming exam payload: visual acuity,
/// IOP method, slit-lamp findings, and the optional visual-field / retina-imaging
/// / fundus-photo strings, plus clinical notes (max 2048 chars).
///
/// # Errors
///
/// [`InvalidInput`](ContractError::InvalidInput) if any field violates its
/// bounds; otherwise `Ok(())`.
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

/// Validate and persist one eye examination under `(EXAM_KEY, record_id)`.
///
/// Requires `caller` to be the authorized vision-records contract.
///
/// # Errors
///
/// [`Unauthorized`](ContractError::Unauthorized) for callers other than the
/// vision-records contract, or
/// [`InvalidInput`](ContractError::InvalidInput) on out-of-bounds strings.
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

    env.storage()
        .persistent()
        .set(&(EXAM_KEY, record_id), &exam);
    Ok(())
}

/// Load the eye examination stored under `record_id`.
///
/// # Errors
///
/// [`Unauthorized`](ContractError::Unauthorized) for the wrong caller or
/// [`RecordNotFound`](ContractError::RecordNotFound) when absent.
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
