//! Custom permissions, pause/resume, ACL groups and access policies.

use super::*;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl VisionRecordsContract {
    /// Grants a custom permission to a user.
    /// Requires the caller to have ManageUsers permission.
    pub fn grant_custom_permission(
        env: Env,
        caller: Address,
        user: Address,
        permission: Permission,
    ) -> Result<(), ContractError> {
        rbac::grant_custom_permission(&env, &caller, user, permission)
    }

    /// Revokes a custom permission from a user.
    /// Requires the caller to have ManageUsers permission.
    pub fn revoke_custom_permission(
        env: Env,
        caller: Address,
        user: Address,
        permission: Permission,
    ) -> Result<(), ContractError> {
        rbac::revoke_custom_permission(&env, &caller, user, permission)
    }

    /// Delegates a role to another user with an expiration timestamp.
    /// The delegator must authenticate the transaction.
    pub fn delegate_role(
        env: Env,
        delegator: Address,
        delegatee: Address,
        role: Role,
        expires_at: u64,
    ) -> Result<(), ContractError> {
        rbac::delegate_role(&env, delegator, delegatee, role, expires_at)
    }

    /// Pauses contract operations for a given scope.
    pub fn pause_contract(
        env: Env,
        caller: Address,
        scope: circuit_breaker::PauseScope,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        circuit_breaker::pause_contract(&env, &caller, scope)
    }

    /// Resumes contract operations for a given scope.
    pub fn resume_contract(
        env: Env,
        caller: Address,
        scope: circuit_breaker::PauseScope,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        circuit_breaker::resume_contract(&env, &caller, scope)
    }

    /// Creates an ACL group.
    pub fn create_acl_group(
        env: Env,
        caller: Address,
        group_name: String,
        permissions: Vec<Permission>,
    ) -> Result<(), ContractError> {
        rbac::create_group(&env, &caller, group_name, permissions)
    }

    /// Adds a user to an ACL group.
    pub fn add_user_to_group(
        env: Env,
        caller: Address,
        user: Address,
        group_name: String,
    ) -> Result<(), ContractError> {
        rbac::add_to_group(&env, &caller, user, group_name)
    }

    /// Removes a user from an ACL group.
    pub fn remove_user_from_group(
        env: Env,
        caller: Address,
        user: Address,
        group_name: String,
    ) -> Result<(), ContractError> {
        rbac::remove_from_group(&env, &caller, user, group_name)
    }

    /// Returns all ACL groups assigned to a user.
    pub fn get_user_groups(env: Env, user: Address) -> Vec<String> {
        rbac::get_user_groups(&env, &user)
    }

    /// Checks if a user has a specific permission.
    /// Returns true if the user has the permission, false otherwise.
    /// Queries the standalone access_control micro-contract if configured.
    pub fn check_permission(env: Env, user: Address, permission: Permission) -> bool {
        Self::has_permission_unified(&env, &user, &permission)
    }

    /// Sets the standalone AccessControl micro-contract address.
    pub fn set_access_control(
        env: Env,
        admin: Address,
        access_control: Address,
    ) -> Result<(), ContractError> {
        let is_super = admin_tiers::require_tier(&env, &admin, &AdminTier::SuperAdmin);
        let is_contract_admin = admin_tiers::require_tier(&env, &admin, &AdminTier::ContractAdmin);
        if !is_super
            && !is_contract_admin
            && !rbac::has_permission(&env, &admin, &Permission::SystemAdmin)
        {
            return Err(ContractError::Unauthorized);
        }
        admin.require_auth();
        env.storage()
            .instance()
            .set(&ACCESS_CONTROL, &access_control);
        Ok(())
    }

    /// Returns the configured AccessControl micro-contract address, if set.
    pub fn get_access_control(env: Env) -> Option<Address> {
        Self::get_access_control_config(&env)
    }

    /// Create an access policy with ABAC attributes
    pub fn create_access_policy(
        env: Env,
        caller: Address,
        policy_id: String,
        name: String,
        required_role: Role,
        time_restriction: TimeRestriction,
        required_credential: CredentialType,
        min_sensitivity_level: SensitivityLevel,
        consent_required: bool,
    ) -> Result<(), ContractError> {
        let conditions = rbac::PolicyConditions {
            required_role,
            time_restriction,
            required_credential,
            min_sensitivity_level,
            consent_required,
        };
        rbac::create_access_policy(&env, &caller, policy_id, name, conditions)
    }

    /// Set credential type for a user
    pub fn set_user_credential(
        env: Env,
        caller: Address,
        user: Address,
        credential: CredentialType,
    ) -> Result<(), ContractError> {
        rbac::set_user_credential(&env, &caller, user, credential)
    }

    /// Set sensitivity level for a record
    pub fn set_record_sensitivity(
        env: Env,
        caller: Address,
        record_id: u64,
        sensitivity: SensitivityLevel,
    ) -> Result<(), ContractError> {
        caller.require_auth();

        // Only record provider or SystemAdmin can set sensitivity
        let record_key = (symbol_short!("RECORD"), record_id);
        let record: VisionRecord = env
            .storage()
            .persistent()
            .get(&record_key)
            .ok_or(ContractError::RecordNotFound)?;

        let has_perm = caller == record.provider
            || Self::has_permission_unified(&env, &caller, &Permission::SystemAdmin);
        if !has_perm {
            return Err(ContractError::Unauthorized);
        }

        rbac::set_record_sensitivity(&env, record_id, sensitivity);
        events::publish_sensitivity_set(&env, record_id, sensitivity, caller);

        Ok(())
    }
}
