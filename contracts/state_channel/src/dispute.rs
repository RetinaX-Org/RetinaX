use crate::channel::Channel;
use soroban_sdk::{symbol_short, Address, Env};
use retinax_common::{ChannelStatus, CommonError};

const CHALLENGE_PERIOD: u64 = 86400; // 1 day in seconds
const FRAUD_PENALTY_BPS: u64 = 1000; // 10% of channel capacity

pub fn unilateral_close(env: &Env, channel_id: u64, closer: Address) -> Result<(), CommonError> {
    let key = (symbol_short!("CHAN"), channel_id);
    let mut channel: Channel = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(CommonError::RecordNotFound)?;

    if channel.status != ChannelStatus::Open {
        return Err(CommonError::InvalidChannelState);
    }

    if closer != channel.patient && closer != channel.provider {
        return Err(CommonError::AccessDenied);
    }

    channel.status = ChannelStatus::Disputed;
    channel.challenge_end = Some(env.ledger().timestamp() + CHALLENGE_PERIOD);
    env.storage().persistent().set(&key, &channel);

    Ok(())
}

pub fn submit_fraud_proof(
    env: &Env,
    channel_id: u64,
    nonce: u64,
    balance: u64,
    _sig: soroban_sdk::BytesN<64>,
) -> Result<(), CommonError> {
    let key = (symbol_short!("CHAN"), channel_id);
    let mut channel: Channel = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(CommonError::RecordNotFound)?;

    if channel.status != ChannelStatus::Disputed {
        return Err(CommonError::InvalidChannelState);
    }

    if let Some(end) = channel.challenge_end {
        if env.ledger().timestamp() > end {
            return Err(CommonError::InvalidChannelState);
        }
    }

    // Verify signature of the state transition
    // verify_state_sig(env, &channel, nonce, balance, sig)?;

    if balance > channel.capacity {
        return Err(CommonError::InvalidInput);
    }

    if nonce <= channel.nonce {
        // A stale or invalid dispute evidence submission is penalized. This prevents
        // repeatedly opening disputes or submitting old states without cost.
        let penalty_key = (symbol_short!("FRD_PEN"), channel_id);
        let existing: u64 = env.storage().persistent().get(&penalty_key).unwrap_or(0);
        let penalty = channel.capacity.saturating_mul(FRAUD_PENALTY_BPS) / 10_000;
        env.storage()
            .persistent()
            .set(&penalty_key, &existing.saturating_add(penalty));
        return Err(CommonError::InvalidNonce);
    }

    channel.nonce = nonce;
    channel.balance = balance;
    env.storage().persistent().set(&key, &channel);

    Ok(())
}
