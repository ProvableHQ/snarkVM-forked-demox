# parameters - Translation Circuit Keys

This document covers the 12 files changed in `parameters`, which provides pre-computed translation circuit parameters.

## Overview

The `parameters` crate provides cryptographic parameters. For dynamic dispatch, this crate:

1. **Adds translation circuit parameters** for credits.aleo
2. **Provides generation scripts** for all networks
3. **Includes pre-computed verifier keys** for instant verification

## Files Requiring Review

### Network Implementations

#### `src/canary/mod.rs`, `src/mainnet/mod.rs`, `src/testnet/mod.rs`
**Purpose:** Network-specific parameter implementations.

**Infrastructure:**
- Translation verifying key loading
- Macro implementations (`impl_remote!`, `impl_local!`)
- Support for multiple network versions

---

### Translation Generation

#### `examples/translation.rs`
**Purpose:** Example code for generating translation circuit parameters.

**Key Function:**
```rust
fn sample_assignment() -> TranslationAssignment {
    // Creates TranslationAssignment with:
    // - Record static/dynamic conversions
    // - Function ID, TVK, index tracking
    // - Dynamic record ID computation
}
```

**Types Used:**
- `DynamicRecord`
- `TranslationAssignment`
- `TranslationAssignmentCircuit`

---

#### `scripts/canary/translation.sh`, `scripts/mainnet/translation.sh`, `scripts/testnet/translation.sh`
**Purpose:** Shell scripts for generating and deploying translation parameters.

**Function:**
- Execute translation circuit parameter generation
- Invoke translation example binary
- Deploy generated keys to parameter repositories

---

### Pre-computed Parameters

#### `translation_credits.metadata`
**Purpose:** Translation circuit metadata.

**Contents:** JSON/binary format with translation circuit information.

---

#### `translation_credits.verifier`
**Purpose:** Serialized verifying key for translation proofs.

**Usage:** Enables instant verification without runtime generation.

---

### Network-Specific Resources

#### `src/canary/resources/credits/translation_credits.metadata`, `translation_credits.verifier`
#### `src/mainnet/resources/credits/translation_credits.metadata`, `translation_credits.verifier`
#### `src/testnet/resources/credits/translation_credits.metadata`, `translation_credits.verifier`

**Purpose:** Network-specific copies of translation parameters.

---

## Parameter Architecture

**Generation Flow:**
1. `examples/translation.rs` generates sample assignments
2. Build scripts invoke translation parameter generation
3. Output stored as `.metadata` and `.verifier` files
4. Resources committed to repository for deployment

**Verification Flow:**
1. Network loads translation verifying key from resources
2. Translation proofs verified using pre-computed key
3. No runtime key generation needed

---

## Testing Notes

**What's Tested:**
- Parameter generation produces valid keys
- Verifying keys match expected format
- Keys work with translation circuit

---

## Security Considerations

1. **Pre-computed Keys:** Verifier keys committed to repository ensure consistent verification.

2. **Network-Specific:** Each network has dedicated translation parameters.

3. **Metadata Integrity:** Metadata files enable verification of parameter correctness.
