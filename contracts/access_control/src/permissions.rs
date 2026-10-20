use crate::types::{
    get_base_permissions, AccessControlError, AccessPolicy, AclGroup, CredentialType, Delegation,
    Permission, PolicyConditions, Role, RoleAssignment, ScopedDelegation, SensitivityLevel,
    TimeRestriction,
};
use soroban_sdk::{symbol_short, Address, Env, Symbol, Vec};

const ROLE_ASSIGN: Symbol = symbol_short!("ROLE_ASN");
const DELEGATION_KEY: Symbol = symbol_short!("DELEG");
const SCOPED_DEL_KEY: Symbol = symbol_short!("SC_DEL");
const ACL_GRP: Symbol = symbol_short!("ACL_GRP");
const USR_GRPS: Symbol = symbol_short!("USR_GRPS");
const ACC_POL: Symbol = symbol_short!("ACC_POL");
const USR_CRED: Symbol = symbol_short!("USR_CRED");
const REC_SENS: Symbol = symbol_short!("REC_SENS");
const MAX_DEL_DEPTH: Symbol = symbol_short!("MAX_D_DP");
const DEL_PARENT: Symbol = symbol_short!("DEL_PAR");

pub const DEFAULT_MAX_DELEGATION_DEPTH: u32 = 3;

/// Sets the maximum delegation chain depth allowed (admin only).
pub fn set_max_delegation_depth(
    env: &Env,
    caller: &Address,
    max_depth: u32,
) -> Result<(), AccessControlError> {
    crate::admin::require_admin(env, caller)?;
    env.storage().instance().set(&MAX_DEL_DEPTH, &max_depth);
    Ok(())
}

/// Gets the configured maximum delegation chain depth (default: 3).
pub fn get_max_delegation_depth(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&MAX_DEL_DEPTH)
        .unwrap_or(DEFAULT_MAX_DELEGATION_DEPTH)
}

/// Helper to compute delegation depth for delegator
fn get_delegator_depth(env: &Env, delegator: &Address) -> u32 {
    let mut depth: u32 = 0;
    let mut current: Option<Address> = env
        .storage()
        .persistent()
        .get(&(DEL_PARENT, delegator.clone()));

    // Traverse upwards with safety ceiling to prevent infinite loop on cycles
    while let Some(parent) = current {
        depth = depth.saturating_add(1);
        if depth > 100 {
            break;
        }
        current = env.storage().persistent().get(&(DEL_PARENT, parent));
    }
    depth
}

/// Assign `role` to `user`, replacing any existing assignment.
///
/// # Arguments
///
/// * `caller` - acting address; must hold [`Permission::ManageUsers`].
/// * `user` - account receiving the role.
/// * `role` - the [`Role`] to assign.
/// * `expires_at` - unix seconds when the assignment lapses; `0` = permanent.
///
/// # Errors
///
/// [`Unauthorized`](AccessControlError::Unauthorized) if `caller` lacks
/// [`Permission::ManageUsers`].
pub fn assign_role(
    env: &Env,
    caller: &Address,
    user: &Address,
    role: Role,
    expires_at: u64,
) -> Result<(), AccessControlError> {
    caller.require_auth();

    // Caller must have ManageUsers permission or be Admin
    if !has_permission(env, caller, &Permission::ManageUsers) {
        return Err(AccessControlError::Unauthorized);
    }

    let assignment = RoleAssignment {
        role,
        custom_grants: Vec::new(env),
        custom_revokes: Vec::new(env),
        expires_at,
    };

    env.storage()
        .persistent()
        .set(&(ROLE_ASSIGN, user.clone()), &assignment);
    Ok(())
}

/// Remove `user`'s role assignment (custom grants/revokes included).
///
/// # Arguments
///
/// * `caller` - must hold [`Permission::ManageUsers`].
/// * `user` - account to strip.
///
/// # Errors
///
/// [`Unauthorized`](AccessControlError::Unauthorized) if `caller` lacks
/// [`Permission::ManageUsers`].
pub fn revoke_role(env: &Env, caller: &Address, user: &Address) -> Result<(), AccessControlError> {
    caller.require_auth();
    if !has_permission(env, caller, &Permission::ManageUsers) {
        return Err(AccessControlError::Unauthorized);
    }

    env.storage()
        .persistent()
        .remove(&(ROLE_ASSIGN, user.clone()));
    Ok(())
}

