#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Data format validation tests for the EMR bridge (issue #319).
//!
//! Covers the four validation areas required by the issue:
//!   1. Data format conversions for common EMR systems
//!   2. Round-trip integrity: export -> import -> verify
//!   3. Handling of missing / optional fields
//!   4. Character encoding for international patient names
//!
//! The contract stores exchange metadata, field mappings and hash-based sync
//! verification; it does not parse FHIR/HL7/CCD payloads. These tests
//! therefore validate the conversion *representation* (the provider's
//! `(EmrSystem, DataFormat)` pair and the `FieldMapping` rows) together with
//! the round-trip fidelity of everything that is persisted on the ledger.

use emr_bridge::{
    types::{DataFormat, EmrSystem, ExchangeDirection, ProviderStatus, SyncStatus},
    EmrBridgeContract, EmrBridgeContractClient, EmrBridgeError,
};
use soroban_sdk::{testutils::Address as _, Address, Bytes, Env, String, Vec};

// ── Setup helper ─────────────────────────────────────────────────────────────

struct Setup<'a> {
    env: Env,
    client: EmrBridgeContractClient<'a>,
    admin: Address,
    provider_id: String,
}

fn setup() -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(EmrBridgeContract, ());
    let client = EmrBridgeContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);

    let provider_id = String::from_str(&env, "epic-fhir-r4");
    client.register_provider(
        &admin,
        &provider_id,
        &String::from_str(&env, "Epic FHIR R4"),
        &EmrSystem::EpicFhir,
        &String::from_str(&env, "https://epic.example.org/api/FHIR/R4"),
        &DataFormat::FhirR4,
    );
    client.activate_provider(&admin, &provider_id);

    Setup {
        env,
        client,
        admin,
        provider_id,
    }
}

// ── 1. Data format conversions for common EMR systems ────────────────────────

/// Every supported EMR system can be registered with its own data format and
/// the pair survives a read back from the ledger.
#[test]
fn test_provider_registers_each_common_emr_system_with_its_data_format() {
    let s = setup();

    let cases = [
        (
            EmrSystem::EpicFhir,
            DataFormat::FhirR4,
            "epic-002",
            "City Hospital Epic",
            "https://epic.cityhospital.org/fhir",
        ),
        (
            EmrSystem::CernerMillennium,
            DataFormat::Hl7V2,
            "cerner-002",
            "County Clinic Cerner",
            "https://cerner.countyclinic.org/hl7v2",
        ),
        (
            EmrSystem::Allscripts,
            DataFormat::CcdA,
            "allscripts-002",
            "Regional Allscripts",
            "https://allscripts.regional.org/ccda",
        ),
        (
            EmrSystem::Athenahealth,
            DataFormat::FhirR4,
            "athena-002",
            "Athena Health Network",
            "https://athena.healthnetwork.org/fhir",
        ),
        (
            EmrSystem::Custom,
            DataFormat::Custom,
            "custom-002",
            "Specialist Custom EMR",
            "https://custom.specialist.org/bridge",
        ),
    ];

    for (emr_system, data_format, id, name, endpoint) in cases {
        let provider_id = String::from_str(&s.env, id);

        let provider = s.client.register_provider(
            &s.admin,
            &provider_id,
            &String::from_str(&s.env, name),
            &emr_system,
            &String::from_str(&s.env, endpoint),
            &data_format,
        );

        assert_eq!(provider.emr_system, emr_system);
        assert_eq!(provider.data_format, data_format);
        assert_eq!(provider.status, ProviderStatus::Pending);

        // The conversion pair is readable back from the ledger.
        let stored = s.client.get_provider(&provider_id);
        assert_eq!(stored.emr_system, emr_system);
        assert_eq!(stored.data_format, data_format);
        assert_eq!(stored.name, String::from_str(&s.env, name));
        assert_eq!(stored.endpoint_url, String::from_str(&s.env, endpoint));
        assert_eq!(stored.status, ProviderStatus::Pending);
    }
}

