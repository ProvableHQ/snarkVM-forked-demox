# synthesizer/program - Instructions and Commands

This document covers the 24 files changed in `synthesizer/program`, which defines Aleo VM instructions and commands.

## Overview

The `synthesizer/program` crate defines the instruction set and command set for Aleo programs. For dynamic dispatch, this crate introduces:

1. **New Instructions:** `call.dynamic`, `get.record.dynamic`
2. **New Commands:** `contains.dynamic`, `get.dynamic`, `get.or_use.dynamic`
3. **Extended casting:** Support for `dynamic.record` cast type

## Files Requiring Review

### New Instructions

#### `logic/instruction/operation/call/dynamic.rs` (NEW)
**Purpose:** Dynamic function call instruction. **High priority for review.**

**Syntax:**
```
call.dynamic <program_name> <program_network> <function_name>
    [with <operands> (as <types>)]
    [into <destinations> (as <types>)]
```

**Example:**
```
call.dynamic r0 r1 r2 with r3 r4 (as u8.public u64.private) into r5 (as u64.public)
```

**Operands:**
- First 3 operands: Field elements representing program name, network, function name
- Optional arguments with explicit type declarations
- Optional destinations with expected return types

**Constraints:**
- Minimum 3 operands (program ID components)
- Cannot pass: Record, ExternalRecord, Future, DynamicFuture as arguments
- Cannot return: Record, ExternalRecord, static Future
- Can return: DynamicFuture

