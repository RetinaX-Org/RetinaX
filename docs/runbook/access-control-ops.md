# Access Control Operations Runbook

Operational procedures for the `access_control` micro-contract: role assignment and revocation, emergency access overrides, delegation management, and troubleshooting.

This runbook is the operational source of truth for the `access_control` contract. The contract source is authoritative — when this document and `contracts/access_control/src/` disagree, the source wins.

## 1. Scope and Prerequisites

### 1.1 What this contract does

`access_control` (source: `contracts/access_control/`) is the standalone RBAC provider for the RetinaX platform. It stores:

| Concern | Storage key | Defined in |
|---|---|---|
| Role assignment (role + custom grants/revokes + expiry) | `ROLE_ASN` | `permissions.rs` |
| Full-role delegation | `DELEG` | `permissions.rs` |
| Scoped (per-permission) delegation | `SC_DEL` | `permissions.rs` |
| Delegation parent link (depth tracking) | `DEL_PAR` | `permissions.rs` |
| ACL group definitions / membership | `ACL_GRP`, `USR_GRPS` | `permissions.rs` |
| ABAC policies, credentials, record sensitivity | `ACC_POL`, `USR_CRED`, `REC_SENS` | `permissions.rs` |
| Max delegation depth | `MAX_D_DP` (default `3`) | `permissions.rs` |
| Contract administrator / pending admin | `ADMIN`, `PEND_ADM` | `admin.rs` |
| Whitelist enforcement, entries, list | `WL_EN`, `WL`, `WL_LIST` | `whitelist.rs` |

Consumers consult it for authorization when configured:

- `vision_records` — `set_access_control` / `get_access_control` (see `docs/deployment.md`).
- `clinical_records` — falls back to `access_control` permission checks when an access-control address is registered (see `contracts/clinical_records/src/admin.rs`).

### 1.2 Required inputs before any operation

- Contract ID of `access_control` for the target network (`local`, `testnet`, `futurenet`, `mainnet`).
- Signing identity that holds the required authority (see §1.3) — it must match the `--caller`/`--source` address.
- Target user / delegatee address.
- Explicit expiry timestamp for every temporary grant (absolute ledger timestamp in **seconds**; `0` means permanent).
- Change ticket, business justification, and the identity of the requester.

### 1.3 Authority matrix

| Operation | Required authority | Source |
|---|---|---|
| `assign_role`, `revoke_role` | `ManageUsers` permission | `permissions.rs` |
| `grant_custom_permission`, `revoke_custom_permission` | `ManageUsers` permission | `permissions.rs` |
| `set_user_credential` | `ManageUsers` permission | `permissions.rs` |
| `create_group`, `add_to_group`, `remove_from_group` | `ManageAccess` permission | `permissions.rs` |
| `create_access_policy` | `SystemAdmin` permission | `permissions.rs` |
| `set_record_sensitivity` | `WriteRecord` permission | `permissions.rs` |
| `delegate_role`, `delegate_permissions` | Delegator's own signature | `permissions.rs` |
| `revoke_delegation`, `revoke_scoped_delegation` | Delegator's own signature | `permissions.rs` |
| `set_max_delegation_depth` | Contract administrator | `permissions.rs` → `admin::require_admin` |
| `set_whitelist_enabled`, `add_to_whitelist`, `batch_add_to_whitelist`, `remove_from_whitelist`, `batch_remove_from_whitelist` | Contract administrator | `whitelist.rs` → `admin::require_admin` |
| `propose_admin` | Contract administrator | `admin.rs` |
| `accept_admin` | The nominated new administrator | `admin.rs` |

The registered contract administrator always passes `has_permission` for every permission and is reported as `Role::Admin` by `get_role` (`permissions.rs`).

### 1.4 Invocation conventions

Commands follow the style used in `docs/runbook/incident-response.md` and `docs/deployment.md`:

```bash
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <IDENTITY> \
  --network <NETWORK> \
  -- \
  <function> \
  --<arg> <value>
```

