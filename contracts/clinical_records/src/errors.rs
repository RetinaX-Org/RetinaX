use soroban_sdk::contracterror;

/// Error codes returned by the clinical-records contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum ContractError {
    /// `initialize` was called a second time.
    AlreadyInitialized = 1,
    /// The contract has not been initialized yet.
    NotInitialized = 2,
    /// Caller is not the admin or the vision-records contract.
    Unauthorized = 3,
    /// A string failed its length bounds.
    InvalidInput = 4,
    /// The record exists but has an unexpected type.
    InvalidRecordType = 5,
    /// No exam or prescription is stored under the given id.
    RecordNotFound = 6,
    /// Two-phase prescription call made out of order (nothing staged).
    InvalidPhase = 7,
}
