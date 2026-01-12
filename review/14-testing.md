# Testing Documentation

This document provides a comprehensive overview of test coverage for the dynamic dispatch feature.

## Test Organization

### Unit Tests (Embedded)

Unit tests are embedded in `#[cfg(test)]` modules within production files.

| Crate | Test Location | Coverage |
|-------|--------------|----------|
| console/program | Each file in `data/dynamic/`, `request/`, `data_types/` | Serialization, parsing, round-trips |
| circuit/program | Each file in `data/dynamic/`, `request/`, `response/` | Circuit correctness, constraint counts |
| synthesizer/program | `call/dynamic.rs`, `get_record_dynamic.rs` | Instruction parsing, validation |
| synthesizer/process | `trace/translation/tests.rs` | Translation circuit correctness |

### Integration Tests

#### `synthesizer/tests/test_vm_execute_and_finalize.rs`
General VM execution tests, updated for V14 compatibility.

#### `synthesizer/process/src/tests/`

| File | Focus |
|------|-------|
| `test_call_graph.rs` | Acyclic call graph validation |
| `test_credits.rs` | Credits program with dynamic dispatch |
| `test_execute.rs` | Execution including dynamic calls |
| `test_serializers.rs` | Serialization for new types |
| `test_utils.rs` | Test utilities |

### V14-Specific Tests

Located in `synthesizer/src/vm/tests/test_v14/`:

| File | Lines | Focus |
|------|-------|-------|
| `call_dynamic.rs` | 1486 | `call.dynamic` instruction |
| `cast.rs` | 350 | Record to `dynamic.record` casting |
| `translation.rs` | 1135 | Record translation proofs |
| `dynamic_futures.rs` | 2526 | `DynamicFuture` await patterns |
| `dynamic_mapping_operations.rs` | 1695 | `contains.dynamic`, `get.dynamic`, `get.or_use.dynamic` |
| `get_record_dynamic.rs` | 916 | `get.record.dynamic` entry extraction |
| `mixed.rs` | 688 | Integration scenarios |
| `recursion.rs` | 518 | Recursion (disabled) |

**Total V14 Test Lines:** ~9,300+

---

## Test Coverage by Feature

### DynamicRecord

**Unit Tests:**
- `console/program/src/data/dynamic/record/mod.rs` - Round-trip conversion, merkleization
- `circuit/program/src/data/dynamic/record/to_id.rs` - ID computation (50 iterations, 3 modes)

**Integration Tests:**
- `cast.rs` - Casting static records to dynamic
- `translation.rs` - Translation proof verification
- `get_record_dynamic.rs` - Entry extraction with Merkle verification

**What's Verified:**
- Static ↔ Dynamic conversion preserves data
- Merkle root determinism
- Owner/nonce/version match after conversion
- ID computation correctness

---

### DynamicFuture

**Unit Tests:**
- `console/program/src/data/dynamic/future/mod.rs` - Round-trip conversion
- `console/program/src/data/future/argument.rs` - Argument enum extension

**Integration Tests:**
- `dynamic_futures.rs` - Comprehensive await pattern tests
- `call_dynamic.rs` - Dynamic futures in async contexts

**What's Verified:**
- Future ↔ DynamicFuture conversion
- Await ordering (in-order, out-of-order)
- Conditional await execution
- Future dependency handling

---

### call.dynamic Instruction

**Unit Tests:**
- `synthesizer/program/src/logic/instruction/operation/call/dynamic.rs` (135 lines)

**Integration Tests:**
- `call_dynamic.rs` (1486 lines)

**Test Scenarios:**
- Dynamic calls to `credits.aleo` functions
- Sequential dynamic calls
- Dynamic calls with record exchange
- Dynamic futures in async contexts

**What's Verified:**
- Program/function resolution from Field values
- Input/output conversion
- Caller context separation
- Translation task collection

---

### get.record.dynamic Instruction

**Unit Tests:**
- `synthesizer/program/src/logic/instruction/operation/get_record_dynamic.rs` (185 lines)

