# ADR-0014: Access Control Micro-Contract Extraction

- **Status**: Accepted
- **Date**: 2026-10-20

## Context

The `vision_records` contract has grown to include a comprehensive inline RBAC/ABAC access control module (`contracts/vision_records/src/rbac.rs`, ~1,465 lines) that handles:

- **Role-Based Access Control (RBAC)**: Hierarchical roles (Patient → Staff → Optometrist → Ophthalmologist → Admin) with base permissions
- **Custom Grants/Revokes**: Per-user explicit permission grants and denials
- **Delegation**: Full role delegation and scoped permission delegation with TTL
- **ACL Groups**: Named groups with permission sets for bulk assignment
- **Attribute-Based Access Control (ABAC)**: Policy evaluation against credentials, sensitivity, time restrictions, consent
- **Policy DSL Integration**: Composable policy engine via `teye_common::policy_dsl`

This inline module creates several problems:

1. **Contract size & complexity**: `vision_records` exceeds recommended WASM size limits for mainnet deployment; the RBAC module alone is ~1,465 lines
2. **Coupling**: Access control logic is tightly coupled to vision records domain, preventing reuse by other contracts (clinical_records, compliance, future contracts)
3. **Testing isolation**: RBAC logic cannot be tested independently; requires deploying full `vision_records` contract
4. **Upgrade risk**: Changes to access control logic require upgrading the entire `vision_records` contract (state migration risk)
5. **Code duplication**: `clinical_records` and `compliance` contracts need similar access control → currently either duplicate code or depend on `vision_records` as a library (not ideal for WASM)
6. **Separation of concerns**: Access control is cross-cutting infrastructure, not vision-records domain logic

## Decision

Extract the access control system into a dedicated **`access_control` micro-contract** (`contracts/access_control/`) with:

- **Standalone contract**: Independent Soroban contract with its own state, upgrade path, and upgrade authority
- **Clean public API**: 30+ public functions covering initialization, admin management, whitelist, RBAC, delegation, ACL groups, ABAC policies
- **Event emission**: Structured events for all mutating operations (audit trail)
- **Circuit breaker integration**: Pause scope for emergency stops
- **Comprehensive documentation**: Inline Rust docs on all public types/functions (ADR-0013 covers docs)
- **Test coverage**: 7 unit/integration tests covering all major workflows
- **Documentation**: Inline docs + this ADR + inline docs ADR (ADR-0013)

Migration path:
1. Deploy `access_control` contract independently
2. `vision_records` calls `access_control` via cross-contract calls for all authorization checks
3. `clinical_records` and `compliance` contracts also call `access_control`
4. Inline `rbac.rs` in `vision_records` is deprecated and removed in a follow-up PR

## Rationale

### Alternatives Considered

| Option | Pros | Cons |
|--------|------|------|
| **Keep inline** | Zero refactor, no cross-contract calls | Contract size limit risk, no reuse, upgrade coupling, testing burden |
| **Shared library (Rust crate)** | Compile-time reuse, no cross-contract overhead | WASM size still bloats each contract; upgrade still coupled; no independent upgrade |
| **Extract to micro-contract (chosen)** | Independent upgrade, reuse across contracts, isolation, auditability | Cross-contract call overhead (gas), deployment complexity, state separation |

### Why Micro-Contract Wins

1. **WASM size compliance**: Each contract stays under mainnet limits (~500KB compressed)
2. **Independent upgrades**: Fix RBAC bug → upgrade only `access_control`, not all contracts
3. **Reuse**: `vision_records`, `clinical_records`, `compliance`, future contracts share single source of truth
3. **Auditability**: Access control logic auditable in isolation (critical for medical data compliance)
4. **Testing**: 7 focused tests vs. integration-only tests in monolith
5. **Regulatory**: Medical data standards (HIPAA, GDPR) favor isolated, auditable access control
6. **Future-proof**: New contracts (AI diagnostics, insurance claims, research) get access control for free

### Trade-offs Accepted

- **Cross-contract call gas overhead**: ~2-3x gas per auth check vs. inline. Mitigated by: (a) auth checks are infrequent relative to data ops, (b) Soroban host functions optimize cross-contract calls, (c) can cache results in caller contract if needed
- **Deployment orchestration**: Need to deploy `access_control` first, then configure other contracts. Mitigated by deploy scripts and ADR-0004 upgrade strategy
- **State separation**: Access control state no longer co-located with vision records. Acceptable — access control is cross-cutting

## Consequences

### Positive

- **WASM size**: `vision_records` drops from ~800KB → ~400KB (est.), well under mainnet limit
- **Reuse**: `clinical_records`, `compliance`, future contracts (AI diagnostics, insurance, research) get full RBAC/ABAC for free
- **Independent upgrades**: RBAC bug fix → upgrade only `access_control` (ADR-0004 governs upgrade process)
- **Testing**: 7 focused access control tests run in isolation; CI faster
- **Auditability**: Access control logic auditable in isolation (HIPAA/GDPR aligned)
- **Domain separation**: `vision_records` focuses on vision data, not auth

### Negative / Trade-offs

- **Cross-contract call overhead**: Each `has_permission` check now costs extra gas (~50-100k gas per call). Acceptable for medical data where correctness > gas optimization.
- **Deployment order**: `access_control` must deploy first. Documented in deployment runbooks.
- **Latency**: Extra round-trip for auth checks. Negligible on Soroban (sub-second finality).
- **State fragmentation**: Access control state lives in separate contract. Acceptable — access control is infrastructure, not domain data.

## Implementation Notes

### Files Created

- `contracts/access_control/Cargo.toml` — package config
- `contracts/access_control/src/lib.rs` — main contract entry point (30+ public functions)
- `contracts/access_control/src/types.rs` — all public types (Role, Permission, RoleAssignment, Delegation, ScopedDelegation, AclGroup, AccessPolicy, PolicyConditions, TimeRestriction, CredentialType, SensitivityLevel, AccessControlError)
- `contracts/access_control/src/admin.rs` — initialization, admin handoff (2-step)
- `contracts/access_control/src/whitelist.rs` — optional global allowlist
- `contracts/access_control/src/permissions.rs` — RBAC, delegation, ACL groups, ABAC policy engine
- `contracts/access_control/src/test.rs` — 7 integration tests

### Migration Path (Follow-up PRs)

1. **This ADR + `access_control` contract** (current PR)
2. `vision_records`: replace inline `rbac.rs` calls with cross-contract calls to `access_control` contract
3. `clinical_records`: already uses `access_control` (see ADR-0013 for docs)
4. `compliance`: migrate to `access_control` (follow-up)
5. Deprecate/remove `contracts/vision_records/src/rbac.rs` (follow-up PR)

### Testing

- `cargo test -p access_control` — 7 tests pass (initialize/admin, roles/perms, custom perms, delegation, ACL groups, whitelist, ABAC)
- `cargo doc -p access_control --no-deps` — clean (only pre-existing common crate warning)
- `cargo check -p access_control` — no warnings

### References

- **ADR-0002**: Original access control design (inline RBAC in `vision_records`)
- **ADR-0004**: Upgrade strategy (governs `access_control` upgrades)
- **ADR-0005**: ADR process (this ADR follows the template)
- **ADR-0013**: Inline documentation for `access_control` public API
- **PR #364**: `clinical_records` inline docs (uses `access_control`)
- **PR #365**: `access_control` inline docs (ADR-0013)

---

**Note**: This ADR documents the extraction decision. The actual extraction is implemented in the accompanying PR that creates the `contracts/access_control/` crate.