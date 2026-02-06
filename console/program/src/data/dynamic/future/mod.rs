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

mod bytes;
mod equal;
mod parse;
mod to_bits;
mod to_fields;

use crate::{Argument, Boolean, Field, Future, Identifier, Network, ProgramID, Result, ToField, ToFields};

use snarkvm_console_algorithms::{Poseidon2, Poseidon8};
use snarkvm_console_collections::merkle_tree::MerkleTree;
use snarkvm_console_network::*;

/// The depth of the future argument tree.
pub const FUTURE_ARGUMENT_TREE_DEPTH: u8 = 4;

/// The future argument tree.
pub type FutureArgumentTree<E> = MerkleTree<E, Poseidon8<E>, Poseidon2<E>, FUTURE_ARGUMENT_TREE_DEPTH>;

/// A dynamic future is a fixed-size representation of a future. Like static
/// `Future`s, a dynamic future contains a program ID and function name. These
/// are however represented as `Field` elements as opposed to `Identifier`s to
/// ensure a fixed size. Dynamic futures also store a hash of the
/// arguments to the future instead of the arguments themselves. This ensures
/// that all dynamic futures have a constant size, regardless of the amount of
/// data they contain.
#[derive(Clone)]
pub struct DynamicFuture<N: Network> {
    /// The program name.
    program_name: Field<N>,
    /// The program network.
    program_network: Field<N>,
    /// The function name.
    function_name: Field<N>,
    /// The hash of the arguments.
    hash: Field<N>,
    /// The optional arguments.
    arguments: Option<Vec<Argument<N>>>,
}

impl<N: Network> DynamicFuture<N> {
    /// Initializes a dynamic future without checking that the hash and arguments are consistent.
    pub fn new_unchecked(
        program_name: Field<N>,
        program_network: Field<N>,
        function_name: Field<N>,
        hash: Field<N>,
        arguments: Option<Vec<Argument<N>>>,
    ) -> Self {
        Self { program_name, program_network, function_name, hash, arguments }
    }
}

impl<N: Network> DynamicFuture<N> {
    /// Returns the program name.
    pub const fn program_name(&self) -> &Field<N> {
        &self.program_name
    }

    /// Returns the program network.
    pub const fn program_network(&self) -> &Field<N> {
        &self.program_network
    }

    /// Returns the function name.
    pub const fn function_name(&self) -> &Field<N> {
        &self.function_name
    }

    /// Returns the hash of the arguments.
    pub const fn hash(&self) -> &Field<N> {
        &self.hash
    }

    /// Returns the optional arguments.
    pub const fn arguments(&self) -> &Option<Vec<Argument<N>>> {
        &self.arguments
    }
}

impl<N: Network> DynamicFuture<N> {
    /// Creates a dynamic future from a static future.
    pub fn from_future(future: &Future<N>) -> Result<Self> {
        // Get the program name.
        let program_name = future.program_id().name().to_field()?;
        // Get the program network.
        let program_network = future.program_id().network().to_field()?;
        // Get the function name.
        let function_name = future.function_name().to_field()?;
        // Get the arguments.
        let arguments = future.arguments().to_vec();

        // Get the bits of the arguments.
        let mut bits = vec![];
        // Prefix the bits with the number of arguments to ensure that different numbers of arguments produce different hashes.
        // Note that the number of arguments is at most 16, so it fits in a single byte.
        bits.extend(u8::try_from(arguments.len())?.to_bits_le());
        // Then, append the bits of each argument.
        // Note that the argument bits themselves are type-prefixed.
        bits.extend(arguments.iter().flat_map(|a| a.to_bits_le()));
        // Then pad the bits to the next multiple of 8 to ensure that the hash is consistent regardless of the number of arguments.
        bits.resize(bits.len().div_ceil(8) * 8, false);

        // Hash the bits of the arguments.
        // TODO: Do we need domain separation or the outer hash?
        let hash = N::hash_bhp256(&N::hash_keccak256(&bits)?)?;

        Ok(Self::new_unchecked(program_name, program_network, function_name, hash, Some(arguments)))
    }

