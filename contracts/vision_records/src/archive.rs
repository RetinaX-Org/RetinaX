//! Record archival (soft-delete).
//!
//! Archiving hides a record from default queries without touching its data.
//! Admins can still read archived records. Each archive or restore is written
//! to the contract audit log, the same trail `add_record` and `get_record`
//! use, so it is queryable through the existing audit endpoints.
//!
//! An archived record keeps one extra persistent entry (`ArchiveInfo`); its
//! presence is the archived flag. Every operation here is O(1) except the
//! admin-only `get_archived_records`, which scans a patient's record list.

use super::*;

const ARCHIVED: Symbol = symbol_short!("REC_ARCH");

fn archive_key(record_id: u64) -> (Symbol, u64) {
    (ARCHIVED, record_id)
}

/// Current archive status of a record, or None if it is not archived.
pub(crate) fn archive_info(env: &Env, record_id: u64) -> Option<ArchiveInfo> {
    env.storage().persistent().get(&archive_key(record_id))
}

pub(crate) fn is_archived(env: &Env, record_id: u64) -> bool {
    env.storage().persistent().has(&archive_key(record_id))
}

/// The contract admin, or anyone holding the SystemAdmin permission.
pub(crate) fn is_admin(env: &Env, caller: &Address) -> bool {
    let stored: Option<Address> = env.storage().instance().get(&ADMIN);
    stored.as_ref() == Some(caller)
        || VisionRecordsContract::has_permission_unified(env, caller, &Permission::SystemAdmin)
}

/// Writes an archival action to the contract audit log.
fn log_audit(
    env: &Env,
    caller: &Address,
    record: &VisionRecord,
    action: AccessAction,
    result: AccessResult,
    reason: Option<String>,
) {
    let entry = audit::create_audit_entry(
        env,
        caller.clone(),
        record.patient.clone(),
        Some(record.id),
        action,
        result,
        reason,
    );
    audit::add_audit_entry(env, &entry);
    events::publish_audit_log_entry(env, &entry);
}

/// The record's patient or provider, or an admin.
fn authorized(env: &Env, caller: &Address, record: &VisionRecord) -> bool {
    *caller == record.patient || *caller == record.provider || is_admin(env, caller)
}

#[contractimpl]
impl VisionRecordsContract {
    /// Archive (soft-delete) a record: the data is kept but the record is
    /// hidden from default queries. Allowed for the record's patient, its
    /// provider, or an admin.
    pub fn archive_record(
        env: Env,
        caller: Address,
        record_id: u64,
        reason: String,
    ) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(
            &env,
            &circuit_breaker::PauseScope::Function(symbol_short!("ARCH_REC")),
        )?;
        caller.require_auth();

        let record = Self::get_record_raw(&env, record_id)?;

        if !authorized(&env, &caller, &record) {
            log_audit(
                &env,
                &caller,
                &record,
                AccessAction::Archive,
                AccessResult::Denied,
                Some(String::from_str(&env, "Insufficient permissions")),
            );
            return Self::unauthorized(&env, &caller, "archive_record", "record_owner_or_admin");
        }

        if is_archived(&env, record_id) {
            return Err(ContractError::RecordArchived);
        }

        let key = archive_key(record_id);
        env.storage().persistent().set(
            &key,
            &ArchiveInfo {
                record_id,
                archived_by: caller.clone(),
                archived_at: env.ledger().timestamp(),
                reason: reason.clone(),
            },
        );
        extend_ttl_u64_key(&env, &key);

        log_audit(
            &env,
            &caller,
            &record,
            AccessAction::Archive,
            AccessResult::Success,
            Some(reason.clone()),
        );
        events::publish_record_archived(&env, record_id, record.patient, caller, reason);

        Ok(())
    }

    /// Restore an archived record so it appears in default queries again.
    /// Allowed for the record's patient, its provider, or an admin.
    pub fn restore_record(env: Env, caller: Address, record_id: u64) -> Result<(), ContractError> {
        circuit_breaker::require_not_paused(
            &env,
            &circuit_breaker::PauseScope::Function(symbol_short!("REST_REC")),
        )?;
        caller.require_auth();

        let record = Self::get_record_raw(&env, record_id)?;

        if !authorized(&env, &caller, &record) {
            log_audit(
                &env,
                &caller,
                &record,
                AccessAction::Restore,
                AccessResult::Denied,
                Some(String::from_str(&env, "Insufficient permissions")),
            );
            return Self::unauthorized(&env, &caller, "restore_record", "record_owner_or_admin");
        }

        if !is_archived(&env, record_id) {
            return Err(ContractError::RecordNotArchived);
        }

        env.storage().persistent().remove(&archive_key(record_id));

        log_audit(
            &env,
            &caller,
            &record,
            AccessAction::Restore,
            AccessResult::Success,
            None,
        );
        events::publish_record_restored(&env, record_id, record.patient, caller);

        Ok(())
    }

    /// Whether a record is currently archived.
    pub fn is_record_archived(env: Env, record_id: u64) -> bool {
        is_archived(&env, record_id)
    }

    /// Current archive status of a record (who archived it, when and why), or
    /// None if it is not archived.
    pub fn get_archive_info(env: Env, record_id: u64) -> Option<ArchiveInfo> {
        archive_info(&env, record_id)
    }

    /// Admin only: IDs of a patient's archived records.
    pub fn get_archived_records(
        env: Env,
        caller: Address,
        patient: Address,
    ) -> Result<Vec<u64>, ContractError> {
        caller.require_auth();
        if !is_admin(&env, &caller) {
            return Self::unauthorized(&env, &caller, "get_archived_records", "SystemAdmin");
        }

        let all: Vec<u64> = env
            .storage()
            .persistent()
            .get(&(symbol_short!("PAT_REC"), patient))
            .unwrap_or(Vec::new(&env));
        let mut archived = Vec::new(&env);
        for id in all.iter() {
            if is_archived(&env, id) {
                archived.push_back(id);
            }
        }
        Ok(archived)
    }
}
