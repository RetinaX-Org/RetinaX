# Audit Data Types

Domain types for the `audit` crate (`contracts/audit`). These types back the tamper-evident log: a hash chain per segment, a Merkle tree over those entries, and optional witness co-signatures.

On-chain storage for the Soroban `AuditContract` uses a smaller set of contract types, listed at the end of this page. Access-log types on `vision_records` (`AuditEntry`, `AccessAction`, `AccessResult`) are documented in [Access Audit Logging](../audit-logging.md).

## Digest

```rust
pub type Digest = [u8; 32];
```

A SHA-256 digest. Used for `prev_hash`, `entry_hash`, and Merkle node hashes. Fixed length so hashing never allocates a digest buffer.

`MerkleRoot` in `merkle_log` is the same 32-byte digest.

## LogSegmentId

```rust
pub struct LogSegmentId(pub(crate) [u8; 64], pub(crate) usize);
```

Logical partition of the log. A segment is an ASCII label of 1 to 64 bytes, stored in a fixed buffer plus a used-length. Segments split the log by contract, tenant, or sensitivity.

| Method | Behavior |
|---|---|
| `LogSegmentId::new(label)` | Builds an id. Returns `AuditError::InvalidSegmentId` when `label` is empty or longer than 64 bytes. |
| `as_bytes()` | Label bytes, excluding padding. |
| `as_str()` | Label as `&str`. |

## LogEntry

```rust
pub struct LogEntry {
    pub sequence: u64,
    pub timestamp: u64,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub result: String,
    pub prev_hash: Digest,
    pub entry_hash: Digest,
    pub segment: LogSegmentId,
}
```

One immutable audit record.

| Field | Meaning |
|---|---|
| `sequence` | Monotonic sequence number inside the segment. First entry is `1`. |
| `timestamp` | Caller-supplied Unix time in seconds. Must be greater than or equal to the previous entry in the segment. |
| `actor` | Who initiated the action (address or user id). |
| `action` | Action label, for example `record.create` or `access.grant`. |
| `target` | Resource the action applied to. |
| `result` | Outcome, for example `ok` or `denied`. |
| `prev_hash` | SHA-256 of the previous entry. The first entry uses `[0u8; 32]`. |
| `entry_hash` | SHA-256 of this entry, set at construction. |
| `segment` | Segment the entry belongs to. |

`canonical_bytes()` is the byte string that is hashed:

```text
sequence(8 LE) ‖ timestamp(8 LE) ‖ actor ‖ 0x00 ‖ action ‖ 0x00
 ‖ target ‖ 0x00 ‖ result ‖ 0x00 ‖ prev_hash(32) ‖ segment_label
```

No field may contain a null byte, because `0x00` separates the string fields. Changing any earlier entry changes every later `prev_hash`.

## WitnessSignature

```rust
pub struct WitnessSignature {
    pub witness_id: String,
    pub root: MerkleRoot,
    pub tree_size: u64,
    pub signed_at: u64,
    pub signature: Vec<u8>,
}
```

A co-signature over a Merkle root at a given log size. Several witnesses endorsing the same `(root, tree_size)` means an attacker must compromise that threshold to publish a forged history. `signature` is stored as opaque bytes; the format is defined by the witness, not by this crate.

## RetentionPolicy

```rust
pub struct RetentionPolicy {
    pub segment: LogSegmentId,
    pub min_retention_secs: u64,
    pub requires_witness_for_deletion: bool,
}
```

How long entries in a segment must be kept.

| Field | Meaning |
|---|---|
| `segment` | Segment the policy applies to. |
| `min_retention_secs` | Minimum seconds an entry is retained before deletion. |
| `requires_witness_for_deletion` | When true, compaction needs witness co-signatures. |

Compaction returns a receipt (below) so an auditor can see which entries were removed and that the remaining log still matches.

## AuditError

Closed error enum. Callers can match every variant.