**V13 Integration:**
- `contains_external_struct()` method returns `false` (dynamic calls don't use external structs directly)

**Implementation:** 617 lines including 135 lines of tests.

---

#### `logic/instruction/operation/get_record_dynamic.rs` (NEW)
**Purpose:** Extract entries from dynamic records with Merkle verification. **High priority for review.**

**Syntax:**
```
get.record.dynamic <dynamic_record>.<entry_name> into <destination> as <plaintext_type>
```

**Example:**
```
get.record.dynamic r0.owner into r1 as address
get.record.dynamic r0.amount into r2 as u64
```

**Operands:**
- Source: Register access of form `r<i>.<identifier>`
- Destination: Simple register `r<i>`
- Type: Expected plaintext type

**Constraints:**
- Source must be a DynamicRecord value
- Entry must exist in record data
- Entry type must match declared type

**Circuit Behavior:**
- Computes/validates Merkle path to entry
- Uses Poseidon8 (leaf hash) and Poseidon2 (path hash)
- During synthesis (data unavailable), patches arbitrary values

**V13 Integration:**
- `contains_external_struct()` method returns `false` (extracts plaintext, not external structs)

**Implementation:** 652 lines including 185 lines of tests.

---

### New Commands (Finalize)

#### `logic/command/contains/dynamic.rs` (NEW)
**Purpose:** Check key existence in dynamically-resolved mapping.

**Syntax:**
```
contains.dynamic <program_name> <program_network> <mapping_name>[<key>] into <destination>;
```

**Operands:**
- program_name: Field element
- program_network: Field element
- mapping_name: Field element
- key: Key to check
- destination: Boolean result register

**Execution:**
1. Extract program ID from field elements
2. Verify mapping exists
3. Check key existence via `contains_mapping_speculative`
4. Store boolean result

---

#### `logic/command/get/dynamic.rs` (NEW)
**Purpose:** Retrieve value from dynamically-resolved mapping.

**Syntax:**
```
get.dynamic <program_name> <program_network> <mapping_name>[<key>] into <destination> as <type>;
```

**Operands:**
- Same as contains.dynamic, plus:
- type: Expected value type

**Constraints:**
- Cannot retrieve: Record, Future, DynamicRecord, DynamicFuture
- Key must match mapping key type
- Fails if key doesn't exist

---

#### `logic/command/get_or_use/dynamic.rs` (NEW)
**Purpose:** Retrieve value with fallback default from dynamically-resolved mapping.

**Syntax:**
```
get.or_use.dynamic <program_name> <program_network> <mapping_name>[<key>] <default> into <destination> as <type>;
```

**Operands:**
- Same as get.dynamic, plus:
- default: Fallback value if key doesn't exist

**Behavior:** Uses default value instead of failing if key absent.

---

### Modified: Instruction Infrastructure

#### `logic/instruction/operation/call/mod.rs`
**Changes:** Added import for `dynamic` module.

---

#### `logic/instruction/operation/call/standard.rs`
**Purpose:** Static call instruction (unchanged).

---

#### `logic/instruction/operation/mod.rs`
**Changes:** Added import for `get_record_dynamic` module.

---

#### `logic/instruction/mod.rs`
**Purpose:** Master Instruction enum. **Important for review.**

**Changes:**
- Added `CallDynamic` variant
- Added `GetRecordDynamic` variant
- Updated instruction macro for V14 consensus
- Updated OPCODES count: 119 → 121

---

#### `logic/instruction/opcode/mod.rs`
**Changes:** Added `GetRecordDynamic(&'static str)` opcode variant.

---

### Modified: Cast Instruction

#### `logic/instruction/operation/cast.rs`
**Purpose:** Type casting instruction.

**Changes:** Extended to support `dynamic.record` cast type.

**New Cast:**
```
cast <record_operand> into <destination> as dynamic.record
```

**Validation:** `validate_dynamic_record_cast()` ensures:
- Exactly one operand (a static record)
- Destination is simple register form

**V13 Integration:**
- `struct_checks()` returns error for `RegisterType::DynamicRecord` and `RegisterType::DynamicFuture`
- These types cannot be nested in structs

---

### Modified: Async Instruction

#### `logic/instruction/operation/async_.rs`
**Purpose:** Asynchronous function call.

**Changes:** Updated to accept `DynamicFuture` as valid argument type alongside `Future`.

---

### Modified: Command Infrastructure

#### `logic/command/mod.rs`
**Purpose:** Master Command enum. **Important for review.**

**Changes:**
- Added `ContainsDynamic`, `GetDynamic`, `GetOrUseDynamic` variants
- Updated parsing (dynamic variants before static)
- Byte serialization: variants 11, 12, 13
- Updated `is_call()` for `CallDynamic`
- Tests for new dynamic commands

**V13 Integration:**
- `contains_external_struct()` returns `false` for all dynamic command variants

---

#### `logic/command/contains/mod.rs`, `get/mod.rs`, `get_or_use/mod.rs`
**Changes:** Added imports for `dynamic` modules.

---

### Other Modified Files

#### `src/function/mod.rs`
**Purpose:** Function definition and validation.

**Changes:** Updated for dynamic dispatch support.

---

#### `src/lib.rs`
**Purpose:** Crate root.

**Changes:** Exports for new types.

---

#### `src/traits/stack_and_registers.rs`
**Purpose:** Stack and register traits.

**Changes:** Support for dynamic types in register operations.

---

#### `benches/instruction.rs`
**Purpose:** Instruction benchmarks.

**Changes:** Benchmarks for new instructions.

---

## Test Files

Tests are embedded within production files:
- `call/dynamic.rs`: 135 lines of tests
- `get_record_dynamic.rs`: 185 lines of tests
- `command/mod.rs`: Comprehensive command tests

---

## Testing Notes

**What's Tested:**
- Instruction parsing and display
- Operand validation
- Type checking
- Byte serialization round-trips
- Command execution semantics

**Coverage Areas:**
- Valid and invalid instruction syntax
- Type constraints enforcement
- Dynamic resolution behavior

---

## Security Considerations

1. **Type Restrictions:** Dynamic calls cannot pass/return records or static futures, preventing leakage of sensitive types.

2. **Merkle Verification:** `get.record.dynamic` cryptographically verifies entry membership via Merkle proofs.

3. **Mapping Access:** Dynamic mapping commands respect existing access controls and speculative execution semantics.

4. **Consensus Gating:** New instructions only available at V14+ consensus version.
