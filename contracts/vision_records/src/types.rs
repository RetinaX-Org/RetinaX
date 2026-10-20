//! Shared contract types: records, users, access grants and two-phase commit payloads.

use crate::prescription;
use crate::rbac::Role;
use soroban_sdk::{contracttype, Address, String};

/// Access levels for record sharing
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccessLevel {
    None,
    Read,
    Write,
    Full,
    Admin,
}

/// Preparation data for user registration
#[contracttype]
#[derive(Clone, Debug)]
pub struct PrepareUserRegistration {
    pub caller: Address,
    pub user: Address,
    pub role: Role,
    pub name: String,
    pub timestamp: u64,
}

/// Preparation data for record addition
#[contracttype]
#[derive(Clone, Debug)]
pub struct PrepareRecordAddition {
    pub caller: Address,
    pub patient: Address,
    pub provider: Address,
    pub record_type: RecordType,
    pub data_hash: String,
    pub timestamp: u64,
}

/// Preparation data for adding a vision record
#[contracttype]
#[derive(Clone, Debug)]
pub struct PrepareAddRecord {
    pub caller: Address,
    pub patient: Address,
    pub provider: Address,
    pub record_type: RecordType,
    pub data_hash: String,
    pub timestamp: u64,
}

/// Preparation data for granting access
#[contracttype]
#[derive(Clone, Debug)]
pub struct PrepareGrantAccess {
    pub caller: Address,
    pub patient: Address,
    pub grantee: Address,
    pub access_level: AccessLevel,
    pub expires_at: Option<u64>,
    pub timestamp: u64,
}

/// Preparation data for adding a prescription
#[contracttype]
#[derive(Clone, Debug)]
pub struct PrepareAddPrescription {
    pub patient: Address,
    pub provider: Address,
    pub prescription_data: prescription::PrescriptionData,
    pub timestamp: u64,
}

/// Vision record types
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecordType {
    /// Eye examination record
    Examination,
    /// Prescription record
    Prescription,
    /// Diagnosis record
    Diagnosis,
    /// Treatment record
    Treatment,
    /// Surgery record
    Surgery,
    /// Laboratory result record
    LabResult,
}

/// User information structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct User {
    pub address: Address,
    pub role: Role,
    pub name: String,
    pub registered_at: u64,
    pub is_active: bool,
}

/// Vision record structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct VisionRecord {
    pub id: u64,
    pub patient: Address,
    pub provider: Address,
    pub record_type: RecordType,
    pub data_hash: String,
    pub key_version: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Access grant structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct AccessGrant {
    pub patient: Address,
    pub grantee: Address,
    pub level: AccessLevel,
    pub granted_at: u64,
    pub expires_at: u64,
}

/// Input for batch record creation
#[contracttype]
#[derive(Clone, Debug)]
pub struct BatchRecordInput {
    pub patient: Address,
    pub record_type: RecordType,
    pub data_hash: String,
}

/// Input for batch access granting
#[contracttype]
#[derive(Clone, Debug)]
pub struct BatchGrantInput {
    pub grantee: Address,
    pub level: AccessLevel,
    pub duration_seconds: u64,
}

/// Archive status of a record. Present only while the record is archived.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveInfo {
    pub record_id: u64,
    pub archived_by: Address,
    pub archived_at: u64,
    pub reason: String,
}