| Variant | When it is returned |
|---|---|
| `HashChainBroken { at_sequence }` | `prev_hash` does not match the previous entry. |
| `InvalidInclusionProof` | A Merkle inclusion proof does not reconstruct the root. |
| `InvalidConsistencyProof` | A consistency proof between two roots is malformed. |
| `InvalidSearchToken` | A search token or search key is the wrong length. |
| `EntryNotFound { sequence }` | No entry at that sequence. |
| `InvalidSegmentId` | Segment label is empty or longer than 64 bytes. |
| `InsufficientWitnesses { required, present }` | Compaction does not have enough witness signatures. |
| `RetentionPolicyViolation { sequence, retained_until }` | The entry is still inside its retention window. |
| `RootMismatch` | A computed Merkle root does not match the claimed root. |
| `InternalError(&'static str)` | An internal invariant failed. |
| `SegmentNotFound` | The named segment does not exist. |
| `SearchKeyNotSet` | Keyword search was requested before a search key was set. |
| `OutOfOrderTimestamp { sequence, supplied, minimum }` | `supplied` is earlier than `minimum` (the previous entry's timestamp). |

## Merkle types

### InclusionProof

```rust
pub struct InclusionProof {
    pub leaf_index: u64,
    pub tree_size: u64,
    pub leaf_hash: Digest,
    pub siblings: Vec<Digest>,
}
```

Proves that `leaf_hash` is leaf `leaf_index` in a tree of `tree_size` leaves. `siblings` runs from the leaf toward the root. `verify(root)` rebuilds the root in O(log n).

### RootCheckpoint

```rust
pub struct RootCheckpoint {
    pub tree_size: u64,
    pub root: MerkleRoot,
    pub published_at: u64,
    pub endorsements: Vec<WitnessSignature>,
}
```

A published root. Consistency proofs are generated between checkpoints.

### CompactionReceipt

```rust
pub struct CompactionReceipt {
    pub old_root: MerkleRoot,
    pub old_size: u64,
    pub new_root: MerkleRoot,
    pub new_size: u64,
    pub deleted_hashes: Vec<Digest>,
    pub compacted_at: u64,
}
```

Proof of a deletion. `deleted_hashes` lists removed leaves in original sequence order. `old_root` / `new_root` let an auditor check that the surviving log is consistent.

### MerkleLog

```rust
pub struct MerkleLog {
    pub segment: LogSegmentId,
    // entries, leaf hashes, checkpoints, and witnesses are private
}
```

Append-only log for one segment. Entries are keyed by sequence number. Leaf index `i` is sequence `i + 1`.

## Consistency types

### ConsistencyProof

```rust
pub struct ConsistencyProof {
    pub size_v1: u64,
    pub size_v2: u64,
    pub root_v1: MerkleRoot,
    pub root_v2: MerkleRoot,
    pub proof_hashes: Vec<Digest>,
}
```

RFC 6962 consistency proof between an older snapshot (`size_v1`, `root_v1`) and a newer one (`size_v2`, `root_v2`). `proof_hashes` is at most `2·⌊log₂(size_v2)⌋ + 2` hashes. `verify()` rebuilds both roots.

## Search types

Search uses searchable symmetric encryption (SSE-1). The index stores tokens, not plaintext keywords.

### SearchToken

```rust
pub type SearchToken = [u8; 32];
```

HMAC-SHA256 of a keyword under a `SearchKey`.

### SearchKey

```rust
pub struct SearchKey([u8; 32]);
```

32-byte symmetric key. `SearchKey::from_bytes` returns `AuditError::InvalidSearchToken` unless the slice is exactly 32 bytes. `token_for(keyword)` returns `HMAC-SHA256(key, keyword)`.

### ForwardIndex

Maps each `SearchToken` to a sorted list of sequence numbers. Callers pass tokens in; the index never stores raw keywords.

### SearchEngine

Holds a `SearchKey` and a `ForwardIndex`. `index_entry` tokenizes `actor`, `action`, `target`, and `result` (plus any extra keywords) and inserts the sequence. `query` returns the matching sequence numbers.

## Contract types

These are the Soroban types stored by `AuditContract` (`contracts/audit/src/contract.rs`). They are the on-chain view of a segment, not the off-chain `LogEntry` above.

### AuditLogEntry

```rust
pub struct AuditLogEntry {
    pub sequence: u64,
    pub timestamp: u64,
    pub actor: Address,
    pub action: Symbol,
    pub target: Symbol,
    pub result: Symbol,
}
```

| Field | Meaning |
|---|---|
| `sequence` | Sequence inside the segment. The first appended entry is `1`. |
| `timestamp` | Ledger timestamp at append time. |
| `actor` | Stellar address that performed the action. |
| `action` | Short symbol naming the action. |
| `target` | Short symbol naming the resource. |
| `result` | Short symbol naming the outcome. |

Symbols are Soroban short symbols (up to 32 characters). Longer labels belong on the off-chain `LogEntry` strings.

### SegmentInfo

```rust
pub struct SegmentInfo {
    pub entries: Vec<AuditLogEntry>,
    pub next_sequence: u64,
}
```

One segment's stored entries and the sequence number that the next `append_entry` will use. A new segment starts at `next_sequence = 1` with an empty `entries` vector.

### AuditContractError

| Code | Variant | Meaning |
|---|---|---|
| 1 | `AlreadyInitialized` | `initialize` was called twice. |
| 2 | `SegmentNotFound` | The segment symbol is not stored. |
| 3 | `SegmentAlreadyExists` | `create_segment` was called for an existing symbol. |
| 4 | `IdentityCheckFailed` | The cross-contract identity check returned false. |
| 5 | `VaultBalanceInsufficient` | The vault balance check returned a negative balance. |
| 6 | `ComplianceCheckFailed` | The compliance check returned false. |
