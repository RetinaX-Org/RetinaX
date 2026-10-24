//! User registration and lookup.

use super::*;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl VisionRecordsContract {
    /// Register a new user
    pub fn register_user(
        env: Env,
        caller: Address,
        user: Address,
        role: Role,
        name: String,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(
            &env,
            &circuit_breaker::PauseScope::Function(symbol_short!("REG_USR")),
        )?;
        caller.require_auth();

        if !whitelist::check_whitelist_access(&env, &caller) {
            return Self::unauthorized(&env, &caller, "register_user", "whitelisted_caller");
        }

        // Unified check: covers direct role, custom grants, and delegated roles
        if !Self::has_permission_unified(&env, &caller, &Permission::ManageUsers) {
            let resource_id = String::from_str(&env, "register_user");
            let context = create_error_context(
                &env,
                ContractError::Unauthorized,
                Some(caller.clone()),
                Some(resource_id.clone()),
            );
            log_error(
                &env,
                ContractError::Unauthorized,
                Some(caller.clone()),
                Some(resource_id),
                None,
            );
            events::publish_error(&env, ContractError::Unauthorized as u32, context);
            return Self::unauthorized(&env, &caller, "register_user", "permission:ManageUsers");
        }

        validation::validate_name(&name)?;

        let user_data = User {
            address: user.clone(),
            role,
            name: name.clone(),
            registered_at: env.ledger().timestamp(),
            is_active: true,
        };

        let key = (symbol_short!("USER"), user.clone());
        env.storage().persistent().set(&key, &user_data);
        extend_ttl_address_key(&env, &key);
        rbac::assign_role(&env, user.clone(), role, 0);

        rbac::assign_role(&env, user.clone(), role, 0);

        // Assign the role in the RBAC system
        rbac::assign_role(&env, user.clone(), role, 0);

        // Create the RBAC role assignment so has_permission works
        rbac::assign_role(&env, user.clone(), role, 0);

        events::publish_user_registered(&env, user, role, name);

        Ok(())
    }

    /// Get user information
    pub fn get_user(env: Env, user: Address) -> Result<User, ContractError> {
        let key = (symbol_short!("USER"), user.clone());
        match env.storage().persistent().get(&key) {
            Some(user_data) => Ok(user_data),
            None => {
                let resource_id = String::from_str(&env, "get_user");
                let context = create_error_context(
                    &env,
                    ContractError::UserNotFound,
                    Some(user.clone()),
                    Some(resource_id.clone()),
                );
                log_error(
                    &env,
                    ContractError::UserNotFound,
                    Some(user),
                    Some(resource_id),
                    None,
                );
                events::publish_error(&env, ContractError::UserNotFound as u32, context);
                Err(ContractError::UserNotFound)
            }
        }
    }
}
