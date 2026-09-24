use crate::{ContractError, KeyLevel};

/// Validates that the requested child key level is a permissible descendant of the parent key level.
pub fn validate_child_level(parent: KeyLevel, child: KeyLevel) -> Result<(), ContractError> {
    match (parent, child) {
        (KeyLevel::Master, KeyLevel::Contract)
        | (KeyLevel::Contract, KeyLevel::Operation)
        | (KeyLevel::Operation, KeyLevel::Session) => Ok(()),
        _ => Err(ContractError::InvalidHierarchy),
    }
}
