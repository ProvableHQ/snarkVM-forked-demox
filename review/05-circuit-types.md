# circuit/types - Integer Circuit Operations

This document covers the 7 files changed in `circuit/types`, which implements circuit primitives for integers and booleans.

## Overview

The `circuit/types` crate provides low-level circuit primitives. For dynamic dispatch, changes relate to **error handling propagation** in constraint enforcement, not dynamic dispatch logic itself.

## Files Requiring Review

### `integers/src/mul_checked.rs`
**Purpose:** Checked multiplication with overflow detection.

**Changes:** Added `.expect()` calls for error handling from `E::enforce`:

```rust
E::assert_eq(positive_product_overflows, E::zero())
    .expect("Signed multiplication positive overflow check failed");

E::assert_eq(negative_product_underflows, E::zero())
    .expect("Signed multiplication negative underflow check failed");

E::enforce(...).expect("Integer multiplication constraint unsatisfied");

E::assert_eq(z2, E::zero())
    .expect("Karatsuba multiplication overflow check failed");
```

**Key Constraints:**
- Karatsuba multiplication algorithm
- Overflow checking for signed/unsigned integers
- Two paths: direct field multiplication or Karatsuba

---

### `integers/src/pow_checked.rs`
**Purpose:** Checked exponentiation with overflow detection.

**Changes:** Added `.expect()` calls:

```rust
E::assert_eq(overflow & bit, E::zero())
    .expect("Integer power overflow check failed");
```

**Key Constraints:**
- Bit-by-bit exponentiation loop
- Overflow flag tracking during multiplication
- Sign handling for negative base

---

### `integers/src/mul_wrapped.rs`
**Purpose:** Wrapped multiplication (allows overflow).

**Changes:** No constraint enforcement changes.

---

### `integers/src/shl_wrapped.rs`
**Purpose:** Left shift with wrapping.

**Changes:** No enforcement changes.

---

### `integers/src/shr_wrapped.rs`
**Purpose:** Right shift with wrapping.

**Changes:** No enforcement changes.

---

### `integers/src/lib.rs`
**Purpose:** Core integer circuit type.

**Changes:** Updated to propagate error types from `E::enforce` changes.

---

### `boolean/src/lib.rs`
**Purpose:** Boolean circuit type.

**Changes:** No direct modifications detected.

---

## Test Files

All files contain comprehensive test modules with macros:
- `test_integer_case!`
- `test_integer_binary!`

---

## Testing Notes

**What's Tested:**
- Overflow detection for all integer sizes (i8-i128, u8-u128)
- Constraint generation counts
- Mode preservation

---

## Security Considerations

1. **Error Propagation:** The `.expect()` calls are temporary pending full circuit type refactoring. In production, these should never panic as constraints are validated during synthesis.

2. **Overflow Protection:** Checked operations properly detect and reject overflows via constraint enforcement.
