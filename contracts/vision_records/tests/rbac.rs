#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
mod common;

use common::{create_test_user, setup_test_env};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, String, Vec};
use vision_records::{
    AccessLevel, CredentialType, Permission, RecordType, Role, SensitivityLevel, TimeRestriction,
};

#[test]
fn test_role_hierarchy_and_inheritance() {
    let ctx = setup_test_env();

    let optometrist = create_test_user(&ctx, Role::Optometrist, "Opto");
    let staff = create_test_user(&ctx, Role::Staff, "Staff");
    let patient = create_test_user(&ctx, Role::Patient, "Pat");

    assert!(ctx
        .client
        .check_permission(&ctx.admin, &Permission::SystemAdmin));
    assert!(ctx
        .client
        .check_permission(&ctx.admin, &Permission::ManageUsers));
    assert!(ctx
        .client
        .check_permission(&ctx.admin, &Permission::WriteRecord));

    assert!(!ctx
        .client
        .check_permission(&optometrist, &Permission::SystemAdmin));
    assert!(ctx
        .client
        .check_permission(&optometrist, &Permission::WriteRecord));
    assert!(ctx
        .client
        .check_permission(&optometrist, &Permission::ManageUsers));

    assert!(ctx
        .client
        .check_permission(&staff, &Permission::ManageUsers));
    assert!(!ctx
        .client
        .check_permission(&staff, &Permission::WriteRecord));

    assert!(!ctx
        .client
        .check_permission(&patient, &Permission::ManageUsers));
    assert!(!ctx
        .client
        .check_permission(&patient, &Permission::WriteRecord));
}

#[test]
fn test_custom_permission_grants() {
    let ctx = setup_test_env();

    let staff = create_test_user(&ctx, Role::Staff, "Staff");
    assert!(!ctx
        .client
        .check_permission(&staff, &Permission::WriteRecord));

    ctx.client
        .grant_custom_permission(&ctx.admin, &staff, &Permission::WriteRecord);
    assert!(ctx
        .client
        .check_permission(&staff, &Permission::WriteRecord));

    ctx.client
        .revoke_custom_permission(&ctx.admin, &staff, &Permission::WriteRecord);
    assert!(!ctx
        .client
        .check_permission(&staff, &Permission::WriteRecord));
}

#[test]
fn test_custom_permission_revocations() {
    let ctx = setup_test_env();

    let optometrist = create_test_user(&ctx, Role::Optometrist, "Opto");
    assert!(ctx
        .client
        .check_permission(&optometrist, &Permission::ManageUsers));

    ctx.client
        .revoke_custom_permission(&ctx.admin, &optometrist, &Permission::ManageUsers);
    assert!(!ctx
        .client
        .check_permission(&optometrist, &Permission::ManageUsers));

    assert!(ctx
        .client
        .check_permission(&optometrist, &Permission::WriteRecord));

    ctx.client
        .grant_custom_permission(&ctx.admin, &optometrist, &Permission::ManageUsers);
    ctx.client
        .grant_custom_permission(&ctx.admin, &optometrist, &Permission::SystemAdmin);

    assert!(ctx
        .client
        .check_permission(&optometrist, &Permission::ManageUsers));
    assert!(ctx
        .client
        .check_permission(&optometrist, &Permission::SystemAdmin));

    // Revoke ManageUsers and prove SystemAdmin remains (catches `!=` mutated to `==`)
    ctx.client
        .revoke_custom_permission(&ctx.admin, &optometrist, &Permission::ManageUsers);

    assert!(!ctx
        .client
        .check_permission(&optometrist, &Permission::ManageUsers));
    assert!(ctx
        .client
        .check_permission(&optometrist, &Permission::SystemAdmin));
}

#[test]
fn test_role_delegation() {
    let ctx = setup_test_env();

    let pt1 = create_test_user(&ctx, Role::Patient, "Pt1");
    let pt2 = create_test_user(&ctx, Role::Patient, "Pt2");
    let future_time = ctx.env.ledger().timestamp() + 86400;
    ctx.client
        .delegate_role(&pt1, &pt2, &Role::Optometrist, &future_time);

    let doctor = create_test_user(&ctx, Role::Optometrist, "Doc");
    ctx.client
        .grant_access(&pt2, &pt1, &doctor, &AccessLevel::Read, &3600);

    assert_eq!(ctx.client.check_access(&pt1, &doctor), AccessLevel::Read);
}

