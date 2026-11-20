//! Table-driven coverage for every supported region and documented data class.

use compliance::{gdpr, hipaa, Jurisdiction, OperationContext, RulesEngine};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
enum DataClass {
    Public,
    Pii,
    Phi,
    Confidential,
}

impl DataClass {
    fn sensitivity(self) -> u32 {
        match self {
            Self::Public => 0,
            Self::Pii => 1,
            Self::Phi => 2,
            Self::Confidential => 3,
        }
    }
}

fn context(jurisdiction: Jurisdiction, data_class: DataClass) -> OperationContext {
    let mut metadata = HashMap::new();
    metadata.insert("encrypted".to_string(), "true".to_string());
    metadata.insert("lawful_basis".to_string(), "consent".to_string());
    metadata.insert("export_format".to_string(), "json".to_string());

    OperationContext {
        actor: "clinician".to_string(),
        actor_role: "clinician".to_string(),
        action: "record.read".to_string(),
        target: "patient:1".to_string(),
        timestamp: 12 * 3600,
        has_consent: true,
        sensitivity: data_class.sensitivity(),
        jurisdiction,
        record_count: 1,
        purpose: "treatment".to_string(),
        metadata,
    }
}

#[test]
fn every_region_and_data_class_has_a_compliant_evaluation_path() {
    let cases = [
        (Jurisdiction::US, 7),
        (Jurisdiction::EU, 7),
        (Jurisdiction::Both, 14),
    ];
    let data_classes = [
        DataClass::Public,
        DataClass::Pii,
        DataClass::Phi,
        DataClass::Confidential,
    ];

    for (jurisdiction, expected_rules) in cases {
        for data_class in data_classes {
            let mut engine = RulesEngine::new();
            hipaa::register_hipaa_rules(&mut engine);
            gdpr::register_gdpr_rules(&mut engine);

            let verdict = engine.evaluate(&context(jurisdiction, data_class));
            assert!(
                verdict.allowed,
                "compliant {:?} {:?} input was rejected: {:?}",
                jurisdiction, data_class, verdict.violations
            );
            assert_eq!(verdict.rules_evaluated, expected_rules);
            assert_eq!(verdict.rules_passed, expected_rules);
        }
    }
}

#[test]
fn regional_rules_block_sensitive_data_without_required_safeguards() {
    let cases = [Jurisdiction::US, Jurisdiction::EU, Jurisdiction::Both];

    for jurisdiction in cases {
        let mut ctx = context(jurisdiction, DataClass::Phi);
        ctx.actor_role = "researcher".to_string();
        ctx.metadata.remove("encrypted");
        ctx.metadata.remove("lawful_basis");

        let mut engine = RulesEngine::new();
        hipaa::register_hipaa_rules(&mut engine);
        gdpr::register_gdpr_rules(&mut engine);

        let verdict = engine.evaluate(&ctx);
        assert!(
            !verdict.allowed,
            "unsafe {:?} input was allowed",
            jurisdiction
        );
        assert!(!verdict.violations.is_empty());
    }
}
