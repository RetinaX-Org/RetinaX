use soroban_sdk::contracterror;

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
    InvalidVersion = 8,
    AlreadyMigrated = 9,
}
