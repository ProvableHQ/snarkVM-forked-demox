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

mod merkle;
use merkle::*;

use crate::{Field, Future, Network, Result, ToField, ToFields};

use snarkvm_console_algorithms::{Poseidon2, Poseidon8};
use snarkvm_console_collections::merkle_tree::{MerklePath, MerkleTree};

// A dynamic future is a fixed-size representation of a future.
// Like static `Future`s, a dynamic future contains a program ID and function name.
// These are however represented as `Field` elements as opposed to `Identifier`s to ensure a fixed size.
// Dynamic futures also store a Merkle root of the arguments to the future instead of the arguments themselves.
// This ensures that all dynamic futures have a constant size, regardless of the amount of data they contain.
//
// Suppose we have the following `finalize` scope:
//
// finalize foo:
//     input r0 as address.public;
//     input r1 as u64.public;
//
// It's merkle-ization is as follows:
//
//        R
//        |
//       P_0
//        |
//       P_1
//        |
//       P_2
//        |
//       P_3
//      /  \
//   L_0    L_1
//
// L_0 := HashPSD8(ToFields(arg_0))
// L_1 := HashPSD8(ToFields(arg_1))
// P_0 := HashPSD2(L_0, L_1)
// P_1 := HashPSD2(P_0, ZERO)
// P_2 := HashPSD2(P_1, ZERO)
// P_3 := HashPSD2(P_2, ZERO)
//   R := HashPSD2(P_3, ZERO)
//
// Note that:
//  - `ZERO` is defined by the `PathHash` implementation for `HashPSD2`.
//  - `ToFields` encodes the arguments's variant.
pub struct DynamicFuture<N: Network> {
    /// The program name.
    program_name: Field<N>,
    /// The program network.
    program_network: Field<N>,
    /// The function name.
    function_name: Field<N>,
    /// The Merkle root of the arguments.
    root: Field<N>,
    /// The Merkle tree of the program data, if it has been provided.
    tree: Option<FutureDataTree<N>>,
}

impl<N: Network> DynamicFuture<N> {
    /// Creates a new dynamic future.
    pub fn new(
        program_name: Field<N>,
        program_network: Field<N>,
        function_name: Field<N>,
        root: Field<N>,
        tree: Option<FutureDataTree<N>>,
    ) -> Self {
        Self { program_name, program_network, function_name, root, tree }
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
    pub const fn arguments(&self) -> &Field<N> {
        &self.root
    }

    /// Returns the Merkle tree of the arguments.
    pub fn tree(&self) -> Option<&FutureDataTree<N>> {
        self.tree.as_ref()
    }
}
