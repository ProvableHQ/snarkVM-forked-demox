# PR #3062: Dynamic Dispatch Code Review

This folder contains detailed code review documentation for the Dynamic Dispatch feature (ARC-0009) implementation.

## PR Overview

| Attribute | Value |
|-----------|-------|
| **PR** | [#3062](https://github.com/ProvableHQ/snarkVM/pull/3062) |
| **Branch** | `feat/dynamic-dispatch` |
| **Target** | `staging` |
| **Files Changed** | 273 |
| **Additions** | ~25,500 |
| **Deletions** | ~1,900 |
| **Consensus Version** | V14 |
| **Depends On** | V13 (External Structs) |

## Feature Summary

Dynamic Dispatch enables runtime function dispatch through dynamic records and futures represented as fixed-size Merkle commitments. This allows programs to call other program functions without compile-time knowledge of targets.

### Core Concepts

**DynamicRecord**: A fixed-size representation of records using a Merkle root (depth-5 tree, max 32 entries) instead of inline data. Enables records to be passed between programs without revealing structure at compile time.

**DynamicFuture**: A fixed-size representation of futures using a Merkle root (depth-4 tree, max 16 arguments) instead of inline arguments. Enables futures to be dynamically constructed and passed.

**Translation Proofs**: Cryptographic proofs verifying that dynamic and static representations are equivalent, ensuring correctness when converting between formats.

### New Instructions

| Instruction | Purpose |
|-------------|---------|
| `call.dynamic` | Runtime function dispatch with automatic record conversion |
| `get.record.dynamic` | Extract entries from dynamic records with Merkle verification |

### New Commands (Finalize)

| Command | Purpose |
|---------|---------|
| `contains.dynamic` | Check key existence in external mapping at runtime |
| `get.dynamic` | Retrieve value from external mapping at runtime |
| `get.or_use.dynamic` | Retrieve value with fallback default at runtime |

### Versioning

- **Request V2**: Adds `dynamic: Option<bool>` flag
- **Transition V2**: Adds `TransitionCallerMetadata` for caller's view of inputs/outputs
- **Input/Output**: New `DynamicRecord` variants

## Documentation Structure

Documents are ordered by logical dependency (foundational types first, then circuits, then higher-level synthesis and ledger).

### Foundation Layer (Console)

| Document | Files | Description |
|----------|-------|-------------|
| [01-console-program.md](./01-console-program.md) | 56 | Core data types: DynamicRecord, DynamicFuture, Value, Request |
| [02-console-network.md](./02-console-network.md) | 6 | Network constants, consensus heights, V14 activation |
| [03-console-collections.md](./03-console-collections.md) | 5 | Merkle tree utilities for dynamic data |

### Circuit Layer

| Document | Files | Description |
|----------|-------|-------------|
| [04-circuit-program.md](./04-circuit-program.md) | 39 | Circuit implementations of dynamic types |
| [05-circuit-types.md](./05-circuit-types.md) | 7 | Integer/Boolean circuit changes |
| [06-circuit-collections.md](./06-circuit-collections.md) | 3 | Circuit Merkle tree verification |

### Algorithm Layer

| Document | Files | Description |
|----------|-------|-------------|
| [07-algorithms.md](./07-algorithms.md) | 6 | Varuna SNARK prover changes |

### Synthesizer Layer

| Document | Files | Description |
|----------|-------|-------------|
| [08-synthesizer-program.md](./08-synthesizer-program.md) | 24 | Instruction/command definitions |
| [09-synthesizer-process.md](./09-synthesizer-process.md) | 46 | Execution logic, call handling, translation |
| [10-synthesizer-core.md](./10-synthesizer-core.md) | 17 | VM integration, restrictions |

### Ledger Layer

| Document | Files | Description |
|----------|-------|-------------|
| [11-ledger-block.md](./11-ledger-block.md) | 20 | Transition/Request versioning, serialization |
| [12-ledger-store.md](./12-ledger-store.md) | 8 | Storage layer changes |

### Supporting

| Document | Files | Description |
|----------|-------|-------------|
| [13-parameters.md](./13-parameters.md) | 12 | Translation proof parameters |
| [14-testing.md](./14-testing.md) | 8+ | Comprehensive test coverage |
| [15-misc.md](./15-misc.md) | 5 | CI, Cargo, package changes |

## Review Guidance

Each crate document contains:

1. **Overview** - Role in dynamic dispatch
2. **Files Requiring Review** - Production code with explanations
3. **Test Files** - Test code (review for coverage, not correctness)
4. **Testing Notes** - What is tested and how
5. **Security Considerations** - Any security-relevant changes

### Priority for Review

**High Priority** (core logic):
- `console/program` - Foundation types
- `circuit/program` - Circuit constraints
- `synthesizer/process` - Execution logic
- `ledger/block` - Versioning/serialization

**Medium Priority** (supporting):
- `synthesizer/program` - Instruction definitions
- `algorithms` - Prover changes

**Lower Priority** (standard patterns):
- `ledger/store` - Storage (follows existing patterns)
- `parameters` - Generated proof data

## Key Architectural Decisions

### Merkle Tree Depths

| Structure | Depth | Max Entries | Rationale |
|-----------|-------|-------------|-----------|
| DynamicRecord | 5 | 32 | Matches `MAX_DATA_ENTRIES` |
| DynamicFuture | 4 | 16 | Matches `MAX_INPUTS` |

### Hashing Strategy

- **Leaf Hash**: Poseidon8 (8-to-1 compression)
- **Path Hash**: Poseidon2 (2-to-1 compression)
- **Deterministic**: Same data always produces same root

### Version Detection

```
Request V1:  dynamic == None
Request V2:  dynamic == Some(_)

Transition V1:  caller_metadata == None
Transition V2:  caller_metadata == Some(_)
```

### Consensus Gating

V14 features activate at network-specific block heights:
- Testnet/Canary: `999_999_999` (placeholder for testing)
- Mainnet: TBD

## Files Changed by Crate

```
console/program      56 files
synthesizer/process  46 files
circuit/program      39 files
synthesizer/program  24 files
ledger/block         20 files
synthesizer/src      17 files
parameters/src        9 files
synthesizer/tests     8 files
ledger/store          8 files
circuit/types         7 files
console/network       6 files
console/collections   5 files
algorithms/src        5 files
misc (CI, Cargo, vm)  5 files
```
