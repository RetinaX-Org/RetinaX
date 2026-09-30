#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol,
    Vec,
};

const ADMIN: Symbol = symbol_short!("ADMIN");
const INITIALIZED: Symbol = symbol_short!("INIT");
const VISION_RECORDS: Symbol = symbol_short!("V_REC");
const EXAM_KEY: Symbol = symbol_short!("EXAM");
const PRESCRIPTION_KEY: Symbol = symbol_short!("RX");
const RX_COUNTER: Symbol = symbol_short!("RX_CTR");

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ContractError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidInput = 4,
    InvalidRecordType = 5,
    RecordNotFound = 6,
    InvalidPhase = 7,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalMeasurement {
    pub left_eye: String,
    pub right_eye: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptPhysicalMeasurement {
    None,
    Some(PhysicalMeasurement),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualAcuity {
    pub uncorrected: PhysicalMeasurement,
    pub corrected: OptPhysicalMeasurement,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntraocularPressure {
    pub left_eye: u32,
    pub right_eye: u32,
    pub method: String,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlitLampFindings {
    pub cornea: String,
    pub anterior_chamber: String,
    pub iris: String,
    pub lens: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualField {
    pub left_eye_reliability: String,
    pub right_eye_reliability: String,
    pub left_eye_defects: String,
    pub right_eye_defects: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptVisualField {
    None,
    Some(VisualField),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetinalImaging {
    pub image_url: String,
    pub image_hash: String,
    pub findings: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptRetinalImaging {
    None,
    Some(RetinalImaging),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundusPhotography {
    pub image_url: String,
    pub image_hash: String,
    pub cup_to_disc_ratio_left: String,
    pub cup_to_disc_ratio_right: String,
    pub macula_status: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptFundusPhotography {
    None,
    Some(FundusPhotography),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EyeExamination {
    pub record_id: u64,
    pub visual_acuity: VisualAcuity,
    pub iop: IntraocularPressure,
    pub slit_lamp: SlitLampFindings,
    pub visual_field: OptVisualField,
    pub retina_imaging: OptRetinalImaging,
    pub fundus_photo: OptFundusPhotography,
    pub clinical_notes: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LensType {
    Glasses,
    ContactLens,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrescriptionData {
    pub sphere: String,
    pub cylinder: String,
    pub axis: String,
    pub add: String,
    pub pd: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactLensData {
    pub base_curve: String,
    pub diameter: String,
    pub brand: String,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptionalContactLensData {
    None,
    Some(ContactLensData),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Prescription {
    pub id: u64,
    pub patient: Address,
    pub provider: Address,
    pub lens_type: LensType,
    pub left_eye: PrescriptionData,
    pub right_eye: PrescriptionData,
    pub contact_data: OptionalContactLensData,
    pub issued_at: u64,
    pub expires_at: u64,
    pub verified: bool,
    pub metadata_hash: String,
}

#[contract]
pub struct ClinicalRecordsContract;

#[contractimpl]
impl ClinicalRecordsContract {
    pub fn initialize(env: Env, admin: Address, vision_records: Address) -> Result<(), ContractError> {
        if env.storage().instance().has(&INITIALIZED) {
            return Err(ContractError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&VISION_RECORDS, &vision_records);
        env.storage().instance().set(&INITIALIZED, &true);
        Ok(())
    }

    pub fn get_admin(env: Env) -> Result<Address, ContractError> {
        env.storage()
            .instance()
            .get(&ADMIN)
            .ok_or(ContractError::NotInitialized)
    }

    pub fn set_vision_records_contract(
        env: Env,
        admin: Address,
        vision_records: Address,
    ) -> Result<(), ContractError> {
        admin.require_auth();
        let stored_admin = Self::get_admin(env.clone())?;
        if admin != stored_admin {
            return Err(ContractError::Unauthorized);
        }
        env.storage().instance().set(&VISION_RECORDS, &vision_records);
        Ok(())
    }

    pub fn get_vision_records_contract(env: Env) -> Option<Address> {
        env.storage().instance().get(&VISION_RECORDS)
    }

    fn require_authorized_vision_records(env: &Env, caller: &Address) -> Result<(), ContractError> {
        let allowed = env
            .storage()
            .instance()
            .get(&VISION_RECORDS)
            .ok_or(ContractError::NotInitialized)?;
        if *caller == allowed {
            Ok(())
        } else {
            Err(ContractError::Unauthorized)
        }
    }

    fn validate_string(value: &String, min_len: u32, max_len: u32) -> Result<(), ContractError> {
        let len = value.len();
        if len < min_len || len > max_len {
            return Err(ContractError::InvalidInput);
        }
        Ok(())
    }

    fn validate_physical_measurement(value: &PhysicalMeasurement) -> Result<(), ContractError> {
        Self::validate_string(&value.left_eye, 1, 64)?;
        Self::validate_string(&value.right_eye, 1, 64)?;
        Ok(())
    }

    fn validate_visual_acuity(value: &VisualAcuity) -> Result<(), ContractError> {
        Self::validate_physical_measurement(&value.uncorrected)?;
        if let OptPhysicalMeasurement::Some(corrected) = &value.corrected {
            Self::validate_physical_measurement(corrected)?;
        }
        Ok(())
    }

    fn validate_iop(value: &IntraocularPressure) -> Result<(), ContractError> {
        Self::validate_string(&value.method, 1, 64)?;
        Ok(())
    }

    fn validate_slit_lamp(value: &SlitLampFindings) -> Result<(), ContractError> {
        Self::validate_string(&value.cornea, 1, 256)?;
        Self::validate_string(&value.anterior_chamber, 1, 256)?;
        Self::validate_string(&value.iris, 1, 256)?;
        Self::validate_string(&value.lens, 1, 256)?;
        Ok(())
    }

    fn validate_visual_field(value: &OptVisualField) -> Result<(), ContractError> {
        if let OptVisualField::Some(field) = value {
            Self::validate_string(&field.left_eye_reliability, 0, 128)?;
            Self::validate_string(&field.right_eye_reliability, 0, 128)?;
            Self::validate_string(&field.left_eye_defects, 0, 256)?;
            Self::validate_string(&field.right_eye_defects, 0, 256)?;
        }
        Ok(())
    }

    fn validate_retinal_imaging(value: &OptRetinalImaging) -> Result<(), ContractError> {
        if let OptRetinalImaging::Some(image) = value {
            Self::validate_string(&image.image_url, 1, 256)?;
            Self::validate_string(&image.image_hash, 1, 128)?;
            Self::validate_string(&image.findings, 0, 512)?;
        }
        Ok(())
    }

    fn validate_fundus_photo(value: &OptFundusPhotography) -> Result<(), ContractError> {
        if let OptFundusPhotography::Some(photo) = value {
            Self::validate_string(&photo.image_url, 1, 256)?;
            Self::validate_string(&photo.image_hash, 1, 128)?;
            Self::validate_string(&photo.cup_to_disc_ratio_left, 1, 64)?;
            Self::validate_string(&photo.cup_to_disc_ratio_right, 1, 64)?;
            Self::validate_string(&photo.macula_status, 0, 256)?;
        }
        Ok(())
    }

    fn validate_examination_strings(
        visual_acuity: &VisualAcuity,
        iop: &IntraocularPressure,
        slit_lamp: &SlitLampFindings,
        visual_field: &OptVisualField,
        retina_imaging: &OptRetinalImaging,
        fundus_photo: &OptFundusPhotography,
        clinical_notes: &String,
    ) -> Result<(), ContractError> {
        Self::validate_visual_acuity(visual_acuity)?;
        Self::validate_iop(iop)?;
        Self::validate_slit_lamp(slit_lamp)?;
        Self::validate_visual_field(visual_field)?;
        Self::validate_retinal_imaging(retina_imaging)?;
        Self::validate_fundus_photo(fundus_photo)?;
        Self::validate_string(clinical_notes, 0, 2048)?;
        Ok(())
    }

    fn validate_prescription_data(value: &PrescriptionData) -> Result<(), ContractError> {
        Self::validate_string(&value.sphere, 1, 64)?;
        Self::validate_string(&value.cylinder, 1, 64)?;
        Self::validate_string(&value.axis, 1, 32)?;
        Self::validate_string(&value.add, 1, 32)?;
        Self::validate_string(&value.pd, 1, 32)?;
        Ok(())
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
        Self::require_authorized_vision_records(&env, &caller)?;
        Self::validate_examination_strings(
            &visual_acuity,
            &iop,
            &slit_lamp,
            &visual_field,
            &retina_imaging,
            &fundus_photo,
            &clinical_notes,
        )?;

        let exam = EyeExamination {
            record_id,
            visual_acuity,
            iop,
            slit_lamp,
            visual_field,
            retina_imaging,
            fundus_photo,
            clinical_notes,
        };

        env.storage().persistent().set(&(EXAM_KEY, record_id), &exam);
        Ok(())
    }

    pub fn get_eye_examination(
        env: Env,
        caller: Address,
        record_id: u64,
    ) -> Result<EyeExamination, ContractError> {
        Self::require_authorized_vision_records(&env, &caller)?;
        env.storage()
            .persistent()
            .get(&(EXAM_KEY, record_id))
            .ok_or(ContractError::RecordNotFound)
    }

    pub fn prepare_add_prescription(
        env: Env,
        caller: Address,
        patient: Address,
        provider: Address,
        prescription_data: PrescriptionData,
    ) -> Result<u64, ContractError> {
        Self::require_authorized_vision_records(&env, &caller)?;
        Self::validate_prescription_data(&prescription_data)?;
        let next_id = env
            .storage()
            .instance()
            .get(&RX_COUNTER)
            .unwrap_or(0u64)
            .saturating_add(1u64);
        env.storage().instance().set(&RX_COUNTER, &next_id);
        let prep_key = (symbol_short!("P_ADD_RX"), next_id);
        let prep = (patient, provider, prescription_data, env.ledger().timestamp());
        env.storage().temporary().set(&prep_key, &prep);
        Ok(next_id)
    }

    pub fn commit_add_prescription(
        env: Env,
        caller: Address,
        rx_id: u64,
    ) -> Result<(), ContractError> {
        Self::require_authorized_vision_records(&env, &caller)?;
        let prep_key = (symbol_short!("P_ADD_RX"), rx_id);
        let prepared: (Address, Address, PrescriptionData, u64) = env
            .storage()
            .temporary()
            .get(&prep_key)
            .ok_or(ContractError::InvalidPhase)?;

        let prescription = Prescription {
            id: rx_id,
            patient: prepared.0,
            provider: prepared.1,
            lens_type: LensType::Glasses,
            left_eye: prepared.2.clone(),
            right_eye: prepared.2,
            contact_data: OptionalContactLensData::None,
            issued_at: prepared.3,
            expires_at: prepared.3.saturating_add(31_536_000),
            verified: false,
            metadata_hash: String::from_str(&env, ""),
        };

        env.storage().persistent().set(&(PRESCRIPTION_KEY, rx_id), &prescription);
        env.storage().temporary().remove(&prep_key);
        Ok(())
    }

    pub fn rollback_add_prescription(
        env: Env,
        caller: Address,
        rx_id: u64,
    ) -> Result<(), ContractError> {
        Self::require_authorized_vision_records(&env, &caller)?;
        env.storage().temporary().remove(&(symbol_short!("P_ADD_RX"), rx_id));
        Ok(())
    }

    pub fn get_prescription_count(env: Env) -> u64 {
        env.storage().instance().get(&RX_COUNTER).unwrap_or(0)
    }

    pub fn get_prescription(env: Env, caller: Address, id: u64) -> Result<Prescription, ContractError> {
        Self::require_authorized_vision_records(&env, &caller)?;
        env.storage()
            .persistent()
            .get(&(PRESCRIPTION_KEY, id))
            .ok_or(ContractError::RecordNotFound)
    }
}
