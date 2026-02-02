# ledger/block - Dynamic Dispatch Support

This document covers the changes in `ledger/block` that implement dynamic dispatch for transitions.

## Overview

The `ledger/block` crate defines blockchain data structures. For dynamic dispatch, this crate:

1. **Extends Input/Output variants** with `*WithDynamicID` types for record translation
2. **Introduces Deployment V3** with translation verifying keys
3. **Updates serialization** for all new variants

## Input/Output Types for Dynamic Dispatch

### Input Variants

```rust
pub enum Input<N: Network> {
    Constant(Field<N>, Option<Plaintext<N>>),
    Public(Field<N>, Option<Plaintext<N>>),
    Private(Field<N>, Option<Ciphertext<N>>),
    Record(Field<N>, Field<N>),                      // (serial_number, tag)
    ExternalRecord(Field<N>),                        // hash
    DynamicRecord(Field<N>),                         // hash
    RecordWithDynamicID(Field<N>, Field<N>, Field<N>),     // (serial_number, tag, dynamic_id)
    ExternalRecordWithDynamicID(Field<N>, Field<N>),       // (hash, dynamic_id)
}
```

### Output Variants

```rust
pub enum Output<N: Network> {
    Constant(Field<N>, Option<Plaintext<N>>),
    Public(Field<N>, Option<Plaintext<N>>),
    Private(Field<N>, Option<Ciphertext<N>>),
    Record(Field<N>, Field<N>, Option<Record<N, Ciphertext<N>>>, Option<Field<N>>),
    ExternalRecord(Field<N>),
    Future(Field<N>, Option<Future<N>>),
    DynamicRecord(Field<N>),
    RecordWithDynamicID(Field<N>, Field<N>, Option<Record<N, Ciphertext<N>>>, Option<Field<N>>, Field<N>),
    ExternalRecordWithDynamicID(Field<N>, Field<N>),
}
```

## Dynamic ID Variants

The `*WithDynamicID` variants are used for record inputs and outputs in dynamic calls:

- **`RecordWithDynamicID`**: A record that was passed to/from a dynamically-called function
- **`ExternalRecordWithDynamicID`**: An external record that was passed to/from a dynamically-called function

These variants:
- Use the **same transition leaf variant** as their non-dynamic counterparts but with **version 2** leaves
- Include a `dynamic_id` field that links to the translation proof
- Convert to `DynamicRecord` when viewed from the caller's perspective via `to_caller_input()`/`to_caller_output()`

### Transition Leaf Versioning

```rust
// RecordWithDynamicID produces leaf with version 2, variant 3, id = sn/cm.
Input::RecordWithDynamicID(sn, ..) => TransitionLeaf::new_dynamic_with_version(index, 3, *sn),

// ExternalRecordWithDynamicID produces leaf with version 2, variant 4, id = hash.
Input::ExternalRecordWithDynamicID(hash, ..) => TransitionLeaf::new_dynamic_with_version(index, 4, *hash),
```

The version 2 leaf allows the verifier to distinguish between static and dynamic record usage in the Merkle tree.

## Transition ID Computation

The transition ID is computed as:

```rust
let id = N::hash_bhp512(&(*function_tree.root(), tcm).to_bits_le())?.into();
```

Where:
- `function_tree` is a Merkle tree over inputs and outputs (depth 5)
- `tcm` is the transition commitment

The `*WithDynamicID` variants affect the transition ID through their leaf representation (version 2 leaves produce different hashes than version 1 leaves).

## Deployment Versioning

### Deployment Versions

- **V1**: No checksum or owner
- **V2**: With checksum and owner
- **V3**: With translation verifying keys (enables dynamic record translation)

```rust
pub(super) enum DeploymentVersion {
    V1,
    V2,
    V3,
}
```

### Translation Verifying Keys

V3 deployments include translation verifying keys for each record type in the program:

```rust
translation_verifying_keys: Option<Vec<(Identifier<N>, (VerifyingKey<N>, Certificate<N>))>>
```

These keys enable the translation proofs that verify record equivalence when records are passed dynamically between programs.

## Caller's View

When a function makes a dynamic call, it sees the callee's inputs/outputs differently:

| Callee's View | Caller's View |
|---------------|---------------|
| `RecordWithDynamicID` | `DynamicRecord(dynamic_id)` |
| `ExternalRecordWithDynamicID` | `DynamicRecord(dynamic_id)` |

This is implemented via `to_caller_input()` and `to_caller_output()` methods.

## Verification

The verification logic in `verify_execution.rs` ensures:

1. Dynamic calls use `*WithDynamicID` variants for record inputs/outputs
2. The `dynamic_id` correctly links to translation proofs
3. Input/output types match the caller's expectations (via `to_caller_input().is_type()` checks)

## Files

### Core Types
- `transition/input/mod.rs` - Input enum with 8 variants
- `transition/output/mod.rs` - Output enum with 9 variants

### Serialization
- `transition/input/bytes.rs`, `transition/input/serialize.rs`
- `transition/output/bytes.rs`, `transition/output/serialize.rs`

### Deployment
- `transaction/deployment/mod.rs` - Deployment struct with V3 support
- `transaction/deployment/bytes.rs`, `transaction/deployment/serialize.rs`

## Security Considerations

1. **Leaf versioning**: Version 2 leaves are cryptographically distinct from version 1 leaves, preventing substitution attacks.

2. **Translation proof binding**: The `dynamic_id` field binds the record to its translation proof.

3. **Type checking**: The `to_caller_input().is_type()` check ensures that dynamic calls use the correct `*WithDynamicID` variants.
