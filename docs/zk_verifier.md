# ZK Verifier: Batch Proof Verification

## Overview

The `zk_verifier` contract exposes `verify_batch`, which verifies several zero-knowledge proofs in a single call in order to amortise the fixed per-call cost (transaction overhead, public input setup, and pairing-check finalisation).

## API

### `verify_batch(proofs, public_inputs) -> bool`

- `proofs`: a list of proofs, length `n`, where `1 <= n <= MAX_BATCH_SIZE`.
- `public_inputs`: a list of public input vectors, one per proof, with the same length `n`.
- Returns `true` only if every proof in the batch is valid. Returns `false` if any proof is invalid.
- Reverts if `n` is zero, if `n` exceeds `MAX_BATCH_SIZE`, or if the two input lists differ in length.

`MAX_BATCH_SIZE` is a contract constant. Its value bounds the gas cost of a single call and is documented alongside the constant in the source.

## Verification strategy

Two strategies are available. The implementation uses the random linear combination (RLC) approach.

### Random linear combination (used)

Each proof's verification equation is combined with a random scalar drawn from a transcript hash over all proofs and public inputs. A single multi-pairing check then covers the whole batch.

- Cost: one multi-pairing check plus `O(n)` scalar multiplications, instead of `n` independent pairing checks.
- Soundness: a batch containing an invalid proof passes with negligible probability, provided the scalars are derived from a binding transcript (Fiat-Shamir) rather than chosen by the caller.
- Trade-off: slightly more complex code and a dependency on a correctly bound transcript.

### Naive loop (fallback)

Each proof is verified independently with the single-proof routine and the results are AND-ed.

- Cost: `n` full verifications; no amortisation.
- Soundness: trivially correct, with no transcript assumptions.
- Trade-off: simple, but gives no performance benefit. It is kept as a reference for testing and benchmarking.

## Benchmarks

Benchmarks compare `verify_batch` against `n` calls to the single-proof `verify`. Measure gas (or wall-clock time in off-chain tests) at several batch sizes, for example 1, 4, 16, and `MAX_BATCH_SIZE`. Record results in the CHANGELOG entry for the release that introduces batching. Expect the RLC path to beat the naive loop from batch size 2 upward, with the gap widening as `n` grows.

Run the benchmark suite with the project's standard test command and include the output table in the PR description.

## Testing

Unit tests cover:

- A batch of all-valid proofs returns `true`.
- A batch containing one invalid proof returns `false`, for each position in the batch.
- Empty batches revert.
- Batches larger than `MAX_BATCH_SIZE` revert.
- Mismatched proof and public input lengths revert.
- Batch results agree with the naive loop on randomised inputs.

## Security notes

- Random scalars must be derived from a transcript that commits to every proof and public input in the batch. Callers must not supply the scalars.
- Do not reuse a batch's scalars across calls.
- `MAX_BATCH_SIZE` limits denial-of-service exposure from oversized calls.

## Contributing

Per `CONTRIBUTING.md`, this feature should not be implemented until a maintainer has confirmed the issue assignment on the GitHub issue thread.
