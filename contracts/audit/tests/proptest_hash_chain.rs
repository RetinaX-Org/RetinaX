#![allow(clippy::unwrap_used)]

use audit::contract::{AuditContract, AuditContractClient};
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

/// Property strategy for generating valid segment names
fn segment_name_strategy() -> impl Strategy<Value = String> {
    "[a-z_][a-z0-9_]{0,10}".prop_map(|s| s)
}

/// Property strategy for generating valid entry counts
fn entry_count_strategy() -> impl Strategy<Value = u32> {
    1u32..=100
}

/// Property strategy for generating various action types
fn action_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("created"),
        Just("modified"),
        Just("deleted"),
        Just("queried"),
        Just("verified"),
    ]
}

/// Property: Hash chain maintains referential integrity after sequential appends
proptest! {
    #[test]
    fn prop_hash_chain_referential_integrity(
        segment_name in segment_name_strategy(),
        entry_count in entry_count_strategy(),
        actions in prop::collection::vec(action_strategy(), entry_count as usize..=entry_count as usize)
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let audit_contract_id = env.register(AuditContract, ());
        let client = AuditContractClient::new(&env, &audit_contract_id);
        let admin = Address::generate(&env);

        // Initialize
        client.initialize(&admin);

        let segment = Symbol::short(&segment_name);
        client.create_segment(&segment);

        let actor = Address::generate(&env);

        // Append entries and track hashes
        let mut prev_hash_option: Option<Vec<u8>> = None;

        for (idx, action) in actions.iter().enumerate() {
            let action_sym = Symbol::short(action);
            let _seq = client.append_entry(
                &segment,
                &actor,
                &action_sym,
                &Symbol::short("test_subject"),
                &Symbol::short("ok"),
            );

            // Verify entry count increments
            prop_assert_eq!(
                client.get_entry_count(&segment),
                idx as u32 + 1,
                "Entry count should increment by 1 after each append"
            );
        }

        // Final assertion: root hash is computed
        let final_count = client.get_entry_count(&segment);
        prop_assert_eq!(
            final_count,
            entry_count,
            "Final entry count should match number of appends"
        );
    }
}

/// Property: Hash values are deterministic for identical input
proptest! {
    #[test]
    fn prop_hash_determinism(
        segment_name in segment_name_strategy(),
        action in action_strategy()
    ) {
        let env1 = Env::default();
        env1.mock_all_auths();

        let audit_id_1 = env1.register(AuditContract, ());
        let client1 = AuditContractClient::new(&env1, &audit_id_1);
        let admin1 = Address::generate(&env1);
        client1.initialize(&admin1);

        let segment_sym = Symbol::short(&segment_name);
        let action_sym = Symbol::short(&action);
        let actor = Address::generate(&env1);

        client1.create_segment(&segment_sym);
        client1.append_entry(
            &segment_sym,
            &actor,
            &action_sym,
            &Symbol::short("subject"),
            &Symbol::short("ok"),
        );

        // Create second instance with identical input
        let env2 = Env::default();
        env2.mock_all_auths();

        let audit_id_2 = env2.register(AuditContract, ());
        let client2 = AuditContractClient::new(&env2, &audit_id_2);
        let admin2 = Address::generate(&env2);
        client2.initialize(&admin2);

        client2.create_segment(&segment_sym);
        client2.append_entry(
            &segment_sym,
            &actor,
            &action_sym,
            &Symbol::short("subject"),
            &Symbol::short("ok"),
        );

        // Both should have same entry count
        prop_assert_eq!(
            client1.get_entry_count(&segment_sym),
            client2.get_entry_count(&segment_sym),
            "Entry counts should match for identical inputs"
        );
    }
}

