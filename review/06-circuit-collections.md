# circuit/collections - Merkle Tree Circuits

This document covers the 3 files changed in `circuit/collections` plus 3 files in `circuit/environment`.

## Overview

The `circuit/collections` crate provides circuit implementations of data structures. The `circuit/environment` crate provides the constraint system infrastructure. Key additions include a generic `MerkleTree` struct for circuit-based tree construction.

## Files Requiring Review

### circuit/collections

#### `src/merkle_tree/verify.rs`
**Purpose:** Merkle path verification circuit.

**Changes:** No enforcement/assertion changes.

**Key Constraints:**
- Bit-by-bit leaf index processing
- Conditional ternary operations for path traversal
- Leaf hash computation and validation

**Type:** Test code (all in `#[cfg(test)]` module)

---

#### `src/merkle_tree/mod.rs`
**Purpose:** Main Merkle tree circuit implementation. **Significant new functionality.**

**New Struct:**
```rust
pub struct MerkleTree<E: Environment, LH: LeafHash, PH: PathHash, const DEPTH: u8> {
    leaf_hasher: LH,
    path_hasher: PH,
    root: PH::Hash,
    tree: Vec<PH::Hash>,
    empty_hash: Field<E>,
    number_of_leaves: usize,
}
```

**Key Methods:**
- `new(leaf_hasher, path_hasher, leaves)` - Constructs circuit Merkle tree from leaves
- `root()` - Returns computed root
- `tree()` - Returns internal hashes
- `empty_hash()` - Returns canonical empty hash
- `number_of_leaves()` - Returns leaf count

**Circuit Behavior:**
- Tree construction adds constraints for each hash operation
- Padding with empty hashes for non-power-of-two leaf counts
- Depth padding from tree depth up to `DEPTH`

**Connection to Dynamic Dispatch:**
This struct enables `DynamicRecord::merkleize_data()` and `DynamicFuture` circuit merkleization.

---

#### `src/kary_merkle_tree/verify.rs`
**Purpose:** K-ary Merkle tree verification (variable branching factor).

**Changes:** No direct changes.

---

### circuit/environment

#### `Cargo.toml`
**Purpose:** Package manifest.

**Changes:** Added error type dependency support.

---

#### `src/helpers/assignment.rs`
**Purpose:** R1CS assignment generation for constraint synthesis. **New struct added.**

**New Struct:**
```rust
pub struct Assignment<F: PrimeField> {
    public: Vec<F>,
    private: Vec<F>,
}
```

**Key Methods:**
- `new(num_public, num_private)` - Allocates assignment with given sizes
- `public()` / `private()` - Returns public/private variable slices
- `public_mut()` / `private_mut()` - Returns mutable slices

**Usage:**
Used for constraint system variable assignment during circuit synthesis, supporting the new error handling from `E::enforce`.

---

#### `src/helpers/updatable_count.rs`
**Purpose:** Test metric tracking for constraint counting.

**Changes:** No changes (pre-existing infrastructure).

**Features:**
- Automatic test count updates via `UPDATE_COUNT` env var
- File-based metric storage

---

## Test Files

- `merkle_tree/verify.rs` - Comprehensive tests for BHP512 and Poseidon2 hashers
- `kary_merkle_tree/verify.rs` - Tests for BHP512, Poseidon2, Keccak256, Sha3_256

---

## Testing Notes

**What's Tested:**
- Merkle path verification correctness
- Multiple hash algorithm support
- Constraint counts for verification

---

## Security Considerations

1. **Merkle Verification:** The circuit correctly enforces path validity through constraint system.

2. **Hash Algorithm Flexibility:** Supports multiple hash algorithms for different security/performance tradeoffs.

---

## Connection to Dynamic Dispatch

These Merkle tree circuits are the foundation for:
- `DynamicRecord` Merkle root verification (depth-5 tree)
- `DynamicFuture` argument root verification (depth-4 tree)

The actual dynamic type Merkle trees are constructed using `Poseidon8` (leaf hash) and `Poseidon2` (path hash) as defined in `circuit/program/src/data/dynamic/record/mod.rs`.
