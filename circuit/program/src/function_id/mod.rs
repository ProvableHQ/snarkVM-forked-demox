// Copyright (c) 2019-2025 Provable Inc.
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

use crate::{Identifier, ProgramID};
use snarkvm_circuit_network::Aleo;
use snarkvm_circuit_types::{Field, U8, U16, environment::prelude::*};

/// Compute the function ID as `Hash(network_id, program_id.len(), program_id, function_name.len(), function_name)`.
pub fn compute_function_id<A: Aleo>(
    network_id: &U16<A>,
    program_id: &ProgramID<A>,
    function_name: &Identifier<A>,
    is_dynamic: bool,
) -> Field<A> {
    match is_dynamic {
        false => A::hash_bhp1024(
            &(
                network_id,
                program_id.name().size_in_bits(),
                program_id.name(),
                program_id.network().size_in_bits(),
                program_id.network(),
                function_name.size_in_bits(),
                function_name,
            )
                .to_bits_le(),
        ),
        true => {
            // Initialize the size in bits of a field as a console `U8`.
            let field_size_in_bits = match u8::try_from(snarkvm_console_program::Field::<A::Network>::SIZE_IN_BITS) {
                Ok(size) => snarkvm_console_program::U8::new(size),
                Err(_) => A::halt("Field size in bits exceeds u8 maximum"),
            };
            // Compute the function ID.
            A::hash_bhp1024(
                &(
                    network_id,
                    U8::<A>::constant(field_size_in_bits),
                    program_id.name().to_field(),
                    U8::<A>::constant(field_size_in_bits),
                    program_id.network().to_field(),
                    U8::<A>::constant(field_size_in_bits),
                    function_name.to_field(),
                )
                    .to_bits_le(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Circuit;
    use snarkvm_circuit_types::environment::{UpdatableCount, assert_scope};

    use snarkvm_console::network::MainnetV0;
    use snarkvm_console::types::U16 as ConsoleU16;
    use snarkvm_console_program::{Identifier as ConsoleIdentifier, ProgramID as ConsoleProgramID};

    use anyhow::Result;

    type CurrentNetwork = MainnetV0;

    fn check(
        mode: Mode,
        network_id: u16,
        program_id: &str,
        function_name: &str,
        is_dynamic: bool,
        expected_count: UpdatableCount,
    ) -> Result<()> {
        // Initialize the console values.
        let console_network_id = ConsoleU16::new(network_id);
        let console_program_id = ConsoleProgramID::from_str(program_id)?;
        let console_function_name = ConsoleIdentifier::from_str(function_name)?;

        // Compute the expected function ID.
        let expected = snarkvm_console_program::compute_function_id(
            &console_network_id,
            &console_program_id,
            &console_function_name,
            is_dynamic,
        )?;

        // Initialize the network ID as a constant.
        let network_id = U16::<Circuit>::constant(console_network_id);

        // Initialize the program ID.
        let program_id = ProgramID::<Circuit>::new(mode, console_program_id);

        // Initialize the function name.
        let function_name = Identifier::<Circuit>::new(mode, console_function_name);

        Circuit::scope(format!("compute_function_id"), || {
            let candidate = compute_function_id(&network_id, &program_id, &function_name, is_dynamic);
            assert_eq!(expected, candidate.eject_value());
            expected_count.assert_matches(
                Circuit::num_constants_in_scope(),
                Circuit::num_public_in_scope(),
                Circuit::num_private_in_scope(),
                Circuit::num_constraints_in_scope(),
            );
        });

        Circuit::reset();
        Ok(())
    }

    #[test]
    fn test_compute_function_id_constant() -> Result<()> {
        check(Mode::Constant, 0, "credits.aleo", "transfer_public", false, count_is!(18153, 0, 0, 0))?;
        check(Mode::Constant, 0, "credits.aleo", "transfer_private", false, count_is!(779, 0, 0, 0))?;
        check(Mode::Constant, 0, "credits.aleo", "transfer_public_to_private", false, count_is!(883, 0, 0, 0))?;
        check(Mode::Constant, 0, "token_registry.aleo", "transfer_public_to_private", false, count_is!(959, 0, 0, 0))?;
        check(Mode::Constant, 0, "my.aleo", "foo", false, count_is!(584, 0, 0, 0))?;

        check(Mode::Constant, 0, "credits.aleo", "transfer_public", true, count_is!(1512, 0, 0, 0))?;
        check(Mode::Constant, 1, "credits.aleo", "transfer_private", true, count_is!(1512, 0, 0, 0))?;
        check(Mode::Constant, 0, "credits.aleo", "transfer_public_to_private", true, count_is!(1512, 0, 0, 0))?;
        check(Mode::Constant, 1, "token_registry.aleo", "transfer_public_to_private", true, count_is!(1512, 0, 0, 0))?;
        check(Mode::Constant, 0, "my.aleo", "foo", true, count_is!(1512, 0, 0, 0))?;

        Ok(())
    }

    #[test]
    fn test_compute_function_id_public() -> Result<()> {
        check(Mode::Public, 0, "credits.aleo", "transfer_public", false, count_is!(18153, 0, 0, 0))?;
        check(Mode::Public, 0, "credits.aleo", "transfer_private", false, count_is!(779, 0, 0, 0))?;
        check(Mode::Public, 0, "credits.aleo", "transfer_public_to_private", false, count_is!(883, 0, 0, 0))?;
        check(Mode::Public, 0, "token_registry.aleo", "transfer_public_to_private", false, count_is!(959, 0, 0, 0))?;
        check(Mode::Public, 0, "my.aleo", "foo", false, count_is!(584, 0, 0, 0))?;

        check(Mode::Public, 0, "credits.aleo", "transfer_public", true, count_is!(1512, 0, 0, 0))?;
        check(Mode::Public, 1, "credits.aleo", "transfer_private", true, count_is!(1512, 0, 0, 0))?;
        check(Mode::Public, 0, "credits.aleo", "transfer_public_to_private", true, count_is!(1512, 0, 0, 0))?;
        check(Mode::Public, 1, "token_registry.aleo", "transfer_public_to_private", true, count_is!(1512, 0, 0, 0))?;
        check(Mode::Public, 0, "my.aleo", "foo", true, count_is!(1512, 0, 0, 0))?;

        Ok(())
    }
}
