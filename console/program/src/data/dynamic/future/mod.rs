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
/// ensure a fixed size. Dynamic futures also store a Merkle root of the
/// arguments to the future instead of the arguments themselves. This ensures
/// that all dynamic futures have a constant size, regardless of the amount of
/// data they contain.
///
/// Suppose we have the following `finalize` scope:
///
/// ```text
/// finalize foo: input r0 as address.public; input r1 as u64.public;
/// ```
///
/// The leaves of its Merkle tree are computed as follows:
/// ```text
/// L_0 := HashPSD8(ToFields(arg_0))
/// L_1 := HashPSD8(ToFields(arg_1))
/// ```
///
/// Note that `ToFields` encodes the arguments's variant.
///
/// The tree has depth `FUTURE_ARGUMENT_TREE_DEPTH = 4` and is constructed with
/// path hasher `HashPSD2` and the padding scheme outlined in
/// [`snarkVM`'s `MerkleTree`](snarkvm_console_collections::merkle_tree::MerkleTree).
#[derive(Clone)]
pub struct DynamicFuture<N: Network> {
    /// The program name.
    program_name: Field<N>,
    /// The program network.
    program_network: Field<N>,
    /// The function name.
    function_name: Field<N>,
    /// The Merkle root of the arguments.
    root: Field<N>,
    /// The optional arguments.
    arguments: Option<Vec<Argument<N>>>,
}

impl<N: Network> DynamicFuture<N> {
    /// Initializes a dynamic future without checking that the root, tree, and arguments are consistent.
    pub fn new_unchecked(
        program_name: Field<N>,
        program_network: Field<N>,
        function_name: Field<N>,
        root: Field<N>,
        arguments: Option<Vec<Argument<N>>>,
    ) -> Self {
        Self { program_name, program_network, function_name, root, arguments }
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

    /// Returns the Merkle root of the arguments.
    pub const fn root(&self) -> &Field<N> {
        &self.root
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

        // Prepare the leaves.
        let leaves = arguments.iter().map(|argument| argument.to_fields()).collect::<Result<Vec<_>>>()?;

        // Initalize the hashers.
        let leaf_hasher = Poseidon8::setup("DynamicFutureLeafHasher")?;
        let path_hasher = Poseidon2::setup("DynamicFuturePathHasher")?;

        // Construct the Merkle tree of the data.
        let tree = FutureArgumentTree::new(&leaf_hasher, &path_hasher, &leaves)?;

        // Get the root.
        let root = *tree.root();

        Ok(Self::new_unchecked(program_name, program_network, function_name, root, Some(arguments)))
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
    fn test_root_determinism() {
        let args1 = vec![Argument::Plaintext(Plaintext::from_str("100u64").unwrap())];
        let args2 = vec![Argument::Plaintext(Plaintext::from_str("100u64").unwrap())];
        let args3 = vec![Argument::Plaintext(Plaintext::from_str("200u64").unwrap())];

        let d1 = DynamicFuture::from_future(&create_test_future(args1)).unwrap();
        let d2 = DynamicFuture::from_future(&create_test_future(args2)).unwrap();
        let d3 = DynamicFuture::from_future(&create_test_future(args3)).unwrap();

        assert_eq!(d1.root(), d2.root(), "Same arguments should produce same root");
        assert_ne!(d1.root(), d3.root(), "Different arguments should produce different roots");
    }

    #[test]
    fn test_membership_proofs() {
        let inner =
            Future::new(ProgramID::from_str("inner.aleo").unwrap(), Identifier::from_str("bar").unwrap(), vec![
                Argument::Plaintext(Plaintext::from_str("1u64").unwrap()),
            ]);
        let arguments = vec![
            Argument::Plaintext(Plaintext::from_str("100u64").unwrap()),
            Argument::Future(inner),
            Argument::Plaintext(Plaintext::from_str("200u64").unwrap()),
        ];
        let dynamic = DynamicFuture::from_future(&create_test_future(arguments.clone())).unwrap();

        // Build tree and verify root matches.
        let leaves: Vec<_> = arguments.iter().map(|a| a.to_fields().unwrap()).collect();
        let leaf_hasher = Poseidon8::setup("DynamicFutureLeafHasher").unwrap();
        let path_hasher = Poseidon2::setup("DynamicFuturePathHasher").unwrap();
        let tree = FutureArgumentTree::new(&leaf_hasher, &path_hasher, &leaves).unwrap();
        assert_eq!(tree.root(), dynamic.root());

        // Valid proofs.
        for (i, leaf) in leaves.iter().enumerate() {
            let path = tree.prove(i, leaf).unwrap();
            assert!(tree.verify(&path, tree.root(), leaf));
        }

        // Invalid proofs.
        let path = tree.prove(0, &leaves[0]).unwrap();
        assert!(!tree.verify(&path, tree.root(), &leaves[1])); // Wrong leaf.
        assert!(!tree.verify(&path, &Field::from_u64(12345), &leaves[0])); // Wrong root.
    }
}
