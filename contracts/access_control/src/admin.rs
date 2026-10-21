use crate::types::AccessControlError;
use soroban_sdk::{symbol_short, Address, Env, Symbol};

pub const ADMIN: Symbol = symbol_short!("ADMIN");
pub const PENDING_ADMIN: Symbol = symbol_short!("PEND_ADM");

/// Registers `admin` as the contract administrator.
///
/// # Arguments
///
/// * `env` - host environment with contract storage.
/// * `admin` - address to install as the initial administrator; must have
///   authorized this call.
///
/// # Errors
///
/// [`AlreadyInitialized`](AccessControlError::AlreadyInitialized) if an
/// administrator is already registered.
pub fn initialize(env: &Env, admin: &Address) -> Result<(), AccessControlError> {
    if env.storage().instance().has(&ADMIN) {
        return Err(AccessControlError::AlreadyInitialized);
    }
    admin.require_auth();
    env.storage().instance().set(&ADMIN, admin);
    Ok(())
}

/// Authorizes administrative actions: requires `caller`'s signature and that
/// `caller` matches the registered administrator.
///
/// # Arguments
///
/// * `caller` - address claiming administrative rights; must authorize the call.
///
/// # Errors
///
/// [`Unauthorized`](AccessControlError::Unauthorized) when the contract was
/// never initialized or `caller` is not the registered administrator.
pub fn require_admin(env: &Env, caller: &Address) -> Result<(), AccessControlError> {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&ADMIN)
        .ok_or(AccessControlError::Unauthorized)?;

    if caller != &admin {
        return Err(AccessControlError::Unauthorized);
    }
    Ok(())
}

/// Returns the registered administrator address, or `None` before
/// [`initialize`].
pub fn get_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&ADMIN)
}

/// Stage a transfer of the administrator role to `new_admin` (step 1 of a
/// 2-step handshake that avoids typos locking the contract out).
///
/// # Arguments
///
/// * `current_admin` - acting administrator; must authorize the call.
/// * `new_admin` - address nominated to take over.
///
/// # Errors
///
/// [`Unauthorized`](AccessControlError::Unauthorized) if `current_admin` is not
/// the registered administrator.
pub fn propose_admin(
    env: &Env,
    current_admin: &Address,
    new_admin: &Address,
) -> Result<(), AccessControlError> {
    require_admin(env, current_admin)?;
    env.storage().instance().set(&PENDING_ADMIN, new_admin);
    Ok(())
}

/// Complete the pending administrator transfer (step 2 of the 2-step
/// handshake); promotes `new_admin` and clears the pending nomination.
///
/// # Arguments
///
/// * `new_admin` - nominated address; must authorize the call.
///
/// # Errors
///
/// * [`NotFound`](AccessControlError::NotFound) - no transfer is pending.
/// * [`Unauthorized`](AccessControlError::Unauthorized) - `new_admin` does not
///   match the pending nomination.
pub fn accept_admin(env: &Env, new_admin: &Address) -> Result<(), AccessControlError> {
    new_admin.require_auth();
    let pending: Address = env
        .storage()
        .instance()
        .get(&PENDING_ADMIN)
        .ok_or(AccessControlError::NotFound)?;

    if new_admin != &pending {
        return Err(AccessControlError::Unauthorized);
    }

    env.storage().instance().set(&ADMIN, new_admin);
    env.storage().instance().remove(&PENDING_ADMIN);
    Ok(())
}

/// Returns the pending administrator nominee, or `None` if no transfer is
/// staged.
pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&PENDING_ADMIN)
}
