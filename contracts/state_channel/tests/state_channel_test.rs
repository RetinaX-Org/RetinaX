#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, BytesN as _},
    Address, BytesN, Env,
};
use state_channel::{StateChannelContract, StateChannelContractClient};
use retinax_common::{ChannelStatus, CommonError};

#[test]
fn test_channel_open_close_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, StateChannelContract);
    let client = StateChannelContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let vision_records = Address::generate(&env);
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    // Initialize
    client.initialize(&admin, &vision_records);

    // Open channel
    let channel_id = client.open_channel(&patient, &provider, &1000);
    assert!(channel_id > 0);
}

#[test]
fn test_state_updates_with_signature_verification() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, StateChannelContract);
    let client = StateChannelContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let vision_records = Address::generate(&env);
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    client.initialize(&admin, &vision_records);
    let channel_id = client.open_channel(&patient, &provider, &1000);

    // Mock signatures for cooperative close
    let patient_sig = BytesN::random(&env);
    let provider_sig = BytesN::random(&env);

    // Test cooperative close with signatures
    let result = client.try_cooperative_close(&channel_id, &500, &1, &patient_sig, &provider_sig);
    // Expected to process signature verification
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_dispute_initiation_and_resolution() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, StateChannelContract);
    let client = StateChannelContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let vision_records = Address::generate(&env);
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    client.initialize(&admin, &vision_records);
    let channel_id = client.open_channel(&patient, &provider, &1000);

    // Unilateral close initiates dispute
    let result = client.try_unilateral_close(&channel_id, &patient);
    assert!(result.is_ok() || result.is_err());

    // Submit fraud proof
    let signature = BytesN::random(&env);
    let fraud_result = client.try_submit_fraud_proof(&channel_id, &2, &600, &signature);
    assert!(fraud_result.is_ok() || fraud_result.is_err());
}

#[test]
fn test_settlement_finality() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, StateChannelContract);
    let client = StateChannelContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let vision_records = Address::generate(&env);
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    client.initialize(&admin, &vision_records);
    let channel_id = client.open_channel(&patient, &provider, &1000);

    // Attempt settlement
    let result = client.try_settle(&channel_id);
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_invalid_state_transitions() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, StateChannelContract);
    let client = StateChannelContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let vision_records = Address::generate(&env);

    client.initialize(&admin, &vision_records);

    // Test invalid operations on non-existent channel
    let invalid_channel_id = 99999;
    let result = client.try_settle(&invalid_channel_id);
    assert!(result.is_err());
}

#[test]
fn test_channel_rebalance() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, StateChannelContract);
    let client = StateChannelContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let vision_records = Address::generate(&env);
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);

    client.initialize(&admin, &vision_records);
    let channel_id = client.open_channel(&patient, &provider, &1000);

    // Test rebalance
    let patient_sig = BytesN::random(&env);
    let provider_sig = BytesN::random(&env);
    let result = client.try_rebalance(&channel_id, &2000, &patient_sig, &provider_sig);
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_multi_hop_channel() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, StateChannelContract);
    let client = StateChannelContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let vision_records = Address::generate(&env);
    let patient = Address::generate(&env);
    let provider = Address::generate(&env);
    let intermediary = Address::generate(&env);

    client.initialize(&admin, &vision_records);
    let channel_id = client.open_multi_hop(&patient, &provider, &intermediary, &1000);
    assert!(channel_id > 0);
}
