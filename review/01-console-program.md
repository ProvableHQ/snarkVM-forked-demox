# console/program - Core Data Types

This document covers the 56 files changed in `console/program`, which defines the foundational data types for dynamic dispatch.

## Overview

The `console/program` crate implements the "native" (non-circuit) versions of Aleo program types. For dynamic dispatch, this crate introduces:

1. **DynamicRecord** and **DynamicFuture** - Fixed-size Merkle-based representations
2. **Value enum extensions** - New variants for dynamic types
3. **Request V2** - Versioned requests with dynamic flag
4. **Type system extensions** - ValueType, RegisterType, FinalizeType variants

## Files Requiring Review

### New Module: data/dynamic/ (8 files)

These files implement the core dynamic data types. **High priority for review.**

#### `data/dynamic/mod.rs`
**Purpose:** Module root exporting DynamicRecord, DynamicFuture, and related types.

**Exports:**
- `DynamicFuture`, `FutureArgumentTree` from future module
- `DynamicRecord`, `RecordData`, `RecordDataTree`, `RECORD_DATA_TREE_DEPTH` from record module

---

#### `data/dynamic/record/mod.rs`
**Purpose:** Core DynamicRecord implementation - a fixed-size record representation using Merkle commitment.

**Key Types:**
```rust
pub struct DynamicRecord<N: Network> {
    owner: Address<N>,
    root: Field<N>,           // Merkle root of record data
    nonce: Group<N>,
    version: U8<N>,           // 0 = public, non-zero = hiding
    data: Option<RecordData<N>>,  // Optional underlying data
}
```

**Constants:**
- `RECORD_DATA_TREE_DEPTH: usize = 5` (max 32 entries)
- `RecordDataTree` = `MerkleTree<E, Poseidon8<E>, Poseidon2<E>, 5>`

**Key Methods:**
- `from_record()` - Convert static Record to DynamicRecord
- `to_record()` - Convert back to static Record (requires data)
- `merkleize_data()` - Compute Merkle root from record entries
- `find()` - Retrieve entry by identifier with Merkle path

**Testing:** Includes tests for round-trip conversion, root determinism, and edge cases.

---

#### `data/dynamic/record/bytes.rs`
**Purpose:** Binary serialization for DynamicRecord.

**Format:** `[owner][root][nonce][version]` - data is not serialized (transient).

---

#### `data/dynamic/record/equal.rs`
**Purpose:** Equality comparison for DynamicRecord based on root hash.

---

#### `data/dynamic/record/find.rs`
**Purpose:** Entry lookup with Merkle path generation.

**Key Method:** `find(&self, identifier: &Identifier) -> Result<(Entry, MerklePath)>`

---

#### `data/dynamic/record/parse.rs`
**Purpose:** String parsing and Display for DynamicRecord.

**Format:** `{ owner: <addr>, root: <field>, nonce: <group>, version: <u8> }`

---

#### `data/dynamic/record/to_bits.rs`, `to_fields.rs`, `to_id.rs`
**Purpose:** Bit/field conversion and ID computation for cryptographic operations.

---

#### `data/dynamic/future/mod.rs`
**Purpose:** Core DynamicFuture implementation - fixed-size future representation.

**Key Types:**
```rust
pub struct DynamicFuture<N: Network> {
    program_name: Field<N>,
    program_network: Field<N>,
    function_name: Field<N>,
    root: Field<N>,           // Merkle root of arguments
    arguments: Option<Vec<Argument<N>>>,
}
```

**Constants:**
- `FUTURE_ARGUMENT_TREE_DEPTH: usize = 4` (max 16 arguments)

**Key Methods:**
- `from_future()` - Convert static Future to DynamicFuture
- `to_future()` - Convert back (requires arguments)
- `merkleize_arguments()` - Compute Merkle root from arguments

---

#### `data/dynamic/future/bytes.rs`, `equal.rs`, `parse.rs`, `to_bits.rs`, `to_fields.rs`
**Purpose:** Supporting implementations for serialization, comparison, parsing, and conversion.

---

### Modified: data/value/ (9 files)

Extended the `Value` enum with DynamicRecord and DynamicFuture variants.

#### `data/value/mod.rs`
**Purpose:** Value enum definition.

**Changes:**
- Added `Value::DynamicRecord(DynamicRecord<N>)` variant
- Added `Value::DynamicFuture(DynamicFuture<N>)` variant
- Added `From` trait implementations for both types
- Updated `From<Argument<N>>` to handle `Argument::DynamicFuture`

