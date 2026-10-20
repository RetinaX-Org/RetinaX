//! Multisig configuration and admin action proposals.

use super::*;
use teye_common::multisig;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl VisionRecordsContract {
    // ── Multisig management ──────────────────────────────────────────────────

    /// Configure M-of-N multisig for admin operations.
    pub fn configure_multisig(
        env: Env,
        caller: Address,
        signers: soroban_sdk::Vec<Address>,
        threshold: u32,
    ) -> Result<(), ContractError> {
        if !Self::is_initialized(env.clone()) {
            return Err(ContractError::NotInitialized);
        }
        caller.require_auth();

        let admin = Self::get_admin(env.clone())?;
        if caller != admin {
            return Err(ContractError::Unauthorized);
        }

        multisig::configure(&env, signers, threshold).map_err(|_| ContractError::InvalidInput)
    }

    pub fn propose_admin_action(
        env: Env,
        proposer: Address,
        action: Symbol,
        data_hash: BytesN<32>,
    ) -> Result<u64, ContractError> {
        if !Self::is_initialized(env.clone()) {
            return Err(ContractError::NotInitialized);
        }
        proposer.require_auth();

        multisig::propose(&env, &proposer, action, data_hash)
            .map_err(|_| ContractError::Unauthorized)
    }

    pub fn approve_admin_action(
        env: Env,
        approver: Address,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        if !Self::is_initialized(env.clone()) {
            return Err(ContractError::NotInitialized);
        }
        approver.require_auth();

        multisig::approve(&env, &approver, proposal_id).map_err(|_| ContractError::Unauthorized)
    }

    pub fn get_multisig_config(env: Env) -> Option<multisig::MultisigConfig> {
        multisig::get_config(&env)
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Option<multisig::Proposal> {
        multisig::get_proposal(&env, proposal_id)
    }
}
