#![no_std]
use soroban_sdk::{contract, contractimpl, Address, Env, String};

/*
 * RetinaX: Rust Integration Example
 * Add to Cargo.toml: soroban-sdk = "22.0.0"
 *
 * PREREQUISITE:
 * This example requires the RetinaX smart contract to be built first, as the macro
 * resolves the WASM file at compile time.
 * Run `stellar contract build` or `cargo build --manifest-path=...` for the
 * retinax contract before compiling this example.
 */

// Generate Client bindings from compiled WASM
soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/retinax_contract.wasm");

#[contract]
pub struct ThirdPartyApp;

#[contractimpl]
impl ThirdPartyApp {
    /// ⚠️  WARNING: Accepting the target contract address as a parameter is for
    ///    illustration only. In production, store and read the trusted contract
    ///    address from contract storage to prevent callers from redirecting calls
    ///    to arbitrary contracts.
    pub fn interact_with_retinax(env: Env, retinax_contract_id: Address, user_data: String) -> String {
        let retinax_client = Client::new(&env, &retinax_contract_id);

        // Call the functions directly via the generated bindings
        let result = retinax_client.create_record(&user_data);
        result
    }
}