**Integration Tests:**
- `get_record_dynamic.rs` (916 lines)

**Test Scenarios:**
- Polymorphic reads (multiple record types)
- Array element access
- Struct field access
- Type mismatch error handling

**What's Verified:**
- Merkle path verification
- Entry type validation
- Field visibility preservation

---

### Dynamic Mapping Operations

**Integration Tests:**
- `dynamic_mapping_operations.rs` (1695 lines)

**Test Scenarios:**
- `contains.dynamic` - Key existence in external mappings
- `get.dynamic` - Value retrieval from external mappings
- `get.or_use.dynamic` - Value retrieval with fallback

**What's Verified:**
- Dynamic program/mapping resolution
- Type checking for keys and values
- Error handling for non-existent programs/mappings

---

### Translation Proofs

**Unit Tests:**
- `synthesizer/process/src/trace/translation/tests.rs`

**Test Scenarios:**
- Simple records (~24K constraints)
- Recursive/nested records (~32K constraints)
- Complex 32-field records (~68K constraints)
- Circuit invariance (same structure = same circuit)
- Circuit variance (different structure = different circuit)
- External record handling

**What's Verified:**
- Constraint count accuracy
- Circuit determinism
- ID computation correctness
- External record special handling

---

### Request V2

**Unit Tests:**
- `console/program/src/request/sign.rs` - Static and dynamic signing
- `console/program/src/request/verify.rs` - Verification tests

**What's Verified:**
- V1/V2 version detection
- Dynamic flag serialization
- Input ID computation for DynamicRecord
- Signature validity

---

### Transition V2

**Unit Tests:**
- `ledger/block/src/transition/bytes.rs` - Binary serialization
- `ledger/block/src/transition/serialize.rs` - JSON serialization

**What's Verified:**
- V1/V2 version handling
- Caller metadata storage/retrieval
- Backward compatibility

---

## Running Tests

### All Tests
```bash
cargo test --release
```

### V14 Tests Only
```bash
cargo test --release -p snarkvm-synthesizer --features test test_v14
```

### Specific Test File
```bash
cargo test --release -p snarkvm-synthesizer --features test test_call_dynamic
```

### Translation Circuit Tests
```bash
cargo test --release -p snarkvm-synthesizer-process test_translation
```

---

## Test Expectations

Test expectation files in `synthesizer/tests/expectations/vm/execute_and_finalize/`:

| File | Purpose |
|------|---------|
| `count_usages.out` | Usage counting expectations |
| `future_out_of_order.out` | Out-of-order future expectations |
| `hello.out` | Basic execution expectations |
| `mint_and_split.out` | Token operations |
| `read_external_mapping.out` | External mapping access |
| `test_branch.out` | Branch instruction |
| `test_rand.out` | Random number generation |

---

## Benchmarks

### console/program
- `benches/dynamic_data.rs` - DynamicFuture and DynamicRecord creation benchmarks

### synthesizer/program
- `benches/instruction.rs` - Instruction benchmarks including new dynamic instructions

### synthesizer/process
- `benches/check_deployment.rs` - Deployment verification
- `benches/stack_operations.rs` - Stack operations including dynamic calls

---

## Coverage Gaps

**Potentially Under-tested:**
1. Multi-program translation chains (complex call graphs)
2. Edge cases in dynamic future resolution
3. Error recovery in dynamic call failures
4. Large-scale batch translation proving

**Disabled Tests:**
- `recursion.rs` - Disabled due to acyclic call graph requirement

---

## Security Testing

**Tested Security Properties:**
1. **Acyclic Call Graph:** `test_call_graph.rs` validates cycle detection
2. **Type Safety:** Type mismatch tests in all instruction tests
3. **Double-Spend Prevention:** `cast.rs` tests consumption patterns
4. **Translation Integrity:** `translation/tests.rs` validates proof correctness
5. **Fee Protection:** Fee transitions cannot be dynamic (enforced in `Fee::from()`)
