# ledger/block - Transition Versioning

This document covers the 20 files changed in `ledger/block`, which implements transition and deployment versioning for dynamic dispatch.

## Overview

The `ledger/block` crate defines blockchain data structures. For dynamic dispatch, this crate:

1. **Introduces Transition V1/V2** with caller metadata
2. **Adds TransitionCallerMetadata** for dynamic call context
3. **Updates serialization** for versioned transitions
4. **Extends Deployment V3** with translation verifying keys

## Files Requiring Review

### Core Versioning: `transition/mod.rs`
**Purpose:** Transition struct definition with versioning. **Highest priority for review.**

**New Types:**

```rust
pub enum TransitionVersion {
    V1 = 1,  // Without caller metadata
    V2 = 2,  // With caller metadata
}

pub struct TransitionCallerMetadata<N: Network> {
    is_dynamic: bool,
    inputs: Vec<Input<N>>,   // Caller's view of inputs
    outputs: Vec<Output<N>>, // Caller's view of outputs
}
```

**Key Methods:**
- `TransitionCallerMetadata::new_static()` - V2 static transition
- `TransitionCallerMetadata::new_dynamic(inputs, outputs)` - V2 dynamic transition
- `TransitionCallerMetadata::inputs()` - Returns `Option<&[Input<N>]>` (Some only if dynamic)
- `TransitionCallerMetadata::outputs()` - Returns `Option<&[Output<N>]>` (Some only if dynamic)

**Transition Structure:**
- New field: `caller_metadata: Option<TransitionCallerMetadata<N>>`
- `version()` - Determines V1/V2 based on metadata presence
- `is_dynamic()` - Checks if metadata exists and is marked dynamic
- `caller_inputs()`, `caller_outputs()` - Access caller's view

**Transition Construction (from Request/Response):**
- For dynamic transitions: constructs caller inputs/outputs
- Converts Record/ExternalRecord outputs to DynamicRecord for caller view
- Handles special conversion logic in `Transition::from()`

---

### Serialization Files

#### `transition/bytes.rs`
**Purpose:** Binary serialization.

**Format:**
- Version byte (1 or 2)
- V1: No caller metadata
- V2: After main data, serializes:
  - `is_dynamic: bool`
  - If dynamic: caller inputs and outputs

**Tests:** `test_bytes_dynamic()` verifies V2 round-trip.

---

#### `transition/serialize.rs`
**Purpose:** JSON serialization.

**Field Counts:**
- No caller metadata: 7 fields
- Static caller metadata: 10 fields
- Dynamic caller metadata: 8 fields

**Fields:**
- Always: `id`, `program`, `function`, `inputs`, `outputs`, `tpk`, `tcm`, `scm`
- Conditional: `is_dynamic`, `caller_inputs`, `caller_outputs`

---

### Input/Output Types

#### `transition/input/mod.rs`
**Input Variants:** `Constant`, `Public`, `Private`, `Record`, `ExternalRecord`, `DynamicRecord`

No versioning changes - used by both transition and caller metadata.

#### `transition/input/bytes.rs`, `transition/input/serialize.rs`
Standard serialization for all 6 input types.

---

#### `transition/output/mod.rs`
**Output Variants:** `Constant`, `Public`, `Private`, `Record`, `ExternalRecord`, `Future`, `DynamicRecord`

No versioning changes - used by both transition and caller metadata.

#### `transition/output/bytes.rs`, `transition/output/serialize.rs`
Standard serialization for all 7 output types.

---

### Transaction Files

#### `transaction/mod.rs`
Contains `Transaction` enum (`Deploy`, `Execute`, `Fee`). Delegates to Transition for handling.

#### `transaction/bytes.rs`, `transaction/serialize.rs`
Transaction serialization. No transition versioning impact.

#### `transaction/merkle.rs`
Merkle tree construction for transactions.

---

### Deployment Versioning

#### `transaction/deployment/mod.rs`
**Purpose:** Introduces DeploymentVersion (separate from transition versioning).

**Versions:**
- V1: No checksum/owner
- V2: With checksum/owner
- V3: With translation verifying keys

Translation verifying keys enable record translation proofs.

#### `transaction/deployment/bytes.rs`, `transaction/deployment/serialize.rs`
Deployment serialization with version-specific fields.

---

### Fee Constraint

#### `transaction/fee/mod.rs`
**Purpose:** Fee transition handling. **Important constraint.**

**Enforces:**
```rust
ensure!(!transition.is_dynamic(), "Fee transition cannot be dynamic");
```

Fee transitions are always V1 (no caller metadata).

---

### Other Files

#### `lib.rs`
Block structure accessors.

#### `transactions/confirmed/mod.rs`
ConfirmedTransaction enum for accepted/rejected.

---

## Versioning Architecture

**Transition Versioning:**
- **V1:** Legacy, no caller metadata, no dynamic dispatch
- **V2:** Extended format with optional `caller_metadata`
  - `is_dynamic = false`: Static transition
  - `is_dynamic = true`: Dynamic transition

**Backward Compatibility:**
- V1 transitions deserialize with `caller_metadata = None`
- V2 always has caller metadata (static or dynamic)
- Binary format uses version byte
- JSON handles missing `is_dynamic` field gracefully

---

## Testing Notes

**What's Tested:**
- Binary round-trip for V1 and V2 transitions
- JSON serialization with optional fields
- Caller metadata construction and access

---

## Security Considerations

1. **Fee Protection:** Fee transitions cannot be dynamic (explicit check).

2. **Version Detection:** Presence of caller_metadata determines V1 vs V2.

3. **Caller View Separation:** Dynamic calls store separate caller_inputs/caller_outputs.

4. **Deployment V3:** Translation keys enable secure record translation proofs.
