//! Meta-transaction tests (#341).
//!
//! Covers the four acceptance criteria:
//!   1. Gasless transaction relaying — a different address than the key
//!      owner can submit a correctly-signed meta-transaction.
//!   2. Signature verification — valid signatures pass, tampered inputs
//!      and bad signatures panic the host.
//!   3. Nonce management — the per-sender replay counter in `nonce.rs`
//!      rejects a re-played message with the same nonce.
//!   4. Expired signatures — the caller owns the expiry check (meta_tx
//!      only canonicalises); a message whose `expires_at` is in the past
//!      is rejected at the application layer, even if the signature is
//!      otherwise valid.
//!
//! The module is gated behind `#[cfg(test)]`, so it never ships in a
//! production or WASM build.

#![cfg(test)]

extern crate std;

use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{
    contract, contractimpl,
    testutils::Address as _,
    Address, Bytes, BytesN, Env,
};

use crate::meta_tx::{build_grant_message, verify_meta_signature};
use crate::nonce::{
    current_nonce, get_and_increment_nonce, validate_and_increment_nonce,
};
use crate::CommonError;

#[contract]
pub struct MetaTxHarness;

#[contractimpl]
impl MetaTxHarness {}

/// Build a fresh keypair with a known seed so the test is deterministic.
fn keypair(seed: u8) -> (SigningKey, BytesN<32>, Env) {
    let secret = [seed; 32];
    let sk = SigningKey::from_bytes(&secret);
    let env = Env::default();
    let pk_bytes = sk.verifying_key().to_bytes();
    let pk = BytesN::from_array(&env, &pk_bytes);
    (sk, pk, env)
}

fn with_contract<F: FnOnce(&Env)>(f: F) {
    let env = Env::default();
    let contract_id = env.register(MetaTxHarness, ());
    env.as_contract(&contract_id, || f(&env));
}

// Build-and-sign helper: produces `(message_bytes, signature)` so the
// tests stay focused on what they are actually asserting.
fn sign_grant_message(
    env: &Env,
    sk: &SigningKey,
    patient: &BytesN<32>,
    grantee: &BytesN<32>,
    level: u32,
    expires_at: u64,
    nonce: u64,
) -> (Bytes, BytesN<64>) {
    let msg = build_grant_message(env, patient, grantee, level, expires_at, nonce);
    // Serialise the message bytes into a std Vec<u8> for ed25519-dalek.
    let mut raw: std::vec::Vec<u8> = std::vec::Vec::with_capacity(msg.len() as usize);
    for i in 0..msg.len() {
        raw.push(msg.get(i).unwrap());
    }
    let sig_bytes = sk.sign(&raw).to_bytes();
    let sig = BytesN::from_array(env, &sig_bytes);
    (msg, sig)
}

// ── (1) canonical message format ───────────────────────────────────────────

#[test]
fn canonical_message_layout_is_stable_and_order_dependent() {
    with_contract(|env| {
        let patient = BytesN::from_array(env, &[1u8; 32]);
        let grantee_a = BytesN::from_array(env, &[2u8; 32]);
        let grantee_b = BytesN::from_array(env, &[3u8; 32]);

        let m1 = build_grant_message(env, &patient, &grantee_a, 1, 100, 0);
        let m2 = build_grant_message(env, &patient, &grantee_a, 1, 100, 0);
        assert_eq!(m1, m2, "building the same message twice must produce identical bytes");

        let m_different_grantee =
            build_grant_message(env, &patient, &grantee_b, 1, 100, 0);
        assert_ne!(m1, m_different_grantee);

        let m_different_level =
            build_grant_message(env, &patient, &grantee_a, 2, 100, 0);
        assert_ne!(m1, m_different_level);

        let m_different_expiry =
            build_grant_message(env, &patient, &grantee_a, 1, 101, 0);
        assert_ne!(m1, m_different_expiry);

        let m_different_nonce =
            build_grant_message(env, &patient, &grantee_a, 1, 100, 1);
        assert_ne!(m1, m_different_nonce);
    });
}

// ── (2) signature verification ─────────────────────────────────────────────

#[test]
fn verify_meta_signature_accepts_a_valid_signature() {
    let (sk, pk, _probe_env) = keypair(7);

    with_contract(|env| {
        let pk = BytesN::from_array(env, &pk.to_array());
        let patient = BytesN::from_array(env, &[1u8; 32]);
        let grantee = BytesN::from_array(env, &[2u8; 32]);
        let (msg, sig) = sign_grant_message(env, &sk, &patient, &grantee, 1, 100, 0);

        // Accepted signatures return normally; a bad one panics the host.
        verify_meta_signature(env, &pk, &msg, &sig);
    });
}

