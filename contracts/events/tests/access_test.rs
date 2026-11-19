#![cfg(test)]

use soroban_sdk::{testutils::Address as _, Address, Env, String};

use events::{EventError, EventStreamContract, EventStreamContractClient};

fn setup() -> (Env, EventStreamContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(EventStreamContract, ());
    let client = EventStreamContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let random_user = Address::generate(&env);

    client.initialize(&admin);

    (env, client, admin, random_user)
}

#[test]
fn test_register_schema_unauthorized() {
    let (env, client, _admin, random_user) = setup();

    let topic = String::from_str(&env, "test-topic");
    let schema_hash = String::from_str(&env, "hash");

    let result = client.try_register_schema(&random_user, &topic, &1, &schema_hash);

    // register_schema enforces admin-only access
    assert_eq!(result, Err(Ok(EventError::Unauthorized)));
}

#[test]
fn test_admin_only_function_reverts_for_random_user() {
    let (env, client, admin, random_user) = setup();

    // Admin can read the admin address
    assert_eq!(client.get_admin(), admin);

    // Non-admin fails the admin check when calling an admin-only function
    let topic = String::from_str(&env, "admin.topic");
    let hash = String::from_str(&env, "sha256:admin");
    assert_eq!(
        client.try_register_schema(&random_user, &topic, &1, &hash),
        Err(Ok(EventError::Unauthorized))
    );
}

#[test]
fn test_initialize_twice_fails() {
    let (_env, client, _admin, random_user) = setup();

    let result = client.try_initialize(&random_user);

    assert_eq!(result, Err(Ok(EventError::AlreadyInitialized)));
}
