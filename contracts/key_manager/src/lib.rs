#![no_std]
#![allow(deprecated)]
#![allow(clippy::too_many_arguments)]

//! # Key Manager Contract
//!
//! A hierarchical deterministic (HD) key management contract for the RetinaX ecosystem.
//! This contract provides secure key generation, derivation, rotation, and recovery
//! capabilities with audit trails and policy enforcement.
//!
//! ## Overview
//!
//! The Key Manager implements a BIP-32-like hierarchical key structure with four levels:
//! - **Master**: Root keys that can derive contract and operation keys
//! - **Contract**: Keys scoped to a specific contract interaction
//! - **Operation**: Keys for specific operations (signing, encryption, etc.)
//! - **Session**: Short-lived keys for ephemeral sessions
//!
//! ## Key Features
//!
//! - **Hierarchical Key Derivation**: Deterministic child key derivation with hardened/non-hardened paths
//! - **Key Rotation**: Time-based and manual key rotation with version history
//! - **Policy Enforcement**: Configurable usage limits, time windows, and allowed operations
//! - **Social Recovery**: Multi-guardian recovery with configurable thresholds
//! - **Audit Trail**: Immutable append-only audit log with cryptographic linking
//! - **Attestation**: Cryptographic proof of key ownership and lineage
//!
//! ## Security Model
//!
//! - All state-changing operations require authentication
//! - Key usage is tracked and limited by policy
//! - Recovery requires M-of-N guardian approvals with time-lock
//! - Audit entries are cryptographically chained to prevent tampering
//!
//! ## Integration
//!
//! This contract integrates with the `identity` contract for guardian management
//! and recovery threshold configuration. The identity contract address is set
//! during initialization and can be updated by the admin.
//!
//! ## Error Handling
//!
//! All fallible operations return `Result<T, ContractError>` with typed errors
//! for precise error handling. See [`ContractError`] for the complete error taxonomy.
//!
//! ## Example Usage
//!
//! ```ignore
//! // Initialize the contract
//! key_manager.initialize(admin, identity_contract);
//!
//! // Create a master encryption key
//! let policy = KeyPolicy {
//!     max_uses: 1000,
//!     not_before: 0,
//!     not_after: 0,
//!     allowed_ops: vec![symbol_short!("ENC")],
//! };
//! let master_key_id = key_manager.create_master_key(
//!     admin,
//!     KeyType::Encryption,
//!     policy,
//!     86400, // 24 hour rotation interval
//!     key_bytes,
//! );
//!
//! // Derive an operation key
//! let op_key_id = key_manager.derive_key(
//!     caller,
//!     master_key_id,
//!     KeyLevel::Operation,
//!     0,
//!     false,
//!     KeyType::Signing,
//!     policy,
//!     3600,
//! );
//!
//! // Use the key
//! let key_material = key_manager.use_key(caller, op_key_id, symbol_short!("SIGN"));
//! ```

mod attestation;
mod derivation;
mod hierarchy;
mod rotation;

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Bytes, BytesN, Env,
    Symbol, Vec,
};

use identity::IdentityContractClient;

use attestation::attest_record;
use derivation::{derive_child_key, derive_record_key};
use hierarchy::validate_child_level;
use rotation::rotation_due;

/// Storage key for the contract admin address
const ADMIN: Symbol = symbol_short!("ADMIN");
/// Storage key for the identity contract address
const IDENTITY: Symbol = symbol_short!("IDENTITY");
/// Storage key for the audit sequence counter
const AUDIT_SEQ: Symbol = symbol_short!("AUD_SEQ");
/// Storage key for the audit log tail hash
const AUDIT_TAIL: Symbol = symbol_short!("AUD_TAIL");
/// Storage key prefix for key records
const KEY: Symbol = symbol_short!("KEY");
/// Storage key prefix for key versions
const KEY_VER: Symbol = symbol_short!("KEY_VER");
/// Storage key prefix for recovery requests
const RECOVERY: Symbol = symbol_short!("RECOV");
/// Storage key prefix for audit entries
const AUDIT: Symbol = symbol_short!("AUDIT");

/// Recovery cooldown period: 24 hours in seconds
const RECOVERY_COOLDOWN: u64 = 86_400;

/// The cryptographic purpose of a key.
///
/// This determines the allowed operations and key derivation behavior.
/// Keys of different types cannot be derived from each other.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyType {
    /// Key used for digital signatures (Ed25519, ECDSA)
    Signing = 1,
    /// Key used for encryption/decryption (X25519, AES)
    Encryption = 2,
    /// Key used for authentication/key agreement
    Authentication = 3,
    /// Key used for delegation/capability tokens
    Delegation = 4,
}

/// Hierarchical level of a key in the derivation tree.
///
/// Levels form a strict hierarchy: Master → Contract → Operation → Session.
/// A key can only derive children at the next level down.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyLevel {
    /// Root key (level 1) - can derive Contract keys
    Master = 1,
    /// Contract-scoped key (level 2) - can derive Operation keys
    Contract = 2,
    /// Operation-specific key (level 3) - can derive Session keys
    Operation = 3,
    /// Ephemeral session key (level 4) - leaf level, cannot derive
    Session = 4,
}

