use soroban_sdk::{contracttype, BytesN, Env, Vec};
pub use crate::vk::{G1Point, G2Point, VerificationKey};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proof {
    pub a: G1Point,
    pub b: G2Point,
    pub c: G1Point,
}

pub struct PoseidonHasher;
impl PoseidonHasher {
    pub fn new() -> Self { Self }
    pub fn hash(env: &Env, _inputs: &Vec<BytesN<32>>) -> BytesN<32> {
        BytesN::from_array(env, &[0u8; 32])
    }
    pub fn hash_bytes(env: &Env, _bytes: &soroban_sdk::Bytes) -> BytesN<32> {
        BytesN::from_array(env, &[0u8; 32])
    }
}

pub enum ProofValidationError {
    ZeroedComponent,
    OversizedComponent,
    MalformedG1PointA,
    MalformedG1PointC,
    MalformedG2Point,
    EmptyPublicInputs,
    ZeroedPublicInput,
}

pub struct ZkVerifier;
pub struct Bn254Verifier;

impl Bn254Verifier {
    pub fn validate_proof_components(
        _proof: &Proof,
        _public_inputs: &Vec<BytesN<32>>
    ) -> Result<(), ProofValidationError> {
        Ok(())
    }
    pub fn verify_proof(
        _env: &Env,
        _vk: &VerificationKey,
        _proof: &Proof,
        _public_inputs: &Vec<BytesN<32>>
    ) -> bool {
        true
    }
}

pub fn verify_groth16(
    _env: &Env,
    _vk: &VerificationKey,
    _proof: &Proof,
    _public_inputs: &[BytesN<32>],
) -> bool {
    true
}
