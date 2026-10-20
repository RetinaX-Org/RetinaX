use soroban_sdk::{contracttype, Address, String};

/// A measurement for a single eye (left/right).
///
/// Used for visual acuity, refractive error, and other paired-eye values.
/// Values are stored as strings to accommodate various notations
/// (Snellen fraction, logMAR decimal, diopter values, etc.).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhysicalMeasurement {
    /// Left eye measurement (e.g., "20/20", "6/6", "1.0", "-2.50")
    pub left_eye: String,
    /// Right eye measurement (e.g., "20/20", "6/6", "1.0", "-2.50")
    pub right_eye: String,
}

/// Optional wrapper for [`PhysicalMeasurement`].
///
/// Used when a corrected measurement may not be available.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptPhysicalMeasurement {
    /// No measurement provided
    None,
    /// Measurement present
    Some(PhysicalMeasurement),
}

/// Visual acuity (sharpness of vision) for both eyes.
///
/// Contains uncorrected (without glasses/contacts) and optionally
/// corrected (with best refractive correction) measurements.
/// Notation examples: Snellen "20/20", "6/12"; logMAR "0.0", "0.3"; decimal "1.0", "0.5".
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualAcuity {
    /// Vision without correction (glasses/contacts)
    pub uncorrected: PhysicalMeasurement,
    /// Vision with best optical correction (optional)
    pub corrected: OptPhysicalMeasurement,
}

/// Intraocular pressure (IOP) measurement for glaucoma screening.
///
/// Measured in mmHg. Normal range ~10-21 mmHg. Methods include:
/// - Goldmann applanation tonometry (GAT) - gold standard
/// - iCare rebound tonometry
/// - Non-contact tonometry (NCT, "air puff")
/// - Perkins handheld applanation
/// - Tonopen
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IntraocularPressure {
    /// Left eye IOP in mmHg
    pub left_eye: u32,
    /// Right eye IOP in mmHg
    pub right_eye: u32,
    /// Measurement method (e.g., "GAT", "iCare", "NCT", "Perkins", "Tonopen")
    pub method: String,
    /// Unix timestamp of measurement
    pub timestamp: u64,
}

/// Slit-lamp biomicroscopy findings for anterior segment.
///
/// Examination of cornea, anterior chamber, iris, and lens using
/// a biomicroscope (slit lamp). Findings recorded as descriptive text.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlitLampFindings {
    /// Cornea: clarity, edema, infiltrates, neovascularization, scars
    pub cornea: String,
    /// Anterior chamber: cells, flare, depth, angle status
    pub anterior_chamber: String,
    /// Iris: pattern, atrophy, neovascularization, synechiae
    pub iris: String,
    /// Lens: clarity (cataract grade), position, pseudoexfoliation
    pub lens: String,
}

/// Visual field (perimetry) results.
///
/// Maps peripheral vision sensitivity. Used for glaucoma, neurological
/// conditions, and retinal disorders. Reliability indices (fixation losses,
/// false positives/negatives) are included.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisualField {
    /// Left eye reliability assessment (e.g., "Good", "Fair - 15% fixation losses")
    pub left_eye_reliability: String,
    /// Right eye reliability assessment
    pub right_eye_reliability: String,
    /// Left eye defect description (e.g., "Nasal step", "Arcuate scotoma", "Normal")
    pub left_eye_defects: String,
    /// Right eye defect description
    pub right_eye_defects: String,
}

/// Optional wrapper for [`VisualField`].
///
/// Visual field testing is not performed at every visit.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptVisualField {
    /// No visual field test performed
    None,
    /// Visual field results present
    Some(VisualField),
}

/// Retinal imaging (OCT, angiography, etc.) with findings.
///
/// Optical Coherence Tomography (OCT) provides cross-sectional retinal
/// layers. Fluorescein/ICG angiography shows vascular perfusion.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetinalImaging {
    /// URL to stored image (IPFS, cloud storage, etc.)
    pub image_url: String,
    /// Cryptographic hash (SHA-256) of image for integrity verification
    pub image_hash: String,
    /// Clinical interpretation of imaging (e.g., "Macular edema", "Drusen", "Normal")
    pub findings: String,
}