#[test]
fn test_role_delegation_expiration() {
    let ctx = setup_test_env();

    let delegator = create_test_user(&ctx, Role::Patient, "Delegator");
    let delegatee = create_test_user(&ctx, Role::Patient, "Delegatee");

    ctx.env.ledger().set_timestamp(100);
    let expire_at = 100;
    ctx.client
        .delegate_role(&delegator, &delegatee, &Role::Optometrist, &expire_at);

    let doctor = create_test_user(&ctx, Role::Optometrist, "Doc");
    let result =
        ctx.client
            .try_grant_access(&delegatee, &delegator, &doctor, &AccessLevel::Read, &3600);
    assert!(result.is_err()); // `>=` mutant killed here since exact == fails access

    ctx.env.ledger().set_timestamp(99);
    let result2 =
        ctx.client
            .try_grant_access(&delegatee, &delegator, &doctor, &AccessLevel::Read, &3600);
    assert!(result2.is_ok()); // `<` mutant killed here since strictly less than is allowed

    // Test infinite duration `expires_at == 0` bound
    ctx.client
        .delegate_role(&delegator, &delegatee, &Role::Optometrist, &0);

    // Jump forward in time 10 years to ensure it never expires
    ctx.env.ledger().set_timestamp(315360000);

    let result =
        ctx.client
            .try_grant_access(&delegatee, &delegator, &doctor, &AccessLevel::Read, &3600);
    assert!(result.is_ok());
}

#[test]
fn test_role_assignment_expiration() {
    let ctx = setup_test_env();

    let user = create_test_user(&ctx, Role::Patient, "User");

    ctx.env.ledger().set_timestamp(100);
    let expire_at = 100;
    ctx.env.as_contract(&ctx.client.address, || {
        vision_records::rbac::assign_role(&ctx.env, user.clone(), Role::Optometrist, expire_at);
    });

    // At timestamp 100, role is EXPIRED (must be strictly > 100)
    assert!(!ctx.client.check_permission(&user, &Permission::WriteRecord));

    // Rewind to timestamp 99, role is VALID
    ctx.env.ledger().set_timestamp(99);
    assert!(ctx.client.check_permission(&user, &Permission::WriteRecord));

    // Test infinite duration `expires_at == 0` bound
    ctx.env.as_contract(&ctx.client.address, || {
        vision_records::rbac::assign_role(&ctx.env, user.clone(), Role::Optometrist, 0);
    });

    // Jump forward in time 10 years to ensure it never expires
    ctx.env.ledger().set_timestamp(315360000);
    assert!(ctx.client.check_permission(&user, &Permission::WriteRecord));
}

#[test]
fn test_record_factory_creates_default_data() {
    let ctx = setup_test_env();
    let patient = create_test_user(&ctx, Role::Patient, "Patient");
    let provider = create_test_user(&ctx, Role::Optometrist, "Provider");

    let id = common::create_test_record(
        &ctx,
        &provider,
        &patient,
        &provider,
        vision_records::RecordType::Diagnosis,
        "e3b0c44298fc1c149afbf4c8996fb924",
    );
    let record = ctx.client.get_record(&provider, &id);
    assert_eq!(record.id, id);
    assert_eq!(record.patient, patient);
}

#[test]
fn test_user_factory_returns_unique_users() {
    let ctx = setup_test_env();
    let a = create_test_user(&ctx, Role::Staff, "UserA");
    let b = create_test_user(&ctx, Role::Staff, "UserB");
    assert_ne!(a, b);
}

#[test]
fn test_access_control_with_generated_addresses() {
    let ctx = setup_test_env();
    let patient = Address::generate(&ctx.env);
    let grantee = Address::generate(&ctx.env);
    assert_eq!(
        ctx.client.check_access(&patient, &grantee),
        AccessLevel::None
    );
}

// --- Scoped delegation (delegate_permissions) vs full role delegation ---

