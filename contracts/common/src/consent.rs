use std::collections::HashMap;
use crate::CommonError;

// ── String Validation Bounds ──────────────────────────────────────────────────

/// Minimum allowed byte length for a consent identifier.
pub const MIN_CONSENT_ID_LEN: usize = 1;
/// Maximum allowed byte length for a consent identifier.
pub const MAX_CONSENT_ID_LEN: usize = 64;

/// Minimum allowed byte length for a consent subject (patient identifier).
pub const MIN_SUBJECT_LEN: usize = 1;
/// Maximum allowed byte length for a consent subject.
pub const MAX_SUBJECT_LEN: usize = 128;

/// Minimum allowed byte length for a consent grantee (practitioner/entity identifier).
pub const MIN_GRANTEE_LEN: usize = 1;
/// Maximum allowed byte length for a consent grantee.
pub const MAX_GRANTEE_LEN: usize = 128;

// ── String Validation Helpers ─────────────────────────────────────────────────

/// Validate that a string slice conforms to strict length and character constraints.
///
/// Ensures:
/// - Byte length is within `[min_len, max_len]`.
/// - String is not solely whitespace.
/// - String contains only printable ASCII/UTF-8 bytes without ASCII control characters (< 32 or 127).
///
/// # Complexity
/// - Time: O(L) where L is the string byte length (bounded by max_len <= 128, effectively O(1)).
/// - Space: O(1) auxiliary memory.
pub fn validate_consent_string(
    s: &str,
    min_len: usize,
    max_len: usize,
) -> Result<(), CommonError> {
    let len = s.len();
    if len < min_len || len > max_len {
        return Err(CommonError::InvalidInput);
    }
    if s.trim().is_empty() {
        return Err(CommonError::InvalidInput);
    }
    for b in s.bytes() {
        if b < 32 || b == 127 {
            return Err(CommonError::InvalidInput);
        }
    }
    Ok(())
}

/// Validate consent identifier string.
#[inline]
pub fn validate_consent_id(id: &str) -> Result<(), CommonError> {
    validate_consent_string(id, MIN_CONSENT_ID_LEN, MAX_CONSENT_ID_LEN)
}

/// Validate consent subject identifier string.
#[inline]
pub fn validate_subject(subject: &str) -> Result<(), CommonError> {
    validate_consent_string(subject, MIN_SUBJECT_LEN, MAX_SUBJECT_LEN)
}

/// Validate consent grantee identifier string.
#[inline]
pub fn validate_grantee(grantee: &str) -> Result<(), CommonError> {
    validate_consent_string(grantee, MIN_GRANTEE_LEN, MAX_GRANTEE_LEN)
}

// ── Consent Types & Records ───────────────────────────────────────────────────

/// Consent status for ABAC evaluation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsentStatus {
    Active,
    Expired,
    Revoked,
    NotGranted,
}

