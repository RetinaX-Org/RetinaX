use soroban_sdk::{Env, Symbol};

/// Sets the contract panic state flag in instance storage.
pub fn set_panic(env: &Env, value: bool) {
    env.storage().instance().set(&Symbol::short("PANIC"), &value);
}

/// Checks whether panic mode is currently enabled in instance storage.
pub fn is_panic(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&Symbol::short("PANIC"))
        .unwrap_or(false)
}

/// Records the timestamp of the most recent key rotation in instance storage.
pub fn set_last_rotation(env: &Env, timestamp: u64) {
    env.storage().instance().set(&Symbol::short("LAST_ROT"), &timestamp);
}

/// Retrieves the timestamp of the last recorded key rotation from instance storage.
pub fn get_last_rotation(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&Symbol::short("LAST_ROT"))
        .unwrap_or(0)
}