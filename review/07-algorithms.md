# algorithms - Varuna SNARK Prover

This document covers the 6 files changed in `algorithms`, which implements the Varuna SNARK proof system.

## Overview

The `algorithms` crate provides cryptographic primitives including the Varuna SNARK. For dynamic dispatch, changes focus on:

1. **Batch proof validation** - Ensuring data structure consistency
2. **Diagnostic capabilities** - Debug printing for troubleshooting
3. **State synchronization checks** - Guards in prover rounds

## Files Requiring Review

### `Cargo.toml`
**Purpose:** Package configuration.

**Changes:** Added feature flag:
```toml
snark-print = [ ]
```

Enables verbose debug printing in Varuna prover/verifier without impacting production performance.

---

### `src/fft/domain.rs`
**Purpose:** FFT domain operations for polynomial arithmetic.

**Changes:**
- Reorganized imports (moved to method scope)
- Removed hardcoded chunk size in `apply_butterfly()` macro call

**Functions Modified:**
- `evaluate_all_lagrange_coefficients()` - Lagrange polynomial evaluation
- `mul_polynomials_in_evaluation_domain()` - Polynomial multiplication
- `apply_butterfly()` - FFT butterfly operation

---

### `src/snark/varuna/ahp/prover/round_functions/mod.rs`
**Purpose:** Varuna prover initialization. **Important for review.**

**Changes:** Added validation in `init_prover()`:
```rust
if randomizing_assignments.len() != circuits_to_constraints.len() {
    return Err(AHPError::AnyhowError(anyhow::anyhow!(
        "[prover Init] Expected {} randomizing assignments, but {} were provided.",
        circuits_to_constraints.len(),
        randomizing_assignments.len()
    )));
}
```

Ensures randomizing assignments count matches circuit count for batch proofs.

---

### `src/snark/varuna/ahp/prover/round_functions/prepare_third.rs`
**Purpose:** Third round preparation for Varuna V2. **Important for review.**

**Changes:** Added validation in `calculate_prep_lineval_sumcheck_witness()`:
```rust
anyhow::ensure!(
    state.circuit_specific_states.len() == fft_precomputations.len(),
    "[calculate Prep Lineval Sumcheck Witness] ..."
);
anyhow::ensure!(
    state.circuit_specific_states.len() == assignments.len(),
    "[calculate Prep Lineval Sumcheck Witness] ..."
);
anyhow::ensure!(
    state.circuit_specific_states.len() == matrix_transposes.len(),
    "[calculate Prep Lineval Sumcheck Witness] ..."
);
```

Validates FFT precomputations, assignments, and matrix transposes are aligned.

---

### `src/snark/varuna/ahp/prover/round_functions/third.rs`
**Purpose:** Third round witness computation. **Important for review.**

**Changes:** Added matching validation in `calculate_lineval_sumcheck_witness()`:
```rust
ensure!(
    state.circuit_specific_states.len() == third_round_batch_combiners.len(),
    "[calculate Lineval Sumcheck Witness] ..."
);
ensure!(
    state.circuit_specific_states.len() == assignments.len(),
    "[calculate Lineval Sumcheck Witness] ..."
);
ensure!(
    state.circuit_specific_states.len() == matrix_transposes.len(),
    "[calculate Lineval Sumcheck Witness] ..."
);
```

Works with V1/V2 dispatch logic for different Varuna versions.

---

### `src/snark/varuna/varuna.rs`
**Purpose:** Main Varuna SNARK implementation. **High priority for review.**

**Changes in `prove_batch()`:**
- Added diagnostic printing for batch sizes (behind `snark-print` feature)
- Added logging for final challenge gamma

**Changes in `verify_batch()`:**
- Added validation for batch size consistency:
```rust
ensure!(
    keys_to_inputs.len() == batch_sizes_vec.len(),
    "[verify batch] Expected {} keys to inputs, but {} were provided.",
    batch_sizes_vec.len(),
    keys_to_inputs.len()
);
```

- Added commitment count validation:
```rust
ensure!(comms.g_a_commitments.len() == comms.g_b_commitments.len(), ...);
ensure!(comms.g_a_commitments.len() == comms.g_c_commitments.len(), ...);
ensure!(comms.g_a_commitments.len() == circuit_ids.len(), ...);
```

- Added circuit commitment count validation:
```rust
ensure!(
    circuit_commitments.len() == circuit_ids.len(),
    "[verify Batch] Expected {} circuit commitments, but {} were provided.",
    circuit_ids.len(),
    circuit_commitments.len()
);
```

- Added diagnostic printing for verification details

---

## Test Files

No dedicated test files for these changes. Testing is via integration tests in synthesizer.

---

## Testing Notes

**What's Tested:**
- Batch proof generation and verification consistency
- Multi-circuit proof composition
- State synchronization across proof rounds

---

## Security Considerations

1. **State Consistency:** Guards ensure all data structures are properly aligned before polynomial operations, preventing undefined behavior.

2. **Commitment Matching:** Validates that g_a, g_b, g_c commitments match expected counts, ensuring proof integrity.

3. **Early Failure:** Validation checks fail fast with descriptive errors rather than producing invalid proofs.

4. **Debug Isolation:** `snark-print` feature keeps diagnostic output out of production builds.

---

## Connection to Dynamic Dispatch

These changes strengthen Varuna for dynamic dispatch by ensuring batch proof verification is robust when:
- Multiple circuits are verified together
- Translation proofs accompany execution proofs
- Complex multi-program call graphs require proof composition
