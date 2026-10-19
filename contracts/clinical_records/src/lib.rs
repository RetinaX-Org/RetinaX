#![no_std]

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

/// The clinical-records contract: stores eye examinations and two-phase
/// spectacle/contact prescriptions on-chain, gated to one authorized
/// vision-records caller.
#[contract]
pub struct ClinicalRecordsContract;

#[contractimpl]
impl ClinicalRecordsContract {
    /// Initialize the contract with its admin and the address of the vision-records
    /// service allowed to read and write exam and prescription data.
    ///
    /// Only callable once; subsequent calls return
    /// [`AlreadyInitialized`](ContractError::AlreadyInitialized).
    pub fn initialize(
        env: Env,
        admin: Address,
        vision_records: Address,
    ) -> Result<(), ContractError> {
        admin::initialize(&env, admin, vision_records)
    }

    /// Return the admin address recorded at initialization.
    ///
    /// # Errors
    ///
    /// Returns [`NotInitialized`](ContractError::NotInitialized) if the contract
    /// has never been initialized.
    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        admin::get_admin(&env)
    }

    /// Point the contract at a new vision-records service address.
    ///
    /// Requires `admin` to be the stored admin address.
    pub fn set_vision_records_contract(
        env: Env,
        admin: Address,
        vision_records: Address,
    ) -> Result<(), ContractError> {
        admin::set_vision_records_contract(&env, admin, vision_records)
    }

    /// Return the currently authorized vision-records address, or `None` if unset.
    pub fn get_vision_records_contract(env: Env) -> Option<Address> {
        admin::get_vision_records_contract(&env)
    }

    /// Record an eye examination for `record_id`.
    ///
    /// Only the authorized vision-records contract may call this. The measurement
    /// strings are length-validated before storage; `visual_acuity` is the
    /// sharpness-of-vision measurement, `iop` is the intraocular pressure in mmHg,
    /// `slit_lamp` holds anterior-segment findings, and the remaining parameters are
    /// optional per-visit imaging (visual field, retina imaging, fundus photo).
    ///
    /// # Errors
    ///
    /// [`Unauthorized`](ContractError::Unauthorized) for any caller other than
    /// the vision-records contract, or
    /// [`InvalidInput`](ContractError::InvalidInput) when any measurement string
    /// fails its length bounds.
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

    /// Fetch the stored eye examination for `record_id`.
    ///
    /// # Errors
    ///
    /// Returns [`Unauthorized`](ContractError::Unauthorized) for any caller other
    /// than the vision-records contract, or
    /// [`RecordNotFound`](ContractError::RecordNotFound) if no exam is stored.
    pub fn get_eye_examination(
        env: Env,
        caller: Address,
        record_id: u64,
    ) -> Result<EyeExamination, ContractError> {
        examination::get_eye_examination(&env, caller, record_id)
    }

    /// Phase 1 of the two-phase prescription write: validate the prescription
    /// strings, allocate the next prescription id, and stage the data in temporary
    /// storage. Returns the allocated id for use in the commit or rollback phase.
    pub fn prepare_add_prescription(
        env: Env,
        caller: Address,
        patient: Address,
        provider: Address,
        prescription_data: PrescriptionData,
    ) -> Result<u64, ContractError> {
        prescription::prepare_add_prescription(&env, caller, patient, provider, prescription_data)
    }

    /// Phase 2: turn the staged prescription `rx_id` into a persistent record and
    /// discard the staged data.
    ///
    /// The committed prescription uses the same refraction for both eyes, expires
    /// after 31,536,000 seconds (1 year), and is unverified until reviewed.
    ///
    /// # Errors
    ///
    /// Returns [`Unauthorized`](ContractError::Unauthorized) for callers other than
    /// the vision-records contract, or
    /// [`InvalidPhase`](ContractError::InvalidPhase) if nothing is staged for
    /// `rx_id`.
    pub fn commit_add_prescription(
        env: Env,
        caller: Address,
        rx_id: u64,
    ) -> Result<(), ContractError> {
        prescription::commit_add_prescription(&env, caller, rx_id)
    }

    /// Abort a staged prescription write by discarding `rx_id` from temporary
    /// storage. No-op beyond access control if nothing was staged.
    pub fn rollback_add_prescription(
        env: Env,
        caller: Address,
        rx_id: u64,
    ) -> Result<(), ContractError> {
        prescription::rollback_add_prescription(&env, caller, rx_id)
    }

    /// Return how many prescriptions have been allocated so far (the id counter).
    pub fn get_prescription_count(env: Env) -> u64 {
        prescription::get_prescription_count(&env)
    }

    /// Fetch the stored prescription `id`.
    ///
    /// # Errors
    ///
    /// Returns [`Unauthorized`](ContractError::Unauthorized) for any caller other
    /// than the vision-records contract, or
    /// [`RecordNotFound`](ContractError::RecordNotFound) if no prescription is
    /// stored under `id`.
    pub fn get_prescription(
        env: Env,
        caller: Address,
        id: u64,
    ) -> Result<Prescription, ContractError> {
        prescription::get_prescription(&env, caller, id)
    }
}
