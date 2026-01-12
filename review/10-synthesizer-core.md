# synthesizer/core - VM Integration

This document covers the 17 files changed in `synthesizer/src`, which integrates dynamic dispatch into the Aleo VM.

## Overview

The `synthesizer/src` crate implements the Aleo VM and integrates all synthesizer components. For dynamic dispatch, this crate:

1. **Integrates translation keys** in deployment
2. **Updates execution/finalization** for dynamic futures
3. **Provides comprehensive V14 tests** for all dynamic dispatch features

## Files Requiring Review

### Production Code

#### `vm/deploy.rs`
**Purpose:** Program deployment transaction creation.

**Changes:**
- V14 consensus check for translation verifying keys
```rust
if consensus_version < ConsensusVersion::V14 {
    deployment.set_translation_verifying_keys_raw(None);
}
```

Ensures pre-V14 blocks don't include translation keys for backward compatibility.

---

#### `vm/execute.rs`
**Purpose:** Program execution and transaction creation.

**Changes:**
- Updated `Trace::prepare()` signature to accept `Process` reference
- Modified cost calculation for new return value structure
- Authorization execution passes Process reference for translation key lookup

---

#### `vm/finalize.rs`
**Purpose:** Transaction finalization and block speculation. **Important for review.**

**Changes:**
- Refactored to use sequential operation thread
- Changed `atomic_speculate()` to accept owned values instead of references
- Added `atomic_speculate_inner()` for sequential processing
- Replaced atomic locks with `ensure_sequential_processing()` check
- Added error logging with `dev_eprintln!`

---

#### `vm/verify.rs`
**Purpose:** Transaction verification.

**Changes:** Contains verification logic for translation proofs and dynamic records.

---

#### `vm/mod.rs`
**Purpose:** VM module definition.

**Changes:**
- Test module organization (test_v8, test_v9, test_v10, test_v11, test_v14)
- Helper reorganization for sequential operations

---

#### `restrictions/mod.rs`
**Purpose:** Program execution restrictions.

**Changes:** None - restrictions operate at program/function level, unaffected by dynamic dispatch.

---

## Test Files

### Module: `vm/tests/test_v14/` (NEW)

Comprehensive test suite for V14 dynamic dispatch features.

#### `mod.rs` (155 lines)
**Purpose:** Test organization and helpers.

**Key Functions:**
- `add_and_test()` - Helper to validate transactions and add blocks

**Documented Test Cases:**
- Single-translation cases (input/output dynamic ↔ static)
- Chained translation cases
- `get.record.dynamic` operations
- Consumption/production semantics
- Key-fetching consistency
- Signature consistency validation

---

#### `call_dynamic.rs` (1486 lines)
**Purpose:** Tests for `call.dynamic` instruction.

**Key Tests:**
- `test_dynamic_calls_to_credits_aleo()` - Dynamic calls to built-in functions:
  - `transfer_public_as_signer`
  - `transfer_public_to_private`
  - `transfer_private`
  - Two sequential dynamic calls
  - Dynamic futures in async contexts

**Programs Tested:**
- `test_dcall.aleo` - Multiple dynamic call patterns

---

#### `cast.rs` (350 lines)
**Purpose:** Tests casting static records to `dynamic.record`.

**Key Tests:**
- `test_circuit_dynamic_record_from_record()` - Circuit ↔ console consistency
- `test_cast_simple()` - Record casting and consumption patterns

**Validates:**
- Merkle root agreement between circuit and console
- Double-spend detection when passing cast records

---

#### `translation.rs` (1135 lines)
**Purpose:** Tests record translation between static and dynamic. **Important for review.**

**Key Tests:**
- Input dynamic → static non-external
- Input dynamic → static external
- Output static → dynamic
- Chained translations

**Validation Points:**
- Merkle root preservation
- Multiple translations use same key
- Different record types use different keys

---

#### `dynamic_futures.rs` (2526 lines)
**Purpose:** Tests `DynamicFuture` behavior and await patterns.

**Key Tests:**
- `test_await_in_order()` - Sequential awaits
- `test_await_out_of_order()` - Out-of-order awaits
- `test_conditional_await()` - Conditional execution

**Covers:**
- Multiple dynamic futures with different await sequences
- Finalize block correctness
- Future dependency handling

---

#### `dynamic_mapping_operations.rs` (1695 lines)
**Purpose:** Tests dynamic mapping operations in finalize blocks.

**Key Tests:**
- `test_dynamic_contains()` - `contains.dynamic` instruction
- `test_dynamic_get()` - `get.dynamic` instruction
- `test_dynamic_get_or_use()` - `get.or_use.dynamic` instruction

**Validates:**
- Operations on external program mappings
- Operations on current program mappings
- Error handling for non-existent programs/mappings

---

#### `get_record_dynamic.rs` (916 lines)
**Purpose:** Tests `get.record.dynamic` for extracting entries.

**Key Tests:**
- Polymorphic reads (multiple record types)
- Array element access
- Struct field access
- Type mismatch error cases

---

#### `mixed.rs` (688 lines)
**Purpose:** Integration tests combining multiple features.

**Key Tests:**
- `test_execution_cost_for_authorization()` - Cost calculation with translations
- `test_translation_get_dynamic_cast_to_dynamic()` - Complex multi-program patterns

---

#### `recursion.rs` (518 lines) - **DISABLED**
**Purpose:** Recursive dynamic function calls.

**Status:** Commented out due to acyclic call graph requirement.

**Note:** "can be re-enabled if we ever allow cycles"

---

### Other Test Files

#### `vm/tests/mod.rs`
Test module organization for all consensus versions.

#### `vm/tests/test_v8.rs`
V8 tests (pre-dynamic dispatch, for regression testing).

---

## Testing Notes

**Test Coverage Summary:**

| File | Lines | Focus |
|------|-------|-------|
| call_dynamic.rs | 1486 | Dynamic calls |
| cast.rs | 350 | Record casting |
| translation.rs | 1135 | Record translation |
| dynamic_futures.rs | 2526 | Future handling |
| dynamic_mapping_operations.rs | 1695 | Dynamic mappings |
| get_record_dynamic.rs | 916 | Entry extraction |
| mixed.rs | 688 | Integration |
| recursion.rs | 518 | Recursion (disabled) |

**Total:** ~9,300+ lines of V14-specific tests

---

## Security Considerations

1. **Sequential Processing:** Finalization uses sequential operation thread for thread safety.

2. **Backward Compatibility:** Translation keys unset for pre-V14 blocks.

3. **Recursion Prevention:** Acyclic call graph requirement (recursion tests disabled).

4. **Comprehensive Testing:** All dynamic dispatch features have dedicated test coverage.
