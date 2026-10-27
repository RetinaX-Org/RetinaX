//! Clinical terminology validation for eye examinations.
//!
//! The checks in this module are advisory. They never reject a record: clinical
//! outliers exist, and a clinician must be able to record what was actually
//! observed. Every finding is returned as a [`ClinicalWarning`] so the caller
//! can surface it in the user interface and in the audit trail.
//!
//! Three groups of checks are implemented:
//!
//! * Intraocular pressure outside the normal range of 10 to 21 mmHg.
//! * Visual acuity that is not written as a fraction such as `20/20` or `6/6`.
//! * Slit lamp findings that do not use the accepted terminology for the
//!   structure that was examined.

use crate::types::{
    ClinicalWarning, IntraocularPressure, OptPhysicalMeasurement, SlitLampFindings, VisualAcuity,
};
use soroban_sdk::{Env, String, Symbol, Vec};

/// Lowest intraocular pressure, in mmHg, that is still considered normal.
pub const IOP_NORMAL_MIN: u32 = 10;

/// Highest intraocular pressure, in mmHg, that is still considered normal.
pub const IOP_NORMAL_MAX: u32 = 21;

/// Warning code raised when an intraocular pressure reading is outside the normal range.
pub const WARN_IOP_OUT_OF_RANGE: &str = "IOP_OUT_OF_RANGE";

/// Warning code raised when a visual acuity value is not written as a fraction.
pub const WARN_VISUAL_ACUITY_FORMAT: &str = "VISUAL_ACUITY_FORMAT";

/// Warning code raised when a slit lamp finding does not use accepted terminology.
pub const WARN_SLIT_LAMP_TERM: &str = "SLIT_LAMP_TERM_UNKNOWN";

/// Largest number of bytes read from a single clinical text field.
const MAX_FIELD_BYTES: usize = 256;

/// Capacity of the scratch buffer used to assemble warning messages.
const MESSAGE_BYTES: usize = 160;

/// Accepted cornea descriptors.
const CORNEA_TERMS: [&str; 18] = [
    "clear",
    "normal",
    "no opacity",
    "no deposits",
    "clean",
    "regular",
    "irregular",
    "opacity",
    "corneal opacity",
    "scar",
    "edema",
    "abrasion",
    "ulcer",
    "keratitis",
    "dry eye",
    "pannus",
    "pterygium",
    "band keratopathy",
];

/// Accepted anterior chamber descriptors.
const ANTERIOR_CHAMBER_TERMS: [&str; 14] = [
    "clear",
    "normal",
    "deep and quiet",
    "quiet",
    "deep",
    "shallow",
    "no cells",
    "no flare",
    "cells and flare",
    "cell and flare",
    "cells present",
    "flare present",
    "hyphema",
    "hypopyon",
];

/// Accepted iris descriptors.
const IRIS_TERMS: [&str; 12] = [
    "normal",
    "normal architecture",
    "iridescent",
    "convex",
    "atrophic",
    "heterochromia",
    "iris bombe",
    "posterior synechiae",
    "anterior synechiae",
    "iridocyclitis",
    "uveitis",
    "neovascularization",
];

/// Accepted lens descriptors.
const LENS_TERMS: [&str; 15] = [
    "clear",
    "clear, no cataract",
    "no cataract",
    "cataract",
    "nuclear sclerosis",
    "posterior subcapsular",
    "cortical cataract",
    "mature cataract",
    "immature cataract",
    "pseudoexfoliation",
    "subluxated",
    "aphakic",
    "pseudophakic",
    "intraocular lens",
    "iol",
];

/// Small fixed capacity buffer used to assemble warning messages.
struct Message {
    bytes: [u8; MESSAGE_BYTES],
    len: usize,
}

impl Message {
    /// Creates an empty message buffer.
    fn new() -> Self {
        Self {
            bytes: [0u8; MESSAGE_BYTES],
            len: 0,
        }
    }

