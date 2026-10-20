//! Admin setup, admin transfer, rate limits, encryption keys, whitelist and admin tiers.

use super::*;
use teye_common::multisig;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl VisionRecordsContract {
    /// Initialize the contract with an admin address
    pub fn initialize(env: Env, admin: Address) -> Result<(), ContractError> {
        if env.storage().instance().has(&INITIALIZED) {
            let context = create_error_context(
                &env,
                ContractError::AlreadyInitialized,
                Some(admin.clone()),
                Some(String::from_str(&env, "initialize")),
            );
            log_error(
                &env,
                ContractError::AlreadyInitialized,
                Some(admin),
                None,
                None,
            );
            events::publish_error(&env, ContractError::AlreadyInitialized as u32, context);
            return Err(ContractError::AlreadyInitialized);
        }

        // admin.require_auth();

        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&INITIALIZED, &true);
        rbac::assign_role(&env, admin.clone(), Role::Admin, 0);

        // Bootstrap the admin with the Admin role so they can register other users
        rbac::assign_role(&env, admin.clone(), Role::Admin, 0);

        // Assign the Admin RBAC role so the admin has permissions
        rbac::assign_role(&env, admin.clone(), Role::Admin, 0);

        // Bootstrap the initializing admin as SuperAdmin in the tier system
        admin_tiers::set_super_admin(&env, &admin);
        admin_tiers::track_admin(&env, &admin);

        events::publish_initialized(&env, admin);

        Ok(())
    }

    /// Get the admin address
    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        match env.storage().instance().get(&ADMIN) {
            Some(admin) => Ok(admin),
            None => {
                let context = create_error_context(
                    &env,
                    ContractError::NotInitialized,
                    None,
                    Some(String::from_str(&env, "get_admin")),
                );
                log_error(&env, ContractError::NotInitialized, None, None, None);
                events::publish_error(&env, ContractError::NotInitialized as u32, context);
                Err(ContractError::NotInitialized)
            }
        }
    }

    /// Check if the contract is initialized
    pub fn is_initialized(env: Env) -> bool {
        env.storage().instance().has(&INITIALIZED)
    }

    /// Configure the clinical_records contract that owns the canonical prescription
    /// and examination data structures.
    pub fn set_clinical_records_contract(
        env: Env,
        admin: Address,
        clinical_records: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Self::unauthorized(
                &env,
                &admin,
                "set_clinical_records_contract",
                "current_admin",
            );
        }
        env.storage()
            .instance()
            .set(&symbol_short!("CLIN_REC"), &clinical_records);
        Ok(())
    }

    /// Return the configured clinical_records contract address, if any.
    pub fn get_clinical_records_contract(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("CLIN_REC"))
    }

    /// Propose a new admin address. Only the current admin can call this.
    /// The new admin must call `accept_admin` to complete the transfer.
    pub fn propose_admin(
        env: Env,
        current_admin: Address,
        new_admin: Address,
    ) -> Result<(), ContractError> {
        current_admin.require_auth();

        let admin = Self::get_admin(env.clone())?;
        if current_admin != admin {
            return Self::unauthorized(&env, &current_admin, "propose_admin", "current_admin");
        }

        env.storage().instance().set(&PENDING_ADMIN, &new_admin);

        events::publish_admin_transfer_proposed(&env, current_admin, new_admin);

        Ok(())
    }

    /// Accept the pending admin transfer. Only the proposed new admin can call this.
    /// Completes the two-step admin transfer process.
    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), ContractError> {
        new_admin.require_auth();

        let pending: Address = env
            .storage()
            .instance()
            .get(&PENDING_ADMIN)
            .ok_or(ContractError::InvalidInput)?;

        if new_admin != pending {
            return Self::unauthorized(&env, &new_admin, "accept_admin", "pending_admin");
        }

        let old_admin = Self::get_admin(env.clone())?;

        env.storage().instance().set(&ADMIN, &new_admin);
        env.storage().instance().remove(&PENDING_ADMIN);

        events::publish_admin_transfer_accepted(&env, old_admin, new_admin);

        Ok(())
    }

    /// Cancel a pending admin transfer. Only the current admin can call this.
    pub fn cancel_admin_transfer(env: Env, current_admin: Address) -> Result<(), ContractError> {
        current_admin.require_auth();

        let admin = Self::get_admin(env.clone())?;
        if current_admin != admin {
            return Self::unauthorized(
                &env,
                &current_admin,
                "cancel_admin_transfer",
                "current_admin",
            );
        }

        let pending: Address = env
            .storage()
            .instance()
            .get(&PENDING_ADMIN)
            .ok_or(ContractError::InvalidInput)?;

        env.storage().instance().remove(&PENDING_ADMIN);

        events::publish_admin_transfer_cancelled(&env, current_admin, pending);

        Ok(())
    }

    /// Get the pending admin address, if any.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&PENDING_ADMIN)
    }

    // ── Admin configuration ──────────────────────────────────────────────────

    /// Configure per-address rate limiting for this contract.
    ///
    /// Requires at least `ContractAdmin` tier, or legacy admin/SystemAdmin.
    /// Uses multisig if configured.
    pub fn set_rate_limit_config(
        env: Env,
        caller: Address,
        max_requests_per_window: u64,
        window_duration_seconds: u64,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();

        if max_requests_per_window == 0 || window_duration_seconds == 0 {
            return Err(ContractError::InvalidInput);
        }

        if !multisig::is_legacy_admin_allowed(&env) {
            if !multisig::is_executable(&env, proposal_id) {
                return Err(ContractError::Unauthorized); // Use Unauthorized for multisig rejection
            }
            multisig::mark_executed(&env, proposal_id).map_err(|_| ContractError::Unauthorized)?;
        } else if !admin_tiers::require_tier(&env, &caller, &AdminTier::ContractAdmin) {
            return Err(ContractError::Unauthorized);
        }

        let auth_session = session::start_or_refresh_session(
            &env,
            &caller,
            progressive_auth::AuthLevel::Level3,
            3_600,
            900,
        );
        let expected_data_hash =
            rate_limit_action_hash(&env, max_requests_per_window, window_duration_seconds);
        let risk = risk_engine::evaluate_risk(
            &env,
            &risk_engine::OperationRiskInput {
                actor: caller.clone(),
                operation: symbol_short!("SET_RATE"),
                action: risk_engine::ActionType::AdminChange,
                sensitivity: risk_engine::DataSensitivity::Sensitive,
                context: risk_engine::RiskContext {
                    off_hours: false,
                    unusual_location: false,
                    unusual_frequency: false,
                    recent_auth_failures: 0,
                    emergency_signal: false,
                },
            },
            None,
        );
        progressive_auth::enforce_for_risk(
            &env,
            &caller,
            risk.final_score,
            auth_session.issued_at,
            Some(proposal_id),
            symbol_short!("SET_RATE"),
            expected_data_hash,
            false,
            &progressive_auth::default_policy(),
        )
        .map_err(|_| ContractError::Unauthorized)?;

        env.storage().instance().set(
            &RATE_CFG,
            &(max_requests_per_window, window_duration_seconds),
        );

        Ok(())
    }

    /// Set or rotate an encryption master key under a given `version`.
    /// Stores the key bytes persistently under (ENC_KEY, version) and updates current.
    pub fn set_encryption_key(
        env: Env,
        caller: Address,
        version: String,
        key: String,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();

        if !multisig::is_legacy_admin_allowed(&env) {
            if !multisig::is_executable(&env, proposal_id) {
                return Err(ContractError::Unauthorized);
            }
            multisig::mark_executed(&env, proposal_id).map_err(|_| ContractError::Unauthorized)?;
        } else {
            let admin = Self::get_admin(env.clone())?;
            let has_system_admin =
                Self::has_permission_unified(&env, &caller, &Permission::SystemAdmin);
            if caller != admin && !has_system_admin {
                return Err(ContractError::Unauthorized);
            }
        }

        // Persist the key hex string under (ENC_KEY, version)
        env.storage()
            .persistent()
            .set(&(ENC_KEY, version.clone()), &key);
        // Update current active version
        env.storage().instance().set(&ENC_CUR, &version);

        Ok(())
    }

    /// Configure the external Key Manager used for per-record key derivation.
    /// Requires at least `ContractAdmin` tier, or legacy admin/SystemAdmin.
    pub fn set_key_manager(
        env: Env,
        caller: Address,
        manager: Address,
        root_key_id: BytesN<32>,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        if !admin_tiers::require_tier(&env, &caller, &AdminTier::ContractAdmin) {
            return Err(ContractError::Unauthorized);
        }

        env.storage().instance().set(&KEY_MGR, &manager);
        env.storage().instance().set(&KEY_MGR_KEY, &root_key_id);

        Ok(())
    }

    /// Return the current rate limiting configuration, if any.
    pub fn get_rate_limit_config(env: Env) -> Option<(u64, u64)> {
        env.storage().instance().get(&RATE_CFG)
    }

    /// Enables or disables whitelist enforcement globally.
    ///
    /// Requires at least `ContractAdmin` tier, or legacy admin/SystemAdmin.
    pub fn set_whitelist_enabled(
        env: Env,
        caller: Address,
        enabled: bool,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        if !admin_tiers::require_tier(&env, &caller, &AdminTier::ContractAdmin) {
            return Self::unauthorized(
                &env,
                &caller,
                "set_whitelist_enabled",
                "admin_tier:ContractAdmin",
            );
        }
        whitelist::set_whitelist_enabled(&env, enabled);
        Ok(())
    }

    /// Adds an address to the whitelist.
    ///
    /// Requires at least `ContractAdmin` tier, or legacy admin/SystemAdmin.
    pub fn add_to_whitelist(env: Env, caller: Address, user: Address) -> Result<(), ContractError> {
        caller.require_auth();
        if !admin_tiers::require_tier(&env, &caller, &AdminTier::ContractAdmin) {
            return Self::unauthorized(
                &env,
                &caller,
                "add_to_whitelist",
                "admin_tier:ContractAdmin",
            );
        }
        whitelist::add_to_whitelist(&env, &user);
        Ok(())
    }

    /// Removes an address from the whitelist.
    ///
    /// Requires at least `ContractAdmin` tier, or legacy admin/SystemAdmin.
    pub fn remove_from_whitelist(
        env: Env,
        caller: Address,
        user: Address,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        if !admin_tiers::require_tier(&env, &caller, &AdminTier::ContractAdmin) {
            return Self::unauthorized(
                &env,
                &caller,
                "remove_from_whitelist",
                "admin_tier:ContractAdmin",
            );
        }
        whitelist::remove_from_whitelist(&env, &user);
        Ok(())
    }

    pub fn is_whitelist_enabled(env: Env) -> bool {
        whitelist::is_whitelist_enabled(&env)
    }

    pub fn is_whitelisted(env: Env, user: Address) -> bool {
        whitelist::is_whitelisted(&env, &user)
    }

    // ── Admin tier management ─────────────────────────────────────────────────

    /// Return the admin tier for a given address.
    pub fn get_admin_tier(env: Env, admin: Address) -> Option<AdminTier> {
        admin_tiers::get_admin_tier(&env, &admin)
    }

    /// Promote a target address to the specified admin tier. SuperAdmin only.
    pub fn promote_admin(
        env: Env,
        caller: Address,
        target: Address,
        tier: AdminTier,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        let is_admin = {
            let stored_admin: Option<Address> =
                env.storage().instance().get(&symbol_short!("ADMIN"));
            stored_admin.is_some_and(|a| a == caller)
        };
        let is_super_admin = admin_tiers::require_tier(&env, &caller, &AdminTier::SuperAdmin);
        if !is_admin && !is_super_admin {
            return Self::unauthorized(&env, &caller, "promote_admin", "SuperAdmin");
        }
        admin_tiers::set_admin_tier(&env, &target, tier);
        admin_tiers::track_admin(&env, &target);
        Ok(())
    }

    /// Remove a target's admin tier. SuperAdmin only.
    pub fn demote_admin(env: Env, caller: Address, target: Address) -> Result<(), ContractError> {
        caller.require_auth();
        let is_admin = {
            let stored_admin: Option<Address> =
                env.storage().instance().get(&symbol_short!("ADMIN"));
            stored_admin.is_some_and(|a| a == caller)
        };
        let is_super_admin = admin_tiers::require_tier(&env, &caller, &AdminTier::SuperAdmin);
        if !is_admin && !is_super_admin {
            return Self::unauthorized(&env, &caller, "demote_admin", "SuperAdmin");
        }
        admin_tiers::untrack_admin(&env, &target);
        Ok(())
    }
}
