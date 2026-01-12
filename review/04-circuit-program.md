# circuit/program - Circuit Data Types

This document covers the 39 files changed in `circuit/program`, which implements circuit (constraint system) versions of the console data types.

## Overview

The `circuit/program` crate provides circuit implementations of Aleo program types for ZK proof generation. For dynamic dispatch, this crate:

1. Implements **DynamicRecord** and **DynamicFuture** in circuit form
2. Extends **Value** enum with dynamic variants
3. Adds **Request V2** circuit verification
4. Implements **DynamicRecord/DynamicFuture** output ID computation

## Files Requiring Review

### New Module: data/dynamic/ (11 files)

These files implement circuit versions of dynamic types. **High priority for review.**

#### `data/dynamic/mod.rs`
**Purpose:** Module root defining circuit-level DynamicRecord with Merkle tree infrastructure.

**Key Types:**
```rust
type CircuitLH<A> = Poseidon8<A>;  // Leaf hasher
type CircuitPH<A> = Poseidon2<A>;  // Path hasher
type RecordDataTree<A> = MerkleTree<A, CircuitLH<A>, CircuitPH<A>, RECORD_DATA_TREE_DEPTH>;
```

**Key Function:**
- `merkleize_data()` - Constructs Merkle tree from ordered entry data during circuit execution

---

#### `data/dynamic/record/mod.rs`
**Purpose:** DynamicRecord circuit struct.

**Structure:**
```rust
pub struct DynamicRecord<A: Aleo> {
    owner: Address<A>,
    root: Field<A>,
    nonce: Group<A>,
    version: U8<A>,
    data: Option<console::RecordData>,  // Non-circuit console data
}
```

**Circuit Behavior:**
- `Inject` impl: All fields injected as `Mode::Private` (witnesses)
- `Eject` impl: Converts back to console type
- Mode combination via `Mode::combine()` for all 4 fields

---

#### `data/dynamic/record/equal.rs`
**Purpose:** Equality comparison circuit.

**Constraints:**
- Creates 4 equality constraints: owner, root, nonce, version
- Combines results using AND gates
- `is_not_equal()` = NOT of `is_equal()`

---

#### `data/dynamic/record/find.rs`
**Purpose:** Record field access in circuit.

**Limitations:**
- Only allows accessing the `owner` field
- Halts with error for other field access (prevents data leakage)

---

#### `data/dynamic/record/to_bits.rs`
**Purpose:** Bit serialization circuit.

**Order:** owner → root → nonce → version

---

#### `data/dynamic/record/to_fields.rs`
**Purpose:** Field element packing circuit.

**Constraints:**
- Adds terminus bit for decryption boundary detection
- Chunks bits into `A::BaseField::size_in_data_bits()`
- Uses `Field::from_bits_le()` constraint per chunk
- Validates size ≤ `MAX_DATA_SIZE_IN_FIELDS`

---

#### `data/dynamic/record/to_id.rs`
**Purpose:** Record ID computation circuit. **Important for review.**

**Constraints:**
- `compute_record_id()`: Hashes `(function_id || record_fields || tvk || index)`
- Uses `A::hash_psd8()` Poseidon8 constraint

**Testing:** 50 iterations across Constant/Public/Private modes.

---

#### `data/dynamic/future/mod.rs`
**Purpose:** DynamicFuture circuit struct.

**Structure:**
```rust
pub struct DynamicFuture<A: Aleo> {
    program_name: Field<A>,
    program_network: Field<A>,
    function_name: Field<A>,
    root: Field<A>,
    arguments: Option<Vec<console::Argument>>,
}
```

**Circuit Behavior:**
- All fields injected as `Mode::Private`
- Uses depth-4 Merkle tree for arguments

---

#### `data/dynamic/future/equal.rs`, `to_bits.rs`, `to_fields.rs`
**Purpose:** Supporting circuit implementations for DynamicFuture.

---

### Modified: data/value/ (7 files)

Extended Value enum with circuit implementations of dynamic variants.

#### `data/value/mod.rs`
**Purpose:** Value enum circuit definition.

**Changes:**
- Added `Value::DynamicRecord(DynamicRecord<A>)` variant
- Added `Value::DynamicFuture(DynamicFuture<A>)` variant
- Extended `Inject::new()` for both variants
- Extended `Eject` for mode combination and value ejection

---

#### `data/value/equal.rs`
**Purpose:** Value equality circuit.

**Changes:**
- Added match arms for DynamicRecord ↔ DynamicRecord comparison
- Added match arms for DynamicFuture ↔ DynamicFuture comparison
- Type mismatches return constant `false`

---

#### `data/value/find.rs`
**Purpose:** Path-based value access circuit.

**Changes:**
- DynamicRecord: Delegates to `DynamicRecord::find()`
- DynamicFuture: Returns error (unsupported)

---

#### `data/value/to_bits.rs`, `to_bits_raw.rs`, `to_fields.rs`, `to_fields_raw.rs`
**Purpose:** Serialization circuits.

**Changes:** Added match arms delegating to inner type implementations.

---

### Modified: data/future/ (3 files)

Extended Future/Argument types for DynamicFuture support.

#### `data/future/argument.rs`
**Purpose:** Argument enum circuit.

