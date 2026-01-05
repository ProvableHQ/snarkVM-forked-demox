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
/// It's merkleization is as follows:
/// ```text
///   L_0    L_1    (leaves: hashed entries)
///     \    /
///      P_0        (internal node)
///       |
///      P_1        (padding level 1)
///       |
///      P_2        (padding level 2)
///       |
///       R         (root, padding level 3)
///
/// L_0 := HashPSD8(ToFields(arg_0))
/// L_1 := HashPSD8(ToFields(arg_1))
/// P_0 := HashPSD2(L_0, L_1)
/// P_1 := HashPSD2(P_0, empty_hash)
/// P_2 := HashPSD2(P_1, empty_hash)
///   R := HashPSD2(P_2, empty_hash)
/// ```
///
/// For finalize scopes with a different number of arguments, leaves are first
/// padded to the next power of 2 using `empty_hash` hashes, then a balanced
/// binary tree is built. Note that, in concrete terms, at most one `empty_hash`
/// leaf is added: the rest are only virtual in that instead nodes with the
/// value `HashPSD2(empty_hash, empty_hash)` are added to the next level, which
/// is indeed full of size equal to a power of 2.
///
/// Padding levels are then added as needed to reach the full tree depth
/// `FUTURE_ARGUMENT_TREE_DEPTH` (4), each of which is constructed by hashing the
/// root of the previous level together with `empty_hash`.
///
/// Note that:
///  - `empty_hash` is the value returned by the `hash_empty` function the
///    `PathHash` implementation for `HashPSD2`.
///  - `ToFields` encodes the arguments's variant.
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
    /// The optional Merkle tree of the arguments.
    tree: Option<FutureArgumentTree<N>>,
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
        tree: Option<FutureArgumentTree<N>>,
        arguments: Option<Vec<Argument<N>>>,
    ) -> Self {
        Self { program_name, program_network, function_name, root, tree, arguments }
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

    /// Returns the optional Merkle tree of the arguments.
    pub const fn tree(&self) -> &Option<FutureArgumentTree<N>> {
        &self.tree
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

        Ok(Self::new_unchecked(program_name, program_network, function_name, root, Some(tree), Some(arguments)))
    }

    /// Creates a static record from a dynamic record.
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

    type CurrentNetwork = MainnetV0;

    #[test]
    fn test_data_depth() {
        assert_eq!(CurrentNetwork::MAX_INPUTS.ilog2(), FUTURE_ARGUMENT_TREE_DEPTH as u32);
    }

    // TODO (dynamic_dispatch): Test different future arguments.
    // TODO (dynamic_dispatch): Test that you can correctly prove membership of an argument.
    // TODO (dynamic_dispatch): Benchmark merkleization performance for futures of various sizes.
}