/// Current status of a key.
///
/// Revoked keys cannot be used for any operation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyStatus {
    /// Key is active and can be used per its policy
    Active = 1,
    /// Key has been permanently revoked
    Revoked = 2,
}

/// Access control policy for a key.
///
/// All fields are enforced on every [`use_key`](KeyManagerContract::use_key) call.
/// Zero values for `max_uses`, `not_before`, and `not_after` indicate no limit.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyPolicy {
    /// Maximum number of uses allowed (0 = unlimited)
    pub max_uses: u32,
    /// Unix timestamp before which the key cannot be used (0 = no restriction)
    pub not_before: u64,
    /// Unix timestamp after which the key cannot be used (0 = no restriction)
    pub not_after: u64,
    /// List of allowed operation symbols (empty = all operations allowed)
    pub allowed_ops: Vec<Symbol>,
}

/// Complete metadata record for a key.
///
/// Stored in persistent storage under the `KEY` prefix keyed by the key ID.
/// Contains all information needed for derivation, rotation, and policy enforcement.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyRecord {
    /// Unique identifier (SHA-256 of parent, type, level, timestamp, index)
    pub id: BytesN<32>,
    /// Address that owns and controls this key
    pub owner: Address,
    /// Parent key ID (None for master keys)
    pub parent: Option<BytesN<32>>,
    /// Hierarchical level in the derivation tree
    pub level: KeyLevel,
    /// Cryptographic purpose of this key
    pub key_type: KeyType,
    /// Chain code for hardened/non-hardened derivation (BIP-32 style)
    pub chain_code: BytesN<32>,
    /// Current active key version number
    pub current_version: u32,
    /// Unix timestamp when this key was created
    pub created_at: u64,
    /// Unix timestamp of last rotation
    pub last_rotated: u64,
    /// Required interval between rotations in seconds (0 = manual only)
    pub rotation_interval: u64,
    /// Total number of times this key has been used
    pub uses: u32,
    /// Access control policy
    pub policy: KeyPolicy,
    /// Current status (active or revoked)
    pub status: KeyStatus,
}

/// A specific version of a key's raw material.
///
/// Each rotation creates a new version. Old versions are retained
/// for deriving historical record keys.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyVersion {
    /// Version number (monotonically increasing)
    pub version: u32,
    /// Raw 32-byte key material
    pub key_bytes: BytesN<32>,
    /// Unix timestamp when this version was created
    pub created_at: u64,
}

/// Result of a record key derivation.
///
/// Returned by [`derive_record_key`](KeyManagerContract::derive_record_key) and
/// [`derive_record_key_with_version`](KeyManagerContract::derive_record_key_with_version).
/// The derived key is deterministic given the same inputs.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedKey {
    /// Derived key material for the specific record
    pub key: BytesN<32>,
    /// Version of the base key used for derivation
    pub version: u32,
}

/// Pending key recovery request.
///
/// Created by [`initiate_recovery`](KeyManagerContract::initiate_recovery),
/// approved by guardians via [`approve_recovery`](KeyManagerContract::approve_recovery),
/// and executed via [`execute_recovery`](KeyManagerContract::execute_recovery)
/// after the cooldown period expires.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryRequest {
    /// Key being recovered
    pub key_id: BytesN<32>,
    /// New key material to replace the current version
    pub new_key: BytesN<32>,
    /// List of guardian addresses that have approved
    pub approvals: Vec<Address>,
    /// Unix timestamp when recovery was initiated
    pub initiated_at: u64,
    /// Unix timestamp after which execution is allowed (initiated_at + cooldown)
    pub execute_after: u64,
}

/// Immutable audit log entry.
///
/// Entries form a cryptographic chain via `prev_hash` and `entry_hash`
/// to prevent tampering. Each entry is published as an event.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditEntry {
    /// Monotonically increasing sequence number
    pub seq: u64,
    /// Address that performed the action
    pub actor: Address,
    /// Action symbol (e.g., "KEY_NEW", "KEY_DER", "KEY_ROT", "KEY_RVK", "REC_NEW", "REC_APP", "REC_EXE")
    pub action: Symbol,
    /// Key ID affected by this action (if applicable)
    pub key_id: Option<BytesN<32>>,
    /// Unix timestamp of the action
    pub timestamp: u64,
    /// Hash of action-specific details
    pub details_hash: BytesN<32>,
    /// Hash of the previous audit entry (chaining)
    pub prev_hash: BytesN<32>,
    /// Hash of this entry (prev_hash + details_hash)
    pub entry_hash: BytesN<32>,
}

