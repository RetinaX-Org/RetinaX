//! Vision record creation and reads, examinations, and write-conflict handling.

use super::*;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl VisionRecordsContract {
    /// Add a vision record
    #[allow(clippy::arithmetic_side_effects)]
    pub fn add_record(
        env: Env,
        caller: Address,
        patient: Address,
        provider: Address,
        record_type: RecordType,
        data_hash: String,
    ) -> Result<u64, ContractError> {
        let _guard = teye_common::ReentrancyGuard::new(&env);
        circuit_breaker::require_not_paused(
            &env,
            &circuit_breaker::PauseScope::Function(symbol_short!("ADD_REC")),
        )?;
        caller.require_auth();

        if !whitelist::check_whitelist_access(&env, &caller) {
            return Self::unauthorized(&env, &caller, "add_record", "whitelisted_caller");
        }

        Self::enforce_rate_limit(&env, &caller)?;

        validation::validate_data_hash(&data_hash)?;

        // If caller is the provider, unified check covers direct + delegated WriteRecord.
        // Otherwise, check if this specific provider delegated to the caller.
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

        // Fall back to SystemAdmin (unified: direct role + any delegation)
        if !has_perm && !Self::has_permission_unified(&env, &caller, &Permission::SystemAdmin) {
            // Log failed write attempt
            let audit_entry = audit::create_audit_entry(
                &env,
                caller.clone(),
                patient.clone(),
                None,
                AccessAction::Write,
                AccessResult::Denied,
                Some(String::from_str(&env, "Insufficient permissions")),
            );
            audit::add_audit_entry(&env, &audit_entry);
            events::publish_audit_log_entry(&env, &audit_entry);

            let context = create_error_context(
                &env,
                ContractError::Unauthorized,
                Some(caller.clone()),
                Some(String::from_str(&env, "add_record")),
            );
            log_error(
                &env,
                ContractError::Unauthorized,
                Some(caller.clone()),
                None,
                None,
            );
            events::publish_error(&env, ContractError::Unauthorized as u32, context);
            return Self::unauthorized(
                &env,
                &caller,
                "add_record",
                "permission:WriteRecord_or_SystemAdmin",
            );
        }

        // Generate record ID
        let counter_key = symbol_short!("REC_CTR");
        let record_id: u64 = env.storage().instance().get(&counter_key).unwrap_or(0) + 1;
        env.storage().instance().set(&counter_key, &record_id);

        // Determine current encryption key version (if any) and load master bytes
        let current_version: Option<String> = env.storage().instance().get(&ENC_CUR);
        let mut master_bytes: StdVec<u8> = StdVec::new();
        if let Some(ver) = current_version.clone() {
            if let Some(sv) = env
                .storage()
                .persistent()
                .get::<(Symbol, String), String>(&(ENC_KEY, ver.clone()))
            {
                let hex = sv.to_string();
                if let Some(bytes) = teye_common::hex_to_bytes(&hex) {
                    master_bytes = bytes;
                }
            }
        }
        // Build KeyManager and encrypt the provided data_hash
        let km = KeyManager::new(master_bytes);
        let plaintext: StdString = data_hash.to_string();
        let ciphertext = km.encrypt(None, &plaintext);
        let stored_hash = String::from_str(&env, &ciphertext);
        let key_version = current_version;

        let record = VisionRecord {
            id: record_id,
            patient: patient.clone(),
            provider: provider.clone(),
            record_type: record_type.clone(),
            data_hash: stored_hash,
            key_version,
            created_at: env.ledger().timestamp(),
            updated_at: env.ledger().timestamp(),
        };

        let key = (symbol_short!("RECORD"), record_id);
        env.storage().persistent().set(&key, &record);
        extend_ttl_u64_key(&env, &key);
        teye_common::concurrency::init_record_version(&env, record_id, 0);

        // Meter: write operation for the provider.
        Self::meter_op(&env, &provider, MeteringOpType::Write);

        // Add to patient's record list
        let patient_key = (symbol_short!("PAT_REC"), patient.clone());
        let mut patient_records: Vec<u64> = env
            .storage()
            .persistent()
            .get(&patient_key)
            .unwrap_or(Vec::new(&env));
        patient_records.push_back(record_id);
        env.storage()
            .persistent()
            .set(&patient_key, &patient_records);

        // Initialize OCC version tracking
        teye_common::concurrency::init_record_version(&env, record_id, 1);

        let _ = lineage::create_node(&env, record_id, provider.clone(), "VisionRecord", None);
        let _ = lineage::add_edge(
            &env,
            record_id,
            record_id,
            RelationshipKind::Created,
            provider.clone(),
            None,
        );

        Ok(record_id)
    }

    /// Add multiple vision records in a single transaction.
    /// Validates provider permission once, then creates all records atomically.
    #[allow(clippy::arithmetic_side_effects)]
    pub fn add_records(
        env: Env,
        provider: Address,
        records: Vec<BatchRecordInput>,
    ) -> Result<Vec<u64>, ContractError> {
        circuit_breaker::require_not_paused(&env, &circuit_breaker::PauseScope::Global)?;
        provider.require_auth();

        if records.is_empty() {
            return Err(ContractError::InvalidInput);
        }

        if !whitelist::check_whitelist_access(&env, &provider) {
            return Self::unauthorized(&env, &provider, "add_records", "whitelisted_provider");
        }

        // Check provider has WriteRecord permission once for the whole batch
        if !Self::has_permission_unified(&env, &provider, &Permission::WriteRecord)
            && !Self::has_permission_unified(&env, &provider, &Permission::SystemAdmin)
        {
            return Self::unauthorized(
                &env,
                &provider,
                "add_records",
                "permission:WriteRecord_or_SystemAdmin",
            );
        }

        let counter_key = symbol_short!("REC_CTR");
        let mut current_id: u64 = env.storage().instance().get(&counter_key).unwrap_or(0);
        let mut record_ids = Vec::new(&env);

        // Load current encryption key/version once for the batch
        let current_version: Option<String> = env.storage().instance().get(&ENC_CUR);
        let key_manager_cfg = Self::get_key_manager_config(&env);
        let key_manager_client = key_manager_cfg
            .as_ref()
            .map(|(mgr, _)| KeyManagerContractClient::new(&env, mgr));
        let mut master_bytes_batch: StdVec<u8> = StdVec::new();
        if let Some(ver) = current_version.clone() {
            if let Some(sv) = env
                .storage()
                .persistent()
                .get::<(Symbol, String), String>(&(ENC_KEY, ver.clone()))
            {
                let hex = sv.to_string();
                if let Some(bytes) = teye_common::hex_to_bytes(&hex) {
                    master_bytes_batch = bytes;
                }
            }
        }

        for input in records.iter() {
            current_id += 1;

            let mut master_bytes = master_bytes_batch.clone();
            let mut key_version = current_version.clone();
            if let Some((_, key_id)) = key_manager_cfg.as_ref() {
                if let Some(client) = key_manager_client.as_ref() {
                    let derived = client.derive_record_key(key_id, &current_id);
                    master_bytes = derived.key.to_array().to_vec();
                    key_version = Some(String::from_str(&env, &derived.version.to_string()));
                }
            }

            // Encrypt input.data_hash with master bytes
            let km = KeyManager::new(master_bytes);
            let plaintext: StdString = input.data_hash.to_string();
            let ciphertext = km.encrypt(None, &plaintext);
            let stored_hash = String::from_str(&env, &ciphertext);

            let record = VisionRecord {
                id: current_id,
                patient: input.patient.clone(),
                provider: provider.clone(),
                record_type: input.record_type.clone(),
                data_hash: stored_hash,
                key_version,
                created_at: env.ledger().timestamp(),
                updated_at: env.ledger().timestamp(),
            };

            let key = (symbol_short!("RECORD"), current_id);
            env.storage().persistent().set(&key, &record);
            teye_common::concurrency::init_record_version(&env, current_id, 0);

            let patient_key = (symbol_short!("PAT_REC"), input.patient.clone());
            let mut patient_records: Vec<u64> = env
                .storage()
                .persistent()
                .get(&patient_key)
                .unwrap_or(Vec::new(&env));
            patient_records.push_back(current_id);
            env.storage()
                .persistent()
                .set(&patient_key, &patient_records);

            events::publish_record_added(
                &env,
                current_id,
                input.patient.clone(),
                provider.clone(),
                input.record_type.clone(),
            );

            record_ids.push_back(current_id);
            // Initialize OCC version tracking
            teye_common::concurrency::init_record_version(&env, current_id, 1);

            let _ = lineage::create_node(&env, current_id, provider.clone(), "VisionRecord", None);
            let _ = lineage::add_edge(
                &env,
                current_id,
                current_id,
                RelationshipKind::Created,
                provider.clone(),
                None,
            );
        }

        env.storage().instance().set(&counter_key, &current_id);

        events::publish_batch_records_added(&env, provider, record_ids.len());

        Ok(record_ids)
    }

    /// Get a vision record by ID.
    pub fn get_record(
        env: Env,
        caller: Address,
        record_id: u64,
    ) -> Result<VisionRecord, ContractError> {
        caller.require_auth();
        let key = (symbol_short!("RECORD"), record_id);
        match env.storage().persistent().get::<_, VisionRecord>(&key) {
            Some(record) => {
                // Check access permissions
                let has_access = if caller == record.patient || caller == record.provider {
                    // Patient can always read their own records
                    // Provider can read records they created
                    true
                } else {
                    // Check if caller has broad read permissions, active consent, or explicit grant
                    Self::has_permission_unified(&env, &caller, &Permission::ReadAnyRecord)
                        || Self::has_permission_unified(&env, &caller, &Permission::SystemAdmin)
                        || has_active_consent(&env, &record.patient, &caller)
                        || {
                            let access_level = Self::check_access(
                                env.clone(),
                                record.patient.clone(),
                                caller.clone(),
                            );
                            access_level != AccessLevel::None
                        }
                        || Self::check_record_access(env.clone(), record_id, caller.clone())
                            != AccessLevel::None
                };

                if !has_access {
                    // Log failed access attempt
                    let audit_entry = audit::create_audit_entry(
                        &env,
                        caller.clone(),
                        record.patient.clone(),
                        Some(record_id),
                        AccessAction::Read,
                        AccessResult::Denied,
                        Some(String::from_str(&env, "Insufficient permissions")),
                    );
                    audit::add_audit_entry(&env, &audit_entry);
                    events::publish_audit_log_entry(&env, &audit_entry);

                    return Self::unauthorized(&env, &caller, "get_record", "record_read_access");
                }

                // Archived records are hidden from everyone except admins.
                if archive::is_archived(&env, record_id) && !archive::is_admin(&env, &caller) {
                    let audit_entry = audit::create_audit_entry(
                        &env,
                        caller.clone(),
                        record.patient.clone(),
                        Some(record_id),
                        AccessAction::Read,
                        AccessResult::Denied,
                        Some(String::from_str(&env, "Record is archived")),
                    );
                    audit::add_audit_entry(&env, &audit_entry);
                    events::publish_audit_log_entry(&env, &audit_entry);
                    return Err(ContractError::RecordArchived);
                }

                // Log successful access
                let audit_entry = audit::create_audit_entry(
                    &env,
                    caller.clone(),
                    record.patient.clone(),
                    Some(record_id),
                    AccessAction::Read,
                    AccessResult::Success,
                    None,
                );
                audit::add_audit_entry(&env, &audit_entry);
                events::publish_audit_log_entry(&env, &audit_entry);

                // Meter: read operation for the caller.
                Self::meter_op(&env, &caller, MeteringOpType::Read);

                // Decrypt data_hash for authorized caller before returning
                let mut out_record = record.clone();
                // Prefer record's key_version, fall back to current instance version
                let key_ver = out_record
                    .key_version
                    .clone()
                    .or_else(|| env.storage().instance().get(&ENC_CUR));
                let mut master_bytes: StdVec<u8> = StdVec::new();
                if let Some(ver) = key_ver {
                    if let Some(sv) = env
                        .storage()
                        .persistent()
                        .get::<(Symbol, String), String>(&(ENC_KEY, ver.clone()))
                    {
                        let hex = sv.to_string();
                        if let Some(bytes) = teye_common::hex_to_bytes(&hex) {
                            master_bytes = bytes;
                        }
                    }
                }

                if !master_bytes.is_empty() || out_record.key_version.is_none() {
                    let km = KeyManager::new(master_bytes);
                    let ciphertext = out_record.data_hash.to_string();
                    if let Some(plain) = km.decrypt(None, &ciphertext) {
                        out_record.data_hash = String::from_str(&env, &plain);
                    }
                }

                Ok(out_record)
            }
            None => {
                // Log failed access attempt (record not found)
                // We don't know the patient, so we'll use caller as placeholder
                let audit_entry = audit::create_audit_entry(
                    &env,
                    caller.clone(),
                    caller.clone(), // Placeholder since we don't know patient
                    Some(record_id),
                    AccessAction::Read,
                    AccessResult::NotFound,
                    Some(String::from_str(&env, "Record not found")),
                );
                audit::add_audit_entry(&env, &audit_entry);
                events::publish_audit_log_entry(&env, &audit_entry);

                let resource_id = String::from_str(&env, "get_record");
                let context = create_error_context(
                    &env,
                    ContractError::RecordNotFound,
                    None,
                    Some(resource_id.clone()),
                );
                log_error(
                    &env,
                    ContractError::RecordNotFound,
                    None,
                    Some(resource_id),
                    None,
                );
                events::publish_error(&env, ContractError::RecordNotFound as u32, context);
                Err(ContractError::RecordNotFound)
            }
        }
    }

    pub(crate) fn get_record_raw(env: &Env, record_id: u64) -> Result<VisionRecord, ContractError> {
        let key = (symbol_short!("RECORD"), record_id);
        env.storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::RecordNotFound)
    }

    /// Add eye examination details for an existing record
    #[allow(clippy::too_many_arguments)]
    pub fn add_eye_examination(
        env: Env,
        caller: Address,
        record_id: u64,
        visual_acuity: VisualAcuity,
        iop: IntraocularPressure,
        slit_lamp: SlitLampFindings,
        visual_field: OptVisualField,
        retina_imaging: OptRetinalImaging,
        fundus_photo: OptFundusPhotography,
        clinical_notes: String,
    ) -> Result<(), ContractError> {
        examination::add_eye_examination(
            &env,
            &caller,
            record_id,
            visual_acuity,
            iop,
            slit_lamp,
            visual_field,
            retina_imaging,
            fundus_photo,
            clinical_notes,
        )
    }

    /// Update eye examination details using optimistic concurrency control (OCC).
    #[allow(clippy::too_many_arguments)]
    pub fn update_examination_versioned(
        env: Env,
        caller: Address,
        record_id: u64,
        expected_version: u64,
        node_id: u32,
        visual_acuity: VisualAcuity,
        iop: IntraocularPressure,
        slit_lamp: SlitLampFindings,
        visual_field: OptVisualField,
        retina_imaging: OptRetinalImaging,
        fundus_photo: OptFundusPhotography,
        clinical_notes: String,
        changed_fields: Vec<FieldChange>,
    ) -> Result<UpdateOutcome, ContractError> {
        examination::update_examination_versioned(
            &env,
            &caller,
            record_id,
            expected_version,
            node_id,
            visual_acuity,
            iop,
            slit_lamp,
            visual_field,
            retina_imaging,
            fundus_photo,
            clinical_notes,
            changed_fields,
        )
    }

    /// Retrieve eye examination details for a record
    pub fn get_eye_examination(
        env: Env,
        caller: Address,
        record_id: u64,
    ) -> Result<EyeExamination, ContractError> {
        examination::get_eye_examination(&env, &caller, record_id)
    }

    /// Return the current OCC version stamp for a record.
    pub fn get_record_version_stamp(env: Env, record_id: u64) -> VersionStamp {
        examination::get_exam_version(&env, record_id)
    }

    /// Configure conflict resolution strategy for a record.
    pub fn set_record_resolution_strategy(
        env: Env,
        caller: Address,
        record_id: u64,
        strategy: ResolutionStrategy,
    ) -> Result<(), ContractError> {
        caller.require_auth();

        let record = Self::get_record_raw(&env, record_id)?;
        let has_perm = if caller == record.provider {
            Self::has_permission_unified(&env, &caller, &Permission::WriteRecord)
        } else {
            Self::has_delegated_permission_unified(
                &env,
                &record.provider,
                &caller,
                &Permission::WriteRecord,
            )
        };

        if !has_perm && !Self::has_permission_unified(&env, &caller, &Permission::SystemAdmin) {
            return Self::unauthorized(
                &env,
                &caller,
                "set_record_resolution_strategy",
                "permission:WriteRecord_or_SystemAdmin",
            );
        }

        teye_common::concurrency::set_resolution_strategy(&env, record_id, &strategy);
        Ok(())
    }

    /// Retrieve conflicts for a specific record.
    pub fn get_record_conflicts(env: Env, record_id: u64) -> Vec<ConflictEntry> {
        teye_common::concurrency::get_record_conflicts(&env, record_id)
    }

    /// Retrieve all pending conflicts.
    pub fn get_pending_conflicts(env: Env) -> Vec<ConflictEntry> {
        teye_common::concurrency::get_pending_conflicts(&env)
    }

    /// Resolve a conflict by marking it handled.
    pub fn resolve_conflict(
        env: Env,
        caller: Address,
        conflict_id: u64,
        record_id: u64,
    ) -> Result<(), ContractError> {
        caller.require_auth();

        let admin = Self::get_admin(env.clone())?;
        let has_admin = caller == admin
            || Self::has_permission_unified(&env, &caller, &Permission::SystemAdmin);

        if !has_admin {
            let key = (symbol_short!("RECORD"), record_id);
            let record = env
                .storage()
                .persistent()
                .get::<_, VisionRecord>(&key)
                .ok_or(ContractError::RecordNotFound)?;

            let has_perm = if caller == record.provider {
                Self::has_permission_unified(&env, &caller, &Permission::WriteRecord)
            } else {
                Self::has_delegated_permission_unified(
                    &env,
                    &record.provider,
                    &caller,
                    &Permission::WriteRecord,
                )
            };

            if !has_perm {
                return Self::unauthorized(
                    &env,
                    &caller,
                    "resolve_conflict",
                    "permission:WriteRecord_or_SystemAdmin",
                );
            }
        }

        if !teye_common::concurrency::resolve_conflict(&env, conflict_id, &caller) {
            return Err(ContractError::RecordNotFound);
        }

        Ok(())
    }

    /// Get all records for a patient
    /// Archived records are left out; admins can list them with get_archived_records.
    pub fn get_patient_records(env: Env, patient: Address) -> Vec<u64> {
        let key = (symbol_short!("PAT_REC"), patient);
        let all: Vec<u64> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));
        let mut active = Vec::new(&env);
        for id in all.iter() {
            if !archive::is_archived(&env, id) {
                active.push_back(id);
            }
        }
        active
    }

    /// Return total number of records added.
    pub fn get_record_count(env: Env) -> u64 {
        let counter_key = symbol_short!("REC_CTR");
        env.storage().instance().get(&counter_key).unwrap_or(0)
    }

    /// Get multiple records by their IDs. Archived records are skipped.
    pub fn get_records(env: Env, ids: Vec<u64>) -> Result<Vec<VisionRecord>, ContractError> {
        let mut records: Vec<VisionRecord> = Vec::new(&env);
        for i in 0..ids.len() {
            let record_id = ids.get(i).unwrap();
            if archive::is_archived(&env, record_id) {
                continue;
            }
            let key = (symbol_short!("RECORD"), record_id);
            let record: VisionRecord = env
                .storage()
                .persistent()
                .get(&key)
                .ok_or(ContractError::RecordNotFound)?;
            records.push_back(record);
        }
        Ok(records)
    }
}
