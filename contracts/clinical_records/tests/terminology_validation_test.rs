//! Clinical terminology validation tests for the clinical records contract.
//!
//! The terminology checks are advisory: a value that is unusual, formatted
//! unexpectedly, or written in unaccepted terminology produces a warning and is
//! still stored.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
#![cfg(test)]

extern crate std;

use clinical_records::{
    events::EyeExaminationWarningEvent, ClinicalRecordsContract, ClinicalRecordsContractClient,
    ClinicalWarning, IntraocularPressure, OptFundusPhotography, OptPhysicalMeasurement,
    OptRetinalImaging, OptVisualField, PhysicalMeasurement, SlitLampFindings, VisualAcuity,
};
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Env, String, Symbol, Vec,
};

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
    client.initialize(&admin, &vision_records, &None);

    (env, client, admin, vision_records)
}

fn acuity(env: &Env, uncorrected_left: &str, uncorrected_right: &str) -> VisualAcuity {
    VisualAcuity {
        uncorrected: PhysicalMeasurement {
            left_eye: String::from_str(env, uncorrected_left),
            right_eye: String::from_str(env, uncorrected_right),
        },
        corrected: OptPhysicalMeasurement::None,
    }
}

fn iop(env: &Env, left: u32, right: u32) -> IntraocularPressure {
    IntraocularPressure {
        left_eye: left,
        right_eye: right,
        method: String::from_str(env, "Goldmann Applanation"),
        timestamp: 1_700_000_000,
    }
}

fn slit_lamp(env: &Env) -> SlitLampFindings {
    SlitLampFindings {
        cornea: String::from_str(env, "Clear"),
        anterior_chamber: String::from_str(env, "Deep and quiet"),
        iris: String::from_str(env, "Normal architecture"),
        lens: String::from_str(env, "Clear, no cataract"),
    }
}

fn warning_codes(env: &Env, warnings: &Vec<ClinicalWarning>) -> Vec<Symbol> {
    let mut codes = Vec::new(env);
    for warning in warnings.iter() {
        codes.push_back(warning.code);
    }
    codes
}

