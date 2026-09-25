#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::arithmetic_side_effects
)]
mod common;

use common::{create_test_user, setup_test_env};
use soroban_sdk::Address;
use vision_records::{AccessLevel, Permission, Role};

/// Micro-test: Non-admin attempting to grant permissions should fail
#[test]
fn test_non_admin_cannot_grant_permissions() {
    let ctx = setup_test_env();
    let non_admin = create_test_user(&ctx, Role::Staff, "Staff");
    let target_user = create_test_user(&ctx, Role::Patient, "Patient");

    // Non-admin attempts to grant permission
    let result = ctx
        .client
        .try_grant_custom_permission(&non_admin, &target_user, &Permission::SystemAdmin);

    assert!(
        result.is_err(),
        "Non-admin should not be able to grant SystemAdmin permission"
    );
}

/// Micro-test: Non-admin attempting to revoke permissions should fail
#[test]
fn test_non_admin_cannot_revoke_permissions() {
    let ctx = setup_test_env();
    let optometrist = create_test_user(&ctx, Role::Optometrist, "Opto");
    let non_admin = create_test_user(&ctx, Role::Staff, "Staff");

    // Admin grants a permission first
    ctx.client.grant_custom_permission(
        &ctx.admin,
        &optometrist,
        &Permission::ManageUsers,
    );
    assert!(ctx
        .client
        .check_permission(&optometrist, &Permission::ManageUsers));

    // Non-admin attempts to revoke permission
    let result = ctx
        .client
        .try_revoke_custom_permission(&non_admin, &optometrist, &Permission::ManageUsers);

    assert!(
        result.is_err(),
        "Non-admin should not be able to revoke permissions"
    );
    assert!(ctx
        .client
        .check_permission(&optometrist, &Permission::ManageUsers),
        "Permission should still exist after failed revocation"
    );
}

/// Micro-test: Non-admin attempting to delegate roles should fail
#[test]
fn test_non_admin_cannot_delegate_role() {
    let ctx = setup_test_env();
    let non_admin = create_test_user(&ctx, Role::Staff, "Staff");
    let delegatee = create_test_user(&ctx, Role::Patient, "Patient");
    let future_time = ctx.env.ledger().timestamp() + 86400;

    // Non-admin attempts to delegate role
    let result = ctx
        .client
        .try_delegate_role(&non_admin, &delegatee, &Role::Optometrist, &future_time);

    assert!(
        result.is_err(),
        "Non-admin should not be able to delegate roles"
    );
}

/// Micro-test: Non-admin attempting to revoke delegation should fail
#[test]
fn test_non_admin_cannot_revoke_delegation() {
    let ctx = setup_test_env();
    let non_admin = create_test_user(&ctx, Role::Staff, "Staff");
    let patient = create_test_user(&ctx, Role::Patient, "Patient");
    let delegatee = create_test_user(&ctx, Role::Patient, "Delegatee");
    let future_time = ctx.env.ledger().timestamp() + 86400;

    // Admin creates delegation first
    ctx.client
        .delegate_role(&patient, &delegatee, &Role::Optometrist, &future_time);

    // Non-admin attempts to revoke delegation
    let result = ctx
        .client
        .try_revoke_delegation(&non_admin, &patient, &delegatee);

    assert!(
        result.is_err(),
        "Non-admin should not be able to revoke delegations"
    );
}

/// Micro-test: Patient role cannot perform SystemAdmin action
#[test]
fn test_patient_cannot_perform_system_admin_action() {
    let ctx = setup_test_env();
    let patient = create_test_user(&ctx, Role::Patient, "Patient");
    let target = Address::generate(&ctx.env);

    // Patient should not have SystemAdmin permission
    assert!(!ctx
        .client
        .check_permission(&patient, &Permission::SystemAdmin));

    // Attempting to perform system admin action should fail
    let result = ctx
        .client
        .try_grant_custom_permission(&patient, &target, &Permission::WriteRecord);

    assert!(
        result.is_err(),
        "Patient should not be able to grant permissions"
    );
}

/// Micro-test: Staff role cannot perform WriteRecord action
#[test]
fn test_staff_cannot_write_record() {
    let ctx = setup_test_env();
    let staff = create_test_user(&ctx, Role::Staff, "Staff");
    let patient = create_test_user(&ctx, Role::Patient, "Patient");

    // Staff should not have WriteRecord permission by default
    assert!(!ctx
        .client
        .check_permission(&staff, &Permission::WriteRecord));

    // Attempting to create a record should fail
    let result = ctx
        .client
        .try_create_record(
            &staff,
            &patient,
            &staff,
            &vision_records::RecordType::Diagnosis,
            "test_content_hash",
        );

    assert!(result.is_err(), "Staff should not be able to create records");
}

/// Micro-test: Non-authenticated user cannot access protected functions
#[test]
fn test_unauthenticated_user_cannot_access_functions() {
    let ctx = setup_test_env();
    let unauthenticated = Address::generate(&ctx.env);
    let target = Address::generate(&ctx.env);

    // Unauthenticated user cannot grant access
    let result = ctx
        .client
        .try_grant_access(&unauthenticated, &target, &target, &AccessLevel::Read, &3600);

    assert!(
        result.is_err(),
        "Unauthenticated user should not be able to grant access"
    );
}

/// Micro-test: Admin attempting invalid action with permission boundary testing
#[test]
fn test_admin_permission_enforcement_boundaries() {
    let ctx = setup_test_env();
    let optometrist = create_test_user(&ctx, Role::Optometrist, "Opto");

    // Admin has ManageUsers by role
    assert!(ctx
        .client
        .check_permission(&ctx.admin, &Permission::ManageUsers));

    // Admin can grant additional permission
    ctx.client.grant_custom_permission(
        &ctx.admin,
        &optometrist,
        &Permission::SystemAdmin,
    );
    assert!(ctx
        .client
        .check_permission(&optometrist, &Permission::SystemAdmin));

    // Admin can revoke that permission
    ctx.client.revoke_custom_permission(
        &ctx.admin,
        &optometrist,
        &Permission::SystemAdmin,
    );
    assert!(!ctx
        .client
        .check_permission(&optometrist, &Permission::SystemAdmin));
}
