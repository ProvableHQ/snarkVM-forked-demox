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
mod find;
mod to_bits;
mod to_fields;
mod to_id;

use crate::{Access, Aleo, Entry, Equal, Identifier, Literal, Plaintext, Record, ToBits, ToFields, Value};

use console::RECORD_DATA_TREE_DEPTH;
use snarkvm_circuit_algorithms::{Poseidon2, Poseidon8};
use snarkvm_circuit_collections::merkle_tree::MerkleTree;
use snarkvm_circuit_types::{Address, Boolean, Field, Group, U8, U16, environment::prelude::*};

type CircuitLH<A> = Poseidon8<A>;
type CircuitPH<A> = Poseidon2<A>;

/// The record data tree.
pub type RecordDataTree<A> = MerkleTree<A, CircuitLH<A>, CircuitPH<A>, RECORD_DATA_TREE_DEPTH>;

// TODO (dynamic_dispatch) correct this and other instances of the specification: that is not the correct structure of the tree (odd-size layers are not filled with a single zero)
/// A dynamic record is a fixed-size representation of a record.
/// Like static `Record`s, a dynamic record contains an owner, nonce, and a version.
/// However, instead of storing the full data, it only stores the Merkle root of the data.
/// This ensures that all dynamic records have a constant size, regardless of the amount of data they contain.
///
/// Suppose we have the following record:
///
/// ```ignore
/// record foo:
///     owner as address.private;
///     microcredits as u64.private;
///     memo as [u8; 32u32].public;
/// ```
///
/// It's merkle-ization is as follows:
///
/// ```ignore
///        R
///        |
///       P_0
///        |
///       P_1
///        |
///       P_2
///        |
///       P_3
///      /  \
///   L_0    L_1
///
/// L_0 := HashPSD8(microcredits || ToFields(entry_0))
/// L_1 := HashPSD8(memo || ToFields(entry_1))
/// P_0 := HashPSD2(L_0, L_1)
/// P_1 := HashPSD2(P_0, ZERO)
/// P_2 := HashPSD2(P_1, ZERO)
/// P_3 := HashPSD2(P_2, ZERO)
///   R := HashPSD2(P_3, ZERO)
/// ```
///
/// Note that:
///  - `ZERO` is defined by the `PathHash` implementation for `HashPSD2`.
///  - `ToFields` encodes the entry's mode and plaintext variant.
#[derive(Clone)]
pub struct DynamicRecord<A: Aleo> {
    /// The owner of the record.
    owner: Address<A>,
    /// The Merkle root of the record data.
    root: Field<A>,
    /// The nonce of the record.
    nonce: Group<A>,
    /// The version of the record.
    version: U8<A>,
    /// The optional console program data.
    /// Note: This is NOT part of the circuit representation.
    data: Option<console::RecordData<A::Network>>,
}

impl<A: Aleo> Inject for DynamicRecord<A> {
    type Primitive = console::DynamicRecord<A::Network>;

    /// Initializes a plaintext record from a primitive.
    fn new(_: Mode, record: Self::Primitive) -> Self {
        Self {
            owner: Inject::new(Mode::Private, *record.owner()),
            root: Inject::new(Mode::Private, *record.root()),
            nonce: Inject::new(Mode::Private, *record.nonce()),
            version: Inject::new(Mode::Private, *record.version()),
            data: record.data().clone(),
        }
    }
}

impl<A: Aleo> DynamicRecord<A> {
    /// Returns the owner of the record.
    pub const fn owner(&self) -> &Address<A> {
        &self.owner
    }

    /// Returns the Merkle root of the record data.
    pub const fn root(&self) -> &Field<A> {
        &self.root
    }

    /// Returns the nonce of the record.
    pub const fn nonce(&self) -> &Group<A> {
        &self.nonce
    }

    /// Returns the version of the record.
    pub const fn version(&self) -> &U8<A> {
        &self.version
    }

    /// Returns console the record data.
    pub const fn data(&self) -> Option<&console::RecordData<A::Network>> {
        self.data.as_ref()
    }
}

impl<A: Aleo> Eject for DynamicRecord<A> {
    type Primitive = console::DynamicRecord<A::Network>;

    /// Ejects the mode of the dynamic record.
    fn eject_mode(&self) -> Mode {
        let owner = self.owner.eject_mode();
        let root = self.root.eject_mode();
        let nonce = self.nonce.eject_mode();
        let version = self.version.eject_mode();

        Mode::combine(owner, [root, nonce, version])
    }

    /// Ejects the dynamic record.
    fn eject_value(&self) -> Self::Primitive {
        Self::Primitive::new_unchecked(
            self.owner.eject_value(),
            self.root.eject_value(),
            self.nonce.eject_value(),
            self.version.eject_value(),
            self.data.clone(),
        )
    }
}

impl<A: Aleo> DynamicRecord<A> {
    /// Creates a dynamic record from a static one.
    pub fn from_record(record: &Record<A, Plaintext<A>>) -> Result<Self> {
        // This mimics the console::DynamicRecord::from_record function.

        // Note that, in most lines below, cloning (e. g. of record.owner())
        // does not introduce a new variable into the witness but rather creates
        // a new reference to the preexisting witness variable.

        // Get the owner.
        let owner = (**record.owner()).clone();
        // Get the record's data (not part of the circuit representation)
        let data = record.data();
        // Get the nonce.
        let nonce = record.nonce().clone();
        // Get the version.
        let version = record.version().clone();

        let tree = Self::merkleize_data(data)?;
        let root = tree.root().clone();

        let console_data =
            data.iter().map(|(identifier, entry)| (identifier, entry).eject_value()).collect::<IndexMap<_, _>>();

        Ok(Self { owner, root, nonce, version, data: Some(console_data) })
    }

    /// Injects the ordered entries in `data` into the circuit and computes the
    /// Merkle tree containing those entires as leaves.
    pub fn merkleize_data(data: &IndexMap<Identifier<A>, Entry<A, Plaintext<A>>>) -> Result<RecordDataTree<A>> {
        // Initalize the circuit hashers.
        let (console_leaf_hasher, console_path_hasher) = console::DynamicRecord::initialize_hashers()?;
        let circuit_leaf_hasher = CircuitLH::<A>::constant(console_leaf_hasher);
        let circuit_path_hasher = CircuitPH::<A>::constant(console_path_hasher);

        // Inject the leaves
        let leaves = data
            .iter()
            .map(|(identifier, entry)| {
                let mut leaf = vec![identifier.to_field()];
                leaf.extend(entry.to_fields());
                leaf
            })
            .collect::<Vec<Vec<Field<A>>>>();

        // Construct the merkle tree
        RecordDataTree::<A>::new(circuit_leaf_hasher, circuit_path_hasher, &leaves)
    }
}
