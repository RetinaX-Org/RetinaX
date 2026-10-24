//! Two-phase commit (prepare/commit/rollback) for users, records and grants.

use super::*;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl VisionRecordsContract {
    /// Check access for a specific record with ABAC evaluation
    /// Prepare phase for register_user operation
    pub fn prepare_register_user(
        env: Env,
        caller: Address,
        user: Address,
        role: Role,
        name: String,
    ) -> Result<(), ContractError> {
        // Validate all inputs without making state changes
        circuit_breaker::require_not_paused(
            &env,
            &circuit_breaker::PauseScope::Function(symbol_short!("REG_USR")),
        )?;

        if !whitelist::check_whitelist_access(&env, &caller) {
            return Err(ContractError::Unauthorized);
        }

        if !Self::has_permission_unified(&env, &caller, &Permission::ManageUsers) {
            return Err(ContractError::Unauthorized);
        }

        validation::validate_name(&name)?;

        // Check if user already exists
        let key = (symbol_short!("USER"), user.clone());
        if env.storage().persistent().get::<_, User>(&key).is_some() {
            return Err(ContractError::UserAlreadyExists);
        }

        // Store temporary preparation data
        let prep_key = (symbol_short!("P_REG_USR"), user.clone());
        let prep_data = PrepareUserRegistration {
            caller: caller.clone(),
            user: user.clone(),
            role,
            name: name.clone(),
            timestamp: env.ledger().timestamp(),
        };
        env.storage().temporary().set(&prep_key, &prep_data);

        Ok(())
    }

    /// Commit phase for register_user operation
    pub fn commit_register_user(
        env: Env,
        caller: Address,
        user: Address,
        role: Role,
        name: String,
    ) -> Result<(), ContractError> {
        // Retrieve preparation data
        let prep_key = (symbol_short!("P_REG_USR"), user.clone());
        let prep_data: PrepareUserRegistration = env
            .storage()
            .temporary()
            .get(&prep_key)
            .ok_or(ContractError::InvalidPhase)?;

        // Verify preparation data matches commit parameters
        if prep_data.caller != caller
            || prep_data.user != user
            || prep_data.role != role
            || prep_data.name != name
        {
            return Err(ContractError::InvalidPhase);
        }

        // Execute the actual registration
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

        // Clean up preparation data
        env.storage().temporary().remove(&prep_key);

        events::publish_user_registered(&env, user, role, name);

        Ok(())
    }

    /// Rollback for register_user operation
    pub fn rollback_register_user(
        env: Env,
        user: Address,
        _role: Role,
        _name: String,
    ) -> Result<(), ContractError> {
        // Clean up preparation data
        let prep_key = (symbol_short!("P_REG_USR"), user.clone());
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }

    // ======================== Two-Phase Commit Hooks ========================

    /// Prepare phase for adding a vision record
    pub fn prepare_add_record(
        env: Env,
        caller: Address,
        patient: Address,
        provider: Address,
        record_type: RecordType,
        data_hash: String,
    ) -> Result<u64, ContractError> {
        // Validate all inputs without making state changes
        circuit_breaker::require_not_paused(
            &env,
            &circuit_breaker::PauseScope::Function(symbol_short!("ADD_REC")),
        )?;

        if !whitelist::check_whitelist_access(&env, &caller) {
            return Err(ContractError::Unauthorized);
        }

        Self::enforce_rate_limit(&env, &caller)?;
        validation::validate_data_hash(&data_hash)?;

        // Check permissions
        let has_perm = if caller == provider {
            Self::has_permission_unified(&env, &caller, &Permission::WriteRecord)
        } else {
            Self::has_delegated_permission_unified(
                &env,
                &provider,
                &caller,
                &Permission::WriteRecord,
            )
        };

        if !has_perm && !Self::has_permission_unified(&env, &caller, &Permission::SystemAdmin) {
            return Err(ContractError::Unauthorized);
        }

        // Generate and return the record ID that will be used
        let counter_key = symbol_short!("REC_CTR");
        let record_id: u64 = env
            .storage()
            .instance()
            .get(&counter_key)
            .unwrap_or(0u64)
            .saturating_add(1u64);

        // Store preparation data temporarily
        let prep_key = (symbol_short!("P_ADD_REC"), record_id);
        let prep_data = PrepareAddRecord {
            caller,
            patient,
            provider,
            record_type,
            data_hash,
            timestamp: env.ledger().timestamp(),
        };
        env.storage().temporary().set(&prep_key, &prep_data);

        Ok(record_id)
    }

    /// Commit phase for adding a vision record
    pub fn commit_add_record(env: Env, record_id: u64) -> Result<(), ContractError> {
        // Retrieve preparation data
        let prep_key = (symbol_short!("P_ADD_REC"), record_id);
        let prep_data: PrepareAddRecord = env
            .storage()
            .temporary()
            .get(&prep_key)
            .ok_or(ContractError::InvalidInput)?;

        // Update the counter
        let counter_key = symbol_short!("REC_CTR");
        env.storage().instance().set(&counter_key, &record_id);

        // Create the actual record
        let record = VisionRecord {
            id: record_id,
            patient: prep_data.patient.clone(),
            provider: prep_data.provider.clone(),
            record_type: prep_data.record_type.clone(),
            data_hash: prep_data.data_hash.clone(),
            key_version: None,
            created_at: prep_data.timestamp,
            updated_at: prep_data.timestamp,
        };

        // Store the record
        let key = (symbol_short!("RECORD"), record_id);
        env.storage().persistent().set(&key, &record);
        extend_ttl_u64_key(&env, &key);

        // Add to patient's record list
        let patient_key = (symbol_short!("PAT_REC"), prep_data.patient.clone());
        let mut patient_records: Vec<u64> = env
            .storage()
            .persistent()
            .get(&patient_key)
            .unwrap_or(Vec::new(&env));
        patient_records.push_back(record_id);
        env.storage()
            .persistent()
            .set(&patient_key, &patient_records);
        extend_ttl_address_key(&env, &patient_key);

        // Clean up preparation data
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }

    /// Rollback phase for adding a vision record
    pub fn rollback_add_record(env: Env, record_id: u64) -> Result<(), ContractError> {
        // Clean up preparation data
        let prep_key = (symbol_short!("P_ADD_REC"), record_id);
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }

    /// Prepare phase for granting access
    pub fn prepare_grant_access(
        env: Env,
        caller: Address,
        patient: Address,
        grantee: Address,
        access_level: AccessLevel,
        expires_at: Option<u64>,
    ) -> Result<(), ContractError> {
        // Validate inputs without state changes
        if caller != patient
            && !Self::has_permission_unified(&env, &caller, &Permission::SystemAdmin)
        {
            return Err(ContractError::Unauthorized);
        }

        if access_level == AccessLevel::None {
            return Err(ContractError::InvalidInput);
        }

        // Store preparation data
        let prep_key = (symbol_short!("P_GNT_ACC"), patient.clone(), grantee.clone());
        let prep_data = PrepareGrantAccess {
            caller,
            patient,
            grantee,
            access_level,
            expires_at,
            timestamp: env.ledger().timestamp(),
        };
        env.storage().temporary().set(&prep_key, &prep_data);

        Ok(())
    }

    /// Commit phase for granting access
    pub fn commit_grant_access(
        env: Env,
        patient: Address,
        grantee: Address,
    ) -> Result<(), ContractError> {
        // Retrieve preparation data
        let prep_key = (symbol_short!("P_GNT_ACC"), patient.clone(), grantee.clone());
        let prep_data: PrepareGrantAccess = env
            .storage()
            .temporary()
            .get(&prep_key)
            .ok_or(ContractError::InvalidInput)?;

        // Create the access grant
        let grant = AccessGrant {
            patient: prep_data.patient.clone(),
            grantee: prep_data.grantee.clone(),
            level: prep_data.access_level,
            granted_at: prep_data.timestamp,
            expires_at: prep_data.expires_at.unwrap_or(0),
        };

        // Store the grant
        let key = (
            symbol_short!("GRANT"),
            prep_data.patient.clone(),
            prep_data.grantee.clone(),
        );
        env.storage().persistent().set(&key, &grant);
        extend_ttl_access_key(&env, &key);

        // Add to patient's grants list
        let grants_key = (symbol_short!("GRANTS"), prep_data.patient.clone());
        let mut grants: Vec<Address> = env
            .storage()
            .persistent()
            .get(&grants_key)
            .unwrap_or(Vec::new(&env));

        if !grants.contains(&prep_data.grantee) {
            grants.push_back(prep_data.grantee.clone());
            env.storage().persistent().set(&grants_key, &grants);
            extend_ttl_address_key(&env, &grants_key);
        }

        // Clean up preparation data
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }

    /// Rollback phase for granting access
    pub fn rollback_grant_access(
        env: Env,
        patient: Address,
        grantee: Address,
    ) -> Result<(), ContractError> {
        // Clean up preparation data
        let prep_key = (symbol_short!("P_GNT_ACC"), patient.clone(), grantee.clone());
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }
}