/// Delegatee with scoped [ManageAccess] can grant_access on behalf of patient but only has that permission.
#[test]
fn test_scoped_delegation_only_grants_specified_permissions() {
    let ctx = setup_test_env();

    let patient = create_test_user(&ctx, Role::Patient, "Patient");
    let delegatee = create_test_user(&ctx, Role::Patient, "Delegatee");
    let doctor = create_test_user(&ctx, Role::Optometrist, "Doctor");

    let mut perms = Vec::new(&ctx.env);
    perms.push_back(Permission::ManageAccess);
    let expires_at = ctx.env.ledger().timestamp() + 86400;
    ctx.client
        .delegate_permissions(&patient, &delegatee, &perms, &expires_at);

    // Delegatee can grant access (ManageAccess) on behalf of patient
    ctx.client
        .grant_access(&delegatee, &patient, &doctor, &AccessLevel::Read, &3600);
    assert_eq!(
        ctx.client.check_access(&patient, &doctor),
        AccessLevel::Read
    );

    // Same patient delegates only ReadAnyRecord to another delegatee — that one must NOT grant access
    let delegatee2 = create_test_user(&ctx, Role::Patient, "Delegatee2");
    let mut perms_read_only = Vec::new(&ctx.env);
    perms_read_only.push_back(Permission::ReadAnyRecord);
    ctx.client
        .delegate_permissions(&patient, &delegatee2, &perms_read_only, &expires_at);

    let doctor2 = create_test_user(&ctx, Role::Optometrist, "Doctor2");
    let result =
        ctx.client
            .try_grant_access(&delegatee2, &patient, &doctor2, &AccessLevel::Read, &3600);
    assert!(result.is_err());
}

/// Full role delegation still works: delegatee gets all permissions of the role.
#[test]
fn test_full_role_delegation_still_works() {
    let ctx = setup_test_env();

    let pt1 = create_test_user(&ctx, Role::Patient, "Pt1");
    let pt2 = create_test_user(&ctx, Role::Patient, "Pt2");
    let future_time = ctx.env.ledger().timestamp() + 86400;
    ctx.client
        .delegate_role(&pt1, &pt2, &Role::Optometrist, &future_time);

    let doctor = create_test_user(&ctx, Role::Optometrist, "Doc");
    ctx.client
        .grant_access(&pt2, &pt1, &doctor, &AccessLevel::Read, &3600);

    assert_eq!(ctx.client.check_access(&pt1, &doctor), AccessLevel::Read);
}

/// Scoped delegations respect expiry: after expires_at, delegatee loses delegated permissions.
#[test]
fn test_scoped_delegation_expiry() {
    let ctx = setup_test_env();

    let patient = create_test_user(&ctx, Role::Patient, "Patient");
    let delegatee = create_test_user(&ctx, Role::Patient, "Delegatee");
    let doctor = create_test_user(&ctx, Role::Optometrist, "Doctor");

    ctx.env.ledger().set_timestamp(100);
    let mut perms = Vec::new(&ctx.env);
    perms.push_back(Permission::ManageAccess);
    let expire_at = 100u64;
    ctx.client
        .delegate_permissions(&patient, &delegatee, &perms, &expire_at);

    // At timestamp 100 or later, scoped delegation is expired
    let result =
        ctx.client
            .try_grant_access(&delegatee, &patient, &doctor, &AccessLevel::Read, &3600);
    assert!(result.is_err());

    // Before expiry it works
    ctx.env.ledger().set_timestamp(99);
    ctx.client
        .grant_access(&delegatee, &patient, &doctor, &AccessLevel::Read, &3600);
    assert_eq!(
        ctx.client.check_access(&patient, &doctor),
        AccessLevel::Read
    );
}