/// Consent attribute for ABAC policies
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentAttribute {
    pub subject: String,
    pub grantee: String,
    pub consent_type: ConsentType,
    pub status: ConsentStatus,
    pub granted_at: u64,
    pub expires_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsentType {
    Treatment,
    Research,
    Sharing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentRecord {
    pub subject: String,
    pub grantee: String,
    pub consent_type: ConsentType,
    pub granted_at: u64,
    pub expires_at: Option<u64>,
    pub revoked: bool,
}

impl ConsentRecord {
    /// Get consent status at a given timestamp
    pub fn get_status_at(&self, now: u64) -> ConsentStatus {
        if self.revoked {
            ConsentStatus::Revoked
        } else if let Some(exp) = self.expires_at {
            if now < exp {
                ConsentStatus::Active
            } else {
                ConsentStatus::Expired
            }
        } else {
            ConsentStatus::Active
        }
    }

    /// Convert to consent attribute for ABAC evaluation
    pub fn to_attribute(&self, now: u64) -> ConsentAttribute {
        ConsentAttribute {
            subject: self.subject.clone(),
            grantee: self.grantee.clone(),
            consent_type: self.consent_type.clone(),
            status: self.get_status_at(now),
            granted_at: self.granted_at,
            expires_at: self.expires_at,
        }
    }
}

/// Detailed consent state change event payload
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsentStateChangeEvent {
    Granted {
        id: String,
        subject: String,
        grantee: String,
        consent_type: ConsentType,
        granted_at: u64,
        expires_at: Option<u64>,
    },
    Revoked {
        id: String,
        revoked_at: u64,
    },
    Expired {
        id: String,
        expired_at: u64,
    },
}

#[derive(Default, Debug, Clone)]
pub struct ConsentManager {
    pub records: HashMap<String, ConsentRecord>,
}

impl ConsentManager {
    /// Grant consent using an externally supplied timestamp.
    ///
    /// Validates `id`, `subject`, and `grantee` string lengths and format before inserting.
    ///
    /// Callers must provide the current time (`now`) rather than relying
    /// on `SystemTime`. In a Soroban contract context this is the ledger
    /// timestamp; in off-chain tooling it is `SystemTime::now()` converted
    /// to seconds since the UNIX epoch.
    pub fn grant(
        &mut self,
        id: &str,
        subject: &str,
        grantee: &str,
        ctype: ConsentType,
        now: u64,
        ttl_secs: Option<u64>,
    ) -> Result<(), CommonError> {
        validate_consent_id(id)?;
        validate_subject(subject)?;
        validate_grantee(grantee)?;

        let expires = ttl_secs.and_then(|t| now.checked_add(t));
        self.records.insert(
            id.to_string(),
            ConsentRecord {
                subject: subject.to_string(),
                grantee: grantee.to_string(),
                consent_type: ctype,
                granted_at: now,
                expires_at: expires,
                revoked: false,
            },
        );
        Ok(())
    }

    /// Revoke consent for a given consent identifier after validating input format.
    pub fn revoke(&mut self, id: &str) -> Result<(), CommonError> {
        validate_consent_id(id)?;
        if let Some(r) = self.records.get_mut(id) {
            r.revoked = true;
            Ok(())
        } else {
            Err(CommonError::RecordNotFound)
        }
    }

    /// Check if consent is active at the given timestamp.
    ///
    /// Returns `Ok(false)` when the record is missing, revoked, or expired
    /// relative to `now`, and `Err(CommonError::InvalidInput)` if the ID is invalid.
    pub fn is_active(&self, id: &str, now: u64) -> Result<bool, CommonError> {
        validate_consent_id(id)?;
        if let Some(r) = self.records.get(id) {
            if r.revoked {
                return Ok(false);
            }
            if let Some(exp) = r.expires_at {
                return Ok(now < exp);
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Get consent attribute for ABAC evaluation after validating input format.
    pub fn get_consent_attribute(
        &self,
        id: &str,
        now: u64,
    ) -> Result<Option<ConsentAttribute>, CommonError> {
        validate_consent_id(id)?;
        Ok(self.records.get(id).map(|record| record.to_attribute(now)))
    }

    /// Check if consent exists and return its status after validating input format.
    pub fn get_consent_status(&self, id: &str, now: u64) -> Result<ConsentStatus, CommonError> {
        validate_consent_id(id)?;
        match self.records.get(id) {
            Some(record) => Ok(record.get_status_at(now)),
            None => Ok(ConsentStatus::NotGranted),
        }
    }

    /// Get all active consents for a specific grantee after validating grantee string.
    pub fn get_active_consents_for_grantee(
        &self,
        grantee: &str,
        now: u64,
    ) -> Result<Vec<ConsentAttribute>, CommonError> {
        validate_grantee(grantee)?;
        Ok(self
            .records
            .values()
            .filter(|record| record.grantee == grantee)
            .filter(|record| record.get_status_at(now) == ConsentStatus::Active)
            .map(|record| record.to_attribute(now))
            .collect())
    }

    /// Get all active consents for a specific subject after validating subject string.
    pub fn get_active_consents_for_subject(
        &self,
        subject: &str,
        now: u64,
    ) -> Result<Vec<ConsentAttribute>, CommonError> {
        validate_subject(subject)?;
        Ok(self
            .records
            .values()
            .filter(|record| record.subject == subject)
            .filter(|record| record.get_status_at(now) == ConsentStatus::Active)
            .map(|record| record.to_attribute(now))
            .collect())
    }
}

// ── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_consent_string_bounds() {
        // Valid strings
        assert_eq!(validate_consent_string("cst_123", 1, 64), Ok(()));
        assert_eq!(validate_consent_string("a", 1, 64), Ok(()));
        assert_eq!(validate_consent_string(&"x".repeat(64), 1, 64), Ok(()));

        // Too short (empty)
        assert_eq!(
            validate_consent_string("", 1, 64),
            Err(CommonError::InvalidInput)
        );

        // Too long
        assert_eq!(
            validate_consent_string(&"x".repeat(65), 1, 64),
            Err(CommonError::InvalidInput)
        );

        // Whitespace only
        assert_eq!(
            validate_consent_string("   ", 1, 64),
            Err(CommonError::InvalidInput)
        );
        assert_eq!(
            validate_consent_string("\t\t", 1, 64),
            Err(CommonError::InvalidInput)
        );

        // Control characters
        assert_eq!(
            validate_consent_string("cst\n123", 1, 64),
            Err(CommonError::InvalidInput)
        );
        assert_eq!(
            validate_consent_string("cst\x00123", 1, 64),
            Err(CommonError::InvalidInput)
        );
        assert_eq!(
            validate_consent_string("cst\x1f123", 1, 64),
            Err(CommonError::InvalidInput)
        );
        assert_eq!(
            validate_consent_string("cst\x7f123", 1, 64),
            Err(CommonError::InvalidInput)
        );
    }

    #[test]
    fn test_consent_manager_grant_and_is_active() {
        let mut manager = ConsentManager::default();
        let now = 1_000_000;

        // Grant with TTL 3600
        assert_eq!(
            manager.grant("cst_1", "patient_alice", "dr_bob", ConsentType::Treatment, now, Some(3600)),
            Ok(())
        );

        // Active within TTL
        assert_eq!(manager.is_active("cst_1", now), Ok(true));
        assert_eq!(manager.is_active("cst_1", now + 1800), Ok(true));
        assert_eq!(manager.is_active("cst_1", now + 3599), Ok(true));

        // Expired at and past TTL
        assert_eq!(manager.is_active("cst_1", now + 3600), Ok(false));
        assert_eq!(manager.is_active("cst_1", now + 5000), Ok(false));

        // Check status
        assert_eq!(manager.get_consent_status("cst_1", now), Ok(ConsentStatus::Active));
        assert_eq!(manager.get_consent_status("cst_1", now + 4000), Ok(ConsentStatus::Expired));
        assert_eq!(manager.get_consent_status("nonexistent", now), Ok(ConsentStatus::NotGranted));
    }

    #[test]
    fn test_consent_manager_revoke() {
        let mut manager = ConsentManager::default();
        let now = 1_000_000;

        assert_eq!(
            manager.grant("cst_2", "patient_alice", "dr_bob", ConsentType::Sharing, now, None),
            Ok(())
        );

        assert_eq!(manager.is_active("cst_2", now), Ok(true));

        // Revoke
        assert_eq!(manager.revoke("cst_2"), Ok(()));
        assert_eq!(manager.is_active("cst_2", now), Ok(false));
        assert_eq!(manager.get_consent_status("cst_2", now), Ok(ConsentStatus::Revoked));

        // Revoke non-existent
        assert_eq!(manager.revoke("cst_unknown"), Err(CommonError::RecordNotFound));
    }

    #[test]
    fn test_consent_manager_strict_input_validation() {
        let mut manager = ConsentManager::default();
        let now = 1_000_000;

        // Empty ID
        assert_eq!(
            manager.grant("", "patient_alice", "dr_bob", ConsentType::Treatment, now, None),
            Err(CommonError::InvalidInput)
        );

        // ID too long (> 64 bytes)
        let long_id = "a".repeat(65);
        assert_eq!(
            manager.grant(&long_id, "patient_alice", "dr_bob", ConsentType::Treatment, now, None),
            Err(CommonError::InvalidInput)
        );

        // Subject too long (> 128 bytes)
        let long_subject = "s".repeat(129);
        assert_eq!(
            manager.grant("cst_valid", &long_subject, "dr_bob", ConsentType::Treatment, now, None),
            Err(CommonError::InvalidInput)
        );

        // Grantee too long (> 128 bytes)
        let long_grantee = "g".repeat(129);
        assert_eq!(
            manager.grant("cst_valid", "patient_alice", &long_grantee, ConsentType::Treatment, now, None),
            Err(CommonError::InvalidInput)
        );

        // Invalid query strings
        assert_eq!(manager.is_active("", now), Err(CommonError::InvalidInput));
        assert_eq!(manager.revoke(""), Err(CommonError::InvalidInput));
        assert_eq!(manager.get_consent_status("", now), Err(CommonError::InvalidInput));
        assert_eq!(manager.get_consent_attribute("", now), Err(CommonError::InvalidInput));
        assert_eq!(manager.get_active_consents_for_grantee("", now), Err(CommonError::InvalidInput));
        assert_eq!(manager.get_active_consents_for_subject("", now), Err(CommonError::InvalidInput));
    }

    #[test]
    fn test_get_active_consents_filtering() {
        let mut manager = ConsentManager::default();
        let now = 1_000_000;

        manager.grant("c1", "p1", "dr1", ConsentType::Treatment, now, Some(1000)).unwrap();
        manager.grant("c2", "p1", "dr1", ConsentType::Research, now, Some(100)).unwrap();
        manager.grant("c3", "p1", "dr2", ConsentType::Sharing, now, Some(1000)).unwrap();
        manager.grant("c4", "p2", "dr1", ConsentType::Treatment, now, Some(1000)).unwrap();

        // At now + 500: c2 is expired (TTL 100), others active
        let dr1_consents = manager.get_active_consents_for_grantee("dr1", now + 500).unwrap();
        assert_eq!(dr1_consents.len(), 2);
        assert!(dr1_consents.iter().any(|c| c.subject == "p1"));
        assert!(dr1_consents.iter().any(|c| c.subject == "p2"));

        let p1_consents = manager.get_active_consents_for_subject("p1", now + 500).unwrap();
        assert_eq!(p1_consents.len(), 2);
        assert!(p1_consents.iter().any(|c| c.grantee == "dr1"));
        assert!(p1_consents.iter().any(|c| c.grantee == "dr2"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consent_manager_grant_and_is_active() {
        let mut manager = ConsentManager::default();
        let now = 1_000_000;

        // Grant consent with TTL (expires at now + 3600)
        manager.grant("c1", "patient_1", "doctor_1", ConsentType::Treatment, now, Some(3600));
        assert!(manager.is_active("c1", now));
        assert!(manager.is_active("c1", now + 1800));
        assert!(!manager.is_active("c1", now + 3600)); // Expired

        // Grant consent without TTL (never expires)
        manager.grant("c2", "patient_1", "doctor_2", ConsentType::Research, now, None);
        assert!(manager.is_active("c2", now + 1_000_000));
    }

    #[test]
    fn test_consent_manager_revoke() {
        let mut manager = ConsentManager::default();
        let now = 1_000_000;

        manager.grant("c1", "patient_1", "doctor_1", ConsentType::Sharing, now, None);
        assert_eq!(manager.get_consent_status("c1", now), ConsentStatus::Active);

        manager.revoke("c1");
        assert!(!manager.is_active("c1", now));
        assert_eq!(manager.get_consent_status("c1", now), ConsentStatus::Revoked);
    }

    #[test]
    fn test_consent_manager_get_consent_attribute() {
        let mut manager = ConsentManager::default();
        let now = 1_000_000;

        manager.grant("c1", "patient_1", "doctor_1", ConsentType::Treatment, now, Some(3600));
        let attr = manager.get_consent_attribute("c1", now).unwrap();

        assert_eq!(attr.subject, "patient_1");
        assert_eq!(attr.grantee, "doctor_1");
        assert_eq!(attr.status, ConsentStatus::Active);
        assert_eq!(attr.granted_at, now);
        assert_eq!(attr.expires_at, Some(now + 3600));

        assert!(manager.get_consent_attribute("nonexistent", now).is_none());
    }

    #[test]
    fn test_consent_manager_get_consent_status() {
        let mut manager = ConsentManager::default();
        let now = 1_000_000;

        manager.grant("c1", "patient_1", "doctor_1", ConsentType::Treatment, now, Some(100));
        manager.grant("c2", "patient_1", "doctor_2", ConsentType::Research, now, None);
        manager.grant("c3", "patient_2", "doctor_1", ConsentType::Sharing, now, None);
        manager.revoke("c3");

        assert_eq!(manager.get_consent_status("c1", now), ConsentStatus::Active);
        assert_eq!(manager.get_consent_status("c1", now + 200), ConsentStatus::Expired);
        assert_eq!(manager.get_consent_status("c2", now), ConsentStatus::Active);
        assert_eq!(manager.get_consent_status("c3", now), ConsentStatus::Revoked);
        assert_eq!(manager.get_consent_status("c4", now), ConsentStatus::NotGranted);
    }

    #[test]
    fn test_consent_manager_query_active_consents() {
        let mut manager = ConsentManager::default();
        let now = 1_000_000;

        manager.grant("c1", "patient_1", "doctor_1", ConsentType::Treatment, now, None);
        manager.grant("c2", "patient_1", "doctor_1", ConsentType::Research, now, Some(10)); // Will expire
        manager.grant("c3", "patient_2", "doctor_1", ConsentType::Sharing, now, None);
        manager.grant("c4", "patient_1", "doctor_2", ConsentType::Treatment, now, None);
        manager.revoke("c4"); // Revoked

        let active_for_doc1 = manager.get_active_consents_for_grantee("doctor_1", now + 20);
        assert_eq!(active_for_doc1.len(), 2);

        let active_for_pat1 = manager.get_active_consents_for_subject("patient_1", now + 20);
        assert_eq!(active_for_pat1.len(), 1);
        assert_eq!(active_for_pat1[0].grantee, "doctor_1");
    }
}
