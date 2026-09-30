use crate::{
    circuit_breaker::PauseScope, ConsentType, ContractError, VisionRecordsContract,
    VisionRecordsContractClient,
};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env};

fn setup() -> (
    Env,
    VisionRecordsContractClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(VisionRecordsContract, ());
    let client = VisionRecordsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let patient = Address::generate(&env);
    let grantee = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin, patient, grantee)
}

#[test]
fn grant_consent_reverts_when_grant_endpoint_is_paused() {
    let (_env, client, admin, patient, grantee) = setup();
    client.pause_contract(&admin, &PauseScope::Function(symbol_short!("GNT_CNS")));

    let result = client.try_grant_consent(&patient, &grantee, &ConsentType::Treatment, &3600);
    assert_eq!(result.unwrap_err().unwrap(), ContractError::Paused);
}

#[test]
fn grant_consent_reverts_for_zero_or_excessive_duration() {
    let (_env, client, _admin, patient, grantee) = setup();

    let zero = client.try_grant_consent(&patient, &grantee, &ConsentType::Treatment, &0);
    assert_eq!(zero.unwrap_err().unwrap(), ContractError::InvalidInput);

    let excessive = client.try_grant_consent(
        &patient,
        &grantee,
        &ConsentType::Treatment,
        &(157_680_000_u64 + 1),
    );
    assert_eq!(excessive.unwrap_err().unwrap(), ContractError::InvalidInput);
}

#[test]
fn revoke_consent_reverts_when_revoke_endpoint_is_paused() {
    let (_env, client, admin, patient, grantee) = setup();
    client.pause_contract(&admin, &PauseScope::Function(symbol_short!("RVK_CNS")));

    let result = client.try_revoke_consent(&patient, &grantee);
    assert_eq!(result.unwrap_err().unwrap(), ContractError::Paused);
}
