#![no_std]

//! Clinical Records Contract
//!
//! This contract manages clinical eye examination records and prescriptions
//! for patients within the RetinaX vision care protocol. It integrates with
//! the Vision Records contract for authorization and data consistency.
//!
//! # Clinical Concepts
//!
//! - **Visual Acuity**: Sharpness of vision, measured uncorrected and with correction (Snellen/logMAR)
//! - **IOP (Intraocular Pressure)**: Pressure inside the eye, measured in mmHg (Goldmann applanation, iCare, etc.)
//! - **Slit Lamp**: Biomicroscopic examination of anterior segment (cornea, anterior chamber, iris, lens)
//! - **Visual Field**: Peripheral vision mapping (Humphrey, Goldmann) with reliability indices
//! - **Retinal Imaging**: OCT, fundus photography, fluorescein angiography with findings
//! - **Fundus Photography**: Color photos of retina; cup-to-disc ratio for glaucoma assessment
//! - **Prescription**: Optical correction (sphere, cylinder, axis, add, PD) for glasses or contact lenses

pub mod admin;
pub mod errors;
pub mod examination;
pub mod prescription;
pub mod types;

#[cfg(test)]
mod test;

pub use errors::ContractError;
pub use types::*;

use soroban_sdk::{contract, contractimpl, Address, Env, String};

/// Main contract struct for clinical records management.
///
/// Provides methods for:
/// - Contract initialization and admin management
/// - Eye examination CRUD operations
/// - Prescription workflow (prepare → commit/rollback)
/// - Vision Records contract coordination
#[contract]
pub struct ClinicalRecordsContract;

#[contractimpl]
impl ClinicalRecordsContract {
    /// Initialize the contract with an admin and Vision Records contract address.
    ///
    /// Must be called exactly once before any other operations. The admin
    /// authorizes all administrative actions. The Vision Records contract
    /// is the only caller allowed to add/retrieve examinations and prescriptions.
    ///
    /// # Parameters
    /// - `env`: Soroban environment
    /// - `admin`: Address authorized for administrative operations
    /// - `vision_records`: Address of the Vision Records contract (authorized caller)
    ///
    /// # Errors
    /// - [`ContractError::AlreadyInitialized`] if contract was already initialized
    pub fn initialize(env: Env, admin: Address, vision_records: Address) -> Result<(), ContractError> {
        admin::initialize(&env, admin, vision_records)
    }