/// Return `user`'s active role.
///
/// # Arguments
///
/// * `user` - account whose role is resolved.
///
/// The contract administrator always resolves to [`Role::Admin`]; a stored
/// assignment past `expires_at` resolves to [`Role::None`], as does an account
/// with no assignment.
pub fn get_role(env: &Env, user: &Address) -> Role {
    if let Some(admin) = crate::admin::get_admin(env) {
        if &admin == user {
            return Role::Admin;
        }
    }

    let assignment: Option<RoleAssignment> =
        env.storage().persistent().get(&(ROLE_ASSIGN, user.clone()));

    match assignment {
        Some(a) => {
            let now = env.ledger().timestamp();
            if a.expires_at > 0 && now >= a.expires_at {
                Role::None
            } else {
                a.role
            }
        }
        None => Role::None,
    }
}

/// Checks whether `user` holds `permission`.
///
/// # Arguments
///
/// * `user` - account to evaluate.
/// * `permission` - the [`Permission`] being tested.
///
/// Evaluation pipeline:
/// 0. If user is registered contract admin, return true
/// 1. Check custom revokes (explicit deny)
/// 2. Check custom grants (explicit grant)
/// 3. Check base permissions of active role
/// 4. Check ACL groups the user belongs to
pub fn has_permission(env: &Env, user: &Address, permission: &Permission) -> bool {
    if let Some(admin) = crate::admin::get_admin(env) {
        if &admin == user {
            return true;
        }
    }

    let assignment: Option<RoleAssignment> =
        env.storage().persistent().get(&(ROLE_ASSIGN, user.clone()));

    if let Some(a) = assignment {
        let now = env.ledger().timestamp();
        let is_valid = a.expires_at == 0 || now < a.expires_at;

        if is_valid {
            // 1. Explicit deny
            for revoked in a.custom_revokes.iter() {
                if &revoked == permission {
                    return false;
                }
            }

            // 2. Explicit grant
            for granted in a.custom_grants.iter() {
                if &granted == permission {
                    return true;
                }
            }

            // 3. Base role permissions
            let base_perms = get_base_permissions(&a.role, env);
            for p in base_perms.iter() {
                if &p == permission {
                    return true;
                }
            }
        }
    }

    // 4. ACL Groups
    let groups: Option<Vec<Symbol>> = env.storage().persistent().get(&(USR_GRPS, user.clone()));

    if let Some(user_groups) = groups {
        for group_name in user_groups.iter() {
            let group: Option<AclGroup> = env.storage().persistent().get(&(ACL_GRP, group_name));

            if let Some(grp) = group {
                for p in grp.permissions.iter() {
                    if &p == permission {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Checks whether `delegatee` holds a delegated permission from `delegator`.
///
/// # Arguments
///
/// * `delegator` - account that granted the delegation.
/// * `delegatee` - account receiving it.
/// * `permission` - the [`Permission`] being tested.
///
/// Both full-role and scoped delegations are honoured; expired delegations are
/// ignored. Returns `false` if neither applies.
pub fn has_delegated_permission(
    env: &Env,
    delegator: &Address,
    delegatee: &Address,
    permission: &Permission,
) -> bool {
    let now = env.ledger().timestamp();

    // Check full role delegation
    let del_key = (DELEGATION_KEY, delegator.clone(), delegatee.clone());
    let delegation: Option<Delegation> = env.storage().persistent().get(&del_key);

    if let Some(d) = delegation {
        if d.expires_at == 0 || now < d.expires_at {
            let base_perms = get_base_permissions(&d.role, env);
            for p in base_perms.iter() {
                if &p == permission {
                    return true;
                }
            }
        }
    }

    // Check scoped delegation
    let scoped_key = (SCOPED_DEL_KEY, delegator.clone(), delegatee.clone());
    let scoped: Option<ScopedDelegation> = env.storage().persistent().get(&scoped_key);

    if let Some(s) = scoped {
        if s.expires_at == 0 || now < s.expires_at {
            for p in s.permissions.iter() {
                if &p == permission {
                    return true;
                }
            }
        }
    }

    false
}

/// Grant `permission` to `user` on top of their role's base permissions.
///
/// # Arguments
///
/// * `caller` - must hold [`Permission::ManageUsers`].
/// * `user` - account receiving the grant; must already have a role
///   assignment.
/// * `permission` - the [`Permission`] to grant.
///
/// # Errors
///
/// * [`Unauthorized`](AccessControlError::Unauthorized) - `caller` lacks
///   [`Permission::ManageUsers`].
/// * [`NotFound`](AccessControlError::NotFound) - `user` has no role
///   assignment to amend.
pub fn grant_custom_permission(
    env: &Env,
    caller: &Address,
    user: &Address,
    permission: Permission,
) -> Result<(), AccessControlError> {
    caller.require_auth();
    if !has_permission(env, caller, &Permission::ManageUsers) {
        return Err(AccessControlError::Unauthorized);
    }

    let key = (ROLE_ASSIGN, user.clone());
    let mut assignment: RoleAssignment = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(AccessControlError::NotFound)?;

    assignment.custom_grants.push_back(permission);
    env.storage().persistent().set(&key, &assignment);
    Ok(())
}

/// Explicitly deny `permission` for `user`, overriding role and group grants.
///
/// # Arguments
///
/// * `caller` - must hold [`Permission::ManageUsers`].
/// * `user` - account to deny; must already have a role assignment.
/// * `permission` - the [`Permission`] to revoke.
///
/// # Errors
///
/// * [`Unauthorized`](AccessControlError::Unauthorized) - `caller` lacks
///   [`Permission::ManageUsers`].
/// * [`NotFound`](AccessControlError::NotFound) - `user` has no role
///   assignment to amend.
pub fn revoke_custom_permission(
    env: &Env,
    caller: &Address,
    user: &Address,
    permission: Permission,
) -> Result<(), AccessControlError> {
    caller.require_auth();
    if !has_permission(env, caller, &Permission::ManageUsers) {
        return Err(AccessControlError::Unauthorized);
    }

    let key = (ROLE_ASSIGN, user.clone());
    let mut assignment: RoleAssignment = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(AccessControlError::NotFound)?;

    assignment.custom_revokes.push_back(permission);
    env.storage().persistent().set(&key, &assignment);
    Ok(())
}

/// Delegate an entire `role` from `delegator` to `delegatee`.
///
/// # Arguments
///
/// * `delegator` - account lending the role; must authorize the call.
/// * `delegatee` - account receiving it.
/// * `role` - the [`Role`] to delegate.
/// * `expires_at` - unix seconds when the delegation lapses; `0` = never.
///
/// Only the delegation's author can revoke it (see [`revoke_delegation`]).
pub fn delegate_role(
    env: &Env,
    delegator: &Address,
    delegatee: &Address,
    role: Role,
    expires_at: u64,
) -> Result<(), AccessControlError> {
    delegator.require_auth();

    let max_depth = get_max_delegation_depth(env);
    let current_depth = get_delegator_depth(env, delegator);
    let new_depth = current_depth.saturating_add(1);
    if new_depth > max_depth {
        return Err(AccessControlError::DelegationDepthExceeded);
    }

    let key = (DELEGATION_KEY, delegator.clone(), delegatee.clone());
    let delegation = Delegation {
        delegator: delegator.clone(),
        delegatee: delegatee.clone(),
        role,
        expires_at,
    };
    env.storage().persistent().set(&key, &delegation);
    env.storage()
        .persistent()
        .set(&(DEL_PARENT, delegatee.clone()), delegator);
    Ok(())
}

/// Delete the full-role delegation from `delegator` to `delegatee`.
///
/// # Arguments
///
/// * `delegator` - delegation author; must authorize the call.
/// * `delegatee` - counterparty of the delegation to remove.
///
/// Succeeds even if no delegation exists.
pub fn revoke_delegation(
    env: &Env,
    delegator: &Address,
    delegatee: &Address,
) -> Result<(), AccessControlError> {
    delegator.require_auth();
    let key = (DELEGATION_KEY, delegator.clone(), delegatee.clone());
    env.storage().persistent().remove(&key);
    env.storage()
        .persistent()
        .remove(&(DEL_PARENT, delegatee.clone()));
    Ok(())
}

/// Delegate a specific list of `permissions` from `delegator` to `delegatee`
/// without transferring the whole role.
///
/// # Arguments
///
/// * `delegator` - account lending the permissions; must authorize the call.
/// * `delegatee` - account receiving them.
/// * `permissions` - the [`Permission`]s covered by this delegation.
/// * `expires_at` - unix seconds when the delegation lapses; `0` = never.
pub fn delegate_permissions(
    env: &Env,
    delegator: &Address,
    delegatee: &Address,
    permissions: Vec<Permission>,
    expires_at: u64,
) -> Result<(), AccessControlError> {
    delegator.require_auth();

    let max_depth = get_max_delegation_depth(env);
    let current_depth = get_delegator_depth(env, delegator);
    let new_depth = current_depth.saturating_add(1);
    if new_depth > max_depth {
        return Err(AccessControlError::DelegationDepthExceeded);
    }

    let key = (SCOPED_DEL_KEY, delegator.clone(), delegatee.clone());
    let scoped = ScopedDelegation {
        delegator: delegator.clone(),
        delegatee: delegatee.clone(),
        permissions,
        expires_at,
    };
    env.storage().persistent().set(&key, &scoped);
    env.storage()
        .persistent()
        .set(&(DEL_PARENT, delegatee.clone()), delegator);
    Ok(())
}

/// Delete the scoped (permission-list) delegation from `delegator` to
/// `delegatee`.
///
/// # Arguments
///
/// * `delegator` - delegation author; must authorize the call.
/// * `delegatee` - counterparty of the delegation to remove.
///
/// Succeeds even if no delegation exists.
pub fn revoke_scoped_delegation(
    env: &Env,
    delegator: &Address,
    delegatee: &Address,
) -> Result<(), AccessControlError> {
    delegator.require_auth();
    let key = (SCOPED_DEL_KEY, delegator.clone(), delegatee.clone());
    env.storage().persistent().remove(&key);
    env.storage()
        .persistent()
        .remove(&(DEL_PARENT, delegatee.clone()));
    Ok(())
}

/// Create (or overwrite) an ACL group named `group_name` with `permissions`.
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
    env: &Env,
    caller: &Address,
    group_name: Symbol,
    permissions: Vec<Permission>,
) -> Result<(), AccessControlError> {
    caller.require_auth();
    if !has_permission(env, caller, &Permission::ManageAccess) {
        return Err(AccessControlError::Unauthorized);
    }

    let grp = AclGroup {
        name: group_name.clone(),
        permissions,
    };
    env.storage().persistent().set(&(ACL_GRP, group_name), &grp);
    Ok(())
}

/// Add `user` to `group_name` (no-op if already a member).
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
    env: &Env,
    caller: &Address,
    user: &Address,
    group_name: Symbol,
) -> Result<(), AccessControlError> {
    caller.require_auth();
    if !has_permission(env, caller, &Permission::ManageAccess) {
        return Err(AccessControlError::Unauthorized);
    }

    let user_key = (USR_GRPS, user.clone());
    let mut groups: Vec<Symbol> = env
        .storage()
        .persistent()
        .get(&user_key)
        .unwrap_or(Vec::new(env));

    // Avoid duplicate group assignments
    for g in groups.iter() {
        if g == group_name {
            return Ok(());
        }
    }

    groups.push_back(group_name);
    env.storage().persistent().set(&user_key, &groups);
    Ok(())
}

/// Remove `user` from `group_name` (no-op if not a member).
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
    env: &Env,
    caller: &Address,
    user: &Address,
    group_name: Symbol,
) -> Result<(), AccessControlError> {
    caller.require_auth();
    if !has_permission(env, caller, &Permission::ManageAccess) {
        return Err(AccessControlError::Unauthorized);
    }

    let user_key = (USR_GRPS, user.clone());
    let groups: Option<Vec<Symbol>> = env.storage().persistent().get(&user_key);

    if let Some(list) = groups {
        let mut new_list = Vec::new(env);
        for g in list.iter() {
            if g != group_name {
                new_list.push_back(g);
            }
        }
        env.storage().persistent().set(&user_key, &new_list);
    }
    Ok(())
}

/// Return the ACL groups `user` belongs to (empty if none).
///
/// # Arguments
///
/// * `user` - account whose group memberships are returned.
pub fn get_user_groups(env: &Env, user: &Address) -> Vec<Symbol> {
    env.storage()
        .persistent()
        .get(&(USR_GRPS, user.clone()))
        .unwrap_or(Vec::new(env))
}

/// Create or update an attribute-based access policy.
///
/// # Arguments
///
/// * `caller` - must hold [`Permission::SystemAdmin`].
/// * `policy_id` - symbol keying the policy.
/// * `name` - human-readable policy name.
/// * `conditions` - role, credential, sensitivity, and time conditions the
///   policy enforces; the policy starts enabled.
///
/// # Errors
///
/// [`Unauthorized`](AccessControlError::Unauthorized) if `caller` lacks
/// [`Permission::SystemAdmin`].
pub fn create_access_policy(
    env: &Env,
    caller: &Address,
    policy_id: Symbol,
    name: Symbol,
    conditions: PolicyConditions,
) -> Result<(), AccessControlError> {
    caller.require_auth();
    if !has_permission(env, caller, &Permission::SystemAdmin) {
        return Err(AccessControlError::Unauthorized);
    }

    let policy = AccessPolicy {
        id: policy_id.clone(),
        name,
        conditions,
        enabled: true,
    };
    env.storage()
        .persistent()
        .set(&(ACC_POL, policy_id), &policy);
    Ok(())
}

/// Record `user`'s professional credential (used by ABAC policy evaluation).
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
    env: &Env,
    caller: &Address,
    user: &Address,
    credential: CredentialType,
) -> Result<(), AccessControlError> {
    caller.require_auth();
    if !has_permission(env, caller, &Permission::ManageUsers) {
        return Err(AccessControlError::Unauthorized);
    }

    env.storage()
        .persistent()
        .set(&(USR_CRED, user.clone()), &credential);
    Ok(())
}

/// Classify the sensitivity of a medical record (used by ABAC evaluation).
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
    env: &Env,
    caller: &Address,
    record_id: u64,
    sensitivity: SensitivityLevel,
) -> Result<(), AccessControlError> {
    caller.require_auth();
    if !has_permission(env, caller, &Permission::WriteRecord) {
        return Err(AccessControlError::Unauthorized);
    }

    env.storage()
        .persistent()
        .set(&(REC_SENS, record_id), &sensitivity);
    Ok(())
}

/// Evaluate ABAC policy `policy_id` for `user` against `record_id` at
/// `current_time` (unix seconds).
///
/// # Arguments
///
/// * `policy_id` - policy to evaluate.
/// * `user` - account requesting access.
/// * `record_id` - record whose stored sensitivity is checked.
/// * `current_time` - clock used for time restrictions.
///
/// Returns `false` when the policy is missing or disabled, the user's role or
/// credential is insufficient, the record's sensitivity is below the policy's
/// minimum, or the time restriction is not satisfied.
pub fn evaluate_access_policy(
    env: &Env,
    policy_id: Symbol,
    user: &Address,
    record_id: u64,
    current_time: u64,
) -> bool {
    let policy: Option<AccessPolicy> = env.storage().persistent().get(&(ACC_POL, policy_id));
    let policy = match policy {
        Some(p) if p.enabled => p,
        _ => return false,
    };

    let user_role = get_role(env, user);
    if (user_role as u32) < (policy.conditions.required_role as u32) {
        return false;
    }

    // Check credential
    let cred: CredentialType = env
        .storage()
        .persistent()
        .get(&(USR_CRED, user.clone()))
        .unwrap_or(CredentialType::None);
    if policy.conditions.required_credential != CredentialType::None
        && cred != policy.conditions.required_credential
    {
        return false;
    }

    // Check sensitivity
    let sens: SensitivityLevel = env
        .storage()
        .persistent()
        .get(&(REC_SENS, record_id))
        .unwrap_or(SensitivityLevel::Standard);
    if (sens as u32) < (policy.conditions.min_sensitivity_level as u32) {
        return false;
    }

    // Check time restriction
    match policy.conditions.time_restriction {
        TimeRestriction::None => {}
        TimeRestriction::BusinessHours => {
            let hour = (current_time / 3600) % 24;
            if !(9..17).contains(&hour) {
                return false;
            }
        }
        TimeRestriction::HourRange(start, end) => {
            let hour = ((current_time / 3600) % 24) as u32;
            if hour < start || hour > end {
                return false;
            }
        }
        TimeRestriction::DaysOfWeek(mask) => {
            let day = ((current_time / 86400 + 4) % 7) as u32;
            if (mask & (1 << day)) == 0 {
                return false;
            }
        }
    }

    true
}
