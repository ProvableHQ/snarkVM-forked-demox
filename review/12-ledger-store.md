# ledger/store - Storage Layer

This document covers the 8 files changed in `ledger/store`, which implements persistent storage for dynamic dispatch data.

## Overview

The `ledger/store` crate provides storage abstractions. For dynamic dispatch, this crate:

1. **Adds storage maps** for dynamic transition metadata
2. **Supports DynamicRecord** input/output variants
3. **Stores caller metadata** for dynamic transitions
4. **Enables translation key storage** in deployments

## Files Requiring Review

### Core Transition Storage

#### `transition/mod.rs`
**Purpose:** Core transition storage trait. **Important for review.**

**New Associated Types:**
```rust
type IsDynamicMap: Map<N::TransitionID, bool>;
type CallerInputsMap: Map<N::TransitionID, Vec<Input<N>>>;
type CallerOutputsMap: Map<N::TransitionID, Vec<Output<N>>>;
```

**Insert Logic:**
```rust
if let Some(caller_metadata) = transition.caller_metadata() {
    self.is_dynamic_map().insert(transition_id, caller_metadata.is_dynamic())?;
    if caller_metadata.is_dynamic() {
        self.caller_inputs_map().insert(...)?;
        self.caller_outputs_map().insert(...)?;
    }
}
```

**Retrieval:**
- `get()` reconstructs caller metadata from stored components
- `get_caller_metadata()`, `get_caller_inputs()`, `get_caller_outputs()` accessors

---

#### `transition/input.rs`
**Purpose:** Transition input storage.

**Changes:**
- Added `DynamicRecordMap` associated type
- Support for `Input::DynamicRecord(input_id)` variant
- Insert/remove logic for dynamic record mapping
- `dynamic_input_ids()` iterator

---

#### `transition/output.rs`
**Purpose:** Transition output storage.

**Changes:**
- Added `DynamicRecordMap` associated type
- Support for `Output::DynamicRecord(output_id)` variant
- Insert/remove logic distinguishing dynamic from static records
- `dynamic_output_ids()` iterator

---

### Memory Storage

#### `helpers/memory/transition.rs`
**Purpose:** In-memory storage implementation.

**New Maps:**
- `is_dynamic_map` - Boolean flag per transition ID
- `caller_inputs_map` - Caller inputs for dynamic transitions
- `caller_outputs_map` - Caller outputs for dynamic transitions

Initialized alongside existing transition metadata (TPK, TCM, SCM).

---

### RocksDB Storage

#### `helpers/rocksdb/internal/id.rs`
**Purpose:** RocksDB map ID enumeration.

**New IDs:**
- `IsDynamicMap` - Tracks dynamic status
- `TransitionCallerInputMap` - Caller inputs
- `TransitionCallerOutputMap` - Caller outputs
- `DynamicRecord` variants in `TransitionInputMap` and `TransitionOutputMap`

---

#### `helpers/rocksdb/transition.rs`
**Purpose:** RocksDB-backed transition storage.

**New Maps:**
- `is_dynamic_map: DataMap<N::TransitionID, bool>`
- `caller_inputs_map: DataMap<N::TransitionID, Vec<Input<N>>>`
- `caller_outputs_map: DataMap<N::TransitionID, Vec<Output<N>>>`

Properly initialized in `open()` with corresponding RocksDB map IDs.

---

### Transaction Storage

#### `transaction/deployment.rs`
**Purpose:** Deployment transaction storage.

**Changes:**
- Support for `translation_verifying_keys` and `translation_certificates`
- Storage of translation keys/certificates for record types
- Removal logic for translation keys when deployments are removed

---

#### `transaction/mod.rs`
**Purpose:** Transaction storage abstraction.

**Changes:**
- Integration with updated `TransitionStorage` trait
- Pass-through methods for transition store access

---

## Storage Architecture

**Three-Tier Map System:**
1. **Indicator Maps:** `is_dynamic_map` - boolean flags
2. **Data Maps:** `caller_inputs_map`, `caller_outputs_map` - actual data
3. **Atomic Operations:** Ensure consistency across all maps

**Database ID Organization:**
- Sequential enum values in `DataID`
- Backward compatibility comments
- Reserved IDs for future features

---

## Testing Notes

**What's Tested:**
- Insert/get round-trip for dynamic transitions
- Caller metadata reconstruction
- Dynamic record input/output handling

---

## Security Considerations

1. **Atomic Operations:** All three maps updated together for consistency.

2. **Optional Storage:** Caller inputs/outputs only stored if transition is dynamic.

3. **Backward Compatibility:** Pre-existing transitions work with `caller_metadata = None`.