    /// Creates a static future from a dynamic future.
    pub fn to_future(&self) -> Result<Future<N>> {
        // Ensure that the arguments are present.
        let Some(arguments) = &self.arguments else {
            bail!("Cannot convert dynamic future to a static future without the arguments being present");
        };

        Ok(Future::new(
            ProgramID::try_from((
                Identifier::from_field(&self.program_name)?,
                Identifier::from_field(&self.program_network)?,
            ))?,
            Identifier::from_field(&self.function_name)?,
            arguments.clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snarkvm_console_network::MainnetV0;

    use crate::Plaintext;

    use core::str::FromStr;

    type CurrentNetwork = MainnetV0;

    #[test]
    fn test_data_depth() {
        assert_eq!(CurrentNetwork::MAX_INPUTS.ilog2(), FUTURE_ARGUMENT_TREE_DEPTH as u32);
    }

    fn create_test_future(arguments: Vec<Argument<CurrentNetwork>>) -> Future<CurrentNetwork> {
        Future::new(ProgramID::from_str("test.aleo").unwrap(), Identifier::from_str("foo").unwrap(), arguments)
    }

    fn assert_round_trip(arguments: Vec<Argument<CurrentNetwork>>) {
        let future = create_test_future(arguments);
        let dynamic = DynamicFuture::from_future(&future).unwrap();
        let recovered = dynamic.to_future().unwrap();
        assert_eq!(future.program_id(), recovered.program_id());
        assert_eq!(future.function_name(), recovered.function_name());
        for (a, b) in future.arguments().iter().zip(recovered.arguments().iter()) {
            assert!(*a.is_equal(b));
        }
    }

    #[test]
    fn test_round_trip_various_arguments() {
        // No arguments.
        assert_round_trip(vec![]);

        // Plaintext literals.
        assert_round_trip(vec![
            Argument::Plaintext(Plaintext::from_str("true").unwrap()),
            Argument::Plaintext(Plaintext::from_str("100u64").unwrap()),
        ]);

        // Struct and array.
        assert_round_trip(vec![Argument::Plaintext(Plaintext::from_str("{ x: 1field, y: 2field }").unwrap())]);

        // Nested Future argument.
        let inner =
            Future::new(ProgramID::from_str("inner.aleo").unwrap(), Identifier::from_str("bar").unwrap(), vec![
                Argument::Plaintext(Plaintext::from_str("42u64").unwrap()),
            ]);
        assert_round_trip(vec![Argument::Future(inner.clone())]);

        // DynamicFuture argument.
        assert_round_trip(vec![Argument::DynamicFuture(DynamicFuture::from_future(&inner).unwrap())]);

        // Max arguments (16).
        let max_args: Vec<_> =
            (0..16).map(|i| Argument::Plaintext(Plaintext::from_str(&format!("{i}u64")).unwrap())).collect();
        assert_round_trip(max_args);
    }

    #[test]
    fn test_hash_determinism() {
        let args1 = vec![Argument::Plaintext(Plaintext::from_str("100u64").unwrap())];
        let args2 = vec![Argument::Plaintext(Plaintext::from_str("100u64").unwrap())];
        let args3 = vec![Argument::Plaintext(Plaintext::from_str("200u64").unwrap())];

        let d1 = DynamicFuture::from_future(&create_test_future(args1)).unwrap();
        let d2 = DynamicFuture::from_future(&create_test_future(args2)).unwrap();
        let d3 = DynamicFuture::from_future(&create_test_future(args3)).unwrap();

        assert_eq!(d1.hash(), d2.hash(), "Same arguments should produce same hash");
        assert_ne!(d1.hash(), d3.hash(), "Different arguments should produce different hashes");
    }
}
