# Miscellaneous Changes

This document covers remaining files not in the main crate documentation.

## CI Configuration

### `.circleci/config.yml`
**Purpose:** CircleCI build configuration.

**Changes:**
- Updated build cache configurations
- Dependency updates for new crates
- Test job configurations for V14 features

---

## Cargo Configuration

### `Cargo.toml` (Root)
**Purpose:** Workspace configuration.

**Changes:**
- Dependency version updates
- New workspace members if any

### `Cargo.lock`
**Purpose:** Locked dependency versions.

**Changes:** Updated for new dependencies and versions.

---

## Crate Cargo.toml Files

### `algorithms/Cargo.toml`
**Changes:**
- Added `snark-print` feature flag for debug printing

### `circuit/environment/Cargo.toml`
**Changes:**
- Error type dependency support

### `console/collections/Cargo.toml`
**Changes:**
- Added `test-utils` feature flag

### `console/program/Cargo.toml`
**Changes:**
- Added `itertools` dependency
- Added `criterion` dev dependency
- Added `dynamic_data` benchmark

### `ledger/block/Cargo.toml`
**Changes:**
- Updates for versioning types

### `ledger/Cargo.toml`
**Changes:**
- Updates for store dependencies

### `synthesizer/Cargo.toml`
**Changes:**
- Updates for V14 features

### `synthesizer/process/Cargo.toml`
**Changes:**
- Updates for translation module

---

## VM Package

### `vm/package/build.rs`
**Purpose:** Package build configuration.

**Changes:**
- Support for translation key generation in package builds

### `vm/package/execute.rs`
**Purpose:** Package execution.

**Changes:**
- Updates for dynamic dispatch execution

---

## Ledger Support Files

### `ledger/narwhal/data/src/lib.rs`
**Purpose:** Narwhal data structures.

**Changes:**
- Minor updates for compatibility

### `ledger/test-helpers/src/lib.rs`
**Purpose:** Test helpers for ledger.

**Changes:**
- Updates for V14 test scenarios

---

## Synthesizer Support Files

### `synthesizer/snark/src/proving_key/mod.rs`
### `synthesizer/snark/src/verifying_key/mod.rs`
**Purpose:** SNARK key management.

**Changes:**
- Support for translation proving/verifying keys

---

## Feature Flags Summary

| Crate | Flag | Purpose |
|-------|------|---------|
| algorithms | `snark-print` | Debug printing for Varuna |
| console/collections | `test-utils` | Merkle tree test utilities |
| console/program | (benchmarks) | Dynamic data benchmarks |

---

## Dependency Changes

**New Dependencies:**
- `itertools` in console/program

**Updated Dependencies:**
- Various version bumps in Cargo.lock

---

## Notes for Review

These files are generally:
1. **Configuration** - Low risk, standard updates
2. **Build Support** - Infrastructure for new features
3. **Test Helpers** - Supporting test code

**Priority:** Lower priority than core logic files, but should be reviewed for:
- Correct feature flag configuration
- Proper dependency versioning
- CI job completeness