- The signing identity (`--source`) must be the address passed in the `caller` argument, because every mutating function calls `require_auth()` on it.
- Enum arguments are written with the variant name (`--role Optometrist`, `--permission WriteRecord`). The on-chain discriminants are listed in §2.1 and §2.2 for cross-referencing logs and indexers.
- Vector arguments (`--permissions`) are written as a JSON array of variant names. If the installed CLI build rejects the array form, issue the call through the JSON-RPC `simulateTransaction` endpoint instead.
- All timestamps below are absolute ledger seconds (Unix epoch). Read the current ledger timestamp from the network before computing an expiry.

## 2. Roles, Permissions, and Evaluation Order

### 2.1 Roles

`Role` is a strict hierarchy (`contracts/access_control/src/types.rs`).

| Role | Level | Base permissions |
|---|---|---|
| `None` | 0 | — |
| `Patient` | 1 | — (own-record access is handled by the record contracts, not RBAC) |
| `Staff` | 2 | `ManageUsers` |
| `Optometrist` | 3 | `ManageUsers`, `WriteRecord`, `ManageAccess`, `ReadAnyRecord` |
| `Ophthalmologist` | 4 | `ManageUsers`, `WriteRecord`, `ManageAccess`, `ReadAnyRecord` |
| `Admin` | 5 | `SystemAdmin`, `ManageUsers`, `WriteRecord`, `ManageAccess`, `ReadAnyRecord` |

### 2.2 Permissions

| Permission | Discriminant | Meaning |
|---|---|---|
| `ReadAnyRecord` | 1 | Read any patient record |
| `WriteRecord` | 2 | Create/modify clinical records |
| `ManageAccess` | 3 | Grant or revoke clinical access |
| `ManageUsers` | 4 | Manage roles, delegations, assignments |
| `SystemAdmin` | 5 | Circuit breaker, upgrades, parameters |

### 2.3 Evaluation order

`has_permission` resolves in this order (`permissions.rs`):

1. Registered contract administrator → always allowed.
2. Custom revokes on the active assignment → **explicit deny wins**.
3. Custom grants on the active assignment → explicit allow.
4. Base permissions of the active role.
5. Permissions of the ACL groups the user belongs to.
6. Otherwise → deny.

Delegated permissions are **not** part of `has_permission`. They are exposed separately through `has_delegated_permission(delegator, delegatee, permission)`.

`get_role` returns `Role::None` for an assignment whose `expires_at` has passed (expiry is evaluated against the ledger timestamp on every read).

## 3. Role Assignment and Revocation

### 3.1 Assign a role

```bash
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <IDENTITY> \
  --network <NETWORK> \
  -- \
  assign_role \
  --caller <CALLER_ADDRESS> \
  --user <TARGET_ADDRESS> \
  --role Optometrist \
  --expires_at <LEDGER_TIMESTAMP_SECONDS>
```

Procedure:

1. Confirm the target address and the requested role with the requester.
2. Choose the expiry: `0` for a permanent assignment, otherwise an absolute ledger timestamp. Prefer a bounded expiry for any temporary elevation.
3. Invoke `assign_role` from an identity holding `ManageUsers`.
4. Verify:

```bash
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --network <NETWORK> \
  -- \
  get_role \
  --user <TARGET_ADDRESS>
```

5. Verify the permissions the role actually confers:

```bash
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --network <NETWORK> \
  -- \
  has_permission \
  --user <TARGET_ADDRESS> \
  --permission ReadAnyRecord
```

> **Overwrite behaviour:** `assign_role` replaces the whole stored assignment. Any previously granted custom permissions and custom revokes on that user are cleared. If the user has custom grants you need to keep, re-apply them after the role change (§3.3).

### 3.2 Revoke a role

```bash
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <IDENTITY> \
  --network <NETWORK> \
  -- \
  revoke_role \
  --caller <CALLER_ADDRESS> \
  --user <TARGET_ADDRESS>
```

`revoke_role` deletes the assignment entirely, which also removes the user's custom grants, custom revokes, and role expiry. Verify with `get_role` (expect `None`) and `has_permission` (expect `false`).

### 3.3 Temporary elevation with custom permissions

Custom permissions adjust a user's existing role without changing the role itself.

```bash
# Grant an additional permission
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <IDENTITY> \
  --network <NETWORK> \
  -- \
  grant_custom_permission \
  --caller <CALLER_ADDRESS> \
  --user <TARGET_ADDRESS> \
  --permission WriteRecord

# Explicitly deny a permission the role would otherwise grant
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <IDENTITY> \
  --network <NETWORK> \
  -- \
  revoke_custom_permission \
  --caller <CALLER_ADDRESS> \
  --user <TARGET_ADDRESS> \
  --permission WriteRecord
```