/// Errors returned by the Key Manager contract.
///
/// Each variant maps to a specific `u32` error code for client-side handling.
#[contracterror]
#[derive(Clone, Debug, Eq, PartialEq, Copy)]
#[repr(u32)]
pub enum ContractError {
    /// Contract has not been initialized
    NotInitialized = 1,
    /// Contract has already been initialized
    AlreadyInitialized = 2,
    /// Caller is not authorized for this operation
    Unauthorized = 3,
    /// Referenced key does not exist
    KeyNotFound = 4,
    /// Key derivation violates hierarchy rules (e.g., Master → Session)
    InvalidHierarchy = 5,
    /// Policy parameters are invalid (e.g., not_after <= not_before)
    InvalidPolicy = 6,
    /// Operation violates key policy (max uses, time window, allowed ops)
    PolicyViolation = 7,
    /// Automatic rotation interval has not elapsed
    RotationNotDue = 8,
    /// A recovery request is already in progress for this key
    RecoveryAlreadyActive = 9,
    /// No active recovery request exists for this key
    RecoveryNotActive = 10,
    /// Caller is not a designated guardian for the key owner
    NotAGuardian = 11,
    /// Guardian has already approved this recovery
    AlreadyApproved = 12,
    /// Insufficient guardian approvals to meet threshold
    InsufficientApprovals = 13,
    /// Recovery cooldown period has not expired
    CooldownNotExpired = 14,
    /// Key has been revoked and cannot be used
    KeyRevoked = 15,
}

/// Main contract implementation for hierarchical key management.
#[contract]
pub struct KeyManagerContract;

