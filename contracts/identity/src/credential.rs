#![allow(deprecated)]
use soroban_sdk::{symbol_short, Address, Bytes, BytesN, Env, Symbol, Vec};


pub mod zk_verifier_client {
    pub type SchemaVersion = u32;
    soroban_sdk::contractimport!(file = "../../target/wasm32-unknown-unknown/release/zk_verifier.wasm");
}

type VkG1Point = Bytes;
type VkG2Point = Bytes;

const ZK_VERIFIER: Symbol = symbol_short!("ZK_VER");

#[soroban_sdk::contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CredentialError {
    Unauthorized = 100,
    VerifierNotSet = 101,
    ZkVerificationFailed = 102,
    InvalidNonce = 103,
    CredentialExpired = 104,
}

pub fn set_zk_verifier(env: &Env, verifier_id: &Address) {
    env.storage().instance().set(&ZK_VERIFIER, verifier_id);
}

pub fn get_zk_verifier(env: &Env) -> Option<Address> {
    env.storage().instance().get(&ZK_VERIFIER)
}

pub fn verify_zk_credential(
    env: &Env,
    user: &Address,
    resource_id: BytesN<32>,
    proof_a: Bytes,
    proof_b: Bytes,
    proof_c: Bytes,
    public_inputs: Vec<BytesN<32>>,
    expires_at: u64,
    nonce: u64,
) -> Result<bool, CredentialError> {
    if env.ledger().timestamp() > expires_at {
        return Err(CredentialError::CredentialExpired);
    }


fn extract_bytesn(env: &Env, bytes: &Bytes, start: u32, end: u32) -> Result<BytesN<32>, CredentialError> {
    let mut buf = [0u8; 32];
    bytes.slice(start..end).copy_into_slice(&mut buf);
    Ok(BytesN::from_array(env, &buf))
}

    let verifier_id = get_zk_verifier(env).ok_or(CredentialError::VerifierNotSet)?;
    let client = zk_verifier_client::Client::new(env, &verifier_id);

    // Reconstruct the proof points from raw bytes.
    // The proof bytes are expected to be in G1 (64 bytes: 32x, 32y) and G2 (128 bytes: 32x0, 32x1, 32y0, 32y1) format.
    let proof = zk_verifier_client::Proof {
        a: zk_verifier_client::G1Point {
            x: extract_bytesn(env, &proof_a, 0, 32)?,
            y: extract_bytesn(env, &proof_a, 32, 64)?,
        },
        b: zk_verifier_client::G2Point {
            x: (
                extract_bytesn(env, &proof_b, 0, 32)?,
                extract_bytesn(env, &proof_b, 32, 64)?,
            ),
            y: (
                extract_bytesn(env, &proof_b, 64, 96)?,
                extract_bytesn(env, &proof_b, 96, 128)?,
            ),
        },
        c: zk_verifier_client::G1Point {
            x: extract_bytesn(env, &proof_c, 0, 32)?,
            y: extract_bytesn(env, &proof_c, 32, 64)?,
        },
    };

    let request = zk_verifier_client::AccessRequest {
        user: user.clone(),
        resource_id,
        proof,
        public_inputs,
        expires_at,
        nonce,
    };

    let is_valid = client.zk_verify_access(&request);
    if is_valid {
        super::events::emit_zk_credential_verified(env, user.clone(), true);
    }
    Ok(is_valid)
}