Procedure notes:

- Both functions require an existing role assignment for the target user. If the user has none, the call fails with `NotFound` (error code 3) — assign a role first (§3.1).
- `revoke_custom_permission` is an explicit deny and outranks the role's base permissions. There is no "un-revoke" call: to clear a deny you must re-assign the role (§3.1, which resets the custom lists) or revoke the role entirely (§3.2).
- A custom grant has no expiry of its own. To keep temporary elevation time-boxed, prefer `assign_role` with an `expires_at`, because that expiry is enforced on every read.

### 3.4 ACL groups

Groups carry a permission set that members inherit (evaluation step 5).

```bash
# Create the group
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <IDENTITY> \
  --network <NETWORK> \
  -- \
  create_group \
  --caller <CALLER_ADDRESS> \
  --group_name <GROUP_NAME> \
  --permissions '["ReadAnyRecord"]'

# Add a member
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <IDENTITY> \
  --network <NETWORK> \
  -- \
  add_to_group \
  --caller <CALLER_ADDRESS> \
  --user <TARGET_ADDRESS> \
  --group_name <GROUP_NAME>

# Remove a member
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <IDENTITY> \
  --network <NETWORK> \
  -- \
  remove_from_group \
  --caller <CALLER_ADDRESS> \
  --user <TARGET_ADDRESS> \
  --group_name <GROUP_NAME>
```

Verify membership:

```bash
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --network <NETWORK> \
  -- \
  get_user_groups \
  --user <TARGET_ADDRESS>
```

`add_to_group` is idempotent: adding a user who is already a member succeeds without creating a duplicate.

### 3.5 Revocation checklist

- [ ] Role revoked with `revoke_role`, or the assignment re-issued with a bounded `expires_at`.
- [ ] Delegations from the user revoked (§5.4) — a revoked role does not cancel delegations the user granted earlier.
- [ ] ACL group memberships removed (§3.4) — group permissions are independent of the user's role.
- [ ] `get_role` returns `None` and `has_permission` returns `false` for the permissions that must be removed.
- [ ] Custom revokes and grants reviewed (`ROLE_ASN` state) so no shadow grants remain.
- [ ] Change ticket updated with the transaction hash.

## 4. Emergency Access Override

### 4.1 What this contract can and cannot do

`access_control` has **no** pause, circuit-breaker, or emergency-grant function. `AccessControlError::Paused` (code 5) is defined in `types.rs` but is never raised by this contract; pause/resume lives in the record contracts (see `docs/runbook/incident-response.md` §3).

Emergency access to **patient records** is granted by `vision_records` under the Emergency Access Protocol (`docs/emergency-protocol.md`), not here. Use the levers below only for RBAC-level access.

### 4.2 Override ladder

Use the narrowest lever that resolves the incident, in this order.

**Step 1 — Time-boxed role assignment (preferred).**

```bash
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <IDENTITY> \
  --network <NETWORK> \
  -- \
  assign_role \
  --caller <CALLER_ADDRESS> \
  --user <RESPONDER_ADDRESS> \
  --role Optometrist \
  --expires_at <LEDGER_TIMESTAMP_SECONDS>
```

The assignment lapses automatically once the ledger passes `expires_at`; no cleanup transaction is required. Record the expiry in the incident log.

**Step 2 — Targeted custom permission grant.**

Use when the responder already holds a role and only needs one extra permission. Follow §3.3, and remove the grant once the incident is closed by re-assigning the role (§3.1) or revoking the role (§3.2).

**Step 3 — Whitelist enforcement toggle (contract administrator only).**

```bash
# Check current state
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --network <NETWORK> \
  -- \
  is_whitelist_enabled

# Disable enforcement (everyone passes the whitelist check)
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <ADMIN_IDENTITY> \
  --network <NETWORK> \
  -- \
  set_whitelist_enabled \
  --caller <ADMIN_ADDRESS> \
  --enabled false

# Re-enable enforcement after the incident
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <ADMIN_IDENTITY> \
  --network <NETWORK> \
  -- \
  set_whitelist_enabled \
  --caller <ADMIN_ADDRESS> \
  --enabled true
```

