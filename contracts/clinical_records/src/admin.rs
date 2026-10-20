use crate::errors::ContractError;
use soroban_sdk::{symbol_short, Address, Env, Symbol};

/// Storage key for admin address.
pub const ADMIN: Symbol = symbol_short!("ADMIN");
/// Storage key for initialization flag.
pub const INITIALIZED: Symbol = symbol_short!("INIT");
/// Storage key for Vision Records contract address.
pub const VISION_RECORDS: Symbol = symbol_short!("V_REC");

/// Initialize the contract with admin and Vision Records contract.
///
/// Can only be called once. Admin must authorize the initialization.
/// Vision Records contract becomes the only authorized caller for
/// examination and prescription operations.
///
/// # Parameters
/// - `env`: Soroban environment
/// - `admin`: Address that will have administrative privileges
/// - `vision_records`: Vision Records contract address (authorized caller)
///
/// # Errors
/// - [`ContractError::AlreadyInitialized`] if contract already initialized
pub fn initialize(env: &Env, admin: Address, vision_records: Address) -> Result<(), ContractError> {
    if env.storage().instance().has(&INITIALIZED) {
        return Err(ContractError::AlreadyInitialized);
    }
    admin.require_auth();
    env.storage().instance().set(&ADMIN, &admin);
    env.storage().instance().set(&VISION_RECORDS, &vision_records);
    env.storage().instance().set(&INITIALIZED, &true);
    Ok(())
}

/// Get the contract admin address.
///
/// # Parameters
/// - `env`: Soroban environment
///
/// # Returns
/// Admin [`Address`]
///
/// # Errors
/// - [`ContractError::NotInitialized`] if contract not initialized
pub fn get_admin(env: &Env) -> Result<Address, ContractError> {
    env.storage()
        .instance()
        .get(&ADMIN)
        .ok_or(ContractError::NotInitialized)
}

/// Update the Vision Records contract address (admin only).
///
/// Admin must authorize. New address becomes the sole authorized
/// caller for clinical data operations.
///
/// # Parameters
/// - `env`: Soroban environment
/// - `admin`: Must match stored admin address
/// - `vision_records`: New Vision Records contract address
///
/// # Errors
/// - [`ContractError::Unauthorized`] if caller is not admin
/// - [`ContractError::NotInitialized`] if contract not initialized
pub fn set_vision_records_contract(
    env: &Env,
    admin: Address,
    vision_records: Address,
) -> Result<(), ContractError> {
    admin.require_auth();
    let stored_admin = get_admin(env)?;
    if admin != stored_admin {
        return Err(ContractError::Unauthorized);
    }
    env.storage().instance().set(&VISION_RECORDS, &vision_records);
    Ok(())
}

/// Get the current Vision Records contract address.
///
/// # Parameters
/// - `env`: Soroban environment
///
/// # Returns
/// [`Option<Address>`] of Vision Records contract, [`None`] if not set
pub fn get_vision_records_contract(env: &Env) -> Option<Address> {
    env.storage().instance().get(&VISION_RECORDS)
}

/// Verify caller is the authorized Vision Records contract.
///
/// Used by all examination and prescription functions to enforce
/// that only the Vision Records contract can modify clinical data.
///
/// # Parameters
/// - `env`: Soroban environment
/// - `caller`: Address to verify
///
/// # Errors
/// - [`ContractError::NotInitialized`] if Vision Records not set
/// - [`ContractError::Unauthorized`] if caller != Vision Records
pub fn require_authorized_vision_records(env: &Env, caller: &Address) -> Result<(), ContractError> {
    let allowed = env
        .storage()
        .instance()
        .get(&VISION_RECORDS)
        .ok_or(ContractError::NotInitialized)?;
    if *caller == allowed {
        Ok(())
    } else {
        Err(ContractError::Unauthorized)
    }
}