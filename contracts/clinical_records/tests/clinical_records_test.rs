#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
#![cfg(test)]

extern crate std;

use clinical_records::{
    ClinicalRecordsContract, ClinicalRecordsContractClient, ContractError, EyeExamination,
    FundusPhotography, IntraocularPressure, LensType, OptFundusPhotography, OptPhysicalMeasurement,
    OptRetinalImaging, OptVisualField, PhysicalMeasurement, Prescription, PrescriptionData,
    RetinalImaging, SlitLampFindings, VisualAcuity, VisualField,
};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup_clinical() -> (
    Env,
    ClinicalRecordsContractClient<'static>,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vision_records = Address::generate(&env);

    let contract_id = env.register(ClinicalRecordsContract, ());
    let client = ClinicalRecordsContractClient::new(&env, &contract_id);
    client.initialize(&admin, &vision_records);

    (env, client, admin, vision_records)
}

fn sample_visual_acuity_full(env: &Env) -> VisualAcuity {
    VisualAcuity {
        uncorrected: PhysicalMeasurement {
            left_eye: String::from_str(env, "20/40"),
            right_eye: String::from_str(env, "20/30"),
        },
        corrected: OptPhysicalMeasurement::Some(PhysicalMeasurement {
            left_eye: String::from_str(env, "20/20"),
            right_eye: String::from_str(env, "20/20"),
        }),
    }
}

fn sample_visual_acuity_uncorrected_only(env: &Env) -> VisualAcuity {
    VisualAcuity {
        uncorrected: PhysicalMeasurement {
            left_eye: String::from_str(env, "20/20"),
            right_eye: String::from_str(env, "20/20"),
        },
        corrected: OptPhysicalMeasurement::None,
    }
}

fn sample_iop(env: &Env) -> IntraocularPressure {
    IntraocularPressure {
        left_eye: 16,
        right_eye: 15,
        method: String::from_str(env, "Goldmann Applanation"),
        timestamp: 1_700_000_000,
    }
}

fn sample_slit_lamp(env: &Env) -> SlitLampFindings {
    SlitLampFindings {
        cornea: String::from_str(env, "Clear"),
        anterior_chamber: String::from_str(env, "Deep and quiet"),
        iris: String::from_str(env, "Normal architecture"),
        lens: String::from_str(env, "Clear, no cataract"),
    }
}

fn sample_prescription_data(env: &Env) -> PrescriptionData {
    PrescriptionData {
        sphere: String::from_str(env, "-2.25"),
        cylinder: String::from_str(env, "-0.75"),
        axis: String::from_str(env, "180"),
        add: String::from_str(env, "+1.50"),
        pd: String::from_str(env, "63"),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Initialization and Admin Functions
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_initialization_lifecycle_and_admin_config() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ClinicalRecordsContract, ());
    let client = ClinicalRecordsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let vision_records = Address::generate(&env);

    // Initial configuration succeeds
    let init_res = client.try_initialize(&admin, &vision_records);
    assert!(init_res.is_ok());

    assert_eq!(client.get_admin(), admin);
    assert_eq!(
        client.get_vision_records_contract(),
        Some(vision_records.clone())
    );

    // Duplicate initialization must return AlreadyInitialized
    let dup_init = client.try_initialize(&admin, &vision_records);
    assert_eq!(dup_init, Err(Ok(ContractError::AlreadyInitialized)));

    // Admin updates vision_records contract address
    let new_vr = Address::generate(&env);
    let update_res = client.try_set_vision_records_contract(&admin, &new_vr);
    assert!(update_res.is_ok());
    assert_eq!(client.get_vision_records_contract(), Some(new_vr.clone()));

    // Non-admin attempting to update vision_records contract address fails
    let unauthorized_caller = Address::generate(&env);
    let unauth_res = client.try_set_vision_records_contract(&unauthorized_caller, &vision_records);
    assert_eq!(unauth_res, Err(Ok(ContractError::Unauthorized)));
}