    /// Appends a piece of text, ignoring anything that does not fit.
    fn push_str(&mut self, text: &str) {
        self.push_bytes(text.as_bytes());
    }

    /// Appends raw bytes, ignoring anything that does not fit.
    fn push_bytes(&mut self, bytes: &[u8]) {
        let end = self.len + bytes.len();
        if end > self.bytes.len() {
            return;
        }
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
    }

    /// Appends a number in decimal notation.
    fn push_u32(&mut self, value: u32) {
        let mut digits = [0u8; 10];
        let mut count = 0usize;
        let mut rest = value;
        if rest == 0 {
            digits[0] = b'0';
            count = 1;
        }
        while rest > 0 {
            digits[count] = b'0' + (rest % 10) as u8;
            rest /= 10;
            count += 1;
        }
        while count > 0 {
            count -= 1;
            if self.len < self.bytes.len() {
                self.bytes[self.len] = digits[count];
                self.len += 1;
            }
        }
    }

    /// Returns the assembled message.
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// Copies the bytes of a clinical text field into `buf` and returns how many bytes were read.
///
/// Fields longer than the buffer are skipped and reported as zero bytes, which never
/// matches the accepted terminology and therefore raises a warning.
fn read_field(value: &String, buf: &mut [u8; MAX_FIELD_BYTES]) -> usize {
    let len = value.len() as usize;
    if len > MAX_FIELD_BYTES {
        return 0;
    }
    value.copy_into_slice(&mut buf[..len]);
    len
}

/// Removes leading and trailing ASCII whitespace.
fn trim_ascii(bytes: &[u8]) -> &[u8] {
    let mut start = 0usize;
    let mut end = bytes.len();
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[start..end]
}

/// Compares two byte strings ignoring ASCII case.
fn eq_ignore_case(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// Returns true when the bytes are a decimal number such as `20` or `20.5`.
fn is_decimal(bytes: &[u8]) -> bool {
    let mut digits = 0usize;
    let mut seen_dot = false;
    for byte in bytes {
        if byte.is_ascii_digit() {
            digits += 1;
        } else if *byte == b'.' && !seen_dot && digits > 0 {
            seen_dot = true;
        } else {
            return false;
        }
    }
    digits > 0
}

/// Returns true when the bytes are a visual acuity fraction such as `20/20` or `6/6`.
fn is_visual_acuity_notation(bytes: &[u8]) -> bool {
    let trimmed = trim_ascii(bytes);
    let mut parts = trimmed.split(|byte| *byte == b'/');
    let numerator = match parts.next() {
        Some(value) => value,
        None => return false,
    };
    let denominator = match parts.next() {
        Some(value) => value,
        None => return false,
    };
    if parts.next().is_some() {
        return false;
    }
    is_decimal(numerator) && is_decimal(denominator)
}

/// Returns true when the text matches one of the accepted terms, ignoring case and padding.
fn is_accepted_term(bytes: &[u8], terms: &[&str]) -> bool {
    let trimmed = trim_ascii(bytes);
    terms
        .iter()
        .any(|term| eq_ignore_case(trimmed, term.as_bytes()))
}

/// Appends a warning to the list.
fn push_warning(
    env: &Env,
    warnings: &mut Vec<ClinicalWarning>,
    code: &str,
    field: &str,
    detail: &[u8],
) {
    warnings.push_back(ClinicalWarning {
        code: Symbol::new(env, code),
        field: String::from_str(env, field),
        detail: String::from_bytes(env, detail),
    });
}

/// Warns when an intraocular pressure reading is outside the normal range.
fn check_iop(env: &Env, warnings: &mut Vec<ClinicalWarning>, field: &str, value: u32) {
    if (IOP_NORMAL_MIN..=IOP_NORMAL_MAX).contains(&value) {
        return;
    }
    let mut message = Message::new();
    message.push_str("IOP ");
    message.push_u32(value);
    message.push_str(" mmHg is outside the normal range ");
    message.push_u32(IOP_NORMAL_MIN);
    message.push_str("-");
    message.push_u32(IOP_NORMAL_MAX);
    message.push_str(" mmHg");
    push_warning(
        env,
        warnings,
        WARN_IOP_OUT_OF_RANGE,
        field,
        message.as_bytes(),
    );
}

/// Warns when a visual acuity value is not written as a fraction.
fn check_acuity(env: &Env, warnings: &mut Vec<ClinicalWarning>, field: &str, value: &String) {
    let mut buf = [0u8; MAX_FIELD_BYTES];
    let len = read_field(value, &mut buf);
    if is_visual_acuity_notation(&buf[..len]) {
        return;
    }
    let mut message = Message::new();
    message.push_str("visual acuity \"");
    message.push_bytes(&buf[..len]);
    message.push_str("\" is not written as a fraction such as 20/20");
    push_warning(
        env,
        warnings,
        WARN_VISUAL_ACUITY_FORMAT,
        field,
        message.as_bytes(),
    );
}

/// Warns when a slit lamp finding does not use the accepted terminology.
fn check_slit_lamp(
    env: &Env,
    warnings: &mut Vec<ClinicalWarning>,
    field: &str,
    value: &String,
    terms: &[&str],
) {
    let mut buf = [0u8; MAX_FIELD_BYTES];
    let len = read_field(value, &mut buf);
    if is_accepted_term(&buf[..len], terms) {
        return;
    }
    let mut message = Message::new();
    message.push_str("\"");
    message.push_bytes(&buf[..len]);
    message.push_str("\" is not part of the accepted slit lamp terminology");
    push_warning(
        env,
        warnings,
        WARN_SLIT_LAMP_TERM,
        field,
        message.as_bytes(),
    );
}

/// Collects every clinical terminology warning for one examination.
///
/// The visual acuity values are only checked for the corrected measurement when
/// it was taken. All other fields are always checked.
///
/// Returns an empty list when the examination uses accepted terminology.
pub fn collect_terminology_warnings(
    env: &Env,
    visual_acuity: &VisualAcuity,
    iop: &IntraocularPressure,
    slit_lamp: &SlitLampFindings,
) -> Vec<ClinicalWarning> {
    let mut warnings = Vec::new(env);

    check_iop(env, &mut warnings, "iop.left_eye", iop.left_eye);
    check_iop(env, &mut warnings, "iop.right_eye", iop.right_eye);

    check_acuity(
        env,
        &mut warnings,
        "visual_acuity.uncorrected.left_eye",
        &visual_acuity.uncorrected.left_eye,
    );
    check_acuity(
        env,
        &mut warnings,
        "visual_acuity.uncorrected.right_eye",
        &visual_acuity.uncorrected.right_eye,
    );
    if let OptPhysicalMeasurement::Some(corrected) = &visual_acuity.corrected {
        check_acuity(
            env,
            &mut warnings,
            "visual_acuity.corrected.left_eye",
            &corrected.left_eye,
        );
        check_acuity(
            env,
            &mut warnings,
            "visual_acuity.corrected.right_eye",
            &corrected.right_eye,
        );
    }

    check_slit_lamp(
        env,
        &mut warnings,
        "slit_lamp.cornea",
        &slit_lamp.cornea,
        &CORNEA_TERMS,
    );
    check_slit_lamp(
        env,
        &mut warnings,
        "slit_lamp.anterior_chamber",
        &slit_lamp.anterior_chamber,
        &ANTERIOR_CHAMBER_TERMS,
    );
    check_slit_lamp(
        env,
        &mut warnings,
        "slit_lamp.iris",
        &slit_lamp.iris,
        &IRIS_TERMS,
    );
    check_slit_lamp(
        env,
        &mut warnings,
        "slit_lamp.lens",
        &slit_lamp.lens,
        &LENS_TERMS,
    );

    warnings
}