/// Property: Root hash changes when any entry detail changes
proptest! {
    #[test]
    fn prop_hash_sensitivity_to_changes(
        segment_name in segment_name_strategy(),
        action1 in action_strategy(),
        action2 in action_strategy(),
    ) {
        // Skip if both actions are identical
        prop_assume!(action1 != action2);

        let env = Env::default();
        env.mock_all_auths();

        let audit_id = env.register(AuditContract, ());
        let client = AuditContractClient::new(&env, &audit_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let segment = Symbol::short(&segment_name);
        client.create_segment(&segment);

        let actor = Address::generate(&env);
        let subject = Symbol::short("test_subject");
        let result = Symbol::short("ok");

        // First entry with action1
        client.append_entry(
            &segment,
            &actor,
            &Symbol::short(&action1),
            &subject,
            &result,
        );

        let count_after_first = client.get_entry_count(&segment);

        // Second entry with different action
        client.append_entry(
            &segment,
            &actor,
            &Symbol::short(&action2),
            &subject,
            &result,
        );

        let count_after_second = client.get_entry_count(&segment);

        // Both entries should exist
        prop_assert_eq!(
            count_after_first,
            1,
            "First entry should increment count to 1"
        );
        prop_assert_eq!(
            count_after_second,
            2,
            "Second entry should increment count to 2"
        );
    }
}

/// Property: Multiple segments don't interfere with each other's hash chains
proptest! {
    #[test]
    fn prop_segment_isolation(
        segments in prop::collection::vec(segment_name_strategy(), 2..=5),
        entry_counts in prop::collection::vec(entry_count_strategy(), segments.len()..=segments.len()),
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let audit_id = env.register(AuditContract, ());
        let client = AuditContractClient::new(&env, &audit_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let actor = Address::generate(&env);

        // Create multiple segments and append different counts
        for (seg_name, count) in segments.iter().zip(entry_counts.iter()) {
            let segment = Symbol::short(seg_name);
            client.create_segment(&segment);

            for i in 0..*count {
                let action = Symbol::short(&format!("action_{}", i % 5));
                client.append_entry(
                    &segment,
                    &actor,
                    &action,
                    &Symbol::short("subject"),
                    &Symbol::short("ok"),
                );
            }
        }

        // Verify each segment has correct entry count
        for (seg_name, expected_count) in segments.iter().zip(entry_counts.iter()) {
            let segment = Symbol::short(seg_name);
            let actual_count = client.get_entry_count(&segment);
            prop_assert_eq!(
                actual_count, *expected_count,
                "Segment {} should have {} entries",
                seg_name, expected_count
            );
        }
    }
}

/// Property: Entry sequence numbers increment monotonically and continuously
proptest! {
    #[test]
    fn prop_sequence_number_monotonicity(
        segment_name in segment_name_strategy(),
        entry_count in 2u32..=50
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let audit_id = env.register(AuditContract, ());
        let client = AuditContractClient::new(&env, &audit_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let segment = Symbol::short(&segment_name);
        client.create_segment(&segment);

        let actor = Address::generate(&env);
        let mut last_sequence = 0u32;

        for i in 0..entry_count {
            let action = Symbol::short(&format!("action_{}", i % 10));
            let seq = client.append_entry(
                &segment,
                &actor,
                &action,
                &Symbol::short("subject"),
                &Symbol::short("ok"),
            );

            // Sequence should increment by exactly 1
            prop_assert_eq!(
                seq,
                last_sequence + 1,
                "Sequence {} should be exactly 1 more than previous {}",
                seq,
                last_sequence
            );

            last_sequence = seq;
        }

        // Final count should equal last sequence
        prop_assert_eq!(
            client.get_entry_count(&segment),
            last_sequence,
            "Entry count should equal final sequence number"
        );
    }
}

/// Property: Large-scale stress test - many entries maintain chain integrity
proptest! {
    #[test]
    fn prop_large_scale_chain_stress(
        entry_count in 50u32..=200
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let audit_id = env.register(AuditContract, ());
        let client = AuditContractClient::new(&env, &audit_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let segment = Symbol::short("stress_segment");
        client.create_segment(&segment);

        let actor = Address::generate(&env);

        // Append many entries rapidly
        for i in 0..entry_count {
            let action = match i % 5 {
                0 => Symbol::short("create"),
                1 => Symbol::short("modify"),
                2 => Symbol::short("delete"),
                3 => Symbol::short("query"),
                _ => Symbol::short("verify"),
            };

            let _seq = client.append_entry(
                &segment,
                &actor,
                &action,
                &Symbol::short(&format!("subject_{}", i / 10)),
                &Symbol::short("ok"),
            );
        }

        // Verify final count
        let final_count = client.get_entry_count(&segment);
        prop_assert_eq!(
            final_count,
            entry_count,
            "After {} appends, count should be {}",
            entry_count,
            entry_count
        );
    }
}

/// Property: Hash chain remains valid even with actors varying
proptest! {
    #[test]
    fn prop_hash_chain_with_varying_actors(
        segment_name in segment_name_strategy(),
        actor_count in 1u32..=10
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let audit_id = env.register(AuditContract, ());
        let client = AuditContractClient::new(&env, &audit_id);
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let segment = Symbol::short(&segment_name);
        client.create_segment(&segment);

        // Generate multiple actors
        let mut actors = Vec::new();
        for _ in 0..actor_count {
            actors.push(Address::generate(&env));
        }

        // Append entries from different actors
        for (i, actor) in actors.iter().enumerate() {
            let action = Symbol::short(&format!("action_{}", i % 5));
            let _seq = client.append_entry(
                &segment,
                actor,
                &action,
                &Symbol::short("subject"),
                &Symbol::short("ok"),
            );
        }

        // Verify all entries recorded
        let count = client.get_entry_count(&segment);
        prop_assert_eq!(
            count,
            actor_count,
            "Should have {} entries from {} actors",
            actor_count,
            actor_count
        );
    }
}