fn warning_fields(env: &Env, warnings: &Vec<ClinicalWarning>) -> Vec<String> {
    let mut fields = Vec::new(env);
    for warning in warnings.iter() {
        fields.push_back(warning.field);
    }
    fields
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Accepted terminology produces no warnings
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_accepted_terminology_produces_no_warnings() {
    let (env, client, _admin, _vision_records) = setup_clinical();

    let warnings = client.validate_examination_terminology(
        &acuity(&env, "20/20", "20/20"),
        &iop(&env, 16, 15),
        &slit_lamp(&env),
    );

    assert_eq!(warnings.len(), 0);
}

#[test]
fn test_valid_visual_acuity_notation_produces_no_warnings() {
    let (env, client, _admin, _vision_records) = setup_clinical();

    for notation in ["20/20", "6/6", "20.5/20", "1/0.5", "6/5"] {
        let warnings = client.validate_examination_terminology(
            &acuity(&env, notation, notation),
            &iop(&env, 12, 21),
            &slit_lamp(&env),
        );
        assert_eq!(
            warnings.len(),
            0,
            "notation {} should be accepted",
            notation
        );
    }
}

#[test]
fn test_iop_at_the_range_boundaries_produces_no_warnings() {
    let (env, client, _admin, _vision_records) = setup_clinical();

    let warnings = client.validate_examination_terminology(
        &acuity(&env, "20/20", "20/20"),
        &iop(&env, 10, 21),
        &slit_lamp(&env),
    );

    assert_eq!(warnings.len(), 0);
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Intraocular pressure outside the normal range warns but never rejects
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_out_of_range_iop_warns_for_each_eye() {
    let (env, client, _admin, _vision_records) = setup_clinical();

    let warnings = client.validate_examination_terminology(
        &acuity(&env, "20/20", "20/20"),
        &iop(&env, 9, 22),
        &slit_lamp(&env),
    );

    assert_eq!(warnings.len(), 2);

    let codes = warning_codes(&env, &warnings);
    assert_eq!(codes.get(0).unwrap(), Symbol::new(&env, "IOP_OUT_OF_RANGE"));
    assert_eq!(codes.get(1).unwrap(), Symbol::new(&env, "IOP_OUT_OF_RANGE"));

    let fields = warning_fields(&env, &warnings);
    assert_eq!(
        fields.get(0).unwrap(),
        String::from_str(&env, "iop.left_eye")
    );
    assert_eq!(
        fields.get(1).unwrap(),
        String::from_str(&env, "iop.right_eye")
    );

    // The message states the recorded value and the normal range.
    assert_eq!(
        warnings.get(0).unwrap().detail,
        String::from_str(&env, "IOP 9 mmHg is outside the normal range 10-21 mmHg")
    );
    assert_eq!(
        warnings.get(1).unwrap().detail,
        String::from_str(&env, "IOP 22 mmHg is outside the normal range 10-21 mmHg")
    );
}

#[test]
fn test_out_of_range_iop_is_still_stored() {
    let (env, client, _admin, vision_records) = setup_clinical();
    let record_id = 7001u64;

    let res = client.try_add_eye_examination(
        &vision_records,
        &record_id,
        &acuity(&env, "20/20", "20/20"),
        &iop(&env, 45, 3),
        &slit_lamp(&env),
        &OptVisualField::None,
        &OptRetinalImaging::None,
        &OptFundusPhotography::None,
        &String::from_str(&env, "High pressure follow up"),
    );
    assert!(res.is_ok(), "an out of range IOP must not be rejected");

    let stored = client.get_eye_examination(&vision_records, &record_id);
    assert_eq!(stored.iop.left_eye, 45);
    assert_eq!(stored.iop.right_eye, 3);
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Visual acuity format
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_malformed_visual_acuity_notation_warns() {
    let (env, client, _admin, _vision_records) = setup_clinical();

    let visual_acuity = VisualAcuity {
        uncorrected: PhysicalMeasurement {
            left_eye: String::from_str(&env, "20-20"),
            right_eye: String::from_str(&env, "N/A"),
        },
        corrected: OptPhysicalMeasurement::Some(PhysicalMeasurement {
            left_eye: String::from_str(&env, "PL"),
            right_eye: String::from_str(&env, "twenty twenty"),
        }),
    };

    let warnings = client.validate_examination_terminology(
        &visual_acuity,
        &iop(&env, 16, 15),
        &slit_lamp(&env),
    );

    assert_eq!(warnings.len(), 4);

    let codes = warning_codes(&env, &warnings);
    for index in 0u32..4 {
        assert_eq!(
            codes.get(index).unwrap(),
            Symbol::new(&env, "VISUAL_ACUITY_FORMAT")
        );
    }

    let fields = warning_fields(&env, &warnings);
    assert_eq!(
        fields.get(0).unwrap(),
        String::from_str(&env, "visual_acuity.uncorrected.left_eye")
    );
    assert_eq!(
        fields.get(1).unwrap(),
        String::from_str(&env, "visual_acuity.uncorrected.right_eye")
    );
    assert_eq!(
        fields.get(2).unwrap(),
        String::from_str(&env, "visual_acuity.corrected.left_eye")
    );
    assert_eq!(
        fields.get(3).unwrap(),
        String::from_str(&env, "visual_acuity.corrected.right_eye")
    );
}

#[test]
fn test_corrected_visual_acuity_is_only_checked_when_present() {
    let (env, client, _admin, _vision_records) = setup_clinical();

    // Corrected acuity is absent, so only the two uncorrected values are checked.
    let warnings = client.validate_examination_terminology(
        &acuity(&env, "20-20", "20/20"),
        &iop(&env, 16, 15),
        &slit_lamp(&env),
    );

    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warning_fields(&env, &warnings).get(0).unwrap(),
        String::from_str(&env, "visual_acuity.uncorrected.left_eye")
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Slit lamp terminology
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_accepted_slit_lamp_terminology_produces_no_warnings() {
    let (env, client, _admin, _vision_records) = setup_clinical();

    let findings = SlitLampFindings {
        cornea: String::from_str(&env, "clear"),
        anterior_chamber: String::from_str(&env, "Deep and quiet"),
        iris: String::from_str(&env, "Normal architecture"),
        lens: String::from_str(&env, "Clear, no cataract"),
    };

    let warnings = client.validate_examination_terminology(
        &acuity(&env, "20/20", "20/20"),
        &iop(&env, 16, 15),
        &findings,
    );

    assert_eq!(warnings.len(), 0);
}

#[test]
fn test_unknown_slit_lamp_term_warns() {
    let (env, client, _admin, _vision_records) = setup_clinical();

    let findings = SlitLampFindings {
        cornea: String::from_str(&env, "wobbly"),
        anterior_chamber: String::from_str(&env, "Deep and quiet"),
        iris: String::from_str(&env, "Normal architecture"),
        lens: String::from_str(&env, "Clear, no cataract"),
    };

    let warnings = client.validate_examination_terminology(
        &acuity(&env, "20/20", "20/20"),
        &iop(&env, 16, 15),
        &findings,
    );

    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warning_codes(&env, &warnings).get(0).unwrap(),
        Symbol::new(&env, "SLIT_LAMP_TERM_UNKNOWN")
    );
    assert_eq!(
        warning_fields(&env, &warnings).get(0).unwrap(),
        String::from_str(&env, "slit_lamp.cornea")
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Warning event
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_warning_event_is_published_only_when_warnings_exist() {
    use soroban_sdk::xdr::ContractEventBody;
    use soroban_sdk::{symbol_short, IntoVal, TryFromVal};

    let (env, client, _admin, vision_records) = setup_clinical();

    // A clean examination only publishes the examination-added event.
    client.add_eye_examination(
        &vision_records,
        &8001u64,
        &acuity(&env, "20/20", "20/20"),
        &iop(&env, 16, 15),
        &slit_lamp(&env),
        &OptVisualField::None,
        &OptRetinalImaging::None,
        &OptFundusPhotography::None,
        &String::from_str(&env, "Routine exam"),
    );

    let binding = env.events().all();
    let all_events = binding.events();
    assert_eq!(all_events.len(), 1);

    // An examination with warnings publishes an additional warning event.
    client.add_eye_examination(
        &vision_records,
        &8002u64,
        &acuity(&env, "20/20", "20/20"),
        &iop(&env, 44, 15),
        &slit_lamp(&env),
        &OptVisualField::None,
        &OptRetinalImaging::None,
        &OptFundusPhotography::None,
        &String::from_str(&env, "High pressure follow up"),
    );

    let binding = env.events().all();
    let all_events = binding.events();
    // The examination-added event plus the warning event.
    assert_eq!(all_events.len(), 2);

    let last_event = all_events.last().unwrap();
    let ContractEventBody::V0(warn_body) = &last_event.body;
    assert_eq!(
        warn_body.topics.first().unwrap(),
        &symbol_short!("EXAM_WARN").into_val(&env)
    );

    let warn_data = EyeExaminationWarningEvent::try_from_val(&env, &warn_body.data).unwrap();
    assert_eq!(warn_data.record_id, 8002u64);
    assert_eq!(warn_data.warnings.len(), 1);
    assert_eq!(
        warn_data.warnings.get(0).unwrap().code,
        Symbol::new(&env, "IOP_OUT_OF_RANGE")
    );
    assert_eq!(
        warn_data.warnings.get(0).unwrap().field,
        String::from_str(&env, "iop.left_eye")
    );
}