> **Blast radius:** while enforcement is disabled, `check_whitelist` returns `true` for every address, so every consumer that gates on the whitelist admits all callers. Treat this as a network-wide bypass: use only with incident-commander approval, keep the window as short as possible, and always re-enable enforcement afterwards. The contract emits a `WL_SET` event on every toggle.

Note that enforcement is **off by default** — `is_whitelist_enabled` returns `false` until it is switched on. Disabling an already-disabled whitelist is a no-op; enabling it is the restricting action.

**Step 4 — Administrator rotation (admin key suspected compromised).**

```bash
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <ADMIN_IDENTITY> \
  --network <NETWORK> \
  -- \
  propose_admin \
  --current_admin <CURRENT_ADMIN_ADDRESS> \
  --new_admin <NEW_ADMIN_ADDRESS>

# Executed by the new administrator identity
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <NEW_ADMIN_IDENTITY> \
  --network <NETWORK> \
  -- \
  accept_admin \
  --new_admin <NEW_ADMIN_ADDRESS>

# Verify
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --network <NETWORK> \
  -- \
  get_admin
```

`propose_admin` stores a pending nominee; `accept_admin` only succeeds when the accepting address matches that nominee exactly, and only the new administrator can complete the handover. The pending nominee is not exposed by the contract interface, so confirm the handover with `get_admin` after `accept_admin` succeeds.

**Step 5 — Record-level emergency access (outside this contract).**

For patient-record access during a clinical emergency, follow `docs/emergency-protocol.md` and the bulk revocation procedure in `docs/runbook/incident-response.md` §4.

### 4.3 Emergency checklist

- [ ] Incident ID (`INC-YYYY-NNN`) and incident commander assigned (`docs/runbook/incident-response.md`).
- [ ] Narrowest lever selected and justification recorded.
- [ ] Expiry timestamp set and communicated to the responder.
- [ ] Whitelist state checked before and after any toggle; enforcement re-enabled.
- [ ] Emergency grants revoked after the incident (§3.5).
- [ ] Administrator rotated if admin authority was involved; old key removed from CI/CD secret stores.
- [ ] Timeline captured for the post-incident review.

## 5. Delegation Management

### 5.1 Delegating a full role

```bash
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <DELEGATOR_IDENTITY> \
  --network <NETWORK> \
  -- \
  delegate_role \
  --delegator <DELEGATOR_ADDRESS> \
  --delegatee <DELEGATEE_ADDRESS> \
  --role Optometrist \
  --expires_at <LEDGER_TIMESTAMP_SECONDS>
```

The delegatee receives the base permissions of the delegated role. Only the delegator's own signature is required — no `ManageUsers` permission is needed, and the delegator is not required to hold the delegated role.

### 5.2 Delegating specific permissions

```bash
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <DELEGATOR_IDENTITY> \
  --network <NETWORK> \
  -- \
  delegate_permissions \
  --delegator <DELEGATOR_ADDRESS> \
  --delegatee <DELEGATEE_ADDRESS> \
  --permissions '["WriteRecord"]' \
  --expires_at <LEDGER_TIMESTAMP_SECONDS>
```

Prefer scoped delegation over full-role delegation for contractors and other limited-scope cover.

### 5.3 Verifying a delegation

```bash
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --network <NETWORK> \
  -- \
  has_delegated_permission \
  --delegator <DELEGATOR_ADDRESS> \
  --delegatee <DELEGATEE_ADDRESS> \
  --permission WriteRecord
```

Delegated permissions are **not** visible through `has_permission` — a delegatee with an active delegation still returns `false` for `has_permission` unless they also hold a matching role or group. Consumers must call `has_delegated_permission` explicitly.

### 5.4 Revoking delegations

```bash
# Revoke the full-role delegation
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <DELEGATOR_IDENTITY> \
  --network <NETWORK> \
  -- \
  revoke_delegation \
  --delegator <DELEGATOR_ADDRESS> \
  --delegatee <DELEGATEE_ADDRESS>

# Revoke the scoped delegation
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <DELEGATOR_IDENTITY> \
  --network <NETWORK> \
  -- \
  revoke_scoped_delegation \
  --delegator <DELEGATOR_ADDRESS> \
  --delegatee <DELEGATEE_ADDRESS>
```

