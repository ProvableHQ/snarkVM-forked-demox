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

impl<A: Aleo> Equal<Self> for DynamicFuture<A> {
    type Output = Boolean<A>;

    /// Returns `true` if `self` and `other` are equal.
    fn is_equal(&self, other: &Self) -> Self::Output {
        self.program_name.is_equal(&other.program_name)
            & self.program_network.is_equal(&other.program_network)
            & self.function_name.is_equal(&other.function_name)
            & self.root.is_equal(&other.root)
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

    /// Creates a sample dynamic future for testing.
    fn sample_dynamic_future(mode: Mode, rng: &mut TestRng) -> DynamicFuture<Circuit> {
        let program_name = console::Field::<CurrentNetwork>::rand(rng);
        let program_network = console::Field::<CurrentNetwork>::rand(rng);
        let function_name = console::Field::<CurrentNetwork>::rand(rng);
        let root = console::Field::<CurrentNetwork>::rand(rng);
        let console_future =
            console::DynamicFuture::new_unchecked(program_name, program_network, function_name, root, None);
        DynamicFuture::new(mode, console_future)
    }

    /// Tests that `is_equal` returns true when comparing a future to itself.
    fn check_is_equal_on_equal(
        mode: Mode,
        num_constants: u64,
        num_public: u64,
        num_private: u64,
        num_constraints: u64,
    ) -> Result<(), console::Error> {
        let rng = &mut TestRng::default();

        // Sample a dynamic future.
        let future = sample_dynamic_future(mode, rng);

        Circuit::scope(format!("{mode}"), || {
            let candidate = future.is_equal(&future);
            assert!(candidate.eject_value());
            assert_scope!(num_constants, num_public, num_private, num_constraints);
        });

        Circuit::reset();
        Ok(())
    }

    /// Tests that `is_equal` returns false when comparing two different futures.
    fn check_is_equal_on_unequal(
        mode: Mode,
        num_constants: u64,
        num_public: u64,
        num_private: u64,
        num_constraints: u64,
    ) -> Result<(), console::Error> {
        let rng = &mut TestRng::default();

        // Sample two distinct dynamic futures for comparison.
        let future = sample_dynamic_future(mode, rng);
        let mismatched_future = sample_dynamic_future(mode, rng);

        Circuit::scope(format!("{mode}"), || {
            let candidate = future.is_equal(&mismatched_future);
            assert!(!candidate.eject_value());
            assert_scope!(num_constants, num_public, num_private, num_constraints);
        });

        Circuit::reset();
        Ok(())
    }

    /// Tests that `is_not_equal` returns true when comparing two different futures.
    fn check_is_not_equal_on_unequal(
        mode: Mode,
        num_constants: u64,
        num_public: u64,
        num_private: u64,
        num_constraints: u64,
    ) -> Result<(), console::Error> {
        let rng = &mut TestRng::default();

        // Sample two distinct dynamic futures for comparison.
        let future = sample_dynamic_future(mode, rng);
        let mismatched_future = sample_dynamic_future(mode, rng);

        Circuit::scope(format!("{mode}"), || {
            let candidate = future.is_not_equal(&mismatched_future);
            assert!(candidate.eject_value());
            assert_scope!(num_constants, num_public, num_private, num_constraints);
        });

        Circuit::reset();
        Ok(())
    }

    /// Tests that `is_not_equal` returns false when comparing a future to itself.
    fn check_is_not_equal_on_equal(
        mode: Mode,
        num_constants: u64,
        num_public: u64,
        num_private: u64,
        num_constraints: u64,
    ) -> Result<(), console::Error> {
        let rng = &mut TestRng::default();

        // Sample a dynamic future.
        let future = sample_dynamic_future(mode, rng);

        Circuit::scope(format!("{mode}"), || {
            let candidate = future.is_not_equal(&future);
            assert!(!candidate.eject_value());
            assert_scope!(num_constants, num_public, num_private, num_constraints);
        });

        Circuit::reset();
        Ok(())
    }

    // Tests for `is_equal` on equal values (self-comparison).

    #[test]
    fn test_is_equal_on_equal_constant() -> Result<(), console::Error> {
        check_is_equal_on_equal(Mode::Constant, 4, 0, 0, 0)
    }

    #[test]
    fn test_is_equal_on_equal_public() -> Result<(), console::Error> {
        check_is_equal_on_equal(Mode::Public, 4, 0, 7, 11)
    }

    #[test]
    fn test_is_equal_on_equal_private() -> Result<(), console::Error> {
        check_is_equal_on_equal(Mode::Private, 4, 0, 7, 11)
    }

    // Tests for `is_equal` on unequal values (different futures).

    #[test]
    fn test_is_equal_on_unequal_constant() -> Result<(), console::Error> {
        check_is_equal_on_unequal(Mode::Constant, 4, 0, 0, 0)
    }

    #[test]
    fn test_is_equal_on_unequal_public() -> Result<(), console::Error> {
        check_is_equal_on_unequal(Mode::Public, 0, 0, 11, 11)
    }

    #[test]
    fn test_is_equal_on_unequal_private() -> Result<(), console::Error> {
        check_is_equal_on_unequal(Mode::Private, 0, 0, 11, 11)
    }

    // Tests for `is_not_equal` on unequal values (different futures).

    #[test]
    fn test_is_not_equal_on_unequal_constant() -> Result<(), console::Error> {
        check_is_not_equal_on_unequal(Mode::Constant, 4, 0, 0, 0)
    }

    #[test]
    fn test_is_not_equal_on_unequal_public() -> Result<(), console::Error> {
        check_is_not_equal_on_unequal(Mode::Public, 0, 0, 11, 11)
    }

    #[test]
    fn test_is_not_equal_on_unequal_private() -> Result<(), console::Error> {
        check_is_not_equal_on_unequal(Mode::Private, 0, 0, 11, 11)
    }

    // Tests for `is_not_equal` on equal values (self-comparison).

    #[test]
    fn test_is_not_equal_on_equal_constant() -> Result<(), console::Error> {
        check_is_not_equal_on_equal(Mode::Constant, 4, 0, 0, 0)
    }

    #[test]
    fn test_is_not_equal_on_equal_public() -> Result<(), console::Error> {
        check_is_not_equal_on_equal(Mode::Public, 4, 0, 7, 11)
    }

    #[test]
    fn test_is_not_equal_on_equal_private() -> Result<(), console::Error> {
        check_is_not_equal_on_equal(Mode::Private, 4, 0, 7, 11)
    }
}
