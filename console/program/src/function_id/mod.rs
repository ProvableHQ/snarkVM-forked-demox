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
use snarkvm_console_account::{ToBits, ToField};
use snarkvm_console_algorithms::Result;
use snarkvm_console_network::Network;
use snarkvm_console_types::{Field, U8, U16};

/// Compute the function ID.
///
/// If the `is_dynamic` flag is set to `false`, then the hash is computed as:
/// `Hash(network_id, program_id.len(), program_id, function_name.len(), function_name)`.
///
/// If the `is_dynamic` flag is set to `true``, the function ID is computed as:
/// `Hash(network_id, program_name.to_field(), program_network.to_field(), function_name.to_field()`.
/// This ensures that the function ID is not dependent on the lengths of the program ID and function name,
pub fn compute_function_id<N: Network>(
    network_id: &U16<N>,
    program_id: &ProgramID<N>,
    function_name: &Identifier<N>,
    is_dynamic: bool,
) -> Result<Field<N>> {
    match is_dynamic {
        false => N::hash_bhp1024(
            &(
                *network_id,
                U8::<N>::new(program_id.name().size_in_bits()),
                program_id.name(),
                U8::<N>::new(program_id.network().size_in_bits()),
                program_id.network(),
                U8::<N>::new(function_name.size_in_bits()),
                function_name,
            )
                .to_bits_le(),
        ),
        true => N::hash_bhp1024(
            &(
                *network_id,
                U8::<N>::new(u8::try_from(Field::<N>::SIZE_IN_BITS)?),
                program_id.name().to_field()?,
                U8::<N>::new(u8::try_from(Field::<N>::SIZE_IN_BITS)?),
                program_id.network().to_field()?,
                U8::<N>::new(u8::try_from(Field::<N>::SIZE_IN_BITS)?),
                function_name.to_field()?,
            )
                .to_bits_le(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_field_size_in_bits() {
        // Ensure that the field size in bits is less than or equal to `u8::MAX`.
        // This is a sanity check for the above encoding.
        assert!(Field::<snarkvm_console_network::MainnetV0>::SIZE_IN_BITS <= u8::MAX as usize);
    }
}