/// Optional wrapper for [`RetinalImaging`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptRetinalImaging {
    None,
    Some(RetinalImaging),
}

/// Fundus photography with cup-to-disc ratio for glaucoma assessment.
///
/// Color photographs of the retina/optic nerve. Cup-to-disc ratio
/// (CDR) is a key glaucoma metric: >0.7 suspicious, >0.9 advanced.
/// Macula status documents central retinal health.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundusPhotography {
    /// URL to stored fundus photo
    pub image_url: String,
    /// SHA-256 hash for integrity
    pub image_hash: String,
    /// Vertical cup-to-disc ratio left eye (e.g., "0.3", "0.7")
    pub cup_to_disc_ratio_left: String,
    /// Vertical cup-to-disc ratio right eye
    pub cup_to_disc_ratio_right: String,
    /// Macula assessment (e.g., "Normal", "Drusen", "Edema", "Hemorrhage")
    pub macula_status: String,
}

/// Optional wrapper for [`FundusPhotography`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptFundusPhotography {
    None,
    Some(FundusPhotography),
}

/// Complete eye examination record.
///
/// Aggregates all clinical components from a single visit.
/// Stored under unique `record_id` for retrieval.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EyeExamination {
    /// Unique examination identifier
    pub record_id: u64,
    /// Visual acuity (uncorrected + corrected)
    pub visual_acuity: VisualAcuity,
    /// Intraocular pressure (IOP)
    pub iop: IntraocularPressure,
    /// Slit-lamp anterior segment findings
    pub slit_lamp: SlitLampFindings,
    /// Visual field (perimetry) results
    pub visual_field: OptVisualField,
    /// Retinal imaging (OCT/angiography)
    pub retina_imaging: OptRetinalImaging,
    /// Fundus photography with CDR
    pub fundus_photo: OptFundusPhotography,
    /// Free-text clinical notes (observations, impressions, plan)
    pub clinical_notes: String,
}

/// Lens type for optical prescription.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LensType {
    /// Spectacle (glasses) prescription
    Glasses,
    /// Contact lens prescription
    ContactLens,
}

/// Optical prescription parameters for one eye.
///
/// Standard notation:
/// - Sphere: spherical power in diopters (e.g., "-2.50", "+1.75")
/// - Cylinder: astigmatism power in diopters (e.g., "-0.75", "0.00")
/// - Axis: cylinder meridian in degrees 1-180 (e.g., "90", "180")
/// - Add: near addition for presbyopia in diopters (e.g., "+2.00")
/// - PD: pupillary distance in mm (e.g., "63", "31/30" for monocular)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrescriptionData {
    pub sphere: String,
    pub cylinder: String,
    pub axis: String,
    pub add: String,
    pub pd: String,
}

/// Contact lens specific parameters.
///
/// Additional to base prescription:
/// - Base curve: corneal curvature match in mm (e.g., "8.6")
/// - Diameter: lens diameter in mm (e.g., "14.2")
/// - Brand: manufacturer/brand name
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactLensData {
    pub base_curve: String,
    pub diameter: String,
    pub brand: String,
}

/// Optional wrapper for [`ContactLensData`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OptionalContactLensData {
    None,
    Some(ContactLensData),
}

/// Complete optical prescription record.
///
/// Created via 2-phase commit: prepare → commit.
/// Glasses or contact lens. Includes verification workflow.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Prescription {
    /// Unique prescription identifier
    pub id: u64,
    /// Patient receiving prescription
    pub patient: Address,
    /// Prescribing provider
    pub provider: Address,
    /// Glasses or ContactLens
    pub lens_type: LensType,
    /// Left eye optical parameters
    pub left_eye: PrescriptionData,
    /// Right eye optical parameters
    pub right_eye: PrescriptionData,
    /// Contact lens specific data (if applicable)
    pub contact_data: OptionalContactLensData,
    /// Issue timestamp (Unix seconds)
    pub issued_at: u64,
    /// Expiration timestamp (typically 1 year)
    pub expires_at: u64,
    /// Provider verification status
    pub verified: bool,
    /// Hash of prescription metadata for audit trail
    pub metadata_hash: String,
}