//! Tests for record archival (soft-delete) and restore.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    audit, ContractError, RecordType, Role, VisionRecordsContract, VisionRecordsContractClient,
};
use crate::audit::AccessAction;
use soroban_sdk::{testutils::Address as _, vec, Address, Env, String};

const VALID_HASH: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

struct Ctx {
    env: Env,
    client: VisionRecordsContractClient<'static>,
    admin: Address,
    provider: Address,
    patient: Address,
}

fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(VisionRecordsContract, ());
    let client = VisionRecordsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    let provider = Address::generate(&env);
    client.register_user(
        &admin,
        &provider,
        &Role::Optometrist,
        &String::from_str(&env, "Dr. Provider"),
    );
    let patient = Address::generate(&env);
    client.register_user(
        &admin,
        &patient,
        &Role::Patient,
        &String::from_str(&env, "Alice"),
    );

    Ctx {
        env,
        client,
        admin,
        provider,
        patient,
    }
}

fn add_record(ctx: &Ctx) -> u64 {
    ctx.client.add_record(
        &ctx.provider,
        &ctx.patient,
        &ctx.provider,
        &RecordType::Examination,
        &String::from_str(&ctx.env, VALID_HASH),
    )
}

fn reason(ctx: &Ctx, text: &str) -> String {
    String::from_str(&ctx.env, text)
}

#[test]
fn archived_record_is_excluded_from_default_queries() {
    let ctx = setup();
    let kept = add_record(&ctx);
    let archived = add_record(&ctx);

    ctx.client
        .archive_record(&ctx.patient, &archived, &reason(&ctx, "duplicate entry"));

    assert!(ctx.client.is_record_archived(&archived));
    assert!(!ctx.client.is_record_archived(&kept));
    assert_eq!(
        ctx.client.get_patient_records(&ctx.patient),
        vec![&ctx.env, kept]
    );

    let records = ctx.client.get_records_by_ids(&vec![&ctx.env, kept, archived]);
    assert_eq!(records.len(), 1);
    assert_eq!(records.get(0).unwrap().id, kept);

    // Non-admin direct reads are rejected.
    assert_eq!(
        ctx.client
            .try_get_record(&ctx.patient, &archived)
            .err()
            .unwrap()
            .unwrap(),
        ContractError::RecordArchived
    );
    assert_eq!(
        ctx.client
            .try_get_record(&ctx.provider, &archived)
            .err()
            .unwrap()
            .unwrap(),
        ContractError::RecordArchived
    );
}

#[test]
fn archiving_keeps_record_data_and_admin_can_read_it() {
    let ctx = setup();
    let id = add_record(&ctx);
    let before = ctx.client.get_record(&ctx.admin, &id);

    ctx.client
        .archive_record(&ctx.provider, &id, &reason(&ctx, "entered in error"));

    let after = ctx.client.get_record(&ctx.admin, &id);
    assert_eq!(after.id, before.id);
    assert_eq!(after.patient, before.patient);
    assert_eq!(after.provider, before.provider);
    assert_eq!(after.data_hash, before.data_hash);
    assert_eq!(after.created_at, before.created_at);

    assert_eq!(
        ctx.client.get_archived_records(&ctx.admin, &ctx.patient),
        vec![&ctx.env, id]
    );
    assert_eq!(ctx.client.get_record_count(), 1);
}

#[test]
fn restore_returns_record_to_default_queries() {
    let ctx = setup();
    let id = add_record(&ctx);

    ctx.client
        .archive_record(&ctx.patient, &id, &reason(&ctx, "not needed"));
    ctx.client.restore_record(&ctx.admin, &id);

    assert!(!ctx.client.is_record_archived(&id));
    assert_eq!(ctx.client.get_archive_info(&id), None);
    assert_eq!(
        ctx.client.get_patient_records(&ctx.patient),
        vec![&ctx.env, id]
    );
    assert_eq!(ctx.client.get_record(&ctx.patient, &id).id, id);
    assert_eq!(
        ctx.client
            .get_archived_records(&ctx.admin, &ctx.patient)
            .len(),
        0
    );
}

#[test]
fn archive_info_and_audit_log_form_an_audit_trail() {
    let ctx = setup();
    let id = add_record(&ctx);
    let why = reason(&ctx, "duplicate entry");

    ctx.client.archive_record(&ctx.patient, &id, &why);

    // The current archive status records who archived it, when and why.
    let info = ctx.client.get_archive_info(&id).unwrap();
    assert_eq!(info.record_id, id);
    assert_eq!(info.archived_by, ctx.patient);
    assert_eq!(info.reason, why);

    ctx.client.restore_record(&ctx.admin, &id);

    // Both actions land in the contract audit log with their actors.
    let log = ctx.env.as_contract(&ctx.client.address, || {
        audit::get_record_audit_log(&ctx.env, id)
    });
    let archive = log
        .iter()
        .find(|e| e.action == AccessAction::Archive)
        .expect("archive audit entry");
    assert_eq!(archive.actor, ctx.patient);
    assert_eq!(archive.reason, Some(why));
    let restore = log
        .iter()
        .find(|e| e.action == AccessAction::Restore)
        .expect("restore audit entry");
    assert_eq!(restore.actor, ctx.admin);
}

#[test]
fn stranger_cannot_archive_restore_or_list() {
    let ctx = setup();
    let id = add_record(&ctx);
    let stranger = Address::generate(&ctx.env);

    assert_eq!(
        ctx.client
            .try_archive_record(&stranger, &id, &reason(&ctx, "nope")),
        Err(Ok(ContractError::Unauthorized))
    );
    assert!(!ctx.client.is_record_archived(&id));

    ctx.client
        .archive_record(&ctx.patient, &id, &reason(&ctx, "mine"));

    assert_eq!(
        ctx.client.try_restore_record(&stranger, &id),
        Err(Ok(ContractError::Unauthorized))
    );
    assert_eq!(
        ctx.client
            .try_get_archived_records(&ctx.patient, &ctx.patient),
        Err(Ok(ContractError::Unauthorized))
    );
    assert!(ctx.client.is_record_archived(&id));
}

#[test]
fn invalid_archive_state_changes_are_rejected() {
    let ctx = setup();
    let id = add_record(&ctx);

    assert_eq!(
        ctx.client.try_restore_record(&ctx.admin, &id),
        Err(Ok(ContractError::RecordNotArchived))
    );

    ctx.client
        .archive_record(&ctx.patient, &id, &reason(&ctx, "first"));
    assert_eq!(
        ctx.client
            .try_archive_record(&ctx.patient, &id, &reason(&ctx, "again")),
        Err(Ok(ContractError::RecordArchived))
    );

    assert_eq!(
        ctx.client
            .try_archive_record(&ctx.admin, &999, &reason(&ctx, "missing")),
        Err(Ok(ContractError::RecordNotFound))
    );
}
