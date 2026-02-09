# synthesizer/process - Execution and Verification

This document covers the 46 files changed in `synthesizer/process`, which implements the core execution, verification, and translation proof logic for dynamic dispatch.

## Overview

The `synthesizer/process` crate is the heart of Aleo program execution. For dynamic dispatch, this crate:

1. **Implements dynamic call execution** via `call.dynamic` handling
2. **Introduces translation proofs** for Record ↔ DynamicRecord verification
3. **Manages type checking** for DynamicRecord/DynamicFuture
5. **Uses structured error types** from staging merge (CallEvalError, CallExecError, etc.)

## Error Type System (from staging)

The merge with staging introduced structured error types that are used throughout:

| Error Type | Usage |
|------------|-------|
| `CallEvalError` | Errors from `CallTrait::evaluate()` |
| `CallExecError` | Errors from `CallTrait::execute()` |
| `InstructionEvalError` | Wraps call errors during instruction evaluation |
| `InstructionExecError` | Wraps call errors during instruction execution |
| `StackEvalError` | Stack-level evaluation errors |
| `ProcessExecError` | Process-level execution errors |

Error conversion pattern: `return Err(anyhow!(...).into())`

## Files Requiring Review

### New Module: stack/call/dynamic.rs (NEW)
**Purpose:** Implements `CallTrait` for `CallDynamic<N>`. **Highest priority for review.**

**Key Functions:**

#### `evaluate()` (lines 18-158)
Evaluates dynamic call in console mode (no circuit constraints).

**Process:**
1. Extract program name, network, function name from Field operands
2. Call `resolve_dynamic_target()` to find actual function
3. Verify target is a function (not closure)
4. Convert caller inputs to callee inputs (DynamicRecord → Record)
5. Evaluate function on substack
6. Convert outputs back to caller context
7. Assign outputs to destination registers

#### `execute()` (lines 160-695)
Executes dynamic call with full circuit constraint generation.

**Process:**
1. Load circuit inputs from registers
2. Eject existing R1CS circuit before external call
3. Resolve target using `resolve_dynamic_target()`
4. Handle call stack modes:
   - **Authorize:** Sign request, push to call stack, execute
   - **Synthesize/CheckDeployment:** Generate dummy inputs/outputs
   - **Execute:** Verify console and circuit outputs match
5. Verify circuit integrity:
   - 4 public variables added (network_id, program_id, function_name, function_id)
   - Input IDs match computed values
6. Process outputs via `circuit::Response::process_outputs_from_callback`
7. Collect record translations for verification

**Key Helper Functions:**

- `resolve_dynamic_target()` - Decodes Field values to ProgramID/function
- `convert_caller_inputs_to_callee_inputs()` - DynamicRecord → Record conversion
- `collect_input_translations()` - Gathers translation proof data for inputs
- `collect_output_translations()` - Gathers translation proof data for outputs

**Constraints:**
- Cannot call closures dynamically
- Cannot pass/return Futures or DynamicFutures
- Cannot return Records (must use DynamicRecord)
- Cannot call `credits.aleo/fee_*` functions

---

### Detailed: stack/call/dynamic.rs Execution Flow

This section provides additional detail on the `call.dynamic` instruction implementation.

#### Data Flow Diagram

```
+----------------+     +------------------+     +----------------+
|   Caller       | --> | call.dynamic     | --> |   Callee       |
|   Function     |     | Instruction      |     |   Function     |
+----------------+     +------------------+     +----------------+
        |                      |                       |
        v                      v                       v
  DynamicRecord          Resolve Target           Record
  (caller view)          program/function        (callee view)
        |                      |                       |
        |              +-------+-------+               |
        |              |               |               |
        v              v               v               v
   caller_inputs  convert inputs   callee_inputs   execute
        |              |               |               |
        |              v               v               |
        |         Translation     Translation          |
        |         Data (input)    Proof data           |
        |              |               |               |
        v              v               v               v
   caller_outputs <-- convert <-- callee_outputs <--+
        |              |
        v              v
   DynamicRecord  Translation
   (caller view)  Data (output)
```

#### Call Stack Mode Handling

| Mode | Behavior |
|------|----------|
| Authorize | Sign request, push to stack, execute callee |
| Synthesize | Generate dummy inputs/outputs (no real execution) |
| CheckDeployment | Same as Synthesize |
| PackageRun | Sign and execute once |
| Evaluate | Error (not allowed) |
| Execute | Full verification: console == circuit outputs |

#### Translation Data Collection

For each DynamicRecord ↔ Record conversion:

1. **Input translations** (caller DynamicRecord → callee Record):
   - Collected via `collect_input_translations()`
   - Stores: record_static, record_dynamic, program_id, function_id,
     record_name, is_input=true, static_is_external, tvk, gamma,
     serial_number (id_static), dynamic_hash (id_dynamic)

