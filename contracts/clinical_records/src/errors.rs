use soroban_sdk::contracterror;

/// Contract-specific error codes.
///
/// All errors are returned as `Result<_, ContractError>` from public
/// contract functions. Error codes are stable and can be used by
/// clients for programmatic error handling.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ContractError {
    /// Contract initialization already performed.
    AlreadyInitialized = 1,
    /// Contract not yet initialized (init must be called first).
    NotInitialized = 2,
    /// Caller not authorized for this operation.
    Unauthorized = 3,
    /// Input validation failed (length, format, bounds).
    InvalidInput = 4,
    /// Record type not recognized (internal consistency error).
    InvalidRecordType = 5,
    /// Requested record not found in storage.
    RecordNotFound = 6,
    /// Prescription not in expected phase (prepared/committed).
    InvalidPhase = 7,
}