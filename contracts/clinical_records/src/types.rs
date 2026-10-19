use soroban_sdk::{contracttype, Address, String};

/// A per-eye measurement recorded as free text, one entry per eye.
///
/// Used wherever a clinician records a value that is conventionally written
/// per eye (e.g. visual acuity such as `20/20`, or an IOP reading such as
/// `15 mmHg`).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalMeasurement {
    pub left_eye: String,
    pub right_eye: String,
}

/// Optional counterpart of [`PhysicalMeasurement`], for measurements that
/// may not have been taken (e.g. corrected acuity when no correction was tried).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptPhysicalMeasurement {
    None,
    Some(PhysicalMeasurement),
}

/// Sharpness of vision for each eye, usually measured on a Snellen or logMAR chart.
///
/// * `uncorrected` - vision without glasses or contact lenses (unaided).
/// * `corrected` - best vision achieved with glasses, contacts, or pinhole;
///   absent when no correction was tried.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualAcuity {
    pub uncorrected: PhysicalMeasurement,
    pub corrected: OptPhysicalMeasurement,
}

/// Intraocular pressure (IOP): the fluid pressure inside the eye, in mmHg.
///
/// Sustained high IOP is the main modifiable risk factor for glaucoma.
/// `method` records how it was measured (e.g. applanation tonometry,
/// non-contact air puff), and `timestamp` when the reading was taken.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntraocularPressure {
    pub left_eye: u32,
    pub right_eye: u32,
    pub method: String,
    pub timestamp: u64,
}

/// Findings from a slit-lamp biomicroscopy exam of the anterior segment of the eye.
///
/// Each field holds the clinician's narrative for one structure: the `cornea`,
/// the `anterior_chamber` (fluid between cornea and iris), the `iris`, and
/// the `lens`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlitLampFindings {
    pub cornea: String,
    pub anterior_chamber: String,
    pub iris: String,
    pub lens: String,
}

/// Perimetry (visual field) results: the extent of peripheral vision.
///
/// Reliability notes how trustworthy the patient's responses were during
/// testing; defects describe any lost areas of vision (scotomas).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualField {
    pub left_eye_reliability: String,
    pub right_eye_reliability: String,
    pub left_eye_defects: String,
    pub right_eye_defects: String,
}

/// Optional [`VisualField`]; absent when no perimetry test was performed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptVisualField {
    None,
    Some(VisualField),
}

/// A retinal image with its integrity hash and the clinician's findings.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetinalImaging {
    pub image_url: String,
    pub image_hash: String,
    pub findings: String,
}

/// Optional [`RetinalImaging`]; absent when no retinal photo was taken.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptRetinalImaging {
    None,
    Some(RetinalImaging),
}

/// Fundus (retinal) photograph plus glaucoma-relevant measurements.
///
/// * `cup_to_disc_ratio_*` - the vertical cup-to-disc (C/D) ratio of the optic
///   nerve head; a larger ratio suggests glaucomatous damage.
/// * `macula_status` - condition of the macula, the region responsible for
///   central vision.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundusPhotography {
    pub image_url: String,
    pub image_hash: String,
    pub cup_to_disc_ratio_left: String,
    pub cup_to_disc_ratio_right: String,
    pub macula_status: String,
}

/// Optional [`FundusPhotography`]; absent when no fundus photo was taken.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptFundusPhotography {
    None,
    Some(FundusPhotography),
}

/// A complete eye-examination record for one visit, keyed by `record_id`.
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

/// Correction type prescribed: spectacles or contact lenses.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LensType {
    Glasses,
    ContactLens,
}

/// Refraction values for one eye, as written on a spectacle prescription.
///
/// * `sphere` (SPH) - spherical power for near/farsightedness.
/// * `cylinder` (CYL) - astigmatism power.
/// * `axis` - orientation of the cylinder, 0-180 degrees.
/// * `add` - reading addition for presbyopia.
/// * `pd` - pupillary distance in millimetres.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrescriptionData {
    pub sphere: String,
    pub cylinder: String,
    pub axis: String,
    pub add: String,
    pub pd: String,
}

/// Fitting parameters for one contact lens: `base_curve`, `diameter`, and `brand`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactLensData {
    pub base_curve: String,
    pub diameter: String,
    pub brand: String,
}

/// Optional [`ContactLensData`]; `None` when the prescription is for spectacles.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptionalContactLensData {
    None,
    Some(ContactLensData),
}

/// A stored eyeglass or contact-lens prescription issued by a provider.
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