2. **Output translations** (callee Record → caller DynamicRecord):
   - Collected via `collect_output_translations()`
   - Same fields but is_input=false, commitment (id_static)

These are later used to generate translation proofs verifying equivalence.

#### Security Constraints

- Cannot call closures dynamically (enforced at target resolution)
- Cannot pass/return Futures or DynamicFutures (type system check)
- Cannot return Records (must use DynamicRecord for caller context)
- Cannot call `credits.aleo/fee_private` or `credits.aleo/fee_public`
- Acyclic call graph enforced (no recursion in dynamic calls)

---

### New Module: trace/translation/ (4 files)
**Purpose:** Translation proofs verify Record ↔ DynamicRecord equivalence. **High priority for review.**

#### `trace/translation/mod.rs`

**Key Types:**

```rust
pub struct RecordTranslationData<N: Network> {
    record_static: Record<N, Plaintext<N>>,
    record_dynamic: DynamicRecord<N>,
    program_id: ProgramID<N>,
    function_id: Field<N>,
    record_name: Identifier<N>,
    is_input: bool,
    static_is_external: bool,
    tvk: Field<N>,
    record_view_key: Option<Field<N>>,
    gamma: Option<Group<N>>,
    input_output_index: u16,
    id_dynamic: Field<N>,
    id_static: Field<N>,
}
```

**Key Methods:**
- `Translation::insert_transition()` - Store translation tasks during execution
- `Translation::prepare_verifier_inputs()` - Construct public inputs for verification

**Translation Scenarios:**

| Direction | From | To | ID Computation |
|-----------|------|-----|----------------|
| Input (Internal) | DynamicRecord | Record | `id_dynamic = Hash(fn_id\|record\|tvk\|idx)`, `id_static = SN(gamma, cm)` |
| Output (Internal) | DynamicRecord | Record | `id_dynamic = Hash(...)`, `id_static = Commit(...)` |
| Input/Output (External) | DynamicRecord | ExternalRecord | Both use `Hash(fn_id\|record\|tvk\|idx)` |

---

#### `trace/translation/assignment.rs`

**Circuit Constraints:**
1. Inject public inputs: `is_input`, `static_is_external`, `function_id`, `translation_count`, `io_index`, `id_static`, `id_dynamic`
2. Inject private inputs: `record_static`, `record_dynamic`, `tvk`, `record_view_key`, `gamma`
3. Compute `actual_id_dynamic = Hash(function_id || record_dynamic || tvk || io_index)`
4. Compute `actual_id_static`:
   - Non-external: commitment or serial number
   - External: Hash formula (same as dynamic)
5. Verify Merkle roots match
6. Assert: owner, nonce, version match; IDs match

**Note:** `translation_index` is injected as a public input for verifier synchronization but is intentionally unused in circuit constraint logic (indicated by `_` prefix in code).

**Constraint Counts** (from `trace/translation/tests.rs`):

| Test | Record Type | is_input | Constants | Public | Private | Constraints |
|------|-------------|----------|-----------|--------|---------|-------------|
| `test_translation_simple` | 8 fields | false | ≤36085 | 8 | 24131 | 24156 |
| `test_translation_simple` | 8 fields | true | ≤6160 | 8 | 24131 | 24156 |
| `test_translation_recursive` | nested structs | false | ≤38785 | 8 | 32721 | 32750 |
| `test_translation_recursive` | nested structs | true | ≤8860 | 8 | 32721 | 32750 |
| `test_translation_complex` | 32 fields | false | ≤41330 | 8 | 68798 | 68844 |
| `test_translation_complex` | 32 fields | true | ≤11405 | 8 | 68798 | 68844 |
| `test_definition_invariance` | invariance check | - | ≤37800 | 8 | 31043 | 31070 |
| `test_external_translation` | external record | - | ≤38800 | 8 | 32562 | 32591 |

Note: Constants are higher for outputs (is_input=false) due to commitment computation vs serial number

---

#### `trace/translation/prepare.rs`

Batches translation tasks by `(ProgramID, RecordName)` for proving.

**Process:**
1. Build reverse call graph (callee → caller mapping)
2. Iterate transitions in post-order
3. Process inputs then outputs for each transition
4. Create `TranslationAssignment` for each DynamicRecord ↔ Record pair
5. Validate all translation tasks consumed

---

#### `trace/translation/tests.rs`
Comprehensive tests:
- Simple records
- Recursive/nested records
- Complex 32-field records
- Circuit invariance (same structure = same circuit)
- Circuit variance (different structure = different circuit)
- External record handling

---

### Modified: trace/mod.rs
**Purpose:** Orchestrates inclusion and translation proofs.