#[test]
fn test_revoke_access_cascades_delegations_and_emits_event() {
    use soroban_sdk::testutils::Events;

    let ctx = setup_test_env();

    let patient = create_test_user(&ctx, Role::Patient, "Patient");
    let grantee = create_test_user(&ctx, Role::Patient, "Grantee");
    let delegatee = create_test_user(&ctx, Role::Patient, "Delegatee");
    let doctor = create_test_user(&ctx, Role::Optometrist, "Doctor");
    let doctor2 = create_test_user(&ctx, Role::Optometrist, "Doctor2");

    ctx.client
        .grant_access(&patient, &patient, &grantee, &AccessLevel::Read, &3600);

    let future_time = ctx.env.ledger().timestamp() + 86400;
    ctx.client
        .delegate_role(&grantee, &delegatee, &Role::Optometrist, &future_time);

    // Delegation is active: delegatee can manage grantee's access.
    ctx.client
        .grant_access(&delegatee, &grantee, &doctor, &AccessLevel::Read, &3600);
    assert_eq!(ctx.client.check_access(&grantee, &doctor), AccessLevel::Read);

    let events_before = ctx.env.events().all().len();
    ctx.client.revoke_access(&patient, &patient, &grantee);
    let events_after = ctx.env.events().all().len();

    // Revoke emits: cascade event + audit event + access_revoked event.
    assert_eq!(events_after - events_before, 3);

    // Cascaded cleanup removed grantee->delegatee delegation.
    let res = ctx
        .client
        .try_grant_access(&delegatee, &grantee, &doctor2, &AccessLevel::Read, &3600);
    assert!(res.is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #36 — Happy-path unit tests for RBAC endpoints
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_happy_path_grant_custom_permission_endpoint() {
    let ctx = setup_test_env();

    let staff = create_test_user(&ctx, Role::Staff, "StaffMember");

    // Baseline: Staff role does NOT have WriteRecord or ManageAccess
    assert!(!ctx.client.check_permission(&staff, &Permission::WriteRecord));
    assert!(!ctx.client.check_permission(&staff, &Permission::ManageAccess));

    // Admin grants WriteRecord to staff
    let res1 = ctx
        .client
        .try_grant_custom_permission(&ctx.admin, &staff, &Permission::WriteRecord);
    assert!(res1.is_ok());

    // Admin grants ManageAccess to staff
    let res2 = ctx
        .client
        .try_grant_custom_permission(&ctx.admin, &staff, &Permission::ManageAccess);
    assert!(res2.is_ok());

    // Staff now holds both granted permissions
    assert!(ctx.client.check_permission(&staff, &Permission::WriteRecord));
    assert!(ctx.client.check_permission(&staff, &Permission::ManageAccess));
}

#[test]
fn test_happy_path_revoke_custom_permission_endpoint() {
    let ctx = setup_test_env();

    let optometrist = create_test_user(&ctx, Role::Optometrist, "DoctorOpto");

    // Baseline: Optometrist role has ManageUsers and WriteRecord
    assert!(ctx.client.check_permission(&optometrist, &Permission::ManageUsers));
    assert!(ctx.client.check_permission(&optometrist, &Permission::WriteRecord));

    // Revoke ManageUsers permission from optometrist
    let res_revoke = ctx
        .client
        .try_revoke_custom_permission(&ctx.admin, &optometrist, &Permission::ManageUsers);
    assert!(res_revoke.is_ok());

    // ManageUsers is revoked, but WriteRecord remains intact
    assert!(!ctx.client.check_permission(&optometrist, &Permission::ManageUsers));
    assert!(ctx.client.check_permission(&optometrist, &Permission::WriteRecord));

    // Re-grant ManageUsers permission to optometrist
    let res_grant = ctx
        .client
        .try_grant_custom_permission(&ctx.admin, &optometrist, &Permission::ManageUsers);
    assert!(res_grant.is_ok());
    assert!(ctx.client.check_permission(&optometrist, &Permission::ManageUsers));
}

#[test]
fn test_happy_path_delegate_role_endpoint() {
    let ctx = setup_test_env();

    let delegator = create_test_user(&ctx, Role::Optometrist, "DelegatorDoc");
    let delegatee = create_test_user(&ctx, Role::Staff, "DelegateeStaff");

    // Delegatee as staff cannot write records initially
    assert!(!ctx.client.check_permission(&delegatee, &Permission::WriteRecord));

    // Happy-path with a future expiration timestamp
    let future_time = ctx.env.ledger().timestamp() + 7200;
    let res = ctx
        .client
        .try_delegate_role(&delegator, &delegatee, &Role::Optometrist, &future_time);
    assert!(res.is_ok());

    // Happy-path with non-expiring delegation (expires_at = 0)
    let res_indefinite =
        ctx.client
            .try_delegate_role(&delegator, &delegatee, &Role::Optometrist, &0);
    assert!(res_indefinite.is_ok());
}

#[test]
fn test_happy_path_acl_group_lifecycle_endpoints() {
    let ctx = setup_test_env();

    let user_a = create_test_user(&ctx, Role::Patient, "UserA");
    let user_b = create_test_user(&ctx, Role::Staff, "UserB");

    // 1. Create ACL group with permissions
    let group_name = String::from_str(&ctx.env, "ClinicalAuditors");
    let mut perms = Vec::new(&ctx.env);
    perms.push_back(Permission::ReadAnyRecord);
    perms.push_back(Permission::WriteRecord);

    let res_create = ctx
        .client
        .try_create_acl_group(&ctx.admin, &group_name, &perms);
    assert!(res_create.is_ok());

    // 2. Add user to group
    let res_add_a = ctx
        .client
        .try_add_user_to_group(&ctx.admin, &user_a, &group_name);
    assert!(res_add_a.is_ok());
    let res_add_b = ctx
        .client
        .try_add_user_to_group(&ctx.admin, &user_b, &group_name);
    assert!(res_add_b.is_ok());

    // 3. Query get_user_groups endpoint
    let groups_a = ctx.client.get_user_groups(&user_a);
    assert_eq!(groups_a.len(), 1);
    assert_eq!(groups_a.get(0).unwrap(), group_name);

    // Both users now inherit permissions from the group
    assert!(ctx.client.check_permission(&user_a, &Permission::ReadAnyRecord));
    assert!(ctx.client.check_permission(&user_a, &Permission::WriteRecord));
    assert!(ctx.client.check_permission(&user_b, &Permission::ReadAnyRecord));
    assert!(ctx.client.check_permission(&user_b, &Permission::WriteRecord));

    // 4. Create and add to a second group
    let group_2 = String::from_str(&ctx.env, "SupportTeam");
    let mut perms_2 = Vec::new(&ctx.env);
    perms_2.push_back(Permission::ManageAccess);
    assert!(ctx
        .client
        .try_create_acl_group(&ctx.admin, &group_2, &perms_2)
        .is_ok());
    assert!(ctx
        .client
        .try_add_user_to_group(&ctx.admin, &user_a, &group_2)
        .is_ok());

    let groups_a_updated = ctx.client.get_user_groups(&user_a);
    assert_eq!(groups_a_updated.len(), 2);
    assert!(ctx.client.check_permission(&user_a, &Permission::ManageAccess));

    // 5. Remove user from first group
    let res_remove =
        ctx.client
            .try_remove_user_from_group(&ctx.admin, &user_a, &group_name);
    assert!(res_remove.is_ok());

    // User A loses permissions from group 1, but retains group 2
    assert!(!ctx.client.check_permission(&user_a, &Permission::ReadAnyRecord));
    assert!(!ctx.client.check_permission(&user_a, &Permission::WriteRecord));
    assert!(ctx.client.check_permission(&user_a, &Permission::ManageAccess));
    assert_eq!(ctx.client.get_user_groups(&user_a).len(), 1);

    // User B was unaffected by User A's removal
    assert!(ctx.client.check_permission(&user_b, &Permission::ReadAnyRecord));
    assert!(ctx.client.check_permission(&user_b, &Permission::WriteRecord));
}

#[test]
fn test_happy_path_create_access_policy_endpoint() {
    let ctx = setup_test_env();

    let policy_id = String::from_str(&ctx.env, "POL-RESEARCH-001");
    let policy_name = String::from_str(&ctx.env, "Research Protocol Access");

    let res = ctx.client.try_create_access_policy(
        &ctx.admin,
        &policy_id,
        &policy_name,
        &Role::Ophthalmologist,
        &TimeRestriction::None,
        &CredentialType::ResearchCredentials,
        &SensitivityLevel::Confidential,
        &true,
    );
    assert!(res.is_ok());

    // Second policy with different constraints
    let policy_id_2 = String::from_str(&ctx.env, "POL-EMERGENCY-002");
    let policy_name_2 = String::from_str(&ctx.env, "Emergency Treatment Access");
    let res_2 = ctx.client.try_create_access_policy(
        &ctx.admin,
        &policy_id_2,
        &policy_name_2,
        &Role::Optometrist,
        &TimeRestriction::BusinessHours,
        &CredentialType::EmergencyCredentials,
        &SensitivityLevel::Standard,
        &false,
    );
    assert!(res_2.is_ok());
}

#[test]
fn test_happy_path_set_user_credential_endpoint() {
    let ctx = setup_test_env();

    let doctor = create_test_user(&ctx, Role::Ophthalmologist, "DrSmith");

    // Set MedicalLicense credential
    let res =
        ctx.client
            .try_set_user_credential(&ctx.admin, &doctor, &CredentialType::MedicalLicense);
    assert!(res.is_ok());

    // Update credential to ResearchCredentials
    let res_update =
        ctx.client
            .try_set_user_credential(&ctx.admin, &doctor, &CredentialType::ResearchCredentials);
    assert!(res_update.is_ok());

    // Set EmergencyCredentials
    let res_emergency =
        ctx.client
            .try_set_user_credential(&ctx.admin, &doctor, &CredentialType::EmergencyCredentials);
    assert!(res_emergency.is_ok());
}

#[test]
fn test_happy_path_set_record_sensitivity_endpoint() {
    let ctx = setup_test_env();

    let patient = create_test_user(&ctx, Role::Patient, "PatientJane");
    let provider = create_test_user(&ctx, Role::Optometrist, "ProviderDan");

    let data_hash = "a1b2c3d4e5f6";
    let record_id = common::create_test_record(
        &ctx,
        &provider,
        &patient,
        &provider,
        RecordType::Examination,
        data_hash,
    );

    // Provider sets record sensitivity to Confidential
    let res_provider = ctx.client.try_set_record_sensitivity(
        &provider,
        &record_id,
        &SensitivityLevel::Confidential,
    );
    assert!(res_provider.is_ok());

    // Admin updates record sensitivity to Restricted
    let res_admin = ctx.client.try_set_record_sensitivity(
        &ctx.admin,
        &record_id,
        &SensitivityLevel::Restricted,
    );
    assert!(res_admin.is_ok());
}

#[test]
fn test_happy_path_check_permission_all_roles() {
    let ctx = setup_test_env();

    let ophthalmologist = create_test_user(&ctx, Role::Ophthalmologist, "Ophth");
    let optometrist = create_test_user(&ctx, Role::Optometrist, "Opto");
    let staff = create_test_user(&ctx, Role::Staff, "Staff");
    let patient = create_test_user(&ctx, Role::Patient, "Patient");

    // Admin has full permissions
    assert!(ctx.client.check_permission(&ctx.admin, &Permission::SystemAdmin));
    assert!(ctx.client.check_permission(&ctx.admin, &Permission::ManageUsers));
    assert!(ctx.client.check_permission(&ctx.admin, &Permission::WriteRecord));
    assert!(ctx.client.check_permission(&ctx.admin, &Permission::ManageAccess));
    assert!(ctx.client.check_permission(&ctx.admin, &Permission::ReadAnyRecord));

    // Ophthalmologist has clinical and user management permissions, but not SystemAdmin
    assert!(!ctx.client.check_permission(&ophthalmologist, &Permission::SystemAdmin));
    assert!(ctx.client.check_permission(&ophthalmologist, &Permission::ManageUsers));
    assert!(ctx.client.check_permission(&ophthalmologist, &Permission::WriteRecord));
    assert!(ctx.client.check_permission(&ophthalmologist, &Permission::ManageAccess));
    assert!(ctx.client.check_permission(&ophthalmologist, &Permission::ReadAnyRecord));

    // Optometrist has clinical and user management permissions, but not SystemAdmin
    assert!(!ctx.client.check_permission(&optometrist, &Permission::SystemAdmin));
    assert!(ctx.client.check_permission(&optometrist, &Permission::ManageUsers));
    assert!(ctx.client.check_permission(&optometrist, &Permission::WriteRecord));
    assert!(ctx.client.check_permission(&optometrist, &Permission::ManageAccess));
    assert!(ctx.client.check_permission(&optometrist, &Permission::ReadAnyRecord));

    // Staff has only ManageUsers
    assert!(!ctx.client.check_permission(&staff, &Permission::SystemAdmin));
    assert!(ctx.client.check_permission(&staff, &Permission::ManageUsers));
    assert!(!ctx.client.check_permission(&staff, &Permission::WriteRecord));
    assert!(!ctx.client.check_permission(&staff, &Permission::ManageAccess));
    assert!(!ctx.client.check_permission(&staff, &Permission::ReadAnyRecord));

    // Patient has no global permissions
    assert!(!ctx.client.check_permission(&patient, &Permission::SystemAdmin));
    assert!(!ctx.client.check_permission(&patient, &Permission::ManageUsers));
    assert!(!ctx.client.check_permission(&patient, &Permission::WriteRecord));
    assert!(!ctx.client.check_permission(&patient, &Permission::ManageAccess));
    assert!(!ctx.client.check_permission(&patient, &Permission::ReadAnyRecord));
}
