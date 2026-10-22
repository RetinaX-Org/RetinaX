#![allow(clippy::unwrap_used, clippy::expect_used)]
#![cfg(test)]

extern crate std;

use access_control::{AccessControlContract, AccessControlContractClient, Role};
use clinical_records::{
    session::MAX_SESSION_TTL_SECS, AuthLevel, ClinicalRecordsContract,
    ClinicalRecordsContractClient, ContractError, IntraocularPressure, OptFundusPhotography,
    OptPhysicalMeasurement, OptRetinalImaging, OptVisualField, PhysicalMeasurement,
    PrescriptionData, SlitLampFindings, VisualAcuity,
};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger as _},
    Address, ConversionError, Env, InvokeError, String,
};

const HOUR: u64 = 3_600;

struct Setup {
    env: Env,
    client: ClinicalRecordsContractClient<'static>,
    doctor: Address,
    patient: Address,
}

/// Clinical records wired to access_control, with an Optometrist (`doctor`) and a `patient`.
fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let doctor = Address::generate(&env);
    let patient = Address::generate(&env);

    let ac_id = env.register(AccessControlContract, ());
    let ac_client = AccessControlContractClient::new(&env, &ac_id);
    ac_client.initialize(&admin);
    ac_client.assign_role(&admin, &doctor, &Role::Optometrist, &0);
    ac_client.assign_role(&admin, &patient, &Role::Patient, &0);

    let cr_id = env.register(ClinicalRecordsContract, ());
    let client = ClinicalRecordsContractClient::new(&env, &cr_id);
    client.initialize(&admin, &Address::generate(&env), &Some(ac_id));

    Setup {
        env,
        client,
        doctor,
        patient,
    }
}

type TryResult = Result<Result<(), ConversionError>, Result<ContractError, InvokeError>>;

fn add_exam(s: &Setup, caller: &Address, record_id: u64) -> TryResult {
    let env = &s.env;
    s.client.try_add_eye_examination(
        caller,
        &record_id,
        &VisualAcuity {
            uncorrected: PhysicalMeasurement {
                left_eye: String::from_str(env, "20/20"),
                right_eye: String::from_str(env, "20/25"),
            },
            corrected: OptPhysicalMeasurement::None,
        },
        &IntraocularPressure {
            left_eye: 15,
            right_eye: 16,
            method: String::from_str(env, "Goldmann"),
            timestamp: 1_000,
        },
        &SlitLampFindings {
            cornea: String::from_str(env, "Clear"),
            anterior_chamber: String::from_str(env, "Deep and quiet"),
            iris: String::from_str(env, "Normal"),
            lens: String::from_str(env, "Clear"),
        },
        &OptVisualField::None,
        &OptRetinalImaging::None,
        &OptFundusPhotography::None,
        &String::from_str(env, "Session test"),
    )
}

fn rx_data(env: &Env) -> PrescriptionData {
    PrescriptionData {
        sphere: String::from_str(env, "-1.75"),
        cylinder: String::from_str(env, "-0.50"),
        axis: String::from_str(env, "90"),
        add: String::from_str(env, "+1.00"),
        pd: String::from_str(env, "62"),
    }
}

// ── Session keys act for their principal ─────────────────────────────────────

#[test]
fn session_key_writes_and_reads_examinations_as_principal() {
    let s = setup();
    let session_key = Address::generate(&s.env);

    let session = s
        .client
        .start_session(&s.doctor, &session_key, &AuthLevel::Level2, &HOUR);
    assert_eq!(session.principal, s.doctor);
    assert_eq!(session.session_key, session_key);
    assert_eq!(session.max_level, AuthLevel::Level2);
    assert_eq!(session.expires_at, session.issued_at + HOUR);
    assert_eq!(s.client.get_session(&session_key), Some(session));

    // The session key has no role of its own; it is authorized as the doctor.
    assert_eq!(add_exam(&s, &session_key, 1), Ok(Ok(())));
    s.client
        .set_examination_patient(&session_key, &1, &s.patient);
    assert_eq!(s.client.get_eye_examination(&session_key, &1).record_id, 1);
}

