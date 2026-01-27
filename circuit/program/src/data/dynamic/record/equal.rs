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
    use snarkvm_circuit_types::environment::{Inject, Mode, assert_scope};
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

    /// Creates a mismatched dynamic record for testing.
    fn sample_mismatched_dynamic_record(mode: Mode, rng: &mut TestRng) -> DynamicRecord<Circuit> {
        // Create a different record with different owner.
        let owner = console::Address::<CurrentNetwork>::rand(rng);
        let root = console::Field::<CurrentNetwork>::rand(rng);
        let nonce = console::Group::<CurrentNetwork>::rand(rng);
        let version = console::U8::<CurrentNetwork>::rand(rng);
        let console_record = console::DynamicRecord::new_unchecked(owner, root, nonce, version, None);
        DynamicRecord::new(mode, console_record)
    }

    fn check_is_equal(
        mode: Mode,
        num_constants: u64,
        num_public: u64,
        num_private: u64,
        num_constraints: u64,
    ) -> Result<()> {
        let rng = &mut TestRng::default();

        // Sample the dynamic records.
        let record = sample_dynamic_record(mode, rng);
        let mismatched_record = sample_mismatched_dynamic_record(mode, rng);

        Circuit::scope(format!("{mode}"), || {
            let candidate = record.is_equal(&record);
            assert!(candidate.eject_value());
            assert_scope!(<=num_constants, <=num_public, <=num_private, <=num_constraints);
        });

        Circuit::scope(format!("{mode}"), || {
            let candidate = record.is_equal(&mismatched_record);
            assert!(!candidate.eject_value());
            assert_scope!(<=num_constants, <=num_public, <=num_private, <=num_constraints);
        });

        Circuit::reset();
        Ok(())
    }

    fn check_is_not_equal(
        mode: Mode,
        num_constants: u64,
        num_public: u64,
        num_private: u64,
        num_constraints: u64,
    ) -> Result<()> {
        let rng = &mut TestRng::default();

        // Sample the dynamic records.
        let record = sample_dynamic_record(mode, rng);
        let mismatched_record = sample_mismatched_dynamic_record(mode, rng);

        Circuit::scope(format!("{mode}"), || {
            let candidate = record.is_not_equal(&mismatched_record);
            assert!(candidate.eject_value());
            assert_scope!(<=num_constants, <=num_public, <=num_private, <=num_constraints);
        });

        Circuit::scope(format!("{mode}"), || {
            let candidate = record.is_not_equal(&record);
            assert!(!candidate.eject_value());
            assert_scope!(<=num_constants, <=num_public, <=num_private, <=num_constraints);
        });

        Circuit::reset();
        Ok(())
    }

    #[test]
    fn test_is_equal_constant() -> Result<()> {
        check_is_equal(Mode::Constant, 10, 0, 20, 20)
    }

    #[test]
    fn test_is_equal_public() -> Result<()> {
        check_is_equal(Mode::Public, 10, 0, 20, 20)
    }

    #[test]
    fn test_is_equal_private() -> Result<()> {
        check_is_equal(Mode::Private, 10, 0, 20, 20)
    }

    #[test]
    fn test_is_not_equal_constant() -> Result<()> {
        check_is_not_equal(Mode::Constant, 10, 0, 20, 20)
    }

    #[test]
    fn test_is_not_equal_public() -> Result<()> {
        check_is_not_equal(Mode::Public, 10, 0, 20, 20)
    }

    #[test]
    fn test_is_not_equal_private() -> Result<()> {
        check_is_not_equal(Mode::Private, 10, 0, 20, 20)
    }
}
