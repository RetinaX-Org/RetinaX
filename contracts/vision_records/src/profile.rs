//! Patient profile, emergency contact and insurance endpoints.

use super::*;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl VisionRecordsContract {
    /// Create a new patient profile
    pub fn create_profile(
        env: Env,
        caller: Address,
        patient: Address,
        date_of_birth_hash: String,
        gender_hash: String,
        blood_type_hash: String,
    ) -> Result<PatientProfile, ContractError> {
        patient_profile::create_profile(
            &env,
            &caller,
            &patient,
            date_of_birth_hash,
            gender_hash,
            blood_type_hash,
        )
    }

    /// Update patient demographics
    pub fn update_demographics(
        env: Env,
        caller: Address,
        patient: Address,
        date_of_birth_hash: String,
        gender_hash: String,
        blood_type_hash: String,
    ) -> Result<PatientProfile, ContractError> {
        patient_profile::update_demographics(
            &env,
            &caller,
            &patient,
            date_of_birth_hash,
            gender_hash,
            blood_type_hash,
        )
    }

    /// Update emergency contact information
    pub fn update_emergency_contact(
        env: Env,
        caller: Address,
        patient: Address,
        contact: Option<EmergencyContact>,
    ) -> Result<(), ContractError> {
        patient_profile::update_emergency_contact(&env, &caller, &patient, contact)
    }

    /// Update insurance information (hashed values only)
    pub fn update_insurance(
        env: Env,
        caller: Address,
        patient: Address,
        insurance_info: Option<InsuranceInfo>,
    ) -> Result<(), ContractError> {
        insurance::update_insurance(&env, &caller, &patient, insurance_info)
    }

    /// Retrieve insurance information for a patient
    pub fn get_insurance(
        env: Env,
        caller: Address,
        patient: Address,
    ) -> Result<OptionalInsuranceInfo, ContractError> {
        insurance::get_insurance(&env, &caller, &patient)
    }

    /// Add medical history reference (IPFS hash or record ID)
    pub fn add_medical_history_reference(
        env: Env,
        caller: Address,
        patient: Address,
        reference: String,
    ) -> Result<(), ContractError> {
        patient_profile::add_medical_history_reference(&env, &caller, &patient, reference)
    }

    /// Get patient profile
    pub fn get_profile(env: Env, patient: Address) -> Result<PatientProfile, ContractError> {
        patient_profile::get_profile(&env, &patient)
    }

    /// Check if patient profile exists
    pub fn profile_exists(env: Env, patient: Address) -> bool {
        patient_profile::profile_exists(&env, &patient)
    }
}