/// Each data format can be carried on an exchange record and is preserved
/// exactly as written, for both import and export direction.
#[test]
fn test_each_data_format_round_trips_through_exchange_record() {
    let s = setup();

    let formats = [
        DataFormat::FhirR4,
        DataFormat::Hl7V2,
        DataFormat::CcdA,
        DataFormat::Custom,
    ];

    for (index, format) in formats.iter().enumerate() {
        let exchange_id = String::from_str(&s.env, &format!("ex-format-{}", index));
        let patient_id = String::from_str(&s.env, &format!("sha256:patient-{}", index));
        let resource_type = String::from_str(&s.env, "Patient");
        let record_hash = String::from_str(&s.env, &format!("sha256:payload-{}", index));

        let record = s.client.record_data_exchange(
            &s.admin,
            &exchange_id,
            &s.provider_id,
            &patient_id,
            &ExchangeDirection::Import,
            format,
            &resource_type,
            &record_hash,
        );

        assert_eq!(record.data_format, *format);
        assert_eq!(record.direction, ExchangeDirection::Import);
        assert_eq!(record.status, SyncStatus::Pending);

        let stored = s.client.get_exchange(&exchange_id);
        assert_eq!(stored.data_format, *format);
        assert_eq!(stored.resource_type, resource_type);
        assert_eq!(stored.record_hash, record_hash);
        assert_eq!(stored.patient_id, patient_id);
    }
}

/// The contract stores the data format verbatim; it does not cross-check it
/// against the format the provider registered, and `InvalidDataFormat` is
/// never raised. This test documents that observed behaviour.
#[test]
fn test_exchange_data_format_is_stored_verbatim_even_when_it_differs_from_provider() {
    let s = setup();

    // The setup provider is registered as EpicFhir / FhirR4.
    let provider = s.client.get_provider(&s.provider_id);
    assert_eq!(provider.data_format, DataFormat::FhirR4);

    let exchange_id = String::from_str(&s.env, "ex-format-mismatch");
    let record = s.client.record_data_exchange(
        &s.admin,
        &exchange_id,
        &s.provider_id,
        &String::from_str(&s.env, "sha256:patient-mismatch"),
        &ExchangeDirection::Import,
        &DataFormat::Hl7V2,
        &String::from_str(&s.env, "Patient"),
        &String::from_str(&s.env, "sha256:hash-mismatch"),
    );

    // Accepted and stored exactly as supplied.
    assert_eq!(record.data_format, DataFormat::Hl7V2);

    let stored = s.client.get_exchange(&exchange_id);
    assert_eq!(stored.data_format, DataFormat::Hl7V2);
}

// ── 2. Round-trip: export -> import -> verify data integrity ─────────────────

/// Full round trip: the same payload is exported and imported, the import is
/// verified against the exported hash, and every field of both records is
/// reconstructible from the ledger afterwards.
#[test]
fn test_export_import_round_trip_preserves_data_integrity() {
    let s = setup();

    let patient_id = String::from_str(&s.env, "sha256:patient-roundtrip");
    let resource_type = String::from_str(&s.env, "Bundle");
    let payload_hash = String::from_str(&s.env, "sha256:canonical-payload");

    // Export the payload to the external EMR system.
    let export_id = String::from_str(&s.env, "ex-export-roundtrip");
    s.client.record_data_exchange(
        &s.admin,
        &export_id,
        &s.provider_id,
        &patient_id,
        &ExchangeDirection::Export,
        &DataFormat::FhirR4,
        &resource_type,
        &payload_hash,
    );

    // Import the very same payload back.
    let import_id = String::from_str(&s.env, "ex-import-roundtrip");
    s.client.record_data_exchange(
        &s.admin,
        &import_id,
        &s.provider_id,
        &patient_id,
        &ExchangeDirection::Import,
        &DataFormat::FhirR4,
        &resource_type,
        &payload_hash,
    );

    // Verification is only accepted once the exchange is in progress.
    s.client
        .update_exchange_status(&s.admin, &import_id, &SyncStatus::InProgress);

    let verification_id = String::from_str(&s.env, "ver-roundtrip");
    let verification = s.client.verify_sync(
        &s.admin,
        &verification_id,
        &import_id,
        &payload_hash,
        &payload_hash,
        &Vec::new(&s.env),
    );

    assert!(verification.is_consistent);
    assert_eq!(verification.source_hash, payload_hash);
    assert_eq!(verification.target_hash, payload_hash);
    assert_eq!(verification.discrepancies.len(), 0);

    // The import completes; the export record is untouched by the import sync.
    let import_record = s.client.get_exchange(&import_id);
    assert_eq!(import_record.status, SyncStatus::Completed);
    assert_eq!(import_record.exchange_id, import_id);
    assert_eq!(import_record.provider_id, s.provider_id);
    assert_eq!(import_record.patient_id, patient_id);
    assert_eq!(import_record.direction, ExchangeDirection::Import);
    assert_eq!(import_record.data_format, DataFormat::FhirR4);
    assert_eq!(import_record.resource_type, resource_type);
    assert_eq!(import_record.record_hash, payload_hash);

    let export_record = s.client.get_exchange(&export_id);
    assert_eq!(export_record.status, SyncStatus::Pending);
    assert_eq!(export_record.direction, ExchangeDirection::Export);
    assert_eq!(export_record.record_hash, payload_hash);
    assert_eq!(export_record.patient_id, patient_id);

    // The verification record itself is reconstructible from the ledger.
    let stored_verification = s.client.get_verification(&verification_id);
    assert_eq!(stored_verification.exchange_id, import_id);
    assert!(stored_verification.is_consistent);
    assert_eq!(stored_verification.discrepancies.len(), 0);

    // Both halves of the round trip are indexed under the same patient.
    let exchanges = s.client.get_patient_exchanges(&patient_id);
    assert_eq!(exchanges.len(), 2);
    assert!(exchanges.contains(&export_id));
    assert!(exchanges.contains(&import_id));
}