#[contractimpl]
#[allow(clippy::too_many_arguments)]
impl KeyManagerContract {
    /// Initialize the contract with admin and identity contract.
    ///
    /// # Arguments
    /// * `admin` - Address that will have admin privileges
    /// * `identity_contract` - Address of the identity contract for guardian management
    ///
    /// # Errors
    /// * [`ContractError::AlreadyInitialized`] - Contract already initialized
    ///
    /// # Events
    /// None
    pub fn initialize(
        env: Env,
        admin: Address,
        identity_contract: Address,
    ) -> Result<(), ContractError> {
        if env.storage().instance().has(&ADMIN) {
            return Err(ContractError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&ADMIN, &admin);
        env.storage().instance().set(&IDENTITY, &identity_contract);
        Ok(())
    }

    /// Update the identity contract address (admin only).
    ///
    /// # Arguments
    /// * `caller` - Must be the admin
    /// * `identity_contract` - New identity contract address
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - Caller is not admin
    ///
    /// # Events
    /// None
    pub fn set_identity_contract(
        env: Env,
        caller: Address,
        identity_contract: Address,
    ) -> Result<(), ContractError> {
        Self::require_admin(&env, &caller)?;
        env.storage().instance().set(&IDENTITY, &identity_contract);
        Ok(())
    }

    /// Create a new master key (admin only).
    ///
    /// Master keys are the root of the hierarchy and can derive Contract-level keys.
    ///
    /// # Arguments
    /// * `caller` - Must be the admin
    /// * `key_type` - Cryptographic purpose of the key
    /// * `policy` - Access control policy
    /// * `rotation_interval` - Seconds between automatic rotations (0 = manual only)
    /// * `key_bytes` - 32-byte raw key material
    ///
    /// # Returns
    /// The new key's unique ID
    ///
    /// # Errors
    /// * [`ContractError::Unauthorized`] - Caller is not admin
    /// * [`ContractError::InvalidPolicy`] - Policy parameters are invalid
    ///
    /// # Events
    /// * `AUDIT` - Published with action `KEY_NEW`
    pub fn create_master_key(
        env: Env,
        caller: Address,
        key_type: KeyType,
        policy: KeyPolicy,
        rotation_interval: u64,
        key_bytes: BytesN<32>,
    ) -> Result<BytesN<32>, ContractError> {
        Self::require_admin(&env, &caller)?;
        Self::validate_policy(&policy)?;

        let now = env.ledger().timestamp();
        let id = Self::new_key_id(&env, None, key_type.clone(), KeyLevel::Master, now, 0);
        let chain_code = Self::chain_code_from(&env, &key_bytes, 0, false);

        let record = KeyRecord {
            id: id.clone(),
            owner: caller.clone(),
            parent: None,
            level: KeyLevel::Master,
            key_type,
            chain_code,
            current_version: 1,
            created_at: now,
            last_rotated: now,
            rotation_interval,
            uses: 0,
            policy,
            status: KeyStatus::Active,
        };

        Self::store_key_record(&env, &record);
        Self::store_key_version(&env, &record.id, 1, key_bytes, now);

        Self::audit(
            &env,
            caller,
            symbol_short!("KEY_NEW"),
            Some(record.id.clone()),
            &record.id,
        );

        Ok(record.id)
    }

    /// Derive a child key from a parent key.
    ///
    /// Implements BIP-32 style hierarchical deterministic derivation.
    /// The child level must be exactly one level below the parent.
    ///
    /// # Arguments
    /// * `caller` - Must be the key owner or admin
    /// * `parent_id` - Parent key ID
    /// * `child_level` - Must be one level below parent (Master→Contract, Contract→Operation, Operation→Session)
    /// * `index` - Derivation index (for multiple children at same level)
    /// * `hardened` - Use hardened derivation (parent private key required)
    /// * `key_type` - Cryptographic purpose of the child key
    /// * `policy` - Access control policy for the child
    /// * `rotation_interval` - Seconds between automatic rotations (0 = manual only)
    ///
    /// # Returns
    /// The new child key's unique ID
    ///
    /// # Errors
    /// * [`ContractError::KeyNotFound`] - Parent key doesn't exist
    /// * [`ContractError::Unauthorized`] - Caller is not owner or admin
    /// * [`ContractError::KeyRevoked`] - Parent key is revoked
    /// * [`ContractError::InvalidHierarchy`] - Child level not exactly one below parent
    /// * [`ContractError::InvalidPolicy`] - Policy parameters are invalid
    ///
    /// # Events
    /// * `AUDIT` - Published with action `KEY_DER`
    #[allow(clippy::too_many_arguments)]
    pub fn derive_key(
        env: Env,
        caller: Address,
        parent_id: BytesN<32>,
        child_level: KeyLevel,
        index: u32,
        hardened: bool,
        key_type: KeyType,
        policy: KeyPolicy,
        rotation_interval: u64,
    ) -> Result<BytesN<32>, ContractError> {
        caller.require_auth();
        let mut parent = Self::load_key_record(&env, &parent_id)?;
        Self::require_owner_or_admin(&env, &caller, &parent.owner)?;
        Self::ensure_active(&parent)?;
        validate_child_level(parent.level.clone(), child_level.clone())?;
        Self::validate_policy(&policy)?;

        let (parent_key, _) = Self::load_key_version(&env, &parent_id, parent.current_version)?;
        let (child_key, child_chain) =
            derive_child_key(&env, &parent_key, &parent.chain_code, index, hardened);

        let now = env.ledger().timestamp();
        let child_id = Self::new_key_id(
            &env,
            Some(parent_id.clone()),
            key_type.clone(),
            child_level.clone(),
            now,
            index,
        );

        let record = KeyRecord {
            id: child_id.clone(),
            owner: parent.owner.clone(),
            parent: Some(parent_id.clone()),
            level: child_level,
            key_type,
            chain_code: child_chain,
            current_version: 1,
            created_at: now,
            last_rotated: now,
            rotation_interval,
            uses: 0,
            policy,
            status: KeyStatus::Active,
        };

        Self::store_key_record(&env, &record);
        Self::store_key_version(&env, &record.id, 1, child_key, now);

        parent.uses = parent.uses.saturating_add(1);
        Self::store_key_record(&env, &parent);

        Self::audit(
            &env,
            caller,
            symbol_short!("KEY_DER"),
            Some(record.id.clone()),
            &record.id,
        );

        Ok(record.id)
    }

    /// Use a key for an operation, returning the key material.
    ///
    /// Increments the key's use counter and enforces policy limits.
    ///
    /// # Arguments
    /// * `caller` - Must be the key owner or admin
    /// * `key_id` - Key to use
    /// * `operation` - Operation symbol (must be in policy's allowed_ops if set)
    ///
    /// # Returns
    /// The current version's raw key material
    ///
    /// # Errors
    /// * [`ContractError::KeyNotFound`] - Key doesn't exist
    /// * [`ContractError::Unauthorized`] - Caller is not owner or admin
    /// * [`ContractError::KeyRevoked`] - Key is revoked
    /// * [`ContractError::PolicyViolation`] - Policy violated (max uses, time window, allowed ops)
    ///
    /// # Events
    /// * `AUDIT` - Published with action `KEY_USE`
    pub fn use_key(
        env: Env,
        caller: Address,
        key_id: BytesN<32>,
        operation: Symbol,
    ) -> Result<BytesN<32>, ContractError> {
        caller.require_auth();
        let mut record = Self::load_key_record(&env, &key_id)?;
        Self::require_owner_or_admin(&env, &caller, &record.owner)?;
        Self::ensure_active(&record)?;
        Self::enforce_policy(&record, &operation, env.ledger().timestamp())?;

        record.uses = record.uses.saturating_add(1);
        Self::store_key_record(&env, &record);

        let (key_bytes, _) = Self::load_key_version(&env, &key_id, record.current_version)?;
        Self::audit(
            &env,
            caller,
            symbol_short!("KEY_USE"),
            Some(key_id),
            &key_bytes,
        );
        Ok(key_bytes)
    }

    /// Derive a deterministic record key from the current key version.
    ///
    /// Used for encrypting individual records without exposing the master key.
    /// The derivation is deterministic: same key_id + record_id always produces the same result.
    ///
    /// # Arguments
    /// * `key_id` - Base key ID
    /// * `record_id` - Unique record identifier
    ///
    /// # Returns
    /// Derived key material and the version used
    ///
    /// # Errors
    /// * [`ContractError::KeyNotFound`] - Key doesn't exist
    /// * [`ContractError::KeyRevoked`] - Key is revoked
    ///
    /// # Events
    /// None
    pub fn derive_record_key(
        env: Env,
        key_id: BytesN<32>,
        record_id: u64,
    ) -> Result<DerivedKey, ContractError> {
        let record = Self::load_key_record(&env, &key_id)?;
        Self::ensure_active(&record)?;
        let (key_bytes, _) = Self::load_key_version(&env, &key_id, record.current_version)?;
        let derived = derive_record_key(&env, &key_bytes, record_id);
        Ok(DerivedKey {
            key: derived,
            version: record.current_version,
        })
    }

    /// Derive a record key from a specific key version.
    ///
    /// Allows deriving keys from historical versions for decrypting old records.
    ///
    /// # Arguments
    /// * `key_id` - Base key ID
    /// * `record_id` - Unique record identifier
    /// * `version` - Specific key version to use
    ///
    /// # Returns
    /// Derived key material and the version used
    ///
    /// # Errors
    /// * [`ContractError::KeyNotFound`] - Key or version doesn't exist
    ///
    /// # Events
    /// None
    pub fn derive_record_key_with_version(
        env: Env,
        key_id: BytesN<32>,
        record_id: u64,
        version: u32,
    ) -> Result<DerivedKey, ContractError> {
        let _record = Self::load_key_record(&env, &key_id)?;
        let (key_bytes, _) = Self::load_key_version(&env, &key_id, version)?;
        let derived = derive_record_key(&env, &key_bytes, record_id);
        Ok(DerivedKey {
            key: derived,
            version,
        })
    }

    /// Rotate a key to a new version.
    ///
    /// Creates a new key version using a deterministic rotation function.
    /// Enforces the key's rotation_interval if set.
    ///
    /// # Arguments
    /// * `caller` - Must be the key owner or admin
    /// * `key_id` - Key to rotate
    ///
    /// # Returns
    /// The new version number
    ///
    /// # Errors
    /// * [`ContractError::KeyNotFound`] - Key doesn't exist
    /// * [`ContractError::Unauthorized`] - Caller is not owner or admin
    /// * [`ContractError::KeyRevoked`] - Key is revoked
    /// * [`ContractError::RotationNotDue`] - Rotation interval has not elapsed
    ///
    /// # Events
    /// * `AUDIT` - Published with action `KEY_ROT`
    pub fn rotate_key(env: Env, caller: Address, key_id: BytesN<32>) -> Result<u32, ContractError> {
        caller.require_auth();
        let mut record = Self::load_key_record(&env, &key_id)?;
        Self::require_owner_or_admin(&env, &caller, &record.owner)?;
        Self::ensure_active(&record)?;

        let now = env.ledger().timestamp();
        if record.rotation_interval > 0
            && !rotation_due(now, record.last_rotated, record.rotation_interval)
        {
            return Err(ContractError::RotationNotDue);
        }

        let (current_key, _) = Self::load_key_version(&env, &key_id, record.current_version)?;
        let new_key = Self::rotate_material(&env, &current_key, now);

        let next_version = record.current_version.saturating_add(1);
        record.current_version = next_version;
        record.last_rotated = now;
        Self::store_key_record(&env, &record);
        Self::store_key_version(&env, &key_id, next_version, new_key, now);

        Self::audit(
            &env,
            caller,
            symbol_short!("KEY_ROT"),
            Some(key_id),
            &record.id,
        );

        Ok(next_version)
    }

    /// Permanently revoke a key (owner or admin).
    ///
    /// Revoked keys cannot be used, rotated, or recovered.
    ///
    /// # Arguments
    /// * `caller` - Must be the key owner or admin
    /// * `key_id` - Key to revoke
    ///
    /// # Errors
    /// * [`ContractError::KeyNotFound`] - Key doesn't exist
    /// * [`ContractError::Unauthorized`] - Caller is not owner or admin
    ///
    /// # Events
    /// * `AUDIT` - Published with action `KEY_RVK`
    pub fn revoke_key(env: Env, caller: Address, key_id: BytesN<32>) -> Result<(), ContractError> {
        caller.require_auth();
        let mut record = Self::load_key_record(&env, &key_id)?;
        Self::require_owner_or_admin(&env, &caller, &record.owner)?;
        record.status = KeyStatus::Revoked;
        Self::store_key_record(&env, &record);
        Self::audit(
            &env,
            caller,
            symbol_short!("KEY_RVK"),
            Some(key_id),
            &record.id,
        );
        Ok(())
    }

    /// Generate a cryptographic attestation for a key.
    ///
    /// Returns a hash proving the key's existence and properties at this point in time.
    /// Does not require authentication - attestations are public.
    ///
    /// # Arguments
    /// * `key_id` - Key to attest
    ///
    /// # Returns
    /// Attestation hash
    ///
    /// # Errors
    /// * [`ContractError::KeyNotFound`] - Key doesn't exist
    ///
    /// # Events
    /// None
    pub fn attest_key(env: Env, key_id: BytesN<32>) -> Result<BytesN<32>, ContractError> {
        let record = Self::load_key_record(&env, &key_id)?;
        Ok(attest_record(&env, &record))
    }

    /// Initiate a key recovery request (guardian only).
    ///
    /// Starts the social recovery process by proposing a new key.
    /// The initiating guardian counts as the first approval.
    /// Requires M-of-N guardian approvals after a 24-hour cooldown.
    ///
    /// # Arguments
    /// * `guardian` - Must be a designated guardian for the key owner
    /// * `key_id` - Key to recover
    /// * `new_key` - New key material to replace the current version
    ///
    /// # Errors
    /// * [`ContractError::KeyNotFound`] - Key doesn't exist
    /// * [`ContractError::NotAGuardian`] - Caller is not a guardian for the key owner
    /// * [`ContractError::RecoveryAlreadyActive`] - Recovery already in progress
    ///
    /// # Events
    /// * `AUDIT` - Published with action `REC_NEW`
    pub fn initiate_recovery(
        env: Env,
        guardian: Address,
        key_id: BytesN<32>,
        new_key: BytesN<32>,
    ) -> Result<(), ContractError> {
        guardian.require_auth();
        let record = Self::load_key_record(&env, &key_id)?;
        let (guardians, threshold) = Self::load_guardians(&env, &record.owner)?;
        if !guardians.contains(&guardian) {
            return Err(ContractError::NotAGuardian);
        }

        let key = (RECOVERY, key_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(ContractError::RecoveryAlreadyActive);
        }

        let mut approvals = Vec::new(&env);
        approvals.push_back(guardian.clone());
        let now = env.ledger().timestamp();
        let request = RecoveryRequest {
            key_id: key_id.clone(),
            new_key,
            approvals,
            initiated_at: now,
            execute_after: now.saturating_add(RECOVERY_COOLDOWN),
        };
        env.storage().persistent().set(&key, &request);

        let details = Self::hash_recovery_details(&env, &request, threshold);
        Self::audit(
            &env,
            guardian,
            symbol_short!("REC_NEW"),
            Some(key_id),
            &details,
        );

        Ok(())
    }

    /// Approve a pending key recovery (guardian only).
    ///
    /// Adds the guardian's approval to the recovery request.
    /// Does not execute the recovery - requires threshold approvals and cooldown expiry.
    ///
    /// # Arguments
    /// * `guardian` - Must be a designated guardian for the key owner
    /// * `key_id` - Key being recovered
    ///
    /// # Errors
    /// * [`ContractError::KeyNotFound`] - Key doesn't exist
    /// * [`ContractError::NotAGuardian`] - Caller is not a guardian for the key owner
    /// * [`ContractError::RecoveryNotActive`] - No recovery in progress
    /// * [`ContractError::AlreadyApproved`] - Guardian already approved
    ///
    /// # Events
    /// * `AUDIT` - Published with action `REC_APP`
    pub fn approve_recovery(
        env: Env,
        guardian: Address,
        key_id: BytesN<32>,
    ) -> Result<(), ContractError> {
        guardian.require_auth();
        let record = Self::load_key_record(&env, &key_id)?;
        let (guardians, threshold) = Self::load_guardians(&env, &record.owner)?;
        if !guardians.contains(&guardian) {
            return Err(ContractError::NotAGuardian);
        }

        let key = (RECOVERY, key_id.clone());
        let mut request: RecoveryRequest = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::RecoveryNotActive)?;

        if request.approvals.contains(&guardian) {
            return Err(ContractError::AlreadyApproved);
        }
        request.approvals.push_back(guardian.clone());
        env.storage().persistent().set(&key, &request);

        let details = Self::hash_recovery_details(&env, &request, threshold);
        Self::audit(
            &env,
            guardian,
            symbol_short!("REC_APP"),
            Some(key_id),
            &details,
        );

        Ok(())
    }