#[test]
fn test_uninitialized_calls_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ClinicalRecordsContract, ());
    let client = ClinicalRecordsContractClient::new(&env, &contract_id);

    assert_eq!(
        client.try_get_admin(),
        Err(Ok(ContractError::NotInitialized))
    );
    assert_eq!(client.get_vision_records_contract(), None);

    let caller = Address::generate(&env);
    let exam_res = client.try_get_eye_examination(&caller, &1);
    assert_eq!(exam_res, Err(Ok(ContractError::NotInitialized)));

    let rx_res = client.try_get_prescription(&caller, &1);
    assert_eq!(rx_res, Err(Ok(ContractError::NotInitialized)));
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Eye Examination CRUD & Optional Fields (None / Some Variants)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_eye_examination_crud_with_all_some_fields() {
    let (env, client, _admin, vision_records) = setup_clinical();
    let record_id = 1001u64;

    let visual_field = OptVisualField::Some(VisualField {
        left_eye_reliability: String::from_str(&env, "Good"),
        right_eye_reliability: String::from_str(&env, "Good"),
        left_eye_defects: String::from_str(&env, "None"),
        right_eye_defects: String::from_str(&env, "None"),
    });

    let retina_imaging = OptRetinalImaging::Some(RetinalImaging {
        image_url: String::from_str(&env, "https://storage.retinax.io/oct/1001.png"),
        image_hash: String::from_str(&env, "0xabcd1234"),
        findings: String::from_str(&env, "Normal macula, intact nerve fiber layer"),
    });

    let fundus_photo = OptFundusPhotography::Some(FundusPhotography {
        image_url: String::from_str(&env, "https://storage.retinax.io/fundus/1001.png"),
        image_hash: String::from_str(&env, "0xfeed5678"),
        cup_to_disc_ratio_left: String::from_str(&env, "0.3"),
        cup_to_disc_ratio_right: String::from_str(&env, "0.3"),
        macula_status: String::from_str(&env, "Healthy"),
    });

    let notes = String::from_str(&env, "Routine comprehensive exam complete.");

    // Store eye examination
    let add_res = client.try_add_eye_examination(
        &vision_records,
        &record_id,
        &sample_visual_acuity_full(&env),
        &sample_iop(&env),
        &sample_slit_lamp(&env),
        &visual_field,
        &retina_imaging,
        &fundus_photo,
        &notes,
    );
    assert!(add_res.is_ok());

    // Retrieve and verify
    let stored: EyeExamination = client.get_eye_examination(&vision_records, &record_id);
    assert_eq!(stored.record_id, record_id);
    assert_eq!(stored.clinical_notes, notes);
    assert_eq!(stored.visual_field, visual_field);
    assert_eq!(stored.retina_imaging, retina_imaging);
    assert_eq!(stored.fundus_photo, fundus_photo);
}

#[test]
fn test_eye_examination_crud_with_all_none_fields() {
    let (env, client, _admin, vision_records) = setup_clinical();
    let record_id = 1002u64;

    let notes = String::from_str(&env, "Basic exam without advanced imaging.");

    let add_res = client.try_add_eye_examination(
        &vision_records,
        &record_id,
        &sample_visual_acuity_uncorrected_only(&env),
        &sample_iop(&env),
        &sample_slit_lamp(&env),
        &OptVisualField::None,
        &OptRetinalImaging::None,
        &OptFundusPhotography::None,
        &notes,
    );
    assert!(add_res.is_ok());

    let stored = client.get_eye_examination(&vision_records, &record_id);
    assert_eq!(stored.record_id, record_id);
    assert_eq!(stored.visual_field, OptVisualField::None);
    assert_eq!(stored.retina_imaging, OptRetinalImaging::None);
    assert_eq!(stored.fundus_photo, OptFundusPhotography::None);
    assert_eq!(stored.visual_acuity.corrected, OptPhysicalMeasurement::None);
}