/// A round trip whose payload changed between export and import is detected:
/// the hashes differ, the verification is inconsistent and the discrepancy is
/// preserved on the ledger for auditing.
#[test]
fn test_round_trip_detects_hash_mismatch_between_export_and_import() {
    let s = setup();

    let patient_id = String::from_str(&s.env, "sha256:patient-mismatch");
    let resource_type = String::from_str(&s.env, "Observation");
    let export_hash = String::from_str(&s.env, "sha256:export-payload");
    let import_hash = String::from_str(&s.env, "sha256:import-payload-differs");

    let export_id = String::from_str(&s.env, "ex-export-mismatch");
    s.client.record_data_exchange(
        &s.admin,
        &export_id,
        &s.provider_id,
        &patient_id,
        &ExchangeDirection::Export,
        &DataFormat::FhirR4,
        &resource_type,
        &export_hash,
    );

    let import_id = String::from_str(&s.env, "ex-import-mismatch");
    s.client.record_data_exchange(
        &s.admin,
        &import_id,
        &s.provider_id,
        &patient_id,
        &ExchangeDirection::Import,
        &DataFormat::FhirR4,
        &resource_type,
        &import_hash,
    );

    s.client
        .update_exchange_status(&s.admin, &import_id, &SyncStatus::InProgress);

    let discrepancy_text = String::from_str(&s.env, "Observation.value differs from export");
    let mut discrepancies = Vec::new(&s.env);
    discrepancies.push_back(discrepancy_text.clone());

    let verification_id = String::from_str(&s.env, "ver-mismatch");
    let verification = s.client.verify_sync(
        &s.admin,
        &verification_id,
        &import_id,
        &export_hash,
        &import_hash,
        &discrepancies,
    );

    assert!(!verification.is_consistent);
    assert_eq!(verification.discrepancies.len(), 1);

    // The exchange is flagged as a partial success rather than completed.
    let record = s.client.get_exchange(&import_id);
    assert_eq!(record.status, SyncStatus::PartialSuccess);

    // The discrepancy survives the write to the ledger.
    let stored_verification = s.client.get_verification(&verification_id);
    assert!(!stored_verification.is_consistent);
    assert!(stored_verification
        .discrepancies
        .contains(&discrepancy_text));
}

/// A field mapping describes how a source EMR field is converted into the
/// internal schema; the full mapping round trips through the ledger intact.
#[test]
fn test_field_mapping_round_trip_preserves_source_target_and_transform_rule() {
    let s = setup();

    let mapping_id = String::from_str(&s.env, "map-patient-name");
    let source = String::from_str(&s.env, "Patient.name[0].family");
    let target = String::from_str(&s.env, "teye.patient.surname");
    let rule = String::from_str(&s.env, "normalize_unicode");

    s.client.create_field_mapping(
        &s.admin,
        &mapping_id,
        &s.provider_id,
        &source,
        &target,
        &rule,
    );

    let stored = s.client.get_field_mapping(&mapping_id);
    assert_eq!(stored.mapping_id, mapping_id);
    assert_eq!(stored.provider_id, s.provider_id);
    assert_eq!(stored.source_field, source);
    assert_eq!(stored.target_field, target);
    assert_eq!(stored.transform_rule, rule);

    // Mappings remain indexed under their provider.
    let mapping_ids = s.client.get_provider_mappings(&s.provider_id);
    assert_eq!(mapping_ids.len(), 1);
    assert!(mapping_ids.contains(&mapping_id));
}

// ── 3. Handling of missing / optional fields ─────────────────────────────────

