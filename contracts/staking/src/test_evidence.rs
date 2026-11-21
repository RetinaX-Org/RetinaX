extern crate std;

use soroban_sdk::{
    testutils::Address as _, token::Client as TokenClient, token::StellarAssetClient,
    Address, BytesN, Env,
};

use crate::{ContractError, EvidenceStatus, StakingContract, StakingContractClient};

fn setup() -> (Env, StakingContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let stake_token = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let reward_token = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let contract_id = env.register(StakingContract, ());
    let client = StakingContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(
        &admin,
        &stake_token.address(),
        &reward_token.address(),
        &0,
        &0,
    );
    (env, client, admin, stake_token.address())
}

fn prepare(
    env: &Env,
    client: &StakingContractClient,
    stake_token: &Address,
) -> (Address, Address, BytesN<32>) {
    let validator = Address::generate(env);
    let reporter = Address::generate(env);
    StellarAssetClient::new(env, stake_token).mint(&validator, &1_000);
    StellarAssetClient::new(env, stake_token).mint(&reporter, &200);
    client.stake(&validator, &1_000);
    (validator, reporter, BytesN::from_array(env, &[7; 32]))
}

#[test]
fn submission_escrows_bond_without_slashing() {
    let (env, client, _admin, stake_token) = setup();
    let (validator, reporter, hash) = prepare(&env, &client, &stake_token);
    let balance = TokenClient::new(&env, &stake_token);
    let evidence_id = client.submit_evidence(&reporter, &validator, &hash, &400, &50);

    assert_eq!(evidence_id, 1);
    assert_eq!(balance.balance(&reporter), 150);
    assert_eq!(balance.balance(&client.address), 1_050);
    assert_eq!(client.get_staked(&validator), 1_000);
    let evidence = client.get_evidence(&evidence_id);
    assert_eq!(evidence.status, EvidenceStatus::Pending);
    assert_eq!(evidence.requested_amount, 400);
    assert_eq!(evidence.evidence_hash, hash);
    assert_eq!(evidence.slash_id, 0);
}

#[test]
fn confirmed_evidence_refunds_bond_and_can_be_appealed() {
    let (env, client, admin, stake_token) = setup();
    let (validator, reporter, hash) = prepare(&env, &client, &stake_token);
    let evidence_id = client.submit_evidence(&reporter, &validator, &hash, &400, &50);
    let slash_id = client.confirm_evidence(&admin, &evidence_id);

    assert_eq!(slash_id, 1);
    assert_eq!(client.get_evidence(&evidence_id).status, EvidenceStatus::Confirmed);
    assert_eq!(client.get_evidence(&evidence_id).slash_id, slash_id);
    assert_eq!(client.get_slash_record(&slash_id).amount, 400);
    assert_eq!(client.get_staked(&validator), 600);
    assert_eq!(client.get_total_staked(), 600);
    assert_eq!(TokenClient::new(&env, &stake_token).balance(&reporter), 200);

    let appeal_id = client.file_appeal(&validator, &slash_id, &200);
    client.resolve_appeal(&admin, &appeal_id, &200);
    assert_eq!(client.get_staked(&validator), 800);

    assert!(matches!(
        client.try_confirm_evidence(&admin, &evidence_id),
        Err(Ok(ContractError::EvidenceNotPending))
    ));
    assert_eq!(client.get_staked(&validator), 800);
}

#[test]
fn rejection_forfeits_bond_and_never_slashes() {
    let (env, client, admin, stake_token) = setup();
    let (validator, reporter, hash) = prepare(&env, &client, &stake_token);
    let evidence_id = client.submit_evidence(&reporter, &validator, &hash, &300, &50);

    client.reject_evidence(&admin, &evidence_id);
    assert_eq!(client.get_evidence(&evidence_id).status, EvidenceStatus::Rejected);
    assert_eq!(client.get_evidence(&evidence_id).slash_id, 0);
    assert_eq!(client.get_staked(&validator), 1_000);
    let token = TokenClient::new(&env, &stake_token);
    assert_eq!(token.balance(&reporter), 150);
    assert_eq!(token.balance(&admin), 50);
    assert!(matches!(
        client.try_reject_evidence(&admin, &evidence_id),
        Err(Ok(ContractError::EvidenceNotPending))
    ));
    assert!(matches!(
        client.try_confirm_evidence(&admin, &evidence_id),
        Err(Ok(ContractError::EvidenceNotPending))
    ));
}

#[test]
fn only_governance_can_review_submissions() {
    let (env, client, _admin, stake_token) = setup();
    let (validator, reporter, hash) = prepare(&env, &client, &stake_token);
    let outsider = Address::generate(&env);
    let evidence_id = client.submit_evidence(&reporter, &validator, &hash, &300, &50);

    assert!(matches!(
        client.try_confirm_evidence(&outsider, &evidence_id),
        Err(Ok(ContractError::Unauthorized))
    ));
    assert!(matches!(
        client.try_reject_evidence(&outsider, &evidence_id),
        Err(Ok(ContractError::Unauthorized))
    ));
    assert_eq!(client.get_evidence(&evidence_id).status, EvidenceStatus::Pending);
}

#[test]
fn validates_amount_bond_validator_and_unknown_evidence() {
    let (env, client, admin, stake_token) = setup();
    let (validator, reporter, hash) = prepare(&env, &client, &stake_token);
    let unstaked = Address::generate(&env);

    assert!(matches!(
        client.try_submit_evidence(&reporter, &validator, &hash, &0, &10),
        Err(Ok(ContractError::InvalidInput))
    ));
    assert!(matches!(
        client.try_submit_evidence(&reporter, &validator, &hash, &100, &0),
        Err(Ok(ContractError::EvidenceBondInvalid))
    ));
    assert!(matches!(
        client.try_submit_evidence(&reporter, &unstaked, &hash, &100, &10),
        Err(Ok(ContractError::InsufficientBalance))
    ));
    assert!(matches!(
        client.try_get_evidence(&99),
        Err(Ok(ContractError::EvidenceNotFound))
    ));
    assert!(matches!(
        client.try_confirm_evidence(&admin, &99),
        Err(Ok(ContractError::EvidenceNotFound))
    ));
}