#[test]
fn test_eye_examination_unauthorized_and_not_found() {
    let (env, client, _admin, vision_records) = setup_clinical();
    let unauthorized_caller = Address::generate(&env);

    // Unauthorized add_eye_examination
    let add_res = client.try_add_eye_examination(
        &unauthorized_caller,
        &1,
        &sample_visual_acuity_uncorrected_only(&env),
        &sample_iop(&env),
        &sample_slit_lamp(&env),
        &OptVisualField::None,
        &OptRetinalImaging::None,
        &OptFundusPhotography::None,
        &String::from_str(&env, "Notes"),
    );
    assert_eq!(add_res, Err(Ok(ContractError::Unauthorized)));

    // Unauthorized get_eye_examination
    let get_res = client.try_get_eye_examination(&unauthorized_caller, &1);
    assert_eq!(get_res, Err(Ok(ContractError::Unauthorized)));

    // RecordNotFound when querying non-existent record
    let not_found_res = client.try_get_eye_examination(&vision_records, &9999);
    assert_eq!(not_found_res, Err(Ok(ContractError::RecordNotFound)));
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Prescription 2-Phase Commit Full Lifecycle
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_prescription_two_phase_commit_lifecycle() {
    let (env, client, _admin, vision_records) = setup_clinical();
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);
    let rx_data = sample_prescription_data(&env);

    assert_eq!(client.get_prescription_count(), 0);

    // Phase 1: Prepare
    let rx_id = client.prepare_add_prescription(&vision_records, &patient, &provider, &rx_data);
    assert_eq!(rx_id, 1);
    assert_eq!(client.get_prescription_count(), 1);

    // Prescription cannot be queried before commit
    let pre_commit_res = client.try_get_prescription(&vision_records, &rx_id);
    assert_eq!(pre_commit_res, Err(Ok(ContractError::RecordNotFound)));

    // Phase 2: Commit
    let commit_res = client.try_commit_add_prescription(&vision_records, &rx_id);
    assert!(commit_res.is_ok());

    // Verified stored prescription
    let prescription: Prescription = client.get_prescription(&vision_records, &rx_id);
    assert_eq!(prescription.id, rx_id);
    assert_eq!(prescription.patient, patient);
    assert_eq!(prescription.provider, provider);
    assert_eq!(prescription.lens_type, LensType::Glasses);
    assert_eq!(prescription.left_eye, rx_data);
    assert_eq!(prescription.right_eye, rx_data);
    assert!(!prescription.verified);
}

#[test]
fn test_prescription_double_commit_prevention() {
    let (env, client, _admin, vision_records) = setup_clinical();
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);
    let rx_data = sample_prescription_data(&env);

    let rx_id = client.prepare_add_prescription(&vision_records, &patient, &provider, &rx_data);

    // First commit succeeds
    assert!(client
        .try_commit_add_prescription(&vision_records, &rx_id)
        .is_ok());

    // Second commit must fail with InvalidPhase
    let double_commit = client.try_commit_add_prescription(&vision_records, &rx_id);
    assert_eq!(double_commit, Err(Ok(ContractError::InvalidPhase)));
}

#[test]
fn test_prescription_rollback_lifecycle() {
    let (env, client, _admin, vision_records) = setup_clinical();
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);
    let rx_data = sample_prescription_data(&env);

    let rx_id = client.prepare_add_prescription(&vision_records, &patient, &provider, &rx_data);

    // Rollback phase 1 preparation
    let rollback_res = client.try_rollback_add_prescription(&vision_records, &rx_id);
    assert!(rollback_res.is_ok());

    // Attempting to commit after rollback must fail with InvalidPhase
    let commit_after_rollback = client.try_commit_add_prescription(&vision_records, &rx_id);
    assert_eq!(commit_after_rollback, Err(Ok(ContractError::InvalidPhase)));

    // Prescription is not in persistent storage
    assert_eq!(
        client.try_get_prescription(&vision_records, &rx_id),
        Err(Ok(ContractError::RecordNotFound))
    );
}

