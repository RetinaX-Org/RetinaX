//! Patient access grants, per-record access, consent and emergency access.

use super::*;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl VisionRecordsContract {
    /// Grant access to a user
    #[allow(clippy::arithmetic_side_effects)]
    pub fn grant_access(
        env: Env,
        caller: Address,
        patient: Address,
        grantee: Address,
        level: AccessLevel,
        duration_seconds: u64,
    ) -> Result<(), ContractError> {
        let _guard = teye_common::ReentrancyGuard::new(&env);
        circuit_breaker::require_not_paused(
            &env,
            &circuit_breaker::PauseScope::Function(symbol_short!("GRT_ACC")),
        )?;
        caller.require_auth();

        Self::enforce_rate_limit(&env, &caller)?;

        validation::validate_duration(duration_seconds)?;

        let has_perm = if caller == patient {
            true // Patient manages own access
        } else {
            // Specific patient→caller delegation for ManageAccess
            Self::has_delegated_permission_unified(&env, &patient, &caller, &Permission::ManageAccess)
                // Or caller has SystemAdmin (unified: direct + any delegation)
                || Self::has_permission_unified(&env, &caller, &Permission::SystemAdmin)
        };

        if !has_perm {
            // Log failed access grant attempt
            let audit_entry = audit::create_audit_entry(
                &env,
                caller.clone(),
                patient.clone(),
                None,
                AccessAction::GrantAccess,
                AccessResult::Denied,
                Some(String::from_str(&env, "Insufficient permissions")),
            );
            audit::add_audit_entry(&env, &audit_entry);
            events::publish_audit_log_entry(&env, &audit_entry);
            return Self::unauthorized(
                &env,
                &caller,
                "grant_access",
                "patient_or_permission:ManageAccess_or_SystemAdmin",
            );
        }

        let expires_at = env.ledger().timestamp() + duration_seconds;
        let grant = AccessGrant {
            patient: patient.clone(),
            grantee: grantee.clone(),
            level: level.clone(),
            granted_at: env.ledger().timestamp(),
            expires_at,
        };

        let key = (symbol_short!("ACCESS"), patient.clone(), grantee.clone());
        env.storage().persistent().set(&key, &grant);
        extend_ttl_access_key(&env, &key);

        // Track the grantee address in the patient's grantee list for purge iteration.
        let list_key = (symbol_short!("ACC_LST"), patient.clone());
        let mut grantees: Vec<Address> = env
            .storage()
            .persistent()
            .get(&list_key)
            .unwrap_or(Vec::new(&env));
        // Avoid duplicates: only append if not already present.
        let mut found = false;
        for i in 0..grantees.len() {
            if grantees.get(i) == Some(grantee.clone()) {
                found = true;
                break;
            }
        }
        if !found {
            grantees.push_back(grantee.clone());
            env.storage().persistent().set(&list_key, &grantees);
        }

        events::publish_access_granted(
            &env,
            patient.clone(),
            grantee.clone(),
            level,
            duration_seconds,
            expires_at,
        );

        let record_ids = Self::get_patient_records(env.clone(), patient.clone());
        for i in 0..record_ids.len() {
            if let Some(record_id) = record_ids.get(i) {
                let _ = lineage::add_edge(
                    &env,
                    record_id,
                    record_id,
                    RelationshipKind::SharedWith,
                    grantee.clone(),
                    None,
                );
            }
        }

        Ok(())
    }

    /// Grant access to multiple users in a single transaction.
    /// Patient authorizes once for the entire batch.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn grant_access_batch(
        env: Env,
        patient: Address,
        grants: Vec<BatchGrantInput>,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env, &circuit_breaker::PauseScope::Global)?;
        patient.require_auth();

        if grants.is_empty() {
            return Err(ContractError::InvalidInput);
        }

        let now = env.ledger().timestamp();
        for grant in grants.iter() {
            let expires_at = now + grant.duration_seconds;
            let access_grant = AccessGrant {
                patient: patient.clone(),
                grantee: grant.grantee.clone(),
                level: grant.level.clone(),
                granted_at: now,
                expires_at,
            };
            let key = (
                symbol_short!("ACCESS"),
                patient.clone(),
                grant.grantee.clone(),
            );
            env.storage().persistent().set(&key, &access_grant);

            events::publish_access_granted(
                &env,
                patient.clone(),
                grant.grantee.clone(),
                grant.level.clone(),
                grant.duration_seconds,
                expires_at,
            );
        }

        events::publish_batch_access_granted(&env, patient, grants.len());

        Ok(())
    }

    /// Check access level with ABAC policy evaluation
    pub fn check_access(env: Env, patient: Address, grantee: Address) -> AccessLevel {
        // First check traditional consent-based access
        if !has_active_consent(&env, &patient, &grantee) {
            return AccessLevel::None;
        }

        let key = (symbol_short!("ACCESS"), patient.clone(), grantee.clone());

        if let Some(grant) = env.storage().persistent().get::<_, AccessGrant>(&key) {
            if grant.expires_at > env.ledger().timestamp() {
                // Check if ABAC policies also allow this access
                let abac_allowed =
                    evaluate_access_policies(&env, &grantee, None, Some(patient.clone()));
                if abac_allowed {
                    return grant.level;
                }
            }
        }
        AccessLevel::None
    }

    /// Grant record-level access to a specific record.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn grant_record_access(
        env: Env,
        patient: Address,
        grantee: Address,
        record_id: u64,
        level: AccessLevel,
        duration_seconds: u64,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env, &circuit_breaker::PauseScope::Global)?;
        patient.require_auth();
        validation::validate_duration(duration_seconds)?;

        let record_key = (symbol_short!("RECORD"), record_id);
        let record: VisionRecord = env
            .storage()
            .persistent()
            .get(&record_key)
            .ok_or(ContractError::RecordNotFound)?;
        if record.patient != patient {
            return Self::unauthorized(&env, &patient, "grant_record_access", "record_owner");
        }

        let now = env.ledger().timestamp();
        let expires_at = now + duration_seconds;
        let grant = AccessGrant {
            patient: patient.clone(),
            grantee: grantee.clone(),
            level: level.clone(),
            granted_at: now,
            expires_at,
        };

        let key = (symbol_short!("REC_ACC"), record_id, grantee.clone());
        env.storage().persistent().set(&key, &grant);
        extend_ttl_record_access_key(&env, &key);

        events::publish_record_access_granted(
            &env,
            patient,
            grantee,
            record_id,
            level,
            duration_seconds,
            expires_at,
        );
        Ok(())
    }

    /// Check record-level access for a specific grantee.
    pub fn check_record_access(env: Env, record_id: u64, grantee: Address) -> AccessLevel {
        let key = (symbol_short!("REC_ACC"), record_id, grantee);
        if let Some(grant) = env.storage().persistent().get::<_, AccessGrant>(&key) {
            if grant.expires_at > env.ledger().timestamp() {
                return grant.level;
            }
        }
        AccessLevel::None
    }

    /// Revoke record-level access for a specific record.
    pub fn revoke_record_access(
        env: Env,
        patient: Address,
        grantee: Address,
        record_id: u64,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env, &circuit_breaker::PauseScope::Global)?;
        patient.require_auth();
        let record_key = (symbol_short!("RECORD"), record_id);
        let record: VisionRecord = env
            .storage()
            .persistent()
            .get(&record_key)
            .ok_or(ContractError::RecordNotFound)?;
        if record.patient != patient {
            return Self::unauthorized(&env, &patient, "revoke_record_access", "record_owner");
        }

        let key = (symbol_short!("REC_ACC"), record_id, grantee);
        env.storage().persistent().remove(&key);
        Ok(())
    }

    /// Grant consent for a grantee.
    pub fn grant_consent(
        env: Env,
        patient: Address,
        grantee: Address,
        consent_type: ConsentType,
        duration_seconds: u64,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env, &circuit_breaker::PauseScope::Global)?;
        patient.require_auth();
        if duration_seconds == 0 {
            return Err(ContractError::InvalidInput);
        }
        let now = env.ledger().timestamp();
        let expires_at = now.saturating_add(duration_seconds);
        let consent = ConsentGrant {
            patient: patient.clone(),
            grantee: grantee.clone(),
            consent_type: consent_type.clone(),
            granted_at: now,
            expires_at,
            revoked: false,
        };
        let key = consent_key(&patient, &grantee);
        let existing = env.storage().persistent().get::<_, ConsentGrant>(&key);
        let old_expires_at = existing.as_ref().map(|c| c.expires_at);

        env.storage().persistent().set(&key, &consent);
        extend_ttl_access_key(&env, &key);

        if let Some(old_exp) = old_expires_at {
            events::publish_consent_updated(
                &env,
                patient.clone(),
                grantee.clone(),
                consent_type.clone(),
                old_exp,
                expires_at,
            );
        }

        events::publish_consent_granted(
            &env,
            patient,
            grantee,
            consent_type,
            now,
            expires_at,
            duration_seconds,
        );
        Ok(())
    }

    /// Revoke previously granted consent.
    pub fn revoke_consent(
        env: Env,
        patient: Address,
        grantee: Address,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env, &circuit_breaker::PauseScope::Global)?;
        patient.require_auth();
        let key = consent_key(&patient, &grantee);
        let now = env.ledger().timestamp();
        if let Some(mut consent) = env.storage().persistent().get::<_, ConsentGrant>(&key) {
            consent.revoked = true;
            env.storage().persistent().set(&key, &consent);
        }
        events::publish_consent_revoked(&env, patient, grantee, now);
        Ok(())
    }

    /// Revoke access
    pub fn revoke_access(
        env: Env,
        patient: Address,
        grantee: Address,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(
            &env,
            &circuit_breaker::PauseScope::Function(symbol_short!("RVK_ACC")),
        )?;
        patient.require_auth();

        let key = (symbol_short!("ACCESS"), patient.clone(), grantee.clone());
        env.storage().persistent().remove(&key);

        // Log successful access revoke
        let audit_entry = audit::create_audit_entry(
            &env,
            patient.clone(),
            patient.clone(),
            None,
            AccessAction::RevokeAccess,
            AccessResult::Success,
            None,
        );
        audit::add_audit_entry(&env, &audit_entry);
        events::publish_audit_log_entry(&env, &audit_entry);

        Ok(())
    }

    /// Return an active emergency access grant for a patient and requester.
    pub fn check_emergency_access(
        env: Env,
        patient: Address,
        requester: Address,
    ) -> Option<EmergencyAccess> {
        emergency::has_active_emergency_access(&env, &patient, &requester)
    }

    pub fn get_emergency_audit_trail(env: Env, access_id: u64) -> Vec<EmergencyAuditEntry> {
        emergency::get_audit_entries(&env, access_id)
    }

    pub fn access_record_via_emergency(
        env: Env,
        requester: Address,
        patient: Address,
        record_id: Option<u64>,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(&env, &circuit_breaker::PauseScope::Global)?;
        requester.require_auth();

        let access = emergency::has_active_emergency_access(&env, &patient, &requester)
            .ok_or(ContractError::AccessDenied)?;
        let timestamp = env.ledger().timestamp();
        emergency::add_audit_entry(
            &env,
            &EmergencyAuditEntry {
                access_id: access.id,
                actor: requester.clone(),
                action: String::from_str(&env, "ACCESSED"),
                timestamp,
            },
        );
        events::publish_emergency_access_used(&env, access.id, patient, requester, record_id);

        Ok(())
    }
}
