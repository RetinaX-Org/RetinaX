use crate::prescription::{self, LensType, OptionalContactLensData, Prescription, PrescriptionData};
use crate::VisionRecordsContract;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env, String};

#[test]
fn test_prescription_direct_save_and_events() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VisionRecordsContract, ());

    let patient = Address::generate(&env);
    let doctor = Address::generate(&env);

    let eye_data = PrescriptionData {
        sphere: String::from_str(&env, "-2.50"),
        cylinder: String::from_str(&env, "-1.25"),
        axis: String::from_str(&env, "180"),
        add: String::from_str(&env, "0.00"),
        pd: String::from_str(&env, "62"),
    };

    let rx = Prescription {
        id: 1,
        patient: patient.clone(),
        provider: doctor.clone(),
        lens_type: LensType::Glasses,
        left_eye: eye_data.clone(),
        right_eye: eye_data.clone(),
        contact_data: OptionalContactLensData::None,
        issued_at: env.ledger().timestamp(),
        expires_at: env.ledger().timestamp() + 31536000,
        verified: false,
        metadata_hash: String::from_str(&env, "meta_hash_12345678901234567890123456789012"),
    };

    env.as_contract(&contract_id, || {
        // Save prescription and verify event emission
        prescription::save_prescription(&env, &rx, None);
        let all1 = env.events().all();
        let initial_events = all1.events();
        assert!(initial_events.len() > 0);

        // Verify prescription and event emission
        assert!(prescription::verify_prescription(&env, 1, doctor.clone()));
        let verified_rx = prescription::get_prescription(&env, 1).unwrap();
        assert!(verified_rx.verified);

        let all2 = env.events().all();
        let updated_events = all2.events();
        assert!(updated_events.len() > initial_events.len());

        // Patient history
        let history = prescription::get_patient_history(&env, patient);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get(0).unwrap(), 1);
    });
}