    /// Execute a key recovery after cooldown and sufficient approvals.
    ///
    /// Replaces the key with the new material proposed during initiation.
    /// Can be called by any address once conditions are met.
    ///
    /// # Arguments
    /// * `caller` - Any address (auth required)
    /// * `key_id` - Key being recovered
    ///
    /// # Returns
    /// The new version number
    ///
    /// # Errors
    /// * [`ContractError::KeyNotFound`] - Key doesn't exist
    /// * [`ContractError::RecoveryNotActive`] - No recovery in progress
    /// * [`ContractError::InvalidThreshold`] - Threshold is 0 or no guardians
    /// * [`ContractError::InsufficientApprovals`] - Not enough valid approvals
    /// * [`ContractError::CooldownNotExpired`] - 24-hour cooldown not elapsed
    ///
    /// # Events
    /// * `AUDIT` - Published with action `REC_EXE`
    pub fn execute_recovery(
        env: Env,
        caller: Address,
        key_id: BytesN<32>,
    ) -> Result<u32, ContractError> {
        caller.require_auth();
        let record = Self::load_key_record(&env, &key_id)?;
        let (guardians, threshold) = Self::load_guardians(&env, &record.owner)?;

        let key = (RECOVERY, key_id.clone());
        let request: RecoveryRequest = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ContractError::RecoveryNotActive)?;

