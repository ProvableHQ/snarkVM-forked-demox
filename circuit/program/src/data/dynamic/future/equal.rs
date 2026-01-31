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

    fn check_is_equal(
        mode: Mode,
        num_constants: u64,
        num_public: u64,
        num_private: u64,
        num_constraints: u64,
    ) -> Result<(), console::Error> {
        let rng = &mut TestRng::default();

        // Sample the dynamic futures.
        let future = sample_dynamic_future(mode, rng);
        let mismatched_future = sample_dynamic_future(mode, rng);

        Circuit::scope(format!("{mode}"), || {
            let candidate = future.is_equal(&future);
            assert!(candidate.eject_value());
            assert_scope!(<=num_constants, <=num_public, <=num_private, <=num_constraints);
        });

        Circuit::scope(format!("{mode}"), || {
            let candidate = future.is_equal(&mismatched_future);
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
    ) -> Result<(), console::Error> {
        let rng = &mut TestRng::default();

        // Sample the dynamic futures.
        let future = sample_dynamic_future(mode, rng);
        let mismatched_future = sample_dynamic_future(mode, rng);

        Circuit::scope(format!("{mode}"), || {
            let candidate = future.is_not_equal(&mismatched_future);
            assert!(candidate.eject_value());
            assert_scope!(<=num_constants, <=num_public, <=num_private, <=num_constraints);
        });

        Circuit::scope(format!("{mode}"), || {
            let candidate = future.is_not_equal(&future);
            assert!(!candidate.eject_value());
            assert_scope!(<=num_constants, <=num_public, <=num_private, <=num_constraints);
        });

        Circuit::reset();
        Ok(())
    }

    #[test]
    fn test_is_equal_constant() -> Result<(), console::Error> {
        check_is_equal(Mode::Constant, 4, 0, 0, 0)
    }

    #[test]
    fn test_is_equal_public() -> Result<(), console::Error> {
        check_is_equal(Mode::Public, 4, 0, 11, 11)
    }

    #[test]
    fn test_is_equal_private() -> Result<(), console::Error> {
        check_is_equal(Mode::Private, 4, 0, 11, 11)
    }

    #[test]
    fn test_is_not_equal_constant() -> Result<(), console::Error> {
        check_is_not_equal(Mode::Constant, 4, 0, 0, 0)
    }

    #[test]
    fn test_is_not_equal_public() -> Result<(), console::Error> {
        check_is_not_equal(Mode::Public, 4, 0, 11, 11)
    }

    #[test]
    fn test_is_not_equal_private() -> Result<(), console::Error> {
        check_is_not_equal(Mode::Private, 4, 0, 11, 11)
    }
}