#[test]
fn session_key_must_sign_its_calls() {
    let s = setup();
    let session_key = Address::generate(&s.env);
    s.client
        .start_session(&s.doctor, &session_key, &AuthLevel::Level2, &HOUR);

    add_exam(&s, &session_key, 1).unwrap().unwrap();
    let signers: std::vec::Vec<Address> = s.env.auths().into_iter().map(|(a, _)| a).collect();
    assert_eq!(signers, std::vec![session_key]);
}

#[test]
fn session_inherits_principal_permissions_only() {
    let s = setup();
    let session_key = Address::generate(&s.env);

    // The patient has no WriteRecord permission, so neither does its session.
    s.client
        .start_session(&s.patient, &session_key, &AuthLevel::Level4, &HOUR);
    assert_eq!(
        add_exam(&s, &session_key, 1),
        Err(Ok(ContractError::Unauthorized))
    );
}

#[test]
fn patient_session_reads_own_examination() {
    let s = setup();
    add_exam(&s, &s.doctor, 7).unwrap().unwrap();
    s.client.set_examination_patient(&s.doctor, &7, &s.patient);

    let session_key = Address::generate(&s.env);
    s.client
        .start_session(&s.patient, &session_key, &AuthLevel::Level1, &HOUR);
    assert_eq!(s.client.get_eye_examination(&session_key, &7).record_id, 7);
}

// ── Sensitive operations need a higher level ─────────────────────────────────

#[test]
fn level1_session_can_read_but_not_write_examinations() {
    let s = setup();
    add_exam(&s, &s.doctor, 1).unwrap().unwrap();

    let session_key = Address::generate(&s.env);
    s.client
        .start_session(&s.doctor, &session_key, &AuthLevel::Level1, &HOUR);

    assert_eq!(s.client.get_eye_examination(&session_key, &1).record_id, 1);
    assert_eq!(
        add_exam(&s, &session_key, 2),
        Err(Ok(ContractError::InsufficientAuthLevel))
    );
    assert_eq!(
        s.client
            .try_set_examination_patient(&session_key, &1, &s.patient),
        Err(Ok(ContractError::InsufficientAuthLevel))
    );
}

#[test]
fn prescriptions_require_level3_session() {
    let s = setup();
    let session_key = Address::generate(&s.env);
    s.client
        .start_session(&s.doctor, &session_key, &AuthLevel::Level2, &HOUR);

    // Level 2 covers examinations but not prescriptions.
    assert_eq!(add_exam(&s, &session_key, 1), Ok(Ok(())));
    assert_eq!(
        s.client.try_prepare_add_prescription(
            &session_key,
            &s.patient,
            &s.doctor,
            &rx_data(&s.env)
        ),
        Err(Ok(ContractError::InsufficientAuthLevel))
    );

    let rx_id =
        s.client
            .prepare_add_prescription(&s.doctor, &s.patient, &s.doctor, &rx_data(&s.env));
    s.client.commit_add_prescription(&s.doctor, &rx_id);
    assert_eq!(
        s.client.try_get_prescription(&session_key, &rx_id),
        Err(Ok(ContractError::InsufficientAuthLevel))
    );
    assert_eq!(
        s.client.try_rollback_add_prescription(&session_key, &rx_id),
        Err(Ok(ContractError::InsufficientAuthLevel))
    );

    // Stepping the session up to Level 3 unlocks the full prescription flow.
    s.client
        .start_session(&s.doctor, &session_key, &AuthLevel::Level3, &HOUR);
    let rx_id =
        s.client
            .prepare_add_prescription(&session_key, &s.patient, &s.doctor, &rx_data(&s.env));
    s.client.commit_add_prescription(&session_key, &rx_id);
    let rx = s.client.get_prescription(&session_key, &rx_id);
    assert_eq!(rx.patient, s.patient);
    assert_eq!(rx.provider, s.doctor);
}

