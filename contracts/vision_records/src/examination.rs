#![allow(clippy::arithmetic_side_effects)]
use crate::audit::AuditManager;
use crate::circuit_breaker::{self, PauseScope};
use crate::errors::ContractError;
use crate::events;
use crate::rbac::{self, Permission};
use crate::validation;
use crate::{AccessLevel, RecordType, VisionRecordsContract};
use alloc::string::ToString;
use soroban_sdk::{
    contracttype, symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, String, Symbol, Vec,
};
use teye_common::concurrency::{self, FieldChange, UpdateOutcome, VersionStamp};
use teye_common::lineage::{self, RelationshipKind};
use teye_common::state_machine::{
    self, EntityKind, LifecycleState, TransitionContext, TransitionRecord, VisionRecordState,
};

const TTL_THRESHOLD: u32 = 5184000;
const TTL_EXTEND_TO: u32 = 10368000;

fn extend_ttl_exam_key(env: &Env, key: &(Symbol, u64)) {
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalMeasurement {
    pub left_eye: String,
    pub right_eye: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptPhysicalMeasurement {
    None,
    Some(PhysicalMeasurement),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualAcuity {
    pub uncorrected: PhysicalMeasurement,
    pub corrected: OptPhysicalMeasurement,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntraocularPressure {
    pub left_eye: u32,
    pub right_eye: u32,
    pub method: String,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlitLampFindings {
    pub cornea: String,
    pub anterior_chamber: String,
    pub iris: String,
    pub lens: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualField {
    pub left_eye_reliability: String,
    pub right_eye_reliability: String,
    pub left_eye_defects: String,
    pub right_eye_defects: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum OptVisualField {
    None,
    Some(VisualField),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetinalImaging {
    pub image_url: String,
    pub image_hash: String,
    pub findings: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum OptRetinalImaging {
    None,
    Some(RetinalImaging),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundusPhotography {
    pub image_url: String,
    pub image_hash: String,
    pub cup_to_disc_ratio_left: String,
    pub cup_to_disc_ratio_right: String,
    pub macula_status: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum OptFundusPhotography {
    None,
    Some(FundusPhotography),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EyeExamination {
    pub record_id: u64,
    pub visual_acuity: VisualAcuity,
    pub iop: IntraocularPressure,
    pub slit_lamp: SlitLampFindings,
    pub visual_field: OptVisualField,
    pub retina_imaging: OptRetinalImaging,
    pub fundus_photo: OptFundusPhotography,
    pub clinical_notes: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExaminationVersion {
    pub version: u64,
    pub examination: EyeExamination,
    pub hash: BytesN<32>,
    pub previous_hash: Option<BytesN<32>>,
}

pub fn lifecycle_state_to_symbol(state: &LifecycleState) -> Symbol {
    match state {
        LifecycleState::Vision(VisionRecordState::Draft) => symbol_short!("DRAFT"),
        LifecycleState::Vision(VisionRecordState::PendingReview) => symbol_short!("REVIEW"),
        LifecycleState::Vision(VisionRecordState::Approved) => symbol_short!("APPROVD"),
        LifecycleState::Vision(VisionRecordState::Archived) => symbol_short!("ARCHIVD"),
        LifecycleState::Vision(VisionRecordState::Purged) => symbol_short!("PURGED"),
        _ => symbol_short!("OTHER"),
    }
}

pub fn update_outcome_to_symbol(outcome: &UpdateOutcome) -> Symbol {
    match outcome {
        UpdateOutcome::Applied(_) => symbol_short!("APPLIED"),
        UpdateOutcome::Merged(_) => symbol_short!("MERGED"),
        UpdateOutcome::Conflicted(_) => symbol_short!("CONFLICT"),
    }
}

pub fn validate_physical_measurement(
    measurement: &PhysicalMeasurement,
) -> Result<(), ContractError> {
    validation::validate_string_length(&measurement.left_eye, 1, 64)?;
    validation::validate_string_length(&measurement.right_eye, 1, 64)?;
    Ok(())
}

pub fn validate_visual_acuity(va: &VisualAcuity) -> Result<(), ContractError> {
    validate_physical_measurement(&va.uncorrected)?;
    if let OptPhysicalMeasurement::Some(ref corrected) = va.corrected {
        validate_physical_measurement(corrected)?;
    }
    Ok(())
}

pub fn validate_iop(iop: &IntraocularPressure) -> Result<(), ContractError> {
    validation::validate_string_length(&iop.method, 1, 64)?;
    Ok(())
}

pub fn validate_slit_lamp(slit_lamp: &SlitLampFindings) -> Result<(), ContractError> {
    validation::validate_string_length(&slit_lamp.cornea, 1, 256)?;
    validation::validate_string_length(&slit_lamp.anterior_chamber, 1, 256)?;
    validation::validate_string_length(&slit_lamp.iris, 1, 256)?;
    validation::validate_string_length(&slit_lamp.lens, 1, 256)?;
    Ok(())
}

pub fn validate_visual_field(vf: &OptVisualField) -> Result<(), ContractError> {
    if let OptVisualField::Some(ref field) = vf {
        validation::validate_string_length(&field.left_eye_reliability, 0, 128)?;
        validation::validate_string_length(&field.right_eye_reliability, 0, 128)?;
        validation::validate_string_length(&field.left_eye_defects, 0, 256)?;
        validation::validate_string_length(&field.right_eye_defects, 0, 256)?;
    }
    Ok(())
}

pub fn validate_retinal_imaging(ri: &OptRetinalImaging) -> Result<(), ContractError> {
    if let OptRetinalImaging::Some(ref imaging) = ri {
        validation::validate_string_length(&imaging.image_url, 1, 256)?;
        validation::validate_string_length(&imaging.image_hash, 1, 128)?;
        validation::validate_string_length(&imaging.findings, 0, 512)?;
    }
    Ok(())
}

pub fn validate_fundus_photo(fp: &OptFundusPhotography) -> Result<(), ContractError> {
    if let OptFundusPhotography::Some(ref photo) = fp {
        validation::validate_string_length(&photo.image_url, 1, 256)?;
        validation::validate_string_length(&photo.image_hash, 1, 128)?;
        validation::validate_string_length(&photo.cup_to_disc_ratio_left, 1, 64)?;
        validation::validate_string_length(&photo.cup_to_disc_ratio_right, 1, 64)?;
        validation::validate_string_length(&photo.macula_status, 0, 256)?;
    }
    Ok(())
}

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
    validation::validate_string_length(clinical_notes, 0, 2048)?;
    Ok(())
}

pub fn exam_key(record_id: u64) -> (Symbol, u64) {
    (symbol_short!("EXAM"), record_id)
}

fn exam_history_count_key(record_id: u64) -> (Symbol, u64) {
    (symbol_short!("EXH_CNT"), record_id)
}

fn exam_history_hash_key(record_id: u64) -> (Symbol, u64) {
    (symbol_short!("EXH_HASH"), record_id)
}

fn exam_history_key(record_id: u64, version: u64) -> (Symbol, u64, u64) {
    (symbol_short!("EXHIST"), record_id, version)
}

pub fn get_examination(env: &Env, record_id: u64) -> Option<EyeExamination> {
    let key = exam_key(record_id);
    env.storage().persistent().get(&key)
}

/// Stores an eye examination record and initialises its lineage node.
pub fn set_examination(
    env: &Env,
    exam: &EyeExamination,
    provider: &Address,
) -> Result<(), ContractError> {
    store_examination_version(env, exam, provider)?;

    // Lineage: create or extend the provenance node for this examination.
    let (_, is_new) =
        lineage::create_node(env, exam.record_id, provider.clone(), "Examination", None);

    if is_new {
        // Genesis edge: Created(provider → record)
        lineage::add_edge(
            env,
            exam.record_id, // self-referential source for genesis
            exam.record_id,
            RelationshipKind::Created,
            provider.clone(),
            None,
        );
    }

    let initial_transition = state_machine::apply_transition(
        env,
        0,
        &EntityKind::VisionRecord,
        exam.record_id,
        LifecycleState::Vision(state_machine::VisionRecordState::PendingReview),
        TransitionContext {
            actor: provider.clone(),
            actor_role: symbol_short!("PROV"),
            now: env.ledger().timestamp(),
            retention_until: 0,
            expires_at: 0,
            prerequisites_met: true,
        },
    );

    if let Ok(rec) = initial_transition {
        let from_sym = lifecycle_state_to_symbol(&rec.from_state);
        let to_sym = lifecycle_state_to_symbol(&rec.to_state);
        events::publish_examination_state_transitioned(
            env,
            exam.record_id,
            provider.clone(),
            from_sym,
            to_sym,
        );
    }
    Ok(())
}

pub fn remove_examination(env: &Env, record_id: u64) {
    let key = exam_key(record_id);
    env.storage().persistent().remove(&key);
}

fn examination_hash(
    env: &Env,
    examination: &EyeExamination,
    version: u64,
    previous_hash: &Option<BytesN<32>>,
) -> BytesN<32> {
    let mut payload = Bytes::from_slice(env, b"RETINAX_EXAM_VERSION");
    payload.append(&Bytes::from_slice(env, &version.to_be_bytes()));
    if let Some(previous_hash) = previous_hash {
        payload.append(&Bytes::from_slice(env, &previous_hash.to_array()));
    }
    payload.append(&examination.clone().to_xdr(env));
    env.crypto().sha256(&payload).into()
}

fn store_examination_version(
    env: &Env,
    exam: &EyeExamination,
    provider: &Address,
) -> Result<(), ContractError> {
    let count_key = exam_history_count_key(exam.record_id);
    let mut version: u64 = env.storage().persistent().get(&count_key).unwrap_or(0);

    // Preserve records written before examination history was introduced.
    if version == 0 {
        if let Some(previous) = get_examination(env, exam.record_id) {
            version = 1;
            let previous_hash = None;
            let hash = examination_hash(env, &previous, version, &previous_hash);
            let history_key = exam_history_key(exam.record_id, version);
            env.storage().persistent().set(
                &history_key,
                &ExaminationVersion {
                    version,
                    examination: previous,
                    hash: hash.clone(),
                    previous_hash,
                },
            );
            env.storage()
                .persistent()
                .extend_ttl(&history_key, TTL_THRESHOLD, TTL_EXTEND_TO);
            let latest_hash_key = exam_history_hash_key(exam.record_id);
            env.storage().persistent().set(&latest_hash_key, &hash);
            env.storage()
                .persistent()
                .extend_ttl(&latest_hash_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
    }

    let was_amendment = version > 0;
    for existing_version in 1..=version {
        let existing_key = exam_history_key(exam.record_id, existing_version);
        if env.storage().persistent().has(&existing_key) {
            env.storage()
                .persistent()
                .extend_ttl(&existing_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
    }
    let previous_hash: Option<BytesN<32>> = if was_amendment {
        env.storage()
            .persistent()
            .get(&exam_history_hash_key(exam.record_id))
    } else {
        None
    };
    version += 1;

    let hash = examination_hash(env, exam, version, &previous_hash);
    let history_key = exam_history_key(exam.record_id, version);
    env.storage().persistent().set(
        &history_key,
        &ExaminationVersion {
            version,
            examination: exam.clone(),
            hash: hash.clone(),
            previous_hash: previous_hash.clone(),
        },
    );
    env.storage()
        .persistent()
        .extend_ttl(&history_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    env.storage().persistent().set(&count_key, &version);
    env.storage()
        .persistent()
        .extend_ttl(&count_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    let latest_hash_key = exam_history_hash_key(exam.record_id);
    env.storage().persistent().set(&latest_hash_key, &hash);
    env.storage()
        .persistent()
        .extend_ttl(&latest_hash_key, TTL_THRESHOLD, TTL_EXTEND_TO);

    let key = exam_key(exam.record_id);
    env.storage().persistent().set(&key, exam);
    extend_ttl_exam_key(env, &key);

    if was_amendment {
        let previous_hash = previous_hash.ok_or(ContractError::StorageError)?;
        events::publish_examination_amended(
            env,
            exam.record_id,
            provider.clone(),
            version,
            hash,
            previous_hash,
        );
    }
    Ok(())
}

/// Performs a versioned (OCC) update of an eye examination record.
pub fn versioned_set_examination(
    env: &Env,
    exam: &EyeExamination,
    expected_version: u64,
    node_id: u32,
    provider: &soroban_sdk::Address,
    changed_fields: &Vec<FieldChange>,
) -> Result<UpdateOutcome, ContractError> {
    let outcome = concurrency::compare_and_swap(
        env,
        exam.record_id,
        expected_version,
        node_id,
        provider,
        changed_fields,
    );

    match &outcome {
        UpdateOutcome::Applied(_) | UpdateOutcome::Merged(_) => {
            store_examination_version(env, exam, provider)?;
            concurrency::save_field_snapshot(env, exam.record_id, changed_fields);

            // Lineage: record a ModifiedBy edge for each successful mutation.
            lineage::add_edge(
                env,
                exam.record_id,
                exam.record_id,
                RelationshipKind::ModifiedBy,
                provider.clone(),
                None,
            );
        }
        UpdateOutcome::Conflicted(_) => {
            // Record not updated — conflict must be resolved first.
        }
    }

    Ok(outcome)
}

/// Retrieves the current OCC version stamp for an examination record.
pub fn get_exam_version(env: &Env, record_id: u64) -> VersionStamp {
    concurrency::get_version_stamp(env, record_id)
}

/// Standalone handler to add eye examination details for an existing record
#[allow(clippy::too_many_arguments)]
pub fn add_eye_examination(
    env: &Env,
    caller: &Address,
    record_id: u64,
    visual_acuity: VisualAcuity,
    iop: IntraocularPressure,
    slit_lamp: SlitLampFindings,
    visual_field: OptVisualField,
    retina_imaging: OptRetinalImaging,
    fundus_photo: OptFundusPhotography,
    clinical_notes: String,
) -> Result<(), ContractError> {
    circuit_breaker::require_not_paused(env, &PauseScope::Global)?;
    caller.require_auth();

    let record = VisionRecordsContract::get_record_raw(env, record_id)?;

    let has_perm = if caller == &record.provider {
        rbac::has_permission(env, caller, &Permission::WriteRecord)
    } else {
        rbac::has_delegated_permission(env, &record.provider, caller, &Permission::WriteRecord)
    };

    if !has_perm && !rbac::has_permission(env, caller, &Permission::SystemAdmin) {
        return VisionRecordsContract::unauthorized(
            env,
            caller,
            "add_eye_examination",
            "permission:WriteRecord_or_SystemAdmin",
        );
    }

    if record.record_type != RecordType::Examination {
        return Err(ContractError::InvalidRecordType);
    }

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

    set_examination(env, &exam, caller)?;

    AuditManager::log_event(
        env,
        caller.clone(),
        "examination.add",
        soroban_sdk::String::from_str(env, &record_id.to_string()),
        "ok",
    );

    events::publish_examination_added(env, record_id, caller.clone(), record.patient.clone());

    Ok(())
}

/// Standalone handler to update eye examination details using optimistic concurrency control (OCC).
#[allow(clippy::too_many_arguments)]
pub fn update_examination_versioned(
    env: &Env,
    caller: &Address,
    record_id: u64,
    expected_version: u64,
    node_id: u32,
    visual_acuity: VisualAcuity,
    iop: IntraocularPressure,
    slit_lamp: SlitLampFindings,
    visual_field: OptVisualField,
    retina_imaging: OptRetinalImaging,
    fundus_photo: OptFundusPhotography,
    clinical_notes: String,
    changed_fields: Vec<FieldChange>,
) -> Result<UpdateOutcome, ContractError> {
    circuit_breaker::require_not_paused(env, &PauseScope::Global)?;
    caller.require_auth();

    let record = VisionRecordsContract::get_record_raw(env, record_id)?;

    let has_perm = if caller == &record.provider {
        rbac::has_permission(env, caller, &Permission::WriteRecord)
    } else {
        rbac::has_delegated_permission(env, &record.provider, caller, &Permission::WriteRecord)
    };

    if !has_perm && !rbac::has_permission(env, caller, &Permission::SystemAdmin) {
        return VisionRecordsContract::unauthorized(
            env,
            caller,
            "update_examination_versioned",
            "permission:WriteRecord_or_SystemAdmin",
        );
    }

    if record.record_type != RecordType::Examination {
        return Err(ContractError::InvalidRecordType);
    }

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

    let outcome = versioned_set_examination(
        env,
        &exam,
        expected_version,
        node_id,
        caller,
        &changed_fields,
    )?;

    AuditManager::log_event(
        env,
        caller.clone(),
        "examination.update",
        soroban_sdk::String::from_str(env, &record_id.to_string()),
        match &outcome {
            UpdateOutcome::Applied(_) => "applied",
            UpdateOutcome::Merged(_) => "merged",
            UpdateOutcome::Conflicted(_) => "conflicted",
        },
    );

    let outcome_sym = update_outcome_to_symbol(&outcome);
    events::publish_examination_updated(env, record_id, caller.clone(), outcome_sym);

    Ok(outcome)
}

/// Standalone handler to retrieve eye examination details for a record
pub fn get_eye_examination(
    env: &Env,
    caller: &Address,
    record_id: u64,
) -> Result<EyeExamination, ContractError> {
    caller.require_auth();
    let record = VisionRecordsContract::get_record_raw(env, record_id)?;

    let has_perm = has_examination_read_access(
        env,
        caller,
        record_id,
        &record.patient,
        &record.provider,
    );

    if !has_perm {
        return VisionRecordsContract::access_denied(
            env,
            caller,
            "get_eye_examination",
            "record_read_access",
        );
    }

    get_examination(env, record_id).ok_or(ContractError::RecordNotFound)
}

pub fn get_record_history(
    env: &Env,
    caller: &Address,
    record_id: u64,
) -> Result<Vec<ExaminationVersion>, ContractError> {
    caller.require_auth();
    let record = VisionRecordsContract::get_record_raw(env, record_id)?;

    if !has_examination_read_access(env, caller, record_id, &record.patient, &record.provider) {
        return VisionRecordsContract::access_denied(
            env,
            caller,
            "get_record_history",
            "record_read_access",
        );
    }

    let count_key = exam_history_count_key(record_id);
    let count: u64 = env
        .storage()
        .persistent()
        .get(&count_key)
        .unwrap_or(0);
    if count == 0 {
        return Err(ContractError::RecordNotFound);
    }
    env.storage()
        .persistent()
        .extend_ttl(&count_key, TTL_THRESHOLD, TTL_EXTEND_TO);
    let latest_hash_key = exam_history_hash_key(record_id);
    env.storage()
        .persistent()
        .extend_ttl(&latest_hash_key, TTL_THRESHOLD, TTL_EXTEND_TO);

    let mut history = Vec::new(env);
    for version in 1..=count {
        let version_key = exam_history_key(record_id, version);
        let entry: ExaminationVersion = env
            .storage()
            .persistent()
            .get(&version_key)
            .ok_or(ContractError::RecordNotFound)?;
        env.storage()
            .persistent()
            .extend_ttl(&version_key, TTL_THRESHOLD, TTL_EXTEND_TO);
        history.push_back(entry);
    }
    Ok(history)
}

fn has_examination_read_access(
    env: &Env,
    caller: &Address,
    record_id: u64,
    patient: &Address,
    provider: &Address,
) -> bool {
    if caller == patient || caller == provider {
        return true;
    }

    let access =
        VisionRecordsContract::check_access(env.clone(), patient.clone(), caller.clone());
    let record_access =
        VisionRecordsContract::check_record_access(env.clone(), record_id, caller.clone());
    access == AccessLevel::Read
        || access == AccessLevel::Write
        || access == AccessLevel::Full
        || access == AccessLevel::Admin
        || record_access != AccessLevel::None
        || rbac::has_permission(env, caller, &Permission::SystemAdmin)
}

pub fn transition_exam_state(
    env: &Env,
    record_id: u64,
    to_state: LifecycleState,
    ctx: TransitionContext,
) -> Result<TransitionRecord, state_machine::StateMachineError> {
    let result = state_machine::apply_transition(
        env,
        0,
        &EntityKind::VisionRecord,
        record_id,
        to_state,
        ctx,
    )?;
    let from_sym = lifecycle_state_to_symbol(&result.from_state);
    let to_sym = lifecycle_state_to_symbol(&result.to_state);
    events::publish_examination_state_transitioned(
        env,
        record_id,
        result.actor.clone(),
        from_sym,
        to_sym,
    );
    Ok(result)
}