        // Revalidate approvals against the *current* guardian set so that
        // stale approvals are invalidated when guardian membership changes.
        if threshold == 0 || guardians.len() == 0 {
            return Err(ContractError::InsufficientApprovals);
        }

        let mut valid_approvals: u32 = 0;
        for i in 0..request.approvals.len() {
            if let Some(approver) = request.approvals.get(i) {
                if guardians.contains(&approver) {
                    valid_approvals = valid_approvals.saturating_add(1);
                }
            }
        }

        if valid_approvals < threshold {
            return Err(ContractError::InsufficientApprovals);
        }

        let now = env.ledger().timestamp();
        if now < request.execute_after {
            return Err(ContractError::CooldownNotExpired);
        }

        let mut record = record;
        let next_version = record.current_version.saturating_add(1);
        record.current_version = next_version;
        record.last_rotated = now;
        Self::store_key_record(&env, &record);
        Self::store_key_version(&env, &key_id, next_version, request.new_key, now);

        env.storage().persistent().remove(&key);

        Self::audit(
            &env,
            caller,
            symbol_short!("REC_EXE"),
            Some(key_id),
            &record.id,
        );

        Ok(next_version)
    }

    /// Get a key's metadata record.
    ///
    /// # Arguments
    /// * `key_id` - Key to query
    ///
    /// # Returns
    /// The key record if it exists, None otherwise
    ///
    /// # Events
    /// None
    pub fn get_key_record(env: Env, key_id: BytesN<32>) -> Option<KeyRecord> {
        env.storage().persistent().get(&(KEY, key_id))
    }

    /// Get a specific version of a key's raw material.
    ///
    /// # Arguments
    /// * `key_id` - Key to query
    /// * `version` - Version number
    ///
    /// # Returns
    /// The key version if it exists, None otherwise
    ///
    /// # Events
    /// None
    pub fn get_key_version(env: Env, key_id: BytesN<32>, version: u32) -> Option<KeyVersion> {
        env.storage().persistent().get(&(KEY_VER, key_id, version))
    }

    /// Get an audit log entry by sequence number.
    ///
    /// # Arguments
    /// * `seq` - Sequence number
    ///
    /// # Returns
    /// The audit entry if it exists, None otherwise
    ///
    /// # Events
    /// None
    pub fn get_audit_entry(env: Env, seq: u64) -> Option<AuditEntry> {
        env.storage().persistent().get(&(AUDIT, seq))
    }

    /// Get the hash of the latest audit entry (chain tail).
    ///
    /// Used to verify audit log integrity.
    ///
    /// # Returns
    /// The tail hash if any entries exist, None otherwise
    ///
    /// # Events
    /// None
    pub fn get_audit_tail(env: Env) -> Option<BytesN<32>> {
        env.storage().instance().get(&AUDIT_TAIL)
    }

    fn require_admin(env: &Env, caller: &Address) -> Result<(), ContractError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN)
            .ok_or(ContractError::NotInitialized)?;
        if caller != &admin {
            return Err(ContractError::Unauthorized);
        }
        Ok(())
    }

    fn require_owner_or_admin(
        env: &Env,
        caller: &Address,
        owner: &Address,
    ) -> Result<(), ContractError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&ADMIN)
            .ok_or(ContractError::NotInitialized)?;
        if caller != owner && caller != &admin {
            return Err(ContractError::Unauthorized);
        }
        Ok(())
    }

    fn ensure_active(record: &KeyRecord) -> Result<(), ContractError> {
        if record.status == KeyStatus::Revoked {
            return Err(ContractError::KeyRevoked);
        }
        Ok(())
    }

    fn validate_policy(policy: &KeyPolicy) -> Result<(), ContractError> {
        if policy.not_after > 0 && policy.not_before > 0 && policy.not_after <= policy.not_before {
            return Err(ContractError::InvalidPolicy);
        }
        Ok(())
    }

    fn enforce_policy(
        record: &KeyRecord,
        operation: &Symbol,
        now: u64,
    ) -> Result<(), ContractError> {
        if record.policy.max_uses > 0 && record.uses >= record.policy.max_uses {
            return Err(ContractError::PolicyViolation);
        }
        if record.policy.not_before > 0 && now < record.policy.not_before {
            return Err(ContractError::PolicyViolation);
        }
        if record.policy.not_after > 0 && now > record.policy.not_after {
            return Err(ContractError::PolicyViolation);
        }
        if !record.policy.allowed_ops.is_empty() && !record.policy.allowed_ops.contains(operation) {
            return Err(ContractError::PolicyViolation);
        }
        Ok(())
    }

    fn new_key_id(
        env: &Env,
        parent: Option<BytesN<32>>,
        key_type: KeyType,
        level: KeyLevel,
        now: u64,
        index: u32,
    ) -> BytesN<32> {
        let mut data = Bytes::new(env);
        match parent {
            Some(p) => data.extend_from_array(&p.to_array()),
            None => data.extend_from_array(b"root"),
        }
        data.extend_from_array(&[key_type as u8]);
        data.extend_from_array(&[level as u8]);
        data.extend_from_array(&now.to_be_bytes());
        data.extend_from_array(&index.to_be_bytes());
        env.crypto().sha256(&data).into()
    }

    fn chain_code_from(
        env: &Env,
        key_bytes: &BytesN<32>,
        index: u32,
        hardened: bool,
    ) -> BytesN<32> {
        let mut data = Bytes::new(env);
        data.extend_from_array(&key_bytes.to_array());
        data.extend_from_array(&index.to_be_bytes());
        data.extend_from_array(&[if hardened { 1 } else { 0 }]);
        data.extend_from_array(b"chain");
        env.crypto().sha256(&data).into()
    }

    fn rotate_material(env: &Env, key_bytes: &BytesN<32>, now: u64) -> BytesN<32> {
        let mut data = Bytes::new(env);
        data.extend_from_array(&key_bytes.to_array());
        data.extend_from_array(&now.to_be_bytes());
        data.extend_from_array(b"rotate");
        env.crypto().sha256(&data).into()
    }

    fn store_key_record(env: &Env, record: &KeyRecord) {
        env.storage()
            .persistent()
            .set(&(KEY, record.id.clone()), record);
    }

    fn store_key_version(
        env: &Env,
        key_id: &BytesN<32>,
        version: u32,
        key_bytes: BytesN<32>,
        created_at: u64,
    ) {
        let version_record = KeyVersion {
            version,
            key_bytes,
            created_at,
        };
        env.storage()
            .persistent()
            .set(&(KEY_VER, key_id.clone(), version), &version_record);
    }

    fn load_key_record(env: &Env, key_id: &BytesN<32>) -> Result<KeyRecord, ContractError> {
        env.storage()
            .persistent()
            .get(&(KEY, key_id.clone()))
            .ok_or(ContractError::KeyNotFound)
    }

    fn load_key_version(
        env: &Env,
        key_id: &BytesN<32>,
        version: u32,
    ) -> Result<(BytesN<32>, u64), ContractError> {
        let entry: KeyVersion = env
            .storage()
            .persistent()
            .get(&(KEY_VER, key_id.clone(), version))
            .ok_or(ContractError::KeyNotFound)?;
        Ok((entry.key_bytes, entry.created_at))
    }

    fn load_guardians(env: &Env, owner: &Address) -> Result<(Vec<Address>, u32), ContractError> {
        let identity_addr: Address = env
            .storage()
            .instance()
            .get(&IDENTITY)
            .ok_or(ContractError::NotInitialized)?;
        let client = IdentityContractClient::new(env, &identity_addr);
        let guardians = client.get_guardians(owner);
        let threshold = client.get_recovery_threshold(owner);
        Ok((guardians, threshold))
    }

    fn audit(
        env: &Env,
        actor: Address,
        action: Symbol,
        key_id: Option<BytesN<32>>,
        details: &BytesN<32>,
    ) {
        let mut seq: u64 = env.storage().instance().get(&AUDIT_SEQ).unwrap_or(0);
        seq = seq.saturating_add(1);

        let prev_hash = env
            .storage()
            .instance()
            .get(&AUDIT_TAIL)
            .unwrap_or(BytesN::from_array(env, &[0u8; 32]));

        let entry_hash = Self::hash_audit(env, &prev_hash, details);

        let entry = AuditEntry {
            seq,
            actor: actor.clone(),
            action,
            key_id,
            timestamp: env.ledger().timestamp(),
            details_hash: details.clone(),
            prev_hash,
            entry_hash: entry_hash.clone(),
        };

        env.storage().persistent().set(&(AUDIT, seq), &entry);
        env.storage().instance().set(&AUDIT_SEQ, &seq);
        env.storage().instance().set(&AUDIT_TAIL, &entry_hash);

        env.events().publish((symbol_short!("AUDIT"), seq), entry);
    }

    fn hash_audit(env: &Env, prev: &BytesN<32>, details: &BytesN<32>) -> BytesN<32> {
        let mut data = Bytes::new(env);
        data.extend_from_array(&prev.to_array());
        data.extend_from_array(&details.to_array());
        env.crypto().sha256(&data).into()
    }

    fn hash_recovery_details(env: &Env, request: &RecoveryRequest, threshold: u32) -> BytesN<32> {
        let mut data = Bytes::new(env);
        data.extend_from_array(&request.key_id.to_array());
        data.extend_from_array(&request.new_key.to_array());
        data.extend_from_array(&request.initiated_at.to_be_bytes());
        data.extend_from_array(&request.execute_after.to_be_bytes());
        data.extend_from_array(&threshold.to_be_bytes());
        data.extend_from_array(&request.approvals.len().to_be_bytes());
        env.crypto().sha256(&data).into()
    }
}
