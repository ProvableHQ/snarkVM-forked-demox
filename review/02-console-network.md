# console/network - Network Constants and Consensus

This document covers the 6 files changed in `console/network`, which defines network-specific constants and consensus version configuration for dynamic dispatch.

## Overview

The `console/network` crate defines the `Network` trait and network-specific implementations (Mainnet, Testnet, Canary). For dynamic dispatch, this crate:

1. Introduces **V14 consensus version** for dynamic dispatch activation
2. Adds **translation circuit keys** for credits.aleo record migration
3. Defines **MAX_BATCH_PROOF_INSTANCES** for batch proof verification

## Files Requiring Review

### `src/consensus_heights.rs`
**Purpose:** Defines consensus versions and their activation heights per network. **High priority for review.**

**Changes:**

Added two new consensus version variants:
```rust
pub enum ConsensusVersion {
    // ... existing versions ...
    V13 = 13,  // Introduces external structs
    V14 = 14,  // Dynamic dispatch
}
```

**Note:** V13 was introduced in staging to support external structs. V14 (dynamic dispatch) builds on top of V13.

**V14 Activation Heights:**

| Network | Height | Status |
|---------|--------|--------|
| Canary | 999,999,999 | Placeholder |
| Mainnet | 999,999,999 | Placeholder |
| Testnet | 999,999,999 | Placeholder |
| Test (feature) | 17 | Active for testing |

**Note:** The 999,999,999 heights indicate V14 is not yet scheduled for mainnet activation.

**Serialization:** Updated `FromBytes`/`ToBytes` to handle V13 and V14.

---

### `src/lib.rs`
**Purpose:** Defines the `Network` trait with network-wide constants and methods.

**Changes:**

**New Constant:**
```rust
const MAX_BATCH_PROOF_INSTANCES: usize = 128;
```
Limits the number of proof instances that can be verified in a single batch.

**New Trait Methods:**
```rust
// Non-WASM
fn translation_credits_proving_key() -> &'static Arc<VarunaProvingKey<Self>>;

// WASM (accepts optional external bytes)
fn translation_credits_proving_key(bytes: Option<Vec<u8>>) -> &'static Arc<VarunaProvingKey<Self>>;

fn translation_credits_verifying_key() -> &'static Arc<VarunaVerifyingKey<Self>>;
```

These methods provide access to translation circuit keys for converting between static and dynamic record formats.

**Additional Change:** Added TODO comment questioning whether `MAX_RECORDS` should be reduced.

---

### `src/mainnet_v0.rs`
**Purpose:** Mainnet network implementation.

**Changes:** Implemented translation circuit key methods.

**Implementation Pattern:**
```rust
fn translation_credits_proving_key() -> &'static Arc<VarunaProvingKey<Self>> {
    static KEY: OnceLock<Arc<VarunaProvingKey<MainnetV0>>> = OnceLock::new();
    KEY.get_or_init(|| {
        // Load from snarkvm_parameters::mainnet::TRANSLATION_CREDITS_PROVING_KEY
        // Skip first byte (version encoding)
    })
}
```

**Features:**
- Uses `OnceLock` for thread-safe lazy initialization
- WASM variant supports custom key bytes with checksum validation
- Loads from pre-generated parameter files

---

### `src/testnet_v0.rs`
**Purpose:** Testnet network implementation.

**Changes:** Identical pattern to MainnetV0, referencing `snarkvm_parameters::testnet::*` variants.

---

### `src/canary_v0.rs`
**Purpose:** Canary network implementation.

**Changes:** Identical pattern to MainnetV0, referencing `snarkvm_parameters::canary::*` variants.

---

### `environment/src/lib.rs`
**Purpose:** Environment prelude with common imports.

**Changes:** Added `many_m_n` parser combinator to nom imports.

```rust
// Added to nom::multi imports
many_m_n
```

This combinator parses m to n repetitions of a pattern, needed for dynamic dispatch program parsing.

---

## Test Files

No dedicated test files - testing is embedded in consensus height constants validation.

---

## Testing Notes

**Coverage:**
- Consensus version serialization is tested via existing bytes tests
- Translation key loading is tested via parameter crate tests
- V14 behavior is tested via synthesizer integration tests (see `synthesizer/src/vm/tests/test_v14/`)

**What's Tested:**
- V14 activates at height 17 in test mode (feature `test_consensus_heights`)
- Translation keys load correctly from parameter files

---

## Security Considerations

1. **Consensus Gating:** V14 features are gated by block height, ensuring coordinated network activation.

2. **Translation Keys:** WASM variant validates checksums when loading external key bytes.

3. **Batch Proof Limits:** MAX_BATCH_PROOF_INSTANCES prevents DoS via oversized batch proofs.

4. **Placeholder Heights:** Using 999,999,999 ensures V14 doesn't accidentally activate before scheduled.