#[test]
fn test_prescription_rollback_after_commit_does_not_destroy_committed_data() {
    let (env, client, _admin, vision_records) = setup_clinical();
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);
    let rx_data = sample_prescription_data(&env);

    let rx_id = client.prepare_add_prescription(&vision_records, &patient, &provider, &rx_data);
    client.commit_add_prescription(&vision_records, &rx_id);

    // Calling rollback after commit cleans up or succeeds idempotently
    assert!(client
        .try_rollback_add_prescription(&vision_records, &rx_id)
        .is_ok());

    // Committed prescription remains intact in persistent storage
    let fetched = client.get_prescription(&vision_records, &rx_id);
    assert_eq!(fetched.id, rx_id);
}

#[test]
fn test_prescription_unauthorized_attempts() {
    let (env, client, _admin, _vr) = setup_clinical();
    let unauthorized_caller = Address::generate(&env);
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);
    let rx_data = sample_prescription_data(&env);

    assert_eq!(
        client.try_prepare_add_prescription(&unauthorized_caller, &patient, &provider, &rx_data),
        Err(Ok(ContractError::Unauthorized))
    );

    assert_eq!(
        client.try_commit_add_prescription(&unauthorized_caller, &1),
        Err(Ok(ContractError::Unauthorized))
    );

    assert_eq!(
        client.try_rollback_add_prescription(&unauthorized_caller, &1),
        Err(Ok(ContractError::Unauthorized))
    );

    assert_eq!(
        client.try_get_prescription(&unauthorized_caller, &1),
        Err(Ok(ContractError::Unauthorized))
    );
}

