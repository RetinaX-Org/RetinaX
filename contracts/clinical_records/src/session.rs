//! Session-key authentication for clinical records.
//!
//! A principal (a provider, or any address holding permissions) signs once with its full key to
//! open a session for a short-lived session key. The session key can then call clinical record
//! functions in the principal's place, passing its own address as `caller`. Each call is checked
//! against the session's auth level and expiry, then authorized as the principal.
//!
//! This is the authorization counterpart of the key_manager `Session` key level: a short-lived
//! key below the principal's full key. Session state (issue time, expiry, max level) is kept by
//! [`common::session`]; this module only records which principal each session key acts for.
//!
//! Required levels:
//!
//! | Operation | Level |
//! |-----------|-------|
//! | Read an examination | [`READ_LEVEL`] (Level 1) |
//! | Write an examination or link its patient | [`EXAM_WRITE_LEVEL`] (Level 2) |
//! | Any prescription read or write | [`PRESCRIPTION_LEVEL`] (Level 3) |
//!
//! Callers without a session (full keys and the `vision_records` contract) skip the level check,
//! but every caller, session key or not, must sign its call.

use crate::admin::require_initialized;
use crate::errors::ContractError;
use common::progressive_auth::AuthLevel;
use common::session as auth_session;
use soroban_sdk::{contracttype, symbol_short, Address, Env, Symbol};

/// Maps a session key to the principal it acts for.
pub const SESSION_GRANT: Symbol = symbol_short!("SESS_GRT");

/// Longest session a principal can open: 24 hours, in seconds.
pub const MAX_SESSION_TTL_SECS: u64 = 86_400;

/// Level a session needs to read examinations.
pub const READ_LEVEL: AuthLevel = AuthLevel::Level1;
/// Level a session needs to write examinations.
pub const EXAM_WRITE_LEVEL: AuthLevel = AuthLevel::Level2;
/// Level a session needs for any prescription operation, which are the sensitive ones.
pub const PRESCRIPTION_LEVEL: AuthLevel = AuthLevel::Level3;

const TTL_THRESHOLD: u32 = 5_184_000;
const TTL_EXTEND_TO: u32 = 10_368_000;

/// A session key's grant, as returned by `get_session`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClinicalSession {
    /// Address whose permissions the session key uses.
    pub principal: Address,
    /// Short-lived address that signs calls during the session.
    pub session_key: Address,
    /// Highest auth level the session grants.
    pub max_level: AuthLevel,
    /// Ledger timestamp in seconds when the session was first started. Refreshing keeps it.
    pub issued_at: u64,
    /// Ledger timestamp in seconds after which the session is rejected.
    pub expires_at: u64,
}

fn grant_key(session_key: &Address) -> (Symbol, Address) {
    (SESSION_GRANT, session_key.clone())
}

fn get_principal(env: &Env, session_key: &Address) -> Option<Address> {
    env.storage().persistent().get(&grant_key(session_key))
}

/// Opens, or refreshes, a session that lets `session_key` act for `principal`.
///
/// Requires authorization from `principal`. Refreshing resets the expiry to `ttl_seconds` from
/// now and replaces the level.
///
/// # Errors
/// Returns `NotInitialized`, `InvalidInput` if `session_key` equals `principal` or `ttl_seconds`
/// is 0 or above `MAX_SESSION_TTL_SECS`, or `Unauthorized` if `session_key` has an unexpired
/// session for a different principal.
pub fn start_session(
    env: &Env,
    principal: Address,
    session_key: Address,
    max_level: AuthLevel,
    ttl_seconds: u64,
) -> Result<ClinicalSession, ContractError> {
    require_initialized(env)?;
    principal.require_auth();

    if session_key == principal || ttl_seconds == 0 || ttl_seconds > MAX_SESSION_TTL_SECS {
        return Err(ContractError::InvalidInput);
    }
    if let Some(existing) = get_session(env, &session_key) {
        if existing.principal != principal {
            if env.ledger().timestamp() <= existing.expires_at {
                return Err(ContractError::Unauthorized);
            }
            // An expired key moving to a new principal starts a fresh session.
            auth_session::end_session(env, &session_key);
        }
    }

    let key = grant_key(&session_key);
    env.storage().persistent().set(&key, &principal);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
    // Decay is disabled: the level holds for the whole session and expiry ends it.
    let session =
        auth_session::start_or_refresh_session(env, &session_key, max_level, ttl_seconds, 0);

    crate::events::publish_session_started(
        env,
        principal.clone(),
        session_key.clone(),
        session.expires_at,
    );
    Ok(ClinicalSession {
        principal,
        session_key,
        max_level: session.max_level,
        issued_at: session.issued_at,
        expires_at: session.expires_at,
    })
}

/// Ends `session_key`'s session before it expires.
///
/// Requires authorization from `principal`, which must be the principal the session acts for.
///
/// # Errors
/// Returns `NotInitialized`, `RecordNotFound` if `session_key` has no session, or `Unauthorized`
/// if the session belongs to a different principal.
pub fn revoke_session(
    env: &Env,
    principal: Address,
    session_key: Address,
) -> Result<(), ContractError> {
    require_initialized(env)?;
    principal.require_auth();

    let owner = get_principal(env, &session_key).ok_or(ContractError::RecordNotFound)?;
    if owner != principal {
        return Err(ContractError::Unauthorized);
    }

    env.storage().persistent().remove(&grant_key(&session_key));
    auth_session::end_session(env, &session_key);
    crate::events::publish_session_revoked(env, principal, session_key);
    Ok(())
}

/// Returns `session_key`'s session, including an expired one, or `None` if it has none.
pub fn get_session(env: &Env, session_key: &Address) -> Option<ClinicalSession> {
    let principal = get_principal(env, session_key)?;
    let session = auth_session::get_session(env, session_key)?;
    Some(ClinicalSession {
        principal,
        session_key: session_key.clone(),
        max_level: session.max_level,
        issued_at: session.issued_at,
        expires_at: session.expires_at,
    })
}

/// Authenticates `caller` and returns the address to authorize it as.
///
/// Every caller must sign the call. When the `vision_records` contract calls in, Soroban
/// authorizes its own address automatically. A caller without a session is returned unchanged.
/// A session key's session must not have expired and its level must be at least `required`;
/// the principal is then returned.
///
/// # Errors
/// Returns `SessionExpired` if the session has expired, or `InsufficientAuthLevel` if its level
/// is below `required`.
pub fn resolve_caller(
    env: &Env,
    caller: &Address,
    required: AuthLevel,
) -> Result<Address, ContractError> {
    caller.require_auth();
    let Some(principal) = get_principal(env, caller) else {
        return Ok(caller.clone());
    };

    let session = auth_session::get_session(env, caller).ok_or(ContractError::SessionExpired)?;
    let level =
        auth_session::effective_level(env, &session).ok_or(ContractError::SessionExpired)?;
    if level.rank() < required.rank() {
        return Err(ContractError::InsufficientAuthLevel);
    }
    Ok(principal)
}