---

#### `data/value/bytes.rs`
**Purpose:** Binary serialization.

**Changes:**
- DynamicRecord at variant index 3
- DynamicFuture at variant index 4

---

#### `data/value/equal.rs`
**Purpose:** Equality comparison.

**Changes:** Added match arms for comparing DynamicRecord and DynamicFuture values.

---

#### `data/value/find.rs`
**Purpose:** Path-based value navigation.

**Changes:**
- DynamicRecord: Recursively searches entries
- DynamicFuture: Returns error "Cannot invoke `find` on a dynamic future value"

---

#### `data/value/parse.rs`
**Purpose:** String parsing and display.

**Changes:** Added parsing/display for "dynamic.record" and "dynamic.future" formats.

---

#### `data/value/to_bits.rs`, `to_bits_raw.rs`, `to_fields.rs`, `to_fields_raw.rs`
**Purpose:** Bit and field conversions.

**Changes:** Added match arms delegating to underlying type methods.

---

### Modified: data/future/ (5 files)

Extended Future/Argument types to support DynamicFuture.

#### `data/future/argument.rs`
**Purpose:** Argument enum used in futures.

**Changes:**
- Added `Argument::DynamicFuture(DynamicFuture<N>)` variant
- Extended `Equal`, `ToBits`, `ToFields` trait implementations
- Special encoding: DynamicFuture uses tag bit + 0u8 discriminator to distinguish from static Future

---

#### `data/future/bytes.rs`
**Purpose:** Serialization for Future and Argument.

**Changes:**
- Argument::DynamicFuture at index 2 (0=Plaintext, 1=Future, 2=DynamicFuture)
- Added deeply nested future test (depth 3900)

---

#### `data/future/find.rs`
**Purpose:** Path-based lookup in futures.

**Changes:** Added `ArgumentRefType::DynamicFuture` for traversal.

---

#### `data/future/mod.rs`
**Purpose:** Module definition.

**Changes:** Imports DynamicFuture type.

---

#### `data/future/parse.rs`
**Purpose:** Parsing for Future.

**Changes:** Added `DynamicFuture::parse` as parsing alternative.

---

### Modified: request/ (8 files)

Implements Request V1/V2 versioning for dynamic dispatch.

#### `request/mod.rs`
**Purpose:** Core Request struct and logic. **High priority for review.**

**Key Changes:**
- Added `RequestVersion` enum (V1 = 1, V2 = 2)
- Added `dynamic: Option<bool>` field (None = V1, Some(_) = V2)
- Added `version()`, `dynamic()`, `is_dynamic()` methods

**New Methods:**
- `caller_input_ids()` - Converts Record inputs to DynamicRecord inputs for dynamic calls
- `caller_inputs()` - Converts Record values to DynamicRecord values

**Logic:** Record → DynamicRecord conversion uses `InputID::dynamic_record()` for hash computation.

---

#### `request/input_id/mod.rs`
**Purpose:** InputID enum for request inputs.

**Changes:**
- Added `InputID::DynamicRecord(Field<N>)` variant
- Added `dynamic_record()` function computing hash of `(function_id || record || tvk || index)`
- Updated `private()` and `record()` to reject DynamicRecord/DynamicFuture values

---

#### `request/input_id/bytes.rs`
**Purpose:** Serialization for InputID.

**Changes:** DynamicRecord at variant 5.

---

#### `request/input_id/serialize.rs`
**Purpose:** JSON serialization.

**Changes:** Added "dynamic_record" type for JSON format.

---

#### `request/sign.rs`
**Purpose:** Request signing logic.

**Changes:**
- Refactored into generic `sign()` with `dynamic: Option<bool>` parameter
- Added `sign_static()` and `sign_dynamic()` convenience wrappers
- Updated input ID computation to handle DynamicRecord

---

#### `request/verify.rs`
**Purpose:** Request verification.

**Changes:** Added `InputID::DynamicRecord` verification case.

---

#### `request/bytes.rs`
**Purpose:** Binary serialization.

**Changes:**
- V1: No dynamic flag
- V2: Includes dynamic flag byte

---

#### `request/serialize.rs`
**Purpose:** JSON serialization.

**Changes:**
- V1: 11 fields
- V2: 12 fields (adds `dynamic`)

---

### Modified: data_types/ (11 files)