#[test]
fn test_input_validation_errors() {
    let (env, client, _admin, vision_records) = setup_clinical();
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    // Empty sphere value (< 1 char)
    let invalid_rx_data = PrescriptionData {
        sphere: String::from_str(&env, ""),
        cylinder: String::from_str(&env, "-1.00"),
        axis: String::from_str(&env, "90"),
        add: String::from_str(&env, "0.00"),
        pd: String::from_str(&env, "60"),
    };

    let res =
        client.try_prepare_add_prescription(&vision_records, &patient, &provider, &invalid_rx_data);
    assert_eq!(res, Err(Ok(ContractError::InvalidInput)));
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Schema Version Tracking, Migration, and Upgrade Mechanisms (#280)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_schema_migration_lifecycle() {
    let (env, client, admin, _vision_records) = setup_clinical();

    // A fresh contract starts at the initial schema version
    assert_eq!(client.get_schema_version(), 1);

    // Migrate from v1 to v2 succeeds
    assert_eq!(client.migrate(&admin, &2), ());
    assert_eq!(client.get_schema_version(), 2);

    // Migrating to the same version returns AlreadyMigrated
    assert_eq!(
        client.try_migrate(&admin, &2),
        Err(Ok(ContractError::AlreadyMigrated))
    );

    // Migrating to a lower version returns InvalidVersion
    assert_eq!(
        client.try_migrate(&admin, &1),
        Err(Ok(ContractError::InvalidVersion))
    );

    // Non-admin migration is rejected
    let non_admin = Address::generate(&env);
    assert_eq!(
        client.try_migrate(&non_admin, &3),
        Err(Ok(ContractError::Unauthorized))
    );
}

#[test]
fn test_backwards_compatible_reads_and_lazy_migration() {
    let (env, client, admin, vision_records) = setup_clinical();

    // 1. Add exam and prescription while on v1
    assert_eq!(client.get_schema_version(), 1);
    let va = sample_visual_acuity_full(&env);
    let iop = sample_iop(&env);
    let slit = sample_slit_lamp(&env);
    let notes = String::from_str(&env, "Pre-upgrade examination notes");

    client.add_eye_examination(
        &vision_records,
        &101,
        &va,
        &iop,
        &slit,
        &OptVisualField::None,
        &OptRetinalImaging::None,
        &OptFundusPhotography::None,
        &notes,
    );

    assert_eq!(client.get_examination_version(&vision_records, &101), 1);

    let patient = Address::generate(&env);
    let provider = Address::generate(&env);
    let rx_data = sample_prescription_data(&env);

    let rx_id = client.prepare_add_prescription(&vision_records, &patient, &provider, &rx_data);
    client.commit_add_prescription(&vision_records, &rx_id);

    assert_eq!(client.get_prescription_version(&vision_records, &rx_id), 1);

    // 2. Schema version upgraded to v2 via migrate
    client.migrate(&admin, &2);
    assert_eq!(client.get_schema_version(), 2);

    // 3. Backwards-compatible reads succeed without error
    let exam_v1 = client.get_eye_examination(&vision_records, &101);
    assert_eq!(exam_v1.record_id, 101);
    assert_eq!(exam_v1.clinical_notes, notes);

    // 4. Verify lazy migration pattern: reading updated the stored record version to v2
    assert_eq!(client.get_examination_version(&vision_records, &101), 2);

    // 5. Prescription backwards-compatible read and lazy migration
    let rx_v1 = client.get_prescription(&vision_records, &rx_id);
    assert_eq!(rx_v1.id, rx_id);
    assert_eq!(rx_v1.patient, patient);
    assert_eq!(client.get_prescription_version(&vision_records, &rx_id), 2);
}

#[test]
fn test_explicit_record_migration() {
    let (env, client, admin, vision_records) = setup_clinical();
    let va = sample_visual_acuity_full(&env);
    let iop = sample_iop(&env);
    let slit = sample_slit_lamp(&env);

    client.add_eye_examination(
        &vision_records,
        &201,
        &va,
        &iop,
        &slit,
        &OptVisualField::None,
        &OptRetinalImaging::None,
        &OptFundusPhotography::None,
        &String::from_str(&env, "Test exam"),
    );

    // A record cannot move past the contract schema version
    assert_eq!(
        client.try_migrate_examination(&vision_records, &201, &3),
        Err(Ok(ContractError::InvalidVersion))
    );

    // Explicit migration to v3 once the contract schema is v3
    client.migrate(&admin, &3);
    assert_eq!(client.migrate_examination(&vision_records, &201, &3), 3);
    assert_eq!(client.get_examination_version(&vision_records, &201), 3);

    // Already migrated
    assert_eq!(
        client.try_migrate_examination(&vision_records, &201, &3),
        Err(Ok(ContractError::AlreadyMigrated))
    );

    // Non-existent record migration returns RecordNotFound
    assert_eq!(
        client.try_migrate_examination(&vision_records, &9999, &2),
        Err(Ok(ContractError::RecordNotFound))
    );

    // Unauthorized caller rejected
    let unauthorized = Address::generate(&env);
    assert_eq!(
        client.try_migrate_examination(&unauthorized, &201, &4),
        Err(Ok(ContractError::Unauthorized))
    );
}

#[test]
fn test_contract_upgrade_auth_check() {
    let (env, client, admin, _vision_records) = setup_clinical();

    // A real build of this contract, so the host accepts the upload.
    // Rebuild with: cargo build -p clinical_records --target wasm32v1-none --release
    let wasm = include_bytes!("fixtures/clinical_records.wasm");
    let wasm_hash = env
        .deployer()
        .upload_contract_wasm(soroban_sdk::Bytes::from_slice(&env, wasm));

    // Non-admin upgrade rejected
    let non_admin = Address::generate(&env);
    assert_eq!(
        client.try_upgrade(&non_admin, &wasm_hash),
        Err(Ok(ContractError::Unauthorized))
    );

    // Admin upgrade succeeds and storage survives it
    assert!(client.try_upgrade(&admin, &wasm_hash).is_ok());
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_schema_version(), 1);
}
