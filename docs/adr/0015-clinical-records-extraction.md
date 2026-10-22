# ADR-0015: Clinical Records Extraction from Vision Records

- **Status**: Accepted
- **Date**: 2026-10-20

## Context

The `vision_records` contract originally contained all clinical data management inline:
- Eye examinations (visual acuity, IOP, slit-lamp, visual field, retinal imaging, fundus photography)
- Prescriptions (glasses, contact lenses with full optical parameters)
- Patient profiles, appointments, insurance, consent management
- Inline RBAC/ABAC access control (`rbac.rs`, ~1,465 lines)

This monolithic approach created several problems:

1. **Contract size**: `vision_records` exceeded recommended WASM size limits for Stellar mainnet deployment
2. **Domain mixing**: Vision data (examinations, prescriptions) conflated with patient identity, appointments, insurance — different domains with different upgrade cycles
3. **Coupled upgrades**: Fixing a prescription bug required upgrading the entire `vision_records` contract (state migration risk, downtime)
4. **No reuse**: Other contracts (`clinical_records`, `compliance`, future AI diagnostics) needed similar clinical data structures but had to duplicate or depend on `vision_records`
5. **Testing burden**: Clinical data logic tested only through full `vision_records` integration tests
6. **Domain separation**: Clinical records are cross-cutting healthcare data, not vision-specific

## Decision

Extract clinical eye examination and prescription data into a dedicated **`clinical_records` micro-contract** (`contracts/clinical_records/`) with:

- **Standalone contract**: Independent Soroban contract with own state, upgrade path
- **Clean public API**: 13 public functions (initialize, admin, examinations, prescriptions)
- **Cross-contract authorization**: Uses `access_control::AccessControlContractClient` for all auth checks
- **Event emission**: Structured events for all mutating operations
- **Circuit breaker integration**: Pause scope for emergency stops
- **Comprehensive documentation**: Inline Rust docs on all public types/functions (ADR-0013)
- **Test coverage**: 14 tests (3 unit + 11 integration)

Migration path:
1. Deploy `clinical_records` independently
2. `vision_records` calls `clinical_records` via cross-contract calls for clinical data
3. `clinical_records` calls `access_control` for all authorization
4. Inline clinical data types in `vision_records` deprecated and removed (follow-up PR)

## Rationale

### Alternatives Considered

| Option | Pros | Cons |
|--------|------|------|
| **Keep inline** | Zero refactor, no cross-contract calls | Contract size limit risk, no reuse, upgrade coupling, testing burden |
| **Shared library (Rust crate)** | Compile-time reuse, no cross-contract overhead | WASM size still bloats each contract; upgrade still coupled; no independent upgrade |
| **Extract to micro-contract (chosen)** | Independent upgrade, reuse across contracts, isolation, auditability | Cross-contract call overhead (gas), deployment complexity, state separation |

### Why Micro-Contract Wins

1. **WASM size compliance**: Each contract stays under mainnet limits (~500KB compressed)
2. **Independent upgrades**: Fix clinical data bug → upgrade only `clinical_records`, not `vision_records`
3. **Reuse**: `vision_records`, `compliance`, future contracts (AI diagnostics, insurance, research) share clinical data layer
4. **Testing**: 14 focused clinical data tests run in isolation; CI faster
5. **Auditability**: Clinical data logic auditable in isolation (HIPAA/GDPR aligned)
6. **Future-proof**: New contracts (AI diagnostics, insurance claims, research) get clinical data layer for free

### Trade-offs Accepted

- **Cross-contract call gas overhead**: ~2-3x gas per clinical data op vs. inline. Mitigated by: (a) clinical ops are infrequent relative to reads, (b) Soroban host functions optimize cross-contract calls
- **Deployment orchestration**: Need to deploy `clinical_records` first, then configure `vision_records`. Mitigated by deploy scripts and ADR-0004 upgrade strategy
- **Latency**: Extra round-trip for clinical data ops. Negligible on Soroban (sub-second finality)
- **State separation**: Clinical data state lives in separate contract. Acceptable — clinical data is cross-cutting

## Consequences

### Positive

- **WASM size**: `vision_records` drops from ~800KB → ~400KB (est.), well under mainnet limit
- **Reuse**: `compliance`, future contracts (AI diagnostics, insurance, research) get clinical data layer for free
- **Independent upgrades**: Clinical data bug fix → upgrade only `clinical_records` (ADR-0004 governs upgrade process)
- **Testing**: 14 focused clinical data tests run in isolation; CI faster
- **Auditability**: Clinical data logic auditable in isolation (HIPAA/GDPR aligned)
- **Domain separation**: `vision_records` focuses on vision-specific workflows, not clinical data structures

### Negative / Trade-offs

- **Cross-contract call overhead**: Each clinical data op costs extra gas (~50-100k). Acceptable for medical data where correctness > gas optimization
- **Deployment order**: `clinical_records` must deploy first. Documented in deployment runbooks
- **Latency**: Extra round-trip for clinical data ops. Negligible on Soroban (sub-second finality)
- **State separation**: Clinical data state lives in separate contract. Acceptable — clinical data is cross-cutting

## Implementation Notes

### Files Created

- `contracts/clinical_records/Cargo.toml` — package config
- `contracts/clinical_records/src/lib.rs` — main contract entry point (13 public functions)
- `contracts/clinical_records/src/types.rs` — all public types (VisualAcuity, IOP, SlitLampFindings, VisualField, RetinalImaging, FundusPhotography, EyeExamination, Prescription, PrescriptionData, ContactLensData, LensType)
- `contracts/clinical_records/src/admin.rs` — initialization, admin handoff (2-step), Vision Records contract coordination
- `contracts/clinical_records/src/examination.rs` — eye examination CRUD with validation
- `contracts/clinical_records/src/prescription.rs` — 2-phase commit prescription workflow (prepare → commit/rollback)
- `contracts/clinical_records/src/errors.rs` — 8 error codes
- `contracts/clinical_records/src/test.rs` — 14 integration tests

### Migration Path (Follow-up PRs)

1. **This ADR + `clinical_records` contract** (current state — already deployed)
2. `vision_records`: replace inline clinical types with cross-contract calls to `clinical_records` contract
3. `compliance`: migrate to `clinical_records` (follow-up)
4. Deprecate/remove inline clinical types from `vision_records` (follow-up PR)

### Testing

- `cargo test -p clinical_records` — 14 tests pass (3 unit + 11 integration)
- `cargo doc -p clinical_records --no-deps` — clean
- `cargo check -p clinical_records` — no warnings

### References

- **ADR-0002**: Original access control design (inline RBAC in `vision_records`)
- **ADR-0004**: Upgrade strategy (governs `clinical_records` upgrades)
- **ADR-0005**: ADR process (this ADR follows the template)
- **ADR-0013**: Inline documentation for `clinical_records` public API (PR #364)
- **ADR-0014**: `access_control` micro-contract extraction (PR #366)
- **PR #364**: `clinical_records` inline docs
- **PR #366**: `access_control` extraction ADR