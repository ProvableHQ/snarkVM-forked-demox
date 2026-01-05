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

mod equal;
mod to_bits;
mod to_fields;

use snarkvm_circuit_network::Aleo;
use snarkvm_circuit_types::{Boolean, Field, environment::prelude::*};

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
pub struct DynamicFuture<A: Aleo> {
    /// The program name.
    program_name: Field<A>,
    /// The program network.
    program_network: Field<A>,
    /// The function name.
    function_name: Field<A>,
    /// The Merkle root of the arguments.
    root: Field<A>,
    /// The optional console Merkle tree of the arguments.
    /// Note: This is NOT part of the circuit representation.
    tree: Option<console::FutureArgumentTree<A::Network>>,
    /// The optional console arguments.
    /// Note: This is NOT part of the circuit representation.
    arguments: Option<Vec<console::Argument<A::Network>>>,
}

impl<A: Aleo> Inject for DynamicFuture<A> {
    type Primitive = console::DynamicFuture<A::Network>;

    /// Initializes a circuit of the given mode and future.
    fn new(mode: Mode, value: Self::Primitive) -> Self {
        DynamicFuture {
            program_name: Inject::new(mode, *value.program_name()),
            program_network: Inject::new(mode, *value.program_network()),
            function_name: Inject::new(mode, *value.function_name()),
            root: Inject::new(mode, *value.root()),
            tree: value.tree().clone(),
            arguments: value.arguments().clone(),
        }
    }
}

impl<A: Aleo> DynamicFuture<A> {
    /// Returns the program name.
    pub const fn program_name(&self) -> &Field<A> {
        &self.program_name
    }

    /// Returns the program network.
    pub const fn program_network(&self) -> &Field<A> {
        &self.program_network
    }

    /// Returns the function name.
    pub const fn function_name(&self) -> &Field<A> {
        &self.function_name
    }

    /// Returns the Merkle root of the arguments.
    pub const fn root(&self) -> &Field<A> {
        &self.root
    }

    /// Returns the optional console Merkle tree of the arguments.
    pub const fn tree(&self) -> &Option<console::FutureArgumentTree<A::Network>> {
        &self.tree
    }

    /// Returns the console arguments.
    pub const fn arguments(&self) -> &Option<Vec<console::Argument<A::Network>>> {
        &self.arguments
    }
}

impl<A: Aleo> Eject for DynamicFuture<A> {
    type Primitive = console::DynamicFuture<A::Network>;

    /// Ejects the mode of the dynamic future.
    fn eject_mode(&self) -> Mode {
        let program_name_mode = Eject::eject_mode(self.program_name());
        let program_network_mode = Eject::eject_mode(self.program_network());
        let function_name_mode = Eject::eject_mode(self.function_name());
        let root_mode = Eject::eject_mode(self.root());
        Mode::combine(program_name_mode, [program_network_mode, function_name_mode, root_mode])
    }

    /// Ejects the dynamic future.
    fn eject_value(&self) -> Self::Primitive {
        Self::Primitive::new_unchecked(
            Eject::eject_value(self.program_name()),
            Eject::eject_value(self.program_network()),
            Eject::eject_value(self.function_name()),
            Eject::eject_value(self.root()),
            self.tree.clone(),
            self.arguments.clone(),
        )
    }
}