#[test]
fn prescription_batches_require_level3_session() {
    let s = setup();
    let session_key = Address::generate(&s.env);
    s.client
        .start_session(&s.doctor, &session_key, &AuthLevel::Level2, &HOUR);

    let rx_id =
        s.client
            .prepare_add_prescription(&s.doctor, &s.patient, &s.doctor, &rx_data(&s.env));
    let ids = soroban_sdk::vec![&s.env, rx_id];
    assert_eq!(
        s.client.try_commit_add_prescriptions(&session_key, &ids),
        Err(Ok(ContractError::InsufficientAuthLevel))
    );

    s.client
        .start_session(&s.doctor, &session_key, &AuthLevel::Level3, &HOUR);
    s.client.commit_add_prescriptions(&session_key, &ids);
    assert_eq!(s.client.get_prescription(&s.doctor, &rx_id).id, rx_id);
}

// ── Expiry and revocation ────────────────────────────────────────────────────

#[test]
fn expired_session_is_rejected_until_refreshed() {
    let s = setup();
    add_exam(&s, &s.doctor, 1).unwrap().unwrap();
    let session_key = Address::generate(&s.env);
    let session = s
        .client
        .start_session(&s.doctor, &session_key, &AuthLevel::Level3, &HOUR);

    // Still valid at the expiry second itself.
    s.env.ledger().set_timestamp(session.expires_at);
    assert_eq!(s.client.get_eye_examination(&session_key, &1).record_id, 1);

    s.env.ledger().set_timestamp(session.expires_at + 1);
    assert_eq!(
        s.client.try_get_eye_examination(&session_key, &1),
        Err(Ok(ContractError::SessionExpired))
    );
    assert_eq!(
        add_exam(&s, &session_key, 2),
        Err(Ok(ContractError::SessionExpired))
    );
    assert_eq!(
        s.client.try_prepare_add_prescription(
            &session_key,
            &s.patient,
            &s.doctor,
            &rx_data(&s.env)
        ),
        Err(Ok(ContractError::SessionExpired))
    );
    // The expired session is still visible for inspection.
    assert_eq!(s.client.get_session(&session_key), Some(session));

    s.client
        .start_session(&s.doctor, &session_key, &AuthLevel::Level3, &HOUR);
    assert_eq!(s.client.get_eye_examination(&session_key, &1).record_id, 1);
}

#[test]
fn revoked_session_key_loses_principal_permissions() {
    let s = setup();
    add_exam(&s, &s.doctor, 1).unwrap().unwrap();
    let session_key = Address::generate(&s.env);
    s.client
        .start_session(&s.doctor, &session_key, &AuthLevel::Level2, &HOUR);

    // Only the principal that opened the session can revoke it.
    assert_eq!(
        s.client.try_revoke_session(&s.patient, &session_key),
        Err(Ok(ContractError::Unauthorized))
    );

    s.client.revoke_session(&s.doctor, &session_key);
    assert_eq!(s.client.get_session(&session_key), None);
    // Without a session the key is just an address with no role.
    assert_eq!(
        s.client.try_get_eye_examination(&session_key, &1),
        Err(Ok(ContractError::Unauthorized))
    );
    assert_eq!(
        s.client.try_revoke_session(&s.doctor, &session_key),
        Err(Ok(ContractError::RecordNotFound))
    );
}

// ── Session creation rules ───────────────────────────────────────────────────

