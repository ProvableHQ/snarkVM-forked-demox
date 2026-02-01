// Copyright (c) 2019-2026 Provable Inc.
// This file is part of the snarkVM library.

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at:

// http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::*;

impl<A: Aleo> Equal<Self> for DynamicRecord<A> {
    type Output = Boolean<A>;

    /// Returns `true` if `self` and `other` are equal.
    fn is_equal(&self, other: &Self) -> Self::Output {
        // Check the `owner`, `root`, `nonce`, and `version`.
        self.owner.is_equal(&other.owner)
            & self.root.is_equal(&other.root)
            & self.nonce.is_equal(&other.nonce)
            & self.version.is_equal(&other.version)
    }

    /// Returns `true` if `self` and `other` are *not* equal.
    fn is_not_equal(&self, other: &Self) -> Self::Output {
        !self.is_equal(other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Circuit;
    use snarkvm_circuit_types::environment::{Eject, Inject, Mode, assert_scope};
    use snarkvm_utilities::{TestRng, Uniform};

    type CurrentNetwork = <Circuit as Environment>::Network;

    /// Creates a sample dynamic record for testing.
    fn sample_dynamic_record(mode: Mode, rng: &mut TestRng) -> DynamicRecord<Circuit> {
        let owner = console::Address::<CurrentNetwork>::rand(rng);
        let root = console::Field::<CurrentNetwork>::rand(rng);
        let nonce = console::Group::<CurrentNetwork>::rand(rng);
        let version = console::U8::<CurrentNetwork>::rand(rng);
        let console_record = console::DynamicRecord::new_unchecked(owner, root, nonce, version, None);
        DynamicRecord::new(mode, console_record)
    }

    /// Tests that `is_equal` returns true when comparing a record to itself.
    fn check_is_equal_on_equal(
        mode: Mode,
        num_constants: u64,
        num_public: u64,
        num_private: u64,
        num_constraints: u64,
    ) -> Result<()> {
        let rng = &mut TestRng::default();

        // Sample a dynamic record.
        let record = sample_dynamic_record(mode, rng);

        Circuit::scope(format!("{mode}"), || {
            let candidate = record.is_equal(&record);
            assert!(candidate.eject_value());
            assert_scope!(num_constants, num_public, num_private, num_constraints);
        });

        Circuit::reset();
        Ok(())
    }

    /// Tests that `is_equal` returns false when comparing two different records.
    fn check_is_equal_on_unequal(
        mode: Mode,
        num_constants: u64,
        num_public: u64,
        num_private: u64,
        num_constraints: u64,
    ) -> Result<()> {
        let rng = &mut TestRng::default();

        // Sample two distinct dynamic records for comparison.
        let record = sample_dynamic_record(mode, rng);
        let mismatched_record = sample_dynamic_record(mode, rng);

        Circuit::scope(format!("{mode}"), || {
            let candidate = record.is_equal(&mismatched_record);
            assert!(!candidate.eject_value());
            assert_scope!(num_constants, num_public, num_private, num_constraints);
        });

        Circuit::reset();
        Ok(())
    }

    /// Tests that `is_not_equal` returns true when comparing two different records.
    fn check_is_not_equal_on_unequal(
        mode: Mode,
        num_constants: u64,
        num_public: u64,
        num_private: u64,
        num_constraints: u64,
    ) -> Result<()> {
        let rng = &mut TestRng::default();

        // Sample two distinct dynamic records for comparison.
        let record = sample_dynamic_record(mode, rng);
        let mismatched_record = sample_dynamic_record(mode, rng);

        Circuit::scope(format!("{mode}"), || {
            let candidate = record.is_not_equal(&mismatched_record);
            assert!(candidate.eject_value());
            assert_scope!(num_constants, num_public, num_private, num_constraints);
        });

        Circuit::reset();
        Ok(())
    }

    /// Tests that `is_not_equal` returns false when comparing a record to itself.
    fn check_is_not_equal_on_equal(
        mode: Mode,
        num_constants: u64,
        num_public: u64,
        num_private: u64,
        num_constraints: u64,
    ) -> Result<()> {
        let rng = &mut TestRng::default();

        // Sample a dynamic record.
        let record = sample_dynamic_record(mode, rng);

        Circuit::scope(format!("{mode}"), || {
            let candidate = record.is_not_equal(&record);
            assert!(!candidate.eject_value());
            assert_scope!(num_constants, num_public, num_private, num_constraints);
        });

        Circuit::reset();
        Ok(())
    }

    // Tests for `is_equal` on equal values (self-comparison).

    #[test]
    fn test_is_equal_on_equal_constant() -> Result<()> {
        check_is_equal_on_equal(Mode::Constant, 6, 0, 11, 17)
    }

    #[test]
    fn test_is_equal_on_equal_public() -> Result<()> {
        check_is_equal_on_equal(Mode::Public, 6, 0, 11, 17)
    }

    #[test]
    fn test_is_equal_on_equal_private() -> Result<()> {
        check_is_equal_on_equal(Mode::Private, 6, 0, 11, 17)
    }

    // Tests for `is_equal` on unequal values (different records).

    #[test]
    fn test_is_equal_on_unequal_constant() -> Result<()> {
        check_is_equal_on_unequal(Mode::Constant, 0, 0, 17, 17)
    }

    #[test]
    fn test_is_equal_on_unequal_public() -> Result<()> {
        check_is_equal_on_unequal(Mode::Public, 0, 0, 17, 17)
    }

    #[test]
    fn test_is_equal_on_unequal_private() -> Result<()> {
        check_is_equal_on_unequal(Mode::Private, 0, 0, 17, 17)
    }

    // Tests for `is_not_equal` on unequal values (different records).

    #[test]
    fn test_is_not_equal_on_unequal_constant() -> Result<()> {
        check_is_not_equal_on_unequal(Mode::Constant, 0, 0, 17, 17)
    }

    #[test]
    fn test_is_not_equal_on_unequal_public() -> Result<()> {
        check_is_not_equal_on_unequal(Mode::Public, 0, 0, 17, 17)
    }

    #[test]
    fn test_is_not_equal_on_unequal_private() -> Result<()> {
        check_is_not_equal_on_unequal(Mode::Private, 0, 0, 17, 17)
    }

    // Tests for `is_not_equal` on equal values (self-comparison).

    #[test]
    fn test_is_not_equal_on_equal_constant() -> Result<()> {
        check_is_not_equal_on_equal(Mode::Constant, 6, 0, 11, 17)
    }

    #[test]
    fn test_is_not_equal_on_equal_public() -> Result<()> {
        check_is_not_equal_on_equal(Mode::Public, 6, 0, 11, 17)
    }

    #[test]
    fn test_is_not_equal_on_equal_private() -> Result<()> {
        check_is_not_equal_on_equal(Mode::Private, 6, 0, 11, 17)
    }
}