There is no bulk "revoke everything from this delegator" call — revoke each delegatee individually. There is also no delegation listing function; the authoritative view is the contract state for `DELEG` / `SC_DEL` / `DEL_PAR`, read through the ledger or an indexer.

### 5.5 Delegation chain depth

```bash
# Current limit (default 3)
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --network <NETWORK> \
  -- \
  get_max_delegation_depth

# Change the limit (contract administrator only)
soroban contract invoke \
  --id <ACCESS_CONTROL_ID> \
  --source <ADMIN_IDENTITY> \
  --network <NETWORK> \
  -- \
  set_max_delegation_depth \
  --caller <ADMIN_ADDRESS> \
  --max_depth 2
```

Each delegation records the delegator as the delegatee's parent, so the chain depth is the number of ancestors. Creating a delegation that would exceed the configured limit fails with `DelegationDepthExceeded` (error code 8).

### 5.6 Delegation notes

- Delegations are keyed by the `(delegator, delegatee)` pair. One full-role delegation and one scoped delegation can exist per pair; re-delegating overwrites the previous record.
- `expires_at = 0` means the delegation never expires. Use a bounded expiry for every delegation that is not a permanent transfer of responsibility.
- Revoking a delegation also removes the parent link, so the delegatee's chain depth decreases.
- Expiry is evaluated against the ledger timestamp on every check; expired delegations stop granting permissions without any cleanup transaction.

## 6. Troubleshooting

### 6.1 Error codes

`AccessControlError` (`contracts/access_control/src/types.rs`):

| Code | Variant | Meaning | Typical cause | Action |
|---|---|---|---|---|
| 1 | `Unauthorized` | Caller is not authorized to execute the operation | Caller lacks `ManageUsers` / `ManageAccess` / `SystemAdmin`, is not the contract administrator, or `accept_admin` was called by an address other than the nominee | Confirm the role/permission with `get_role` and `has_permission`; check `get_admin`; run the call from the identity that holds the authority |
| 2 | `InvalidInput` | Empty or oversized string parameter | String identifiers exceed `MAX_STRING_LEN` (64) | Re-check the argument values. This code is defined but not raised by the current implementation |
| 3 | `NotFound` | Role assignment or entity not found | `grant_custom_permission` / `revoke_custom_permission` called for a user with no role assignment; `accept_admin` called with no pending nomination | Assign a role first (§3.1); confirm a nomination is pending before `accept_admin` |
| 4 | `DelegationExpired` | Delegation expired or invalid | Delegation past its `expires_at` | Expected behaviour, not a fault — issue a new delegation (§5.1/§5.2) |
| 5 | `Paused` | Contract paused by emergency admin | — | Not raised by this contract; pause/resume lives in the record contracts (`docs/runbook/incident-response.md` §3) |
| 6 | `AlreadyExists` | Target already a member of the group/role | — | Not raised by this contract; `add_to_group` is idempotent and returns success on a duplicate membership |
| 7 | `AlreadyInitialized` | Contract already initialized | `initialize` called a second time | Expected behaviour; the administrator is already set — verify with `get_admin` |
| 8 | `DelegationDepthExceeded` | Delegation depth exceeds the limit | Chain longer than `get_max_delegation_depth` (default 3) | Shorten the chain, or raise the limit as contract administrator (§5.5) |

### 6.2 Symptom-driven diagnosis

| Symptom | Likely cause | Where to look |
|---|---|---|
| `get_role` returns `None` for a user who should have a role | Assignment expired (`expires_at` passed) or was revoked | `get_role`, `get_admin`; re-assign with a valid expiry (§3.1) |
| Role is correct but `has_permission` is `false` | A custom revoke (explicit deny) on the same assignment outranks the role | Re-assign the role to clear custom lists (§3.1), then re-apply needed grants (§3.3) |
| `has_permission` is `false` although the role should grant it | User is not in the required ACL group | `get_user_groups` (§3.4) |
| Delegatee is denied a permission the delegator holds | Delegation expired, or the check used `has_permission` instead of `has_delegated_permission` | `has_delegated_permission` (§5.3) |
| Delegation call fails with code 8 | Chain depth limit reached | `get_max_delegation_depth` (§5.5) |
| A user is rejected by a consumer despite valid RBAC | Whitelist enforcement is on and the user is not listed | `is_whitelist_enabled`, `check_whitelist`, `get_whitelisted_addresses` (§7) |
| Custom grant/revoke fails with code 3 | No role assignment exists for that user | Assign a role first (§3.1) |
| Admin-only call fails with code 1 | Caller is not the registered administrator | `get_admin`; complete the two-step handover (§4.2 step 4) |
| Read-only query shows the expected role but the consumer still denies access | The consumer is not wired to this contract, or evaluates its own inline RBAC | `get_access_control` on `vision_records` (`docs/deployment.md`) |

