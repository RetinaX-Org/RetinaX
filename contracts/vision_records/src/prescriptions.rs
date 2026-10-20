//! Two-phase commit for prescriptions.

use super::*;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl VisionRecordsContract {
    /// Prepare phase for adding a prescription
    pub fn prepare_add_prescription(
        env: Env,
        patient: Address,
        provider: Address,
        prescription_data: prescription::PrescriptionData,
    ) -> Result<u64, ContractError> {
        if let Some(clinical_records) = Self::get_clinical_records_contract(env.clone()) {
            let client =
                clinical_records::ClinicalRecordsContractClient::new(&env, &clinical_records);
            let contract_caller = env.current_contract_address();
            let clinical_data = clinical_records::PrescriptionData {
                sphere: prescription_data.sphere.clone(),
                cylinder: prescription_data.cylinder.clone(),
                axis: prescription_data.axis.clone(),
                add: prescription_data.add.clone(),
                pd: prescription_data.pd.clone(),
            };
            return Ok(client.prepare_add_prescription(
                &contract_caller,
                &patient,
                &provider,
                &clinical_data,
            ));
        }

        // Validate without state changes
        validation::validate_prescription_data(&prescription_data)?;

        // Check provider permissions
        let caller = provider.clone();
        let has_perm = Self::has_permission_unified(&env, &caller, &Permission::WriteRecord)
            || Self::has_delegated_permission_unified(
                &env,
                &provider,
                &caller,
                &Permission::WriteRecord,
            )
            || Self::has_permission_unified(&env, &caller, &Permission::SystemAdmin);

        if !has_perm {
            return Err(ContractError::Unauthorized);
        }

        // Generate prescription ID
        let counter_key = symbol_short!("RX_CTR");
        let rx_id: u64 = env
            .storage()
            .instance()
            .get(&counter_key)
            .unwrap_or(0u64)
            .saturating_add(1u64);

        // Store preparation data
        let prep_key = (symbol_short!("P_ADD_RX"), rx_id);
        let prep_data = PrepareAddPrescription {
            patient,
            provider,
            prescription_data,
            timestamp: env.ledger().timestamp(),
        };
        env.storage().temporary().set(&prep_key, &prep_data);

        Ok(rx_id)
    }

    /// Commit phase for adding a prescription
    pub fn commit_add_prescription(env: Env, rx_id: u64) -> Result<(), ContractError> {
        if let Some(clinical_records) = Self::get_clinical_records_contract(env.clone()) {
            let client =
                clinical_records::ClinicalRecordsContractClient::new(&env, &clinical_records);
            let contract_caller = env.current_contract_address();
            return Ok(client.commit_add_prescription(&contract_caller, &rx_id));
        }

        // Retrieve preparation data
        let prep_key = (symbol_short!("P_ADD_RX"), rx_id);
        let prep_data: PrepareAddPrescription = env
            .storage()
            .temporary()
            .get(&prep_key)
            .ok_or(ContractError::InvalidInput)?;

        // Update counter
        let counter_key = symbol_short!("RX_CTR");
        env.storage().instance().set(&counter_key, &rx_id);

        // Create and persist the prescription via the prescription module.
        let prescription = prescription::build_standalone_glasses(
            &env,
            rx_id,
            &prep_data.patient,
            &prep_data.provider,
            &prep_data.prescription_data,
            prep_data.timestamp,
        );
        prescription::store(&env, &prescription);

        // Add to patient's prescription history and lineage records.
        prescription::save_prescription(&env, &prescription, None);

        // Clean up preparation data
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }

    /// Rollback phase for adding a prescription
    pub fn rollback_add_prescription(env: Env, rx_id: u64) -> Result<(), ContractError> {
        // Clean up preparation data
        let prep_key = (symbol_short!("P_ADD_RX"), rx_id);
        env.storage().temporary().remove(&prep_key);

        Ok(())
    }

    // ── Query helpers ─────────────────────────────────────────────────────────

    /// Return total number of prescriptions committed.
    pub fn get_prescription_count(env: Env) -> u64 {
        let counter_key = symbol_short!("RX_CTR");
        env.storage().instance().get(&counter_key).unwrap_or(0)
    }
}
