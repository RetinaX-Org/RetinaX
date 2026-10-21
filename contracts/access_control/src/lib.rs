#![no_std]

pub mod admin;
pub mod permissions;
pub mod types;
pub mod whitelist;

#[cfg(test)]
mod test;

pub use admin::*;
pub use permissions::*;
pub use types::*;
pub use whitelist::*;

use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec};

/// AccessControl micro-contract for decentralized role-based and attribute-based permissions.
#[contract]
pub struct AccessControlContract;

#[contractimpl]
impl AccessControlContract {
    /// Initializes the AccessControl micro-contract with an initial administrator.
    ///
    /// # Arguments
    ///
    /// * `admin` - address to register; must authorize this call.
    ///
    /// # Errors
    ///
    /// [`AlreadyInitialized`](AccessControlError::AlreadyInitialized) if an
    /// administrator is already registered.
    pub fn initialize(env: Env, admin: Address) -> Result<(), AccessControlError> {
        admin::initialize(&env, &admin)
    }

    /// Returns the currently active administrator address (`None` before
    /// [`initialize`]).
    pub fn get_admin(env: Env) -> Option<Address> {
        admin::get_admin(&env)
    }

    /// Proposes an administrative transfer in a two-step handshake.
    ///
    /// # Arguments
    ///
    /// * `current_admin` - acting administrator; must authorize the call.
    /// * `new_admin` - address nominated to take over.
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](AccessControlError::Unauthorized) if `caller`
    /// is not the registered administrator.
    pub fn propose_admin(
        env: Env,
        current_admin: Address,
        new_admin: Address,
    ) -> Result<(), AccessControlError> {
        admin::propose_admin(&env, &current_admin, &new_admin)
    }

    /// Accepts a proposed administrative role, completing the 2-step transfer.
    ///
    /// # Arguments
    ///
    /// * `new_admin` - nominated address; must authorize the call.
    ///
    /// # Errors
    ///
    /// * [`NotFound`](AccessControlError::NotFound) - no transfer is pending.
    /// * [`Unauthorized`](AccessControlError::Unauthorized) - `new_admin` does not
    ///   match the pending nomination.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), AccessControlError> {
        admin::accept_admin(&env, &new_admin)
    }

    // ── Whitelist Management ────────────────────────────────────────────────

    /// Activates or deactivates whitelist enforcement.
    ///
    /// # Arguments
    ///
    /// * `caller` - must be the administrator.
    /// * `enabled` - `true` to enforce, `false` to allow every address.
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](AccessControlError::Unauthorized) if `caller`
    /// is not the registered administrator.
    pub fn set_whitelist_enabled(
        env: Env,
        caller: Address,
        enabled: bool,
    ) -> Result<(), AccessControlError> {
        whitelist::set_whitelist_enabled(&env, &caller, enabled)
    }

    /// Returns whether whitelist enforcement is active (`false` by default).
    pub fn is_whitelist_enabled(env: Env) -> bool {
        whitelist::is_whitelist_enabled(&env)
    }

    /// Whitelists a user address.
    ///
    /// # Arguments
    ///
    /// * `caller` - must be the administrator.
    /// * `user` - address to allow; idempotent if already present.
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](AccessControlError::Unauthorized) if `caller`
    /// is not the registered administrator.
    pub fn add_to_whitelist(
        env: Env,
        caller: Address,
        user: Address,
    ) -> Result<(), AccessControlError> {
        whitelist::add_to_whitelist(&env, &caller, &user)
    }

    /// Batch adds user addresses to the whitelist.
    pub fn batch_add_to_whitelist(
        env: Env,
        caller: Address,
        users: Vec<Address>,
    ) -> Result<(), AccessControlError> {
        whitelist::batch_add_to_whitelist(&env, &caller, &users)
    }

    /// Removes a user address from the whitelist (no-op if absent).
    ///
    /// # Arguments
    ///
    /// * `caller` - must be the administrator.
    /// * `user` - address to remove.
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](AccessControlError::Unauthorized) if `caller`
    /// is not the registered administrator.
    pub fn remove_from_whitelist(
        env: Env,
        caller: Address,
        user: Address,
    ) -> Result<(), AccessControlError> {
        whitelist::remove_from_whitelist(&env, &caller, &user)
    }

    /// Batch removes user addresses from the whitelist.
    pub fn batch_remove_from_whitelist(
        env: Env,
        caller: Address,
        users: Vec<Address>,
    ) -> Result<(), AccessControlError> {
        whitelist::batch_remove_from_whitelist(&env, &caller, &users)
    }

    /// Returns a paginated list of whitelisted addresses along with total count.
    pub fn get_whitelisted_addresses(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> (Vec<Address>, u32) {
        whitelist::get_whitelisted_addresses(&env, offset, limit)
    }

    /// Checks if a user is whitelisted (returns true if whitelist is disabled).
    ///
    /// # Arguments
    ///
    /// * `user` - address to look up.
    pub fn check_whitelist(env: Env, user: Address) -> bool {
        whitelist::check_whitelist(&env, &user)
    }

    /// Returns raw whitelist status for an address, ignoring whether enforcement
    /// is enabled (unlike [`check_whitelist`]).
    pub fn is_whitelisted(env: Env, user: Address) -> bool {
        whitelist::is_whitelisted(&env, &user)
    }

    // ── RBAC & Role Management ──────────────────────────────────────────────

    /// Assigns a role to a user, replacing any existing assignment.
    ///
    /// # Arguments
    ///
    /// * `caller` - must hold [`Permission::ManageUsers`].
    /// * `user` - account receiving the role.
    /// * `role` - the [`Role`] to assign.
    /// * `expires_at` - unix seconds when the assignment lapses; `0` = permanent.
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](AccessControlError::Unauthorized) if `caller` lacks
    /// [`Permission::ManageUsers`].
    pub fn assign_role(
        env: Env,
        caller: Address,
        user: Address,
        role: Role,
        expires_at: u64,
    ) -> Result<(), AccessControlError> {
        permissions::assign_role(&env, &caller, &user, role, expires_at)
    }

    /// Revokes an assigned role.
    ///
    /// # Arguments
    ///
    /// * `caller` - must hold [`Permission::ManageUsers`].
    /// * `user` - account to strip of its role.
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](AccessControlError::Unauthorized) if `caller` lacks
    /// [`Permission::ManageUsers`].
    pub fn revoke_role(env: Env, caller: Address, user: Address) -> Result<(), AccessControlError> {
        permissions::revoke_role(&env, &caller, &user)
    }

    /// Returns a user's active role: admins always resolve to [`Role::Admin`];
    /// expired or missing assignments resolve to [`Role::None`].
    pub fn get_role(env: Env, user: Address) -> Role {
        permissions::get_role(&env, &user)
    }

    /// Checks if a user holds a permission, honoring custom grants/revokes, the
    /// active role's base permissions, and ACL group membership.
    ///
    /// # Arguments
    ///
    /// * `user` - account to evaluate.
    /// * `permission` - the [`Permission`] being tested.
    pub fn has_permission(env: Env, user: Address, permission: Permission) -> bool {
        permissions::has_permission(&env, &user, &permission)
    }

    /// Checks if a delegatee holds a delegated permission from a delegator
    /// (full-role or scoped; expired delegations ignored).
    ///
    /// # Arguments
    ///
    /// * `delegator` - account that granted the delegation.
    /// * `delegatee` - account receiving it.
    /// * `permission` - the [`Permission`] being tested.
    pub fn has_delegated_permission(
        env: Env,
        delegator: Address,
        delegatee: Address,
        permission: Permission,
    ) -> bool {
        permissions::has_delegated_permission(&env, &delegator, &delegatee, &permission)
    }

    /// Grants a custom permission to a user, on top of their role's base
    /// permissions.
    ///
    /// # Arguments
    ///
    /// * `caller` - must hold [`Permission::ManageUsers`].
    /// * `user` - account receiving the grant; must have a role assignment.
    /// * `permission` - the [`Permission`] to grant.
    ///
    /// # Errors
    ///
    /// * [`Unauthorized`](AccessControlError::Unauthorized) - `caller` lacks
    /// [`Permission::ManageUsers`].
    /// * [`NotFound`](AccessControlError::NotFound) - `user` has no role
    ///   assignment to amend.
    pub fn grant_custom_permission(
        env: Env,
        caller: Address,
        user: Address,
        permission: Permission,
    ) -> Result<(), AccessControlError> {
        permissions::grant_custom_permission(&env, &caller, &user, permission)
    }

    /// Explicitly revokes a custom permission from a user (explicit deny).
    ///
    /// # Arguments
    ///
    /// * `caller` - must hold [`Permission::ManageUsers`].
    /// * `user` - account being denied; must have a role assignment.
    /// * `permission` - the [`Permission`] to revoke.
    ///
    /// # Errors
    ///
    /// * [`Unauthorized`](AccessControlError::Unauthorized) - `caller` lacks
    /// [`Permission::ManageUsers`].
    /// * [`NotFound`](AccessControlError::NotFound) - `user` has no role
    ///   assignment to amend.
    pub fn revoke_custom_permission(
        env: Env,
        caller: Address,
        user: Address,
        permission: Permission,
    ) -> Result<(), AccessControlError> {
        permissions::revoke_custom_permission(&env, &caller, &user, permission)
    }

    /// Sets the maximum delegation chain depth allowed.
    pub fn set_max_delegation_depth(
        env: Env,
        caller: Address,
        max_depth: u32,
    ) -> Result<(), AccessControlError> {
        permissions::set_max_delegation_depth(&env, &caller, max_depth)
    }

    /// Gets the maximum delegation chain depth allowed.
    pub fn get_max_delegation_depth(env: Env) -> u32 {
        permissions::get_max_delegation_depth(&env)
    }

    /// Delegates a complete role to a delegatee.
    ///
    /// # Arguments
    ///
    /// * `delegator` - account lending the role; must authorize the call.
    /// * `delegatee` - account receiving it.
    /// * `role` - the [`Role`] to delegate.
    /// * `expires_at` - unix seconds when the delegation lapses; `0` = never.
    pub fn delegate_role(
        env: Env,
        delegator: Address,
        delegatee: Address,
        role: Role,
        expires_at: u64,
    ) -> Result<(), AccessControlError> {
        permissions::delegate_role(&env, &delegator, &delegatee, role, expires_at)
    }

    /// Revokes a role delegation (succeeds even if none exists).
    ///
    /// # Arguments
    ///
    /// * `delegator` - delegation author; must authorize the call.
    /// * `delegatee` - counterparty of the delegation to remove.
    pub fn revoke_delegation(
        env: Env,
        delegator: Address,
        delegatee: Address,
    ) -> Result<(), AccessControlError> {
        permissions::revoke_delegation(&env, &delegator, &delegatee)
    }

    /// Delegates specific permissions to a delegatee without transferring the
    /// whole role.
    ///
    /// # Arguments
    ///
    /// * `delegator` - account lending the permissions; must authorize the call.
    /// * `delegatee` - account receiving them.
    /// * `permissions` - the [`Permission`]s covered by this delegation.
    /// * `expires_at` - unix seconds when the delegation lapses; `0` = never.
    pub fn delegate_permissions(
        env: Env,
        delegator: Address,
        delegatee: Address,
        permissions: Vec<Permission>,
        expires_at: u64,
    ) -> Result<(), AccessControlError> {
        permissions::delegate_permissions(&env, &delegator, &delegatee, permissions, expires_at)
    }

    /// Revokes a scoped delegation (succeeds even if none exists).
    ///
    /// # Arguments
    ///
    /// * `delegator` - delegation author; must authorize the call.
    /// * `delegatee` - counterparty of the delegation to remove.
    pub fn revoke_scoped_delegation(
        env: Env,
        delegator: Address,
        delegatee: Address,
    ) -> Result<(), AccessControlError> {
        permissions::revoke_scoped_delegation(&env, &delegator, &delegatee)
    }

    // ── ACL Group Management ────────────────────────────────────────────────

    /// Creates an ACL group with specified permissions (overwrites same-named
    /// groups).
    ///
    /// # Arguments
    ///
    /// * `caller` - must hold [`Permission::ManageAccess`].
    /// * `group_name` - symbol identifying the group.
    /// * `permissions` - permissions every member inherits.
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](AccessControlError::Unauthorized) if `caller` lacks
    /// [`Permission::ManageAccess`].
    pub fn create_group(
        env: Env,
        caller: Address,
        group_name: Symbol,
        permissions: Vec<Permission>,
    ) -> Result<(), AccessControlError> {
        permissions::create_group(&env, &caller, group_name, permissions)
    }

    /// Adds a user to an ACL group (no-op if already a member).
    ///
    /// # Arguments
    ///
    /// * `caller` - must hold [`Permission::ManageAccess`].
    /// * `user` - account to add.
    /// * `group_name` - existing group.
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](AccessControlError::Unauthorized) if `caller` lacks
    /// [`Permission::ManageAccess`].
    pub fn add_to_group(
        env: Env,
        caller: Address,
        user: Address,
        group_name: Symbol,
    ) -> Result<(), AccessControlError> {
        permissions::add_to_group(&env, &caller, &user, group_name)
    }

    /// Removes a user from an ACL group (no-op if not a member).
    ///
    /// # Arguments
    ///
    /// * `caller` - must hold [`Permission::ManageAccess`].
    /// * `user` - account to remove.
    /// * `group_name` - existing group.
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](AccessControlError::Unauthorized) if `caller` lacks
    /// [`Permission::ManageAccess`].
    pub fn remove_from_group(
        env: Env,
        caller: Address,
        user: Address,
        group_name: Symbol,
    ) -> Result<(), AccessControlError> {
        permissions::remove_from_group(&env, &caller, &user, group_name)
    }

    /// Returns all groups a user belongs to (empty if none).
    ///
    /// # Arguments
    ///
    /// * `user` - account whose group memberships are returned.
    pub fn get_user_groups(env: Env, user: Address) -> Vec<Symbol> {
        permissions::get_user_groups(&env, &user)
    }

    // ── Attribute-Based Access Control (ABAC) ───────────────────────────────

    /// Creates an ABAC policy (or updates it if `policy_id` already exists).
    ///
    /// # Arguments
    ///
    /// * `caller` - must hold [`Permission::SystemAdmin`].
    /// * `policy_id` - symbol keying the policy.
    /// * `name` - human-readable policy name.
    /// * `conditions` - role, credential, sensitivity, and time conditions.
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](AccessControlError::Unauthorized) if `caller` lacks
    /// [`Permission::SystemAdmin`].
    pub fn create_access_policy(
        env: Env,
        caller: Address,
        policy_id: Symbol,
        name: Symbol,
        conditions: PolicyConditions,
    ) -> Result<(), AccessControlError> {
        permissions::create_access_policy(&env, &caller, policy_id, name, conditions)
    }

    /// Sets user professional credential, used by ABAC policy evaluation.
    ///
    /// # Arguments
    ///
    /// * `caller` - must hold [`Permission::ManageUsers`].
    /// * `user` - account being credentialed.
    /// * `credential` - the [`CredentialType`] to store.
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](AccessControlError::Unauthorized) if `caller` lacks
    /// [`Permission::ManageUsers`].
    pub fn set_user_credential(
        env: Env,
        caller: Address,
        user: Address,
        credential: CredentialType,
    ) -> Result<(), AccessControlError> {
        permissions::set_user_credential(&env, &caller, &user, credential)
    }

    /// Sets record sensitivity classification, used by ABAC policy evaluation.
    ///
    /// # Arguments
    ///
    /// * `caller` - must hold [`Permission::WriteRecord`].
    /// * `record_id` - record being classified.
    /// * `sensitivity` - the [`SensitivityLevel`] to store.
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](AccessControlError::Unauthorized) if `caller` lacks
    /// [`Permission::WriteRecord`].
    pub fn set_record_sensitivity(
        env: Env,
        caller: Address,
        record_id: u64,
        sensitivity: SensitivityLevel,
    ) -> Result<(), AccessControlError> {
        permissions::set_record_sensitivity(&env, &caller, record_id, sensitivity)
    }

    /// Evaluates an ABAC policy against contextual variables (user role,
    /// credential, record sensitivity, and `current_time` for time restrictions).
    ///
    /// # Arguments
    ///
    /// * `policy_id` - policy to evaluate.
    /// * `user` - account requesting access.
    /// * `record_id` - record whose stored sensitivity is checked.
    /// * `current_time` - unix seconds used for time restrictions.
    ///
    /// Returns `false` when the policy is missing or disabled or any condition
    /// fails.
    pub fn evaluate_access_policy(
        env: Env,
        policy_id: Symbol,
        user: Address,
        record_id: u64,
        current_time: u64,
    ) -> bool {
        permissions::evaluate_access_policy(&env, policy_id, &user, record_id, current_time)
    }
}