### 6.3 Note on stale documentation

`docs/access-control-architecture.md` is a design document and has drifted from the implementation: it references delegation storage keys `DELEGATE` / `DLG_SCOPE` (the contract uses `DELEG` / `SC_DEL`), a delegation index, and a bulk `revoke_delegations_from` API that does not exist. Use `contracts/access_control/src/permissions.rs` as the reference for this runbook.

### 6.4 Audit trail

Role changes, custom grants, delegations, and group changes in this contract **do not emit events** — the only events published by `access_control` are `WL_SET`, `WL_ADD`, and `WL_REM` from the whitelist module (`whitelist.rs`). To reconstruct who changed what, read the contract state history from the ledger or an indexer and correlate with transaction history. Record the transaction hash of every change in the change ticket.

## 7. Verification Commands

All commands below are read-only and safe to run at any time.

```bash
# Administrator
soroban contract invoke --id <ACCESS_CONTROL_ID> --network <NETWORK> -- --fn get_admin

# Role and permissions
soroban contract invoke --id <ACCESS_CONTROL_ID> --network <NETWORK> -- --fn get_role --user <TARGET_ADDRESS>
soroban contract invoke --id <ACCESS_CONTROL_ID> --network <NETWORK> -- --fn has_permission --user <TARGET_ADDRESS> --permission ReadAnyRecord

# Delegations
soroban contract invoke --id <ACCESS_CONTROL_ID> --network <NETWORK> -- --fn has_delegated_permission --delegator <DELEGATOR_ADDRESS> --delegatee <DELEGATEE_ADDRESS> --permission WriteRecord
soroban contract invoke --id <ACCESS_CONTROL_ID> --network <NETWORK> -- --fn get_max_delegation_depth

# Groups
soroban contract invoke --id <ACCESS_CONTROL_ID> --network <NETWORK> -- --fn get_user_groups --user <TARGET_ADDRESS>

# Whitelist
soroban contract invoke --id <ACCESS_CONTROL_ID> --network <NETWORK> -- --fn is_whitelist_enabled
soroban contract invoke --id <ACCESS_CONTROL_ID> --network <NETWORK> -- --fn check_whitelist --user <TARGET_ADDRESS>
soroban contract invoke --id <ACCESS_CONTROL_ID> --network <NETWORK> -- --fn is_whitelisted --user <TARGET_ADDRESS>
soroban contract invoke --id <ACCESS_CONTROL_ID> --network <NETWORK> -- --fn get_whitelisted_addresses --offset 0 --limit 50
```

`get_whitelisted_addresses` returns the requested page of addresses together with the total count, so paginate with `--offset` until the page is empty.

## 8. References

- `contracts/access_control/src/lib.rs` — public contract API
- `contracts/access_control/src/types.rs` — `Role`, `Permission`, `AccessControlError`, record types
- `contracts/access_control/src/permissions.rs` — roles, custom grants, delegation, groups, ABAC
- `contracts/access_control/src/admin.rs` — administrator lifecycle
- `contracts/access_control/src/whitelist.rs` — whitelist enforcement
- `docs/rbac-quick-reference.md` — role/permission base matrix
- `docs/access-control-architecture.md` — design background (see §6.3 for known drift)
- `docs/adr/014-access-control-micro-contract.md` — extraction decision and migration path
- `docs/storage-key-conventions.md` — storage key registry
- `docs/emergency-protocol.md` — record-level emergency access
- `docs/runbook/incident-response.md` — circuit breaker, emergency grant revocation, key compromise
- `docs/emergency-contacts.md` — escalation contacts
- `docs/security-audit-checklist.md` — access control review checklist