/// Optional fields are accepted empty and stored verbatim — the contract only
/// validates the mapping source and target, not the rest of the payload.
#[test]
fn test_empty_optional_fields_are_accepted_and_preserved() {
    let s = setup();

    let empty = String::from_str(&s.env, "");
    let exchange_id = String::from_str(&s.env, "ex-optional-empty");

    let record = s.client.record_data_exchange(
        &s.admin,
        &exchange_id,
        &s.provider_id,
        &empty,
        &ExchangeDirection::Import,
        &DataFormat::FhirR4,
        &empty,
        &empty,
    );

    assert_eq!(record.resource_type, empty);
    assert_eq!(record.record_hash, empty);
    assert_eq!(record.patient_id, empty);

    let stored = s.client.get_exchange(&exchange_id);
    assert_eq!(stored.resource_type, empty);
    assert_eq!(stored.record_hash, empty);
    assert_eq!(stored.patient_id, empty);

    // The transform rule is optional for a field mapping.
    let mapping_id = String::from_str(&s.env, "map-optional-rule");
    let source = String::from_str(&s.env, "Observation.value");
    let target = String::from_str(&s.env, "teye.metric.value");

    let mapping = s.client.create_field_mapping(
        &s.admin,
        &mapping_id,
        &s.provider_id,
        &source,
        &target,
        &empty,
    );
    assert_eq!(mapping.transform_rule, empty);

    let stored_mapping = s.client.get_field_mapping(&mapping_id);
    assert_eq!(stored_mapping.transform_rule, empty);
    assert_eq!(stored_mapping.source_field, source);
    assert_eq!(stored_mapping.target_field, target);
}

/// A mapping without a source or a target field is rejected: these are the
/// only two fields the contract validates.
#[test]
fn test_empty_source_or_target_field_mapping_is_rejected() {
    let s = setup();

    let empty = String::from_str(&s.env, "");

    let result = s.client.try_create_field_mapping(
        &s.admin,
        &String::from_str(&s.env, "map-empty-source"),
        &s.provider_id,
        &empty,
        &String::from_str(&s.env, "teye.patient.surname"),
        &String::from_str(&s.env, "direct_copy"),
    );
    assert_eq!(result.unwrap_err(), Ok(EmrBridgeError::InvalidMapping));

    let result = s.client.try_create_field_mapping(
        &s.admin,
        &String::from_str(&s.env, "map-empty-target"),
        &s.provider_id,
        &String::from_str(&s.env, "Patient.name[0].family"),
        &empty,
        &String::from_str(&s.env, "direct_copy"),
    );
    assert_eq!(result.unwrap_err(), Ok(EmrBridgeError::InvalidMapping));
}

/// A verification with no hash information and no discrepancies is consistent:
/// the contract compares the two hashes and the discrepancy list only.
#[test]
fn test_verification_with_empty_hashes_and_no_discrepancies_is_consistent() {
    let s = setup();

    let empty = String::from_str(&s.env, "");
    let exchange_id = String::from_str(&s.env, "ex-empty-hash");

    s.client.record_data_exchange(
        &s.admin,
        &exchange_id,
        &s.provider_id,
        &String::from_str(&s.env, "sha256:patient-empty-hash"),
        &ExchangeDirection::Import,
        &DataFormat::Hl7V2,
        &String::from_str(&s.env, "Patient"),
        &empty,
    );

    s.client
        .update_exchange_status(&s.admin, &exchange_id, &SyncStatus::InProgress);

    let verification = s.client.verify_sync(
        &s.admin,
        &String::from_str(&s.env, "ver-empty-hash"),
        &exchange_id,
        &empty,
        &empty,
        &Vec::new(&s.env),
    );

    assert!(verification.is_consistent);
    assert_eq!(verification.discrepancies.len(), 0);

    let record = s.client.get_exchange(&exchange_id);
    assert_eq!(record.status, SyncStatus::Completed);
}

/// An exchange recorded without a patient identifier is still indexed, under
/// the empty patient key, so it remains retrievable for auditing.
#[test]
fn test_missing_patient_id_still_indexes_under_the_empty_key() {
    let s = setup();

    let empty = String::from_str(&s.env, "");
    let exchange_id = String::from_str(&s.env, "ex-no-patient-id");

    s.client.record_data_exchange(
        &s.admin,
        &exchange_id,
        &s.provider_id,
        &empty,
        &ExchangeDirection::Import,
        &DataFormat::FhirR4,
        &String::from_str(&s.env, "Patient"),
        &String::from_str(&s.env, "sha256:hash"),
    );

    let exchanges = s.client.get_patient_exchanges(&empty);
    assert_eq!(exchanges.len(), 1);
    assert!(exchanges.contains(&exchange_id));
}