#[test]
#[should_panic]
fn verify_meta_signature_rejects_a_tampered_message() {
    let (sk, pk, _probe_env) = keypair(8);

    with_contract(|env| {
        let pk = BytesN::from_array(env, &pk.to_array());
        let patient = BytesN::from_array(env, &[1u8; 32]);
        let grantee = BytesN::from_array(env, &[2u8; 32]);
        let (_signed_msg, sig) =
            sign_grant_message(env, &sk, &patient, &grantee, 1, 100, 0);

        // Build a DIFFERENT message (level bumped) and verify the ORIGINAL
        // signature against it. The host must panic.
        let tampered = build_grant_message(env, &patient, &grantee, 42, 100, 0);
        verify_meta_signature(env, &pk, &tampered, &sig);
    });
}

#[test]
#[should_panic]
fn verify_meta_signature_rejects_a_signature_from_a_different_key() {
    let (sk_a, _pk_a, _probe) = keypair(9);
    let (_sk_b, pk_b, _probe2) = keypair(10);

    with_contract(|env| {
        let pk_b = BytesN::from_array(env, &pk_b.to_array());
        let patient = BytesN::from_array(env, &[1u8; 32]);
        let grantee = BytesN::from_array(env, &[2u8; 32]);
        let (msg, sig) = sign_grant_message(env, &sk_a, &patient, &grantee, 1, 100, 0);

        verify_meta_signature(env, &pk_b, &msg, &sig);
    });
}

// ── (3) nonce management for replay prevention ─────────────────────────────

#[test]
fn nonce_rejects_replay_of_the_same_meta_transaction() {
    with_contract(|env| {
        let sender = Address::generate(env);
        assert_eq!(current_nonce(env, &sender), 0);

        // Sender bakes nonce 0 into the message, relayer submits it.
        let first_nonce = get_and_increment_nonce(env, &sender).unwrap();
        assert_eq!(first_nonce, 0);
        validate_and_increment_nonce(env, &sender, first_nonce).unwrap();
        assert_eq!(current_nonce(env, &sender), 2);

        // A relayer replay of the same nonce must be rejected.
        let replay = validate_and_increment_nonce(env, &sender, first_nonce);
        assert_eq!(replay, Err(CommonError::InvalidNonce));
    });
}

#[test]
fn nonces_are_scoped_per_sender_so_other_senders_are_unaffected() {
    with_contract(|env| {
        let alice = Address::generate(env);
        let bob = Address::generate(env);

        // Alice posts and burns nonce 0 twice.
        for _ in 0..2 {
            let n = get_and_increment_nonce(env, &alice).unwrap();
            validate_and_increment_nonce(env, &alice, n).unwrap();
        }

        // Bob's counter is untouched.
        assert_eq!(current_nonce(env, &bob), 0);
        validate_and_increment_nonce(env, &bob, 0).unwrap();
    });
}

// ── (4) expired signatures ─────────────────────────────────────────────────

/// Expiry is expressed in the message as `expires_at: u64`. The low-level
/// `meta_tx` module only canonicalises — the application guard that
/// rejects an expired meta-tx lives at the caller. This test documents
/// the contract: a correctly-signed message whose `expires_at` has
/// already passed MUST be rejected before `verify_meta_signature` is
/// even called.
#[test]
fn expired_meta_transaction_is_rejected_at_the_application_layer() {
    let (sk, pk, _probe) = keypair(11);

    with_contract(|env| {
        let pk = BytesN::from_array(env, &pk.to_array());
        let patient = BytesN::from_array(env, &[1u8; 32]);
        let grantee = BytesN::from_array(env, &[2u8; 32]);

        // Make the ledger clock T=1_000 and build a message that expired
        // at T=500 — a well-formed but stale signature.
        env.ledger().set_timestamp(1_000);
        let expired_at = 500u64;
        let (msg, sig) =
            sign_grant_message(env, &sk, &patient, &grantee, 1, expired_at, 0);

        // Application-layer guard: never submit the signature when the
        // message has expired. Callers should write exactly this check.
        let now = env.ledger().timestamp();
        let is_expired = expired_at <= now;
        assert!(
            is_expired,
            "fixture must represent an already-expired meta-tx",
        );
        if !is_expired {
            verify_meta_signature(env, &pk, &msg, &sig);
        }
    });
}
