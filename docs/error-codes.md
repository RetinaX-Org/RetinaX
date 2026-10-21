# Error Codes

Standardised error codes used across all RetinaX contracts.
Every contract re-exports `CommonError` from the `common` crate as `ContractError` so that error codes are identical everywhere.

## Code Ranges

| Range   | Category                       | Description                                   |
|---------|--------------------------------|-----------------------------------------------|
| 1 – 9   | Lifecycle / Initialisation     | Contract setup and one-time operations         |
| 10 – 19 | Authentication & Authorisation | Caller identity and permission checks          |
| 20 – 29 | Resource Not Found             | Requested entity does not exist in storage     |
| 30 – 39 | Validation / Input             | Invalid parameters or malformed data           |
| 40 – 49 | Contract State                 | Runtime state constraints (paused, locked, …)  |
| 100+    | Contract-Specific              | Reserved for individual contract extensions    |

## Common Errors

| Code | Variant              | When it is returned                                                                                   |
|------|----------------------|-------------------------------------------------------------------------------------------------------|
| 1    | `NotInitialized`     | A function that requires prior initialisation is called before `initialize()`.                        |
| 2    | `AlreadyInitialized` | `initialize()` is called more than once on the same contract instance.                                |
| 10   | `AccessDenied`       | The caller lacks the required role or permission (e.g. not admin, not record owner, expired grant).   |
| 20   | `UserNotFound`       | The requested user address does not exist in contract storage.                                        |
| 21   | `RecordNotFound`     | The requested record ID does not exist in contract storage.                                           |
| 30   | `InvalidInput`       | One or more parameters are invalid (empty list, zero duration, malformed hash, etc.).                 |
| 40   | `Paused`             | The contract is temporarily paused and cannot process the request.                                    |

## Contract-Specific Extensions

### AccessControlError (`access_control` contract)

| Code | Variant | When it is returned |
|------|---------|---------------------|
| 1 | `Unauthorized` | Caller lacks required permission (ManageUsers, ManageAccess, SystemAdmin, WriteRecord) or is not admin |
| 2 | `InvalidInput` | Parameter validation failed (empty string, exceeds MAX_RBAC_STRING_LEN, malformed) |
| 3 | `NotFound` | Entity not found (no pending admin proposal, no role assignment, no delegation) |
| 4 | `DelegationExpired` | Delegation timestamp exceeded (expires_at > 0 and current_time >= expires_at) |
| 5 | `Paused` | Contract is paused via circuit breaker (global or operation-specific scope) |
| 6 | `AlreadyExists` | Duplicate group name or role assignment (idempotent add_to_group returns Ok) |
| 7 | `AlreadyInitialized` | `initialize()` called more than once |

### ContractError (`clinical_records` contract)

| Code | Variant | When it is returned |
|------|---------|---------------------|
| 1 | `AlreadyInitialized` | `initialize()` called more than once |
| 2 | `NotInitialized` | Contract not initialized (admin/Vision Records not set) |
| 3 | `Unauthorized` | Caller not Vision Records contract (checked via access_control) |
| 4 | `InvalidInput` | Parameter validation failed (exceeds MAX_STRING_LEN, empty required field) |
| 5 | `InvalidRecordType` | Internal consistency error (unexpected record type in storage) |
| 6 | `RecordNotFound` | Requested examination/prescription ID does not exist |
| 7 | `InvalidPhase` | Prescription not in expected phase (prepare/commit/rollback mismatch) |

## Migration Notes

### `Unauthorized` → `AccessDenied` (code 10)

Previous versions of `vision_records` defined two overlapping error variants:
- `Unauthorized` (code 3) — caller is not allowed
- `AccessDenied` (code 7) — caller is not allowed

These have been consolidated into a single `AccessDenied` (code **10**).
Any off-chain code that matched on error code `3` or `7` should now match on `10`.

## Extending Errors

Contracts that need domain-specific error codes should define them in their own `#[contracterror]` enum starting at code **100** to avoid collisions with the common set.

```rust
#[contracterror]
#[derive(Clone, Debug, Eq, PartialEq, Copy)]
#[repr(u32)]
pub enum VisionError {
    /// The emergency access window has expired.
    EmergencyExpired = 100,
    /// Maximum record limit per patient exceeded.
    RecordLimitExceeded = 101,
}
```