#[test]
fn start_session_validates_inputs() {
    let s = setup();
    let session_key = Address::generate(&s.env);

    assert_eq!(
        s.client
            .try_start_session(&s.doctor, &session_key, &AuthLevel::Level1, &0),
        Err(Ok(ContractError::InvalidInput))
    );
    assert_eq!(
        s.client.try_start_session(
            &s.doctor,
            &session_key,
            &AuthLevel::Level1,
            &(MAX_SESSION_TTL_SECS + 1)
        ),
        Err(Ok(ContractError::InvalidInput))
    );
    assert_eq!(
        s.client
            .try_start_session(&s.doctor, &s.doctor, &AuthLevel::Level1, &HOUR),
        Err(Ok(ContractError::InvalidInput))
    );
    assert!(s
        .client
        .try_start_session(
            &s.doctor,
            &session_key,
            &AuthLevel::Level1,
            &MAX_SESSION_TTL_SECS
        )
        .is_ok());
}

#[test]
fn active_session_key_cannot_be_taken_over() {
    let s = setup();
    let session_key = Address::generate(&s.env);
    let session = s
        .client
        .start_session(&s.doctor, &session_key, &AuthLevel::Level3, &HOUR);

    assert_eq!(
        s.client
            .try_start_session(&s.patient, &session_key, &AuthLevel::Level1, &HOUR),
        Err(Ok(ContractError::Unauthorized))
    );

    // Once it expires, the key can be reused by someone else.
    s.env.ledger().set_timestamp(session.expires_at + 1);
    let reused = s
        .client
        .start_session(&s.patient, &session_key, &AuthLevel::Level1, &HOUR);
    assert_eq!(reused.principal, s.patient);
    assert_eq!(reused.issued_at, session.expires_at + 1);
    assert_eq!(reused.max_level, AuthLevel::Level1);
}

#[test]
fn full_keys_are_unaffected_by_sessions() {
    let s = setup();
    let session_key = Address::generate(&s.env);
    s.client
        .start_session(&s.doctor, &session_key, &AuthLevel::Level1, &HOUR);

    // The doctor's own key keeps full access regardless of its session's level.
    assert_eq!(add_exam(&s, &s.doctor, 1), Ok(Ok(())));
    let rx_id =
        s.client
            .prepare_add_prescription(&s.doctor, &s.patient, &s.doctor, &rx_data(&s.env));
    s.client.commit_add_prescription(&s.doctor, &rx_id);
}

// ── Every caller must sign ───────────────────────────────────────────────────

#[test]
fn unsigned_call_with_a_providers_address_is_rejected() {
    let s = setup();
    add_exam(&s, &s.doctor, 1).unwrap().unwrap();

    // Stop mocking signatures: passing the doctor's address without the doctor's signature
    // must not grant the doctor's permissions.
    s.env.set_auths(&[]);
    assert!(s.client.try_get_eye_examination(&s.doctor, &1).is_err());
    assert!(add_exam(&s, &s.doctor, 2).is_err());
    assert!(s
        .client
        .try_prepare_add_prescription(&s.doctor, &s.patient, &s.doctor, &rx_data(&s.env))
        .is_err());
}

/// Stands in for `vision_records`: calls clinical_records with its own address as `caller`.
#[contract]
struct VisionRecordsStub;

#[contractimpl]
impl VisionRecordsStub {
    pub fn prepare(
        env: Env,
        clinical_records: Address,
        patient: Address,
        provider: Address,
        data: PrescriptionData,
    ) -> u64 {
        ClinicalRecordsContractClient::new(&env, &clinical_records).prepare_add_prescription(
            &env.current_contract_address(),
            &patient,
            &provider,
            &data,
        )
    }
}

#[test]
fn vision_records_contract_calls_are_authorized_without_signatures() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let stub_id = env.register(VisionRecordsStub, ());
    let cr_id = env.register(ClinicalRecordsContract, ());
    let client = ClinicalRecordsContractClient::new(&env, &cr_id);

    env.mock_all_auths();
    client.initialize(&admin, &stub_id, &None);
    env.set_auths(&[]);

    // No mocked signatures: the calling contract is authorized for its own address.
    let rx_id = VisionRecordsStubClient::new(&env, &stub_id).prepare(
        &cr_id,
        &Address::generate(&env),
        &Address::generate(&env),
        &rx_data(&env),
    );
    assert_eq!(rx_id, 1);
}
