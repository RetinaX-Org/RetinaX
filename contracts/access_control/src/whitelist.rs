use crate::admin::require_admin;
use crate::types::AccessControlError;
use soroban_sdk::{symbol_short, Address, Env, Symbol};

pub const WL_EN: Symbol = symbol_short!("WL_EN");
pub const WL: Symbol = symbol_short!("WL");

/// Turn whitelist enforcement on or off.
///
/// # Arguments
///
/// * `caller` - must be the administrator.
/// * `enabled` - `true` to enforce, `false` to allow every address.
///
/// # Errors
///
/// [`Unauthorized`](AccessControlError::Unauthorized) for non-admin callers.
pub fn set_whitelist_enabled(
    env: &Env,
    caller: &Address,
    enabled: bool,
) -> Result<(), AccessControlError> {
    require_admin(env, caller)?;
    env.storage().instance().set(&WL_EN, &enabled);
    Ok(())
}

/// Returns whether whitelist enforcement is on (`false` by default).
pub fn is_whitelist_enabled(env: &Env) -> bool {
    env.storage().instance().get(&WL_EN).unwrap_or(false)
}

/// Allow `user` when enforcement is on.
///
/// # Arguments
///
/// * `caller` - must be the administrator.
/// * `user` - address to add; idempotent if already present.
///
/// # Errors
///
/// [`Unauthorized`](AccessControlError::Unauthorized) for non-admin callers.
pub fn add_to_whitelist(
    env: &Env,
    caller: &Address,
    user: &Address,
) -> Result<(), AccessControlError> {
    require_admin(env, caller)?;
    env.storage().persistent().set(&(WL, user.clone()), &true);
    Ok(())
}

/// Remove `user` from the whitelist (no-op if absent).
///
/// # Arguments
///
/// * `caller` - must be the administrator.
/// * `user` - address to remove.
///
/// # Errors
///
/// [`Unauthorized`](AccessControlError::Unauthorized) for non-admin callers.
pub fn remove_from_whitelist(
    env: &Env,
    caller: &Address,
    user: &Address,
) -> Result<(), AccessControlError> {
    require_admin(env, caller)?;
    env.storage().persistent().remove(&(WL, user.clone()));
    Ok(())
}

/// Returns whether access is allowed: `true` when enforcement is disabled,
/// otherwise whether `user` is on the whitelist.
pub fn check_whitelist(env: &Env, user: &Address) -> bool {
    if !is_whitelist_enabled(env) {
        return true;
    }
    env.storage()
        .persistent()
        .get(&(WL, user.clone()))
        .unwrap_or(false)
}

/// Returns the raw whitelist flag for `user`, ignoring whether enforcement is
/// enabled (unlike [`check_whitelist`]).
pub fn is_whitelisted(env: &Env, user: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&(WL, user.clone()))
        .unwrap_or(false)
}