**Key Changes:**
- Added `translation_tasks: Translation<N>` field
- `prepare()` now calls `translation_tasks.prepare()` with call graph
- `prove_batch()` includes translation assignments in Varuna proof
- `verify_batch()` constructs translation verifier inputs

---

### Modified: trace/inclusion/assignment.rs, assignment_v0.rs
**Purpose:** V0 and V1 inclusion circuits.

**V1 Changes:** Added block height validation for upgrades.

---

### Modified: Execution Files

#### `execute.rs`
- Initializes `translations: Arc<RwLock<Vec>>` for tracking translation data
- Passes translations to `CallStack::execute()`
- Constructs call graph after execution

#### `deploy.rs`
- **NEW:** Inserts translation verifying keys for record translations
- `load_deployment()` handles optional translation verifying keys

#### `finalize.rs`
- **Dynamic future resolution:** Maps `(program_name, network, function_name, root)` → Future
- Validates transition counts for dynamic vs static calls
- Handles `Value::DynamicFuture` in await processing

#### `verify_execution.rs`
**Key Changes:**
- Dynamic call detection via `contains_dynamic_call()`
- **Acyclic call graph validation** (V14+)
- Translation proof batch verification:
```rust
let batch_translation_inputs = Translation::prepare_verifier_inputs(
    execution.transitions(),
    &transition_map,
    &|(program_id, record_name)| {
        self.get_stack(program_id).and_then(|stack|
            stack.get_translation_verifying_key(record_name))
    },
)?;
```

#### `verify_fee.rs`
- No dynamic dispatch in fee functions (explicit check)
- Fee verification separate from translation

#### `cost.rs`
- `execution_cost_v3()`: Dynamic future cost via concrete transition iteration
- `execution_finalize_cost()`: Per-transition finalize cost calculation

---

### Modified: Stack Files

#### `stack/mod.rs`
- Translation key management
- `synthesize_translation_key()` method

#### `stack/authorization/mod.rs`
- Authorization handling for dynamic calls

#### `stack/authorize.rs`
- Request signing for dynamic dispatch

#### `stack/evaluate.rs`, `stack/execute.rs`
- Dynamic call handling in evaluation/execution paths

#### `stack/deploy.rs`
- Translation key deployment support

---

### Modified: Type System Files

#### `stack/register_types/mod.rs`
- `RegisterType::DynamicRecord`: Only `owner` member accessible directly
- `RegisterType::DynamicFuture`: No direct access allowed

#### `stack/register_types/initialize.rs`
- DynamicFuture forbidden as inputs/outputs
- DynamicRecord allowed with runtime checks

#### `stack/register_types/matches.rs`
- Neither dynamic type can be nested in structs/arrays/records

#### `stack/finalize_types/mod.rs`, `initialize.rs`, `matches.rs`
- `FinalizeType::DynamicFuture` must be awaited
- Cannot use in control flow or mappings

#### `stack/registers/registers_circuit.rs`, `registers_trait.rs`
- Dynamic records support `find()` for path access
- Dynamic futures are opaque

#### `stack/finalize_registers/registers_trait.rs`
- Runtime validation for DynamicFuture values

---

### Modified: Other Stack Files

#### `stack/helpers/initialize.rs`, `sample.rs`, `stack_trait.rs`, `synthesize.rs`
- Infrastructure for dynamic dispatch support

---

### Modified: lib.rs
- Translation key synthesis in `setup()`
- `synthesize_translation_key()` public API

---

## Test Files

#### `tests/mod.rs`
Module registration for test files.

#### `tests/test_credits.rs`
Credits program tests with dynamic dispatch.

#### `tests/test_execute.rs`
Execution tests including dynamic calls.

#### `tests/test_serializers.rs`
Serialization tests for new types.

#### `tests/test_utils.rs`
Test utilities for dynamic dispatch testing.

---

## Testing Notes

**Translation Proof Tests (tests.rs):**
- Circuit invariance/variance
- Constraint count verification
- External record handling
- Multiple record complexity levels

**Call Graph Tests:**
- Acyclic validation
- Post-order traversal correctness
- Reverse call graph construction

**Execution Tests:**
- Dynamic call with record exchange
- Translation proof integration
- Cost calculation accuracy

---

## Security Considerations

1. **Acyclic Call Graph:** Prevents infinite recursion in dynamic calls (V14+).

2. **Translation Proof Security:** Cryptographically verifies Record ↔ DynamicRecord equivalence.

3. **Type Restrictions:** Dynamic types cannot be nested, preventing complex attack vectors.

4. **Fee Function Protection:** `credits.aleo/fee_*` cannot be dynamically called.

5. **Caller Context Separation:** Dynamic calls maintain separate caller_inputs/caller_outputs.

6. **Translation Count Consistency:** Prover and verifier use identical iteration order.
