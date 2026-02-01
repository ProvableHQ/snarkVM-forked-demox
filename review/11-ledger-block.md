# ledger/block - Transition Versioning

This document covers the 20 files changed in `ledger/block`, which implements transition and deployment versioning for dynamic dispatch.

## Overview

The `ledger/block` crate defines blockchain data structures. For dynamic dispatch, this crate:

1. **Introduces Transition V1/V2** with caller metadata
2. **Adds TransitionCallerMetadata** for dynamic call context
3. **Updates serialization** for versioned transitions
4. **Extends Deployment V3** with translation verifying keys

## Transaction ID and Execution ID Computation

### Transition ID vs Inclusion ID

V2 transitions store TWO identifiers. This distinction is essential because:
- The fee must commit to the full transaction identity (including dynamic call context)
- The SNARK circuits remain unchanged (they only verify inclusion_id)

```
                    +------------------------+
                    |      function_tree     |
                    |       (depth 5)        |
                    |   [inputs | outputs]   |
                    +-----------+------------+
                                |
                          function_root
                                |
                    +-----------+-----------+
                    |                       |
              +-----v-----+           +-----v-----+
              | inclusion |           |    id     |
              |    _id    |           | (full ID) |
              +-----------+           +-----------+
              |           |           |           |
              | Hash_BHP( |           | Hash_BHP( |
              |  root,    |           |  root,    |
              |  tcm      |           |  tcm,     |
              | )         |           |  caller_  |
              |           |           |  metadata |
              |           |           | )         |
              +-----------+           +-----------+
                    |                       |
                    v                       v
              Used for:               Used for:
              - SNARK proofs          - Fee binding
              - State paths           - Execution ID
              - Inclusion tree        - Transaction map
```

**V1 Transitions (no caller_metadata):**
```
id = inclusion_id = Hash_BHP512(function_tree.root || tcm)
```

**V2 Transitions (with caller_metadata):**
```
inclusion_id = Hash_BHP512(function_tree.root || tcm)
id           = Hash_BHP512(function_tree.root || tcm || caller_metadata)
```

---

### Execution ID vs Inclusion Tree

Two DIFFERENT Merkle trees exist over the same transitions:

```
EXECUTION ID TREE                    INCLUSION TREE
(for fee binding)                    (for SNARK proofs)

    execution_id                     inclusion_tree_root
         |                                  |
   +-----+-----+                      +-----+-----+
   |     |     |                      |     |     |
t0.id  t1.id  t2.id               t0.incl t1.incl t2.incl
   |     |     |                      |       |       |
  V1    V2    V1                     V1      V2      V1
   |     |     |                      |       |       |
 [same] [diff] [same]               [same]  [same]  [same]

Note: For V1: id == inclusion_id
      For V2: id != inclusion_id (id includes caller_metadata)
```

**Purpose of each tree:**

| Tree | Method | Uses | Purpose |
|------|--------|------|---------|
| Execution ID Tree | `Execution::compute_execution_id` | `transition.id()` | Fee binding - commits to COMPLETE execution identity |
| Inclusion Tree | `Transaction::transitions_tree` | `transition.inclusion_id()` | SNARK inclusion proofs - circuit doesn't know about caller_metadata |

---

### Full Tree Hierarchy

```
Global State Root
      |
+-----+-----+ Block Header Tree (depth 3)
|           |
|     Transactions Root
|           |
|     +-----+-----+ Transactions Tree (depth 20)
|     |           |
|   TX_0        TX_n
|     |
|  Transaction Tree (depth 5)
|     |
+-----+-----+
|           |
Transition  Fee
leaves      leaf
   |
   +-- Each leaf contains:
       - variant (execution=1)
       - index
       - inclusion_id (NOT full id)

Transition (Function) Tree (depth 5)
            |
      +-----+-----+
      |           |
   Inputs      Outputs
   (≤16)       (≤16)
      |           |
   TransitionLeaf for each:
   - variant (Constant/Public/Private/Record/External/Dynamic...)
   - index
   - id (hash, serial_number, commitment, etc.)
```

**Tree Depths:**

| Tree | Depth | Purpose |
|------|-------|---------|
| Transition Tree | 5 | Inputs/outputs within a transition |
| Transaction/Execution Tree | 5 | Transitions within an execution |
| Transactions Tree | 20 | Transactions within a block |

---

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
**Input Variants:** `Constant`, `Public`, `Private`, `Record`, `ExternalRecord`, `DynamicRecord`, `RecordWithDynamicID`, `ExternalRecordWithDynamicID`

The `*WithDynamicID` variants are used for record inputs in dynamic calls:
- `RecordWithDynamicID(serial_number, tag, dynamic_id)` - A record input that was passed dynamically
- `ExternalRecordWithDynamicID(hash, dynamic_id)` - An external record input that was passed dynamically

These variants:
- Use the same transition leaf variant as their non-dynamic counterparts (3 for Record, 4 for ExternalRecord) but with version 2
- Convert to `DynamicRecord` when viewed from the caller's perspective via `to_caller_input()`
- Include a `dynamic_id` field that links to the translation proof

#### `transition/input/bytes.rs`, `transition/input/serialize.rs`
Standard serialization for all 8 input types.

---

#### `transition/output/mod.rs`
**Output Variants:** `Constant`, `Public`, `Private`, `Record`, `ExternalRecord`, `Future`, `DynamicRecord`, `DynamicFuture`, `RecordWithDynamicID`, `ExternalRecordWithDynamicID`

The `*WithDynamicID` variants are used for record outputs in dynamic calls:
- `RecordWithDynamicID(commitment, checksum, sender_ciphertext, dynamic_id)` - A record output from a dynamic call
- `ExternalRecordWithDynamicID(hash, dynamic_id)` - An external record output from a dynamic call

These variants:
- Use the same transition leaf variant as their non-dynamic counterparts but with version 2
- Convert to `DynamicRecord` when viewed from the caller's perspective via `to_caller_output()`
- Include a `dynamic_id` field that links to the translation proof

#### `transition/output/bytes.rs`, `transition/output/serialize.rs`
Standard serialization for all 10 output types.

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