// ── 4. Character encoding for international patient names ────────────────────

/// Patient names and other free-text fields survive a write/read round trip
/// byte for byte, across Latin diacritics, CJK, Cyrillic, Arabic, Hebrew,
/// Thai, emoji and combining marks.
#[test]
fn test_international_patient_names_round_trip_without_corruption() {
    let s = setup();

    let names = [
        "José Álvarez",
        "李明",
        "Müller",
        "Ольга Иванова",
        "أحمد بن محمد",
        "עברית שם",
        "Ólafur Þór",
        "Nguyễn Thị Hương",
        "ผู้ป่วย",
        "😀 emoji",
        "e\u{301}lodie",
    ];

    for (index, name) in names.iter().enumerate() {
        let exchange_id = String::from_str(&s.env, &format!("ex-unicode-{}", index));
        let patient_id = String::from_str(&s.env, name);
        let resource_type = String::from_str(&s.env, &format!("Patient/{}", name));
        let record_hash = String::from_str(&s.env, name);

        s.client.record_data_exchange(
            &s.admin,
            &exchange_id,
            &s.provider_id,
            &patient_id,
            &ExchangeDirection::Import,
            &DataFormat::FhirR4,
            &resource_type,
            &record_hash,
        );

        let stored = s.client.get_exchange(&exchange_id);
        assert_eq!(stored.patient_id, patient_id);
        assert_eq!(stored.resource_type, resource_type);
        assert_eq!(stored.record_hash, record_hash);

        // Byte-level fidelity: the ledger holds exactly the UTF-8 bytes.
        assert_eq!(
            stored.patient_id.to_bytes(),
            Bytes::from_slice(&s.env, name.as_bytes())
        );
        assert_eq!(stored.patient_id.len() as usize, name.len());
    }
}

/// International values in a field mapping are preserved as well, including
/// the internal target field name.
#[test]
fn test_international_names_in_field_mapping_round_trip() {
    let s = setup();

    let mapping_id = String::from_str(&s.env, "map-unicode");
    let source = String::from_str(&s.env, "Patient.name[0].family");
    let target_text = "患者.姓";
    let target = String::from_str(&s.env, target_text);
    let rule = String::from_str(&s.env, "normalizar_unicode");

    s.client.create_field_mapping(
        &s.admin,
        &mapping_id,
        &s.provider_id,
        &source,
        &target,
        &rule,
    );

    let stored = s.client.get_field_mapping(&mapping_id);
    assert_eq!(stored.source_field, source);
    assert_eq!(stored.target_field, target);
    assert_eq!(stored.transform_rule, rule);
    assert_eq!(
        stored.target_field.to_bytes(),
        Bytes::from_slice(&s.env, target_text.as_bytes())
    );
}

/// The export -> import round trip keeps international names intact and the
/// verification of that round trip stays consistent.
#[test]
fn test_export_import_round_trip_with_international_name() {
    let s = setup();

    let name = "Ольга Иванова";
    let patient_id = String::from_str(&s.env, name);
    let resource_type = String::from_str(&s.env, "Patient");
    let payload_hash = String::from_str(&s.env, "sha256:unicode-payload");

    let export_id = String::from_str(&s.env, "ex-unicode-export");
    s.client.record_data_exchange(
        &s.admin,
        &export_id,
        &s.provider_id,
        &patient_id,
        &ExchangeDirection::Export,
        &DataFormat::FhirR4,
        &resource_type,
        &payload_hash,
    );

    let import_id = String::from_str(&s.env, "ex-unicode-import");
    s.client.record_data_exchange(
        &s.admin,
        &import_id,
        &s.provider_id,
        &patient_id,
        &ExchangeDirection::Import,
        &DataFormat::FhirR4,
        &resource_type,
        &payload_hash,
    );

    s.client
        .update_exchange_status(&s.admin, &import_id, &SyncStatus::InProgress);

    let verification = s.client.verify_sync(
        &s.admin,
        &String::from_str(&s.env, "ver-unicode"),
        &import_id,
        &payload_hash,
        &payload_hash,
        &Vec::new(&s.env),
    );
    assert!(verification.is_consistent);

    let stored = s.client.get_exchange(&import_id);
    assert_eq!(stored.status, SyncStatus::Completed);
    assert_eq!(stored.patient_id, patient_id);
    assert_eq!(
        stored.patient_id.to_bytes(),
        Bytes::from_slice(&s.env, name.as_bytes())
    );
}