    /// Get the contract admin address.
    ///
    /// # Parameters
    /// - `env`: Soroban environment
    ///
    /// # Returns
    /// Admin [`Address`] if initialized.
    ///
    /// # Errors
    /// - [`ContractError::NotInitialized`] if contract not yet initialized
    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        admin::get_admin(&env)
    }

    /// Update the Vision Records contract address (admin only).
    ///
    /// # Parameters
    /// - `env`: Soroban environment
    /// - `admin`: Must match stored admin address
    /// - `vision_records`: New Vision Records contract address
    ///
    /// # Errors
    /// - [`ContractError::Unauthorized`] if caller is not admin
    /// - [`ContractError::NotInitialized`] if contract not initialized
    pub fn set_vision_records_contract(
        env: Env,
        admin: Address,
        vision_records: Address,
    ) -> Result<(), ContractError> {
        admin::set_vision_records_contract(&env, admin, vision_records)
    }

    /// Get the current Vision Records contract address.
    ///
    /// # Parameters
    /// - `env`: Soroban environment
    ///
    /// # Returns
    /// [`Option<Address>`] of Vision Records contract, or [`None`] if not set
    pub fn get_vision_records_contract(env: Env) -> Option<Address> {
        admin::get_vision_records_contract(&env)
    }

    /// Add a complete eye examination record.
    ///
    /// Creates a new examination record with all clinical components.
    /// Only the authorized Vision Records contract may call this.
    ///
    /// # Parameters
    /// - `env`: Soroban environment
    /// - `caller`: Must be the Vision Records contract address
    /// - `record_id`: Unique identifier for this examination
    /// - `visual_acuity`: Uncorrected and corrected vision measurements
    /// - `iop`: Intraocular pressure (mmHg) for both eyes + measurement method
    /// - `slit_lamp`: Anterior segment findings (cornea, chamber, iris, lens)
    /// - `visual_field`: Optional peripheral vision test results
    /// - `retina_imaging`: Optional retinal imaging (OCT, angiography) with findings
    /// - `fundus_photo`: Optional fundus photography with cup-to-disc ratios
    /// - `clinical_notes`: Free-text clinical observations (max 2048 chars)
    ///
    /// # Errors
    /// - [`ContractError::Unauthorized`] if caller not Vision Records contract
    /// - [`ContractError::InvalidInput`] if any field exceeds length limits or is empty
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
            caller,
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

    /// Retrieve an eye examination record by ID.
    ///
    /// Only the authorized Vision Records contract may call this.
    ///
    /// # Parameters
    /// - `env`: Soroban environment
    /// - `caller`: Must be the Vision Records contract address
    /// - `record_id`: Examination ID to retrieve
    ///
    /// # Returns
    /// Complete [`EyeExamination`] record
    ///
    /// # Errors
    /// - [`ContractError::Unauthorized`] if caller not Vision Records contract
    /// - [`ContractError::RecordNotFound`] if no examination exists with that ID
    pub fn get_eye_examination(
        env: Env,
        caller: Address,
        record_id: u64,
    ) -> Result<EyeExamination, ContractError> {
        examination::get_eye_examination(&env, caller, record_id)
    }

    /// Prepare a prescription for a patient (stages data for commit/rollback).
    ///
    /// Stage 1 of 2-phase commit: validates and stores prescription data
    /// temporarily. Must be followed by [`ClinicalRecordsContract::commit_add_prescription`]
    /// or [`ClinicalRecordsContract::rollback_add_prescription`].
    ///
    /// # Parameters
    /// - `env`: Soroban environment
    /// - `caller`: Must be the Vision Records contract address
    /// - `patient`: Patient address receiving the prescription
    /// - `provider`: Prescribing provider address
    /// - `prescription_data`: Optical parameters (sphere, cylinder, axis, add, PD)
    ///
    /// # Returns
    /// Generated prescription ID (`rx_id`) for commit/rollback
    ///
    /// # Errors
    /// - [`ContractError::Unauthorized`] if caller not Vision Records contract
    /// - [`ContractError::InvalidInput`] if prescription data validation fails
    pub fn prepare_add_prescription(
        env: Env,
        caller: Address,
        patient: Address,
        provider: Address,
        prescription_data: PrescriptionData,
    ) -> Result<u64, ContractError> {
        prescription::prepare_add_prescription(&env, caller, patient, provider, prescription_data)
    }

    /// Commit a prepared prescription (finalize and make permanent).
    ///
    /// Stage 2 of 2-phase commit: moves prescription from temporary to
    /// persistent storage. Prescription becomes queryable via [`ClinicalRecordsContract::get_prescription`].
    ///
    /// # Parameters
    /// - `env`: Soroban environment
    /// - `caller`: Must be the Vision Records contract address
    /// - `rx_id`: Prescription ID returned by [`ClinicalRecordsContract::prepare_add_prescription`]
    ///
    /// # Errors
    /// - [`ContractError::Unauthorized`] if caller not Vision Records contract
    /// - [`ContractError::InvalidPhase`] if prescription was not prepared or already committed
    pub fn commit_add_prescription(
        env: Env,
        caller: Address,
        rx_id: u64,
    ) -> Result<(), ContractError> {
        prescription::commit_add_prescription(&env, caller, rx_id)
    }

    /// Rollback a prepared prescription (discard staged data).
    ///
    /// Cancels a previously prepared prescription, removing temporary data.
    ///
    /// # Parameters
    /// - `env`: Soroban environment
    /// - `caller`: Must be the Vision Records contract address
    /// - `rx_id`: Prescription ID returned by [`ClinicalRecordsContract::prepare_add_prescription`]
    ///
    /// # Errors
    /// - [`ContractError::Unauthorized`] if caller not Vision Records contract
    /// - [`ContractError::InvalidPhase`] if prescription was not prepared
    pub fn rollback_add_prescription(
        env: Env,
        caller: Address,
        rx_id: u64,
    ) -> Result<(), ContractError> {
        prescription::rollback_add_prescription(&env, caller, rx_id)
    }

    /// Get total number of committed prescriptions.
    ///
    /// # Parameters
    /// - `env`: Soroban environment
    ///
    /// # Returns
    /// Count of prescriptions in persistent storage
    pub fn get_prescription_count(env: Env) -> u64 {
        prescription::get_prescription_count(&env)
    }

    /// Retrieve a committed prescription by ID.
    ///
    /// Only the authorized Vision Records contract may call this.
    ///
    /// # Parameters
    /// - `env`: Soroban environment
    /// - `caller`: Must be the Vision Records contract address
    /// - `id`: Prescription ID to retrieve
    ///
    /// # Returns
    /// Complete [`Prescription`] record
    ///
    /// # Errors
    /// - [`ContractError::Unauthorized`] if caller not Vision Records contract
    /// - [`ContractError::RecordNotFound`] if no prescription exists with that ID
    pub fn get_prescription(env: Env, caller: Address, id: u64) -> Result<Prescription, ContractError> {
        prescription::get_prescription(&env, caller, id)
    }
}