**Changes:**
- Added `Argument::DynamicFuture(DynamicFuture<A>)` variant
- Extended `Equal`, `ToBits` traits

**Disambiguation:** DynamicFuture uses tag bit `1` + 8 zero padding bits to distinguish from static Future.

---

#### `data/future/find.rs`
**Purpose:** Path-based access in futures.

**Changes:**
- Added `ArgumentRefType::DynamicFuture` for traversal
- Returns `Value::DynamicFuture` for dynamic future paths

---

#### `data/future/mod.rs`
**Purpose:** Module definition.

**Changes:** Minor - no structural changes to Future itself.

---

### Modified: request/ (2 files)

V2 request support with dynamic record inputs.

#### `request/mod.rs`
**Purpose:** Request circuit structure. **High priority for review.**

**Changes:**
- Added `InputID::DynamicRecord(Field<A>)` variant
- Added `dynamic: Option<bool>` field (console-only, not in circuit)
- Extended input handling for `Value::DynamicRecord`

---

#### `request/verify.rs`
**Purpose:** Request verification circuit. **High priority for review.**

**Key Changes:**
- `check_input_ids()` now accepts optional `function_id: Option<Field<A>>` for dynamic dispatch
- Added `InputID::DynamicRecord` verification case (lines 347-377)

**DynamicRecord Verification:**
```rust
// Hash: (function_id, record, tvk, input_index)
let candidate_hash = A::hash_psd8(&preimage);
```

**Constraint Counts (from tests):**
- Static request with records: ~7043 constraints
- Dynamic request with records: Additional constraints for public function ID

---

### Modified: response/ (3 files)

Response output ID computation for dynamic types.

#### `response/mod.rs`
**Purpose:** Response and OutputID circuit.

**Changes:**
- Added `OutputID::DynamicRecord(Field<A>)` variant
- Added `OutputID::DynamicFuture(Field<A>)` variant
- Helper methods: `dynamic_record()`, `dynamic_future()`

---

#### `response/from_outputs.rs`
**Purpose:** Output ID computation from function outputs. **High priority for review.**

**Changes:**
- Added `ValueType::DynamicRecord` handling (lines 214-237)
- Added `ValueType::DynamicFuture` handling (lines 239-263)

**Hash Computation:**
```rust
// DynamicRecord: (function_id, record, tvk, index)
// DynamicFuture: (function_id, future, tcm, index)
```

---

#### `response/process_outputs_from_callback.rs`
**Purpose:** Process outputs from external function callbacks.

**Changes:**
- Added optional `function_id: Option<Field<A>>` parameter
- Added DynamicRecord case (lines 276-304)
- Added DynamicFuture case (lines 306-335)

---

### Modified: Other Files (13 files)

#### `data/mod.rs`
**Purpose:** Module exports.

**Changes:** Exports DynamicFuture, DynamicRecord, RecordDataTree, compute_record_id, Argument.

---

#### `data/access/mod.rs`
**Purpose:** Register/struct/array access patterns.

**Changes:** No direct changes for dynamic dispatch.

---

#### `data/identifier/mod.rs`, `data/identifier/to_bits.rs`
**Purpose:** Identifier circuit type.

**Changes:** No direct changes. Tests verify zero constraints for bit operations.

---

#### `data/plaintext/mod.rs`
**Purpose:** Plaintext circuit with lazy bit evaluation.

**Changes:** No direct changes.

---

#### `data/record/mod.rs`, `data/record/entry/mod.rs`, `data/record/entry/to_bits.rs`, `data/record/entry/to_fields.rs`
**Purpose:** Record circuit implementations.

**Changes:** No direct changes for dynamic dispatch.

---

#### `id/mod.rs`, `id/to_address.rs`, `id/to_bits.rs`
**Purpose:** ProgramID circuit.

**Changes:** No structural changes. `public()` method essential for dynamic dispatch.

---

#### `function_id/mod.rs`
**Purpose:** Function ID computation circuit.

**Changes:** No structural changes. Used throughout request/response processing.

---

## Test Files

Testing is embedded in `#[cfg(test)]` modules within production files:

- `data/dynamic/record/to_id.rs` - 50 iterations across 3 modes
- `request/verify.rs` - Static and dynamic request verification
- `response/from_outputs.rs` - Static and dynamic response generation
- `response/process_outputs_from_callback.rs` - Callback processing

---

## Testing Notes

**Constraint Counts Verified:**
- DynamicRecord equality: 4 field comparisons + AND gates
- Request verification: ~7043 constraints (with records)
- Function ID computation: ~1901-2141 constraints (public mode)

**Coverage Areas:**
- Round-trip Inject/Eject consistency
- Mode preservation (Constant/Public/Private)
- Hash computation correctness
- Type validation at constraint boundaries

---

## Security Considerations

1. **Mode Handling:** DynamicRecord/DynamicFuture injected as `Mode::Private` to ensure confidentiality.

2. **Field Access Restriction:** `DynamicRecord::find()` only allows `owner` access to prevent data leakage.

3. **Hash Binding:** All dynamic types use `hash_psd8` with function_id to cryptographically bind to specific functions.

4. **Type Validation:** Circuit halts if value type doesn't match expected InputID/OutputID variant.

5. **Disambiguation:** Argument encoding uses 8 zero padding bits to distinguish DynamicFuture from static Future.