Extended type enums with dynamic variants.

#### `data_types/value_type/mod.rs`
**Changes:**
- Added `ValueType::DynamicRecord` (variant 6)
- Added `ValueType::DynamicFuture` (variant 7)

#### `data_types/value_type/bytes.rs`
**Changes:** Serialization for new variants.

#### `data_types/value_type/parse.rs`
**Changes:** Parsing for "dynamic.record" and "dynamic.future".

---

#### `data_types/register_type/mod.rs`
**Changes:**
- Added `RegisterType::DynamicRecord` (variant 4)
- Added `RegisterType::DynamicFuture` (variant 5)
- Updated `From<ValueType>` and `From<FinalizeType>` conversions
- Updated `qualify()` method to handle dynamic types (returns `self` unchanged)

#### `data_types/register_type/bytes.rs`, `parse.rs`, `size_in_bits.rs`
**Changes:** Serialization, parsing, and size calculation for new variants.

**V13 Integration (size_in_bits.rs):**
- Added `get_external_struct` parameter to size calculation functions
- DynamicRecord/DynamicFuture have fixed sizes (don't need external struct lookup)

---

#### `data_types/finalize_type/mod.rs`
**Changes:** Added `FinalizeType::DynamicFuture` (variant 2).

#### `data_types/finalize_type/bytes.rs`, `parse.rs`, `size_in_bits.rs`
**Changes:** Supporting implementations for new variant.

---

### Modified: Other Files (5 files)

#### `data/mod.rs`
**Purpose:** Main data module exports.

**Changes:** Exports DynamicFuture, DynamicRecord, and related types.

---

#### `data/record/entry/mod.rs`, `data/record/entry/to_fields.rs`
**Purpose:** Record entry definitions.

**Changes:** Minor - no direct dynamic dispatch changes.

---

#### `data/identifier/parse.rs`
**Purpose:** Identifier parsing.

**Changes:** Enforced that identifiers must start with ASCII letter. This property is critical for sound DynamicFuture encoding (ensures program IDs cannot start with zero byte).

---

#### `function_id/mod.rs`
**Purpose:** Function ID computation.

**Changes:** No direct changes for dynamic dispatch.

---

#### `response/mod.rs`
**Purpose:** Response and OutputID definitions.

**Changes:**
- Added `OutputID::DynamicRecord(Field<N>)` variant
- Added `OutputID::DynamicFuture(Field<N>)` variant
- Extended `Response::new()` to compute output IDs for dynamic types
- Added `caller_outputs()` - converts static records/futures to dynamic variants
- Added `caller_output_ids()` - recomputes output IDs for dynamic variants

---

## Test Files

#### `benches/dynamic_data.rs` (NEW)
**Purpose:** Benchmark for dynamic data operations.

**Benchmarks:**
- `DynamicFuture::from_future` with 1, 4, 8, 16 arguments
- `DynamicRecord::merkleize_data` with 1, 4, 8, 16 entries

---

## Testing Notes

**Unit Tests:** Embedded in `#[cfg(test)]` modules within each file:
- Round-trip serialization (bytes, JSON)
- Parsing and display
- Equality comparison
- Deep nesting (futures tested to depth 3900)

**Coverage Areas:**
- DynamicRecord ↔ Record conversion
- DynamicFuture ↔ Future conversion
- Request V1/V2 signing and verification
- All new enum variants in serialization paths

**What's Tested:**
- `data/dynamic/record/mod.rs` - Conversion, merkleization, find operations
- `data/dynamic/future/mod.rs` - Conversion, merkleization
- `request/sign.rs` / `verify.rs` - Both static and dynamic request flows
- All bytes.rs files - Serialization round-trips

---

## Cargo.toml Changes

**Dependencies Added:**
- `itertools` (workspace)

**Dev Dependencies Added:**
- `criterion` (workspace)

**Benchmarks Added:**
- `dynamic_data` benchmark suite

---

## Security Considerations

1. **Merkle Root Determinism:** Same data must always produce same root for proof validity.

2. **Identifier Constraints:** Identifiers must start with ASCII letter to ensure sound encoding disambiguation between static and dynamic futures.

3. **Version Detection:** Request versioning uses `Option<bool>` where `None` = V1 to maintain backward compatibility.

4. **Input ID Security:** DynamicRecord input IDs use same hashing pattern as ExternalRecord (tvk-based), ensuring cryptographic binding.
