#![cfg(test)]

use crate::{ZkVerifier, ZkVerifierClient, MAX_BATCH_SIZE};
use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env, Vec};

fn setup() -> (Env, ZkVerifierClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ZkVerifier);
    let client = ZkVerifierClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin)
}

fn dummy_proof(env: &Env, seed: u8) -> Bytes {
    let mut bytes = Bytes::new(env);
    for i in 0..32u8 {
        bytes.push_back(seed.wrapping_add(i));
    }
    bytes
}

fn dummy_public_inputs(env: &Env, seed: u8) -> BytesN<32> {
    let mut arr = [0u8; 32];
    for (i, b) in arr.iter_mut().enumerate() {
        *b = seed.wrapping_mul(3).wrapping_add(i as u8);
    }
    BytesN::from_array(env, &arr)
}

#[test]
fn test_verify_batch_empty_returns_true() {
    let (env, client, _admin) = setup();
    let proofs: Vec<Bytes> = Vec::new(&env);
    let inputs: Vec<BytesN<32>> = Vec::new(&env);
    assert!(client.verify_batch(&proofs, &inputs));
}

#[test]
fn test_verify_batch_matches_single_verification() {
    let (env, client, _admin) = setup();
    let mut proofs = Vec::new(&env);
    let mut inputs = Vec::new(&env);
    for seed in 1..=4u8 {
        proofs.push_back(dummy_proof(&env, seed));
        inputs.push_back(dummy_public_inputs(&env, seed));
    }

    let mut all_single = true;
    for i in 0..proofs.len() {
        let p = proofs.get(i).unwrap();
        let x = inputs.get(i).unwrap();
        all_single &= client.verify(&p, &x);
    }
    assert_eq!(client.verify_batch(&proofs, &inputs), all_single);
}

#[test]
fn test_verify_batch_rejects_length_mismatch() {
    let (env, client, _admin) = setup();
    let mut proofs = Vec::new(&env);
    proofs.push_back(dummy_proof(&env, 1));
    let inputs: Vec<BytesN<32>> = Vec::new(&env);
    let result = client.try_verify_batch(&proofs, &inputs);
    assert!(result.is_err());
}

#[test]
fn test_verify_batch_rejects_oversized_batch() {
    let (env, client, _admin) = setup();
    let mut proofs = Vec::new(&env);
    let mut inputs = Vec::new(&env);
    for seed in 0..(MAX_BATCH_SIZE + 1) as u8 {
        proofs.push_back(dummy_proof(&env, seed));
        inputs.push_back(dummy_public_inputs(&env, seed));
    }
    let result = client.try_verify_batch(&proofs, &inputs);
    assert!(result.is_err());
}

#[test]
fn test_verify_batch_accepts_max_batch_size() {
    let (env, client, _admin) = setup();
    let mut proofs = Vec::new(&env);
    let mut inputs = Vec::new(&env);
    for seed in 0..MAX_BATCH_SIZE as u8 {
        proofs.push_back(dummy_proof(&env, seed));
        inputs.push_back(dummy_public_inputs(&env, seed));
    }
    let result = client.try_verify_batch(&proofs, &inputs);
    assert!(result.is_ok());
}
