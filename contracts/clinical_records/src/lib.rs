#![no_std]

pub mod admin;
pub mod errors;
pub mod examination;
pub mod migration;
pub mod prescription;
pub mod types;

#[cfg(test)]
mod test;

pub use errors::ContractError;
pub use types::*;

use examination::{EXAM_KEY, EXAM_VERSION_KEY};
use prescription::{PRESCRIPTION_KEY, RX_VERSION_KEY};
use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};

#[contract]
pub struct ClinicalRecordsContract;

#[contractimpl]
impl ClinicalRecordsContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        vision_records: Address,
    ) -> Result<(), ContractError> {
        admin::initialize(&env, admin, vision_records)
    }

    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        admin::get_admin(&env)
    }

    pub fn set_vision_records_contract(
        env: Env,
        admin: Address,
        vision_records: Address,
    ) -> Result<(), ContractError> {
        admin::set_vision_records_contract(&env, admin, vision_records)
    }

    pub fn get_vision_records_contract(env: Env) -> Option<Address> {
        admin::get_vision_records_contract(&env)
    }

    pub fn upgrade(
        env: Env,
        admin: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        migration::upgrade(&env, admin, new_wasm_hash)
    }

    pub fn get_schema_version(env: Env) -> u32 {
        migration::get_schema_version(&env)
    }

    pub fn migrate(env: Env, admin: Address, target_version: u32) -> Result<(), ContractError> {
        migration::migrate(&env, admin, target_version)
    }

    pub fn get_examination_version(
        env: Env,
        caller: Address,
        record_id: u64,
    ) -> Result<u32, ContractError> {
        migration::get_record_version(
            &env,
            &caller,
            &(EXAM_KEY, record_id),
            &(EXAM_VERSION_KEY, record_id),
        )
    }

    pub fn migrate_examination(
        env: Env,
        caller: Address,
        record_id: u64,
        target_version: u32,
    ) -> Result<u32, ContractError> {
        migration::migrate_record(
            &env,
            &caller,
            &(EXAM_KEY, record_id),
            &(EXAM_VERSION_KEY, record_id),
            target_version,
        )
    }

    pub fn get_prescription_version(
        env: Env,
        caller: Address,
        rx_id: u64,
    ) -> Result<u32, ContractError> {
        migration::get_record_version(
            &env,
            &caller,
            &(PRESCRIPTION_KEY, rx_id),
            &(RX_VERSION_KEY, rx_id),
        )
    }

    pub fn migrate_prescription(
        env: Env,
        caller: Address,
        rx_id: u64,
        target_version: u32,
    ) -> Result<u32, ContractError> {
        migration::migrate_record(
            &env,
            &caller,
            &(PRESCRIPTION_KEY, rx_id),
            &(RX_VERSION_KEY, rx_id),
            target_version,
        )
    }

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

    pub fn get_eye_examination(
        env: Env,
        caller: Address,
        record_id: u64,
    ) -> Result<EyeExamination, ContractError> {
        examination::get_eye_examination(&env, caller, record_id)
    }

    pub fn prepare_add_prescription(
        env: Env,
        caller: Address,
        patient: Address,
        provider: Address,
        prescription_data: PrescriptionData,
    ) -> Result<u64, ContractError> {
        prescription::prepare_add_prescription(&env, caller, patient, provider, prescription_data)
    }

    pub fn commit_add_prescription(
        env: Env,
        caller: Address,
        rx_id: u64,
    ) -> Result<(), ContractError> {
        prescription::commit_add_prescription(&env, caller, rx_id)
    }

    pub fn rollback_add_prescription(
        env: Env,
        caller: Address,
        rx_id: u64,
    ) -> Result<(), ContractError> {
        prescription::rollback_add_prescription(&env, caller, rx_id)
    }

    pub fn get_prescription_count(env: Env) -> u64 {
        prescription::get_prescription_count(&env)
    }

    pub fn get_prescription(
        env: Env,
        caller: Address,
        id: u64,
    ) -> Result<Prescription, ContractError> {
        prescription::get_prescription(&env, caller, id)
    }
}
