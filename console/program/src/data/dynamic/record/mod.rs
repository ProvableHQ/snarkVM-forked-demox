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

use crate::{Field, Group, Network, Owner, Plaintext, Record, Result, ToField, ToFields, U8, Visibility};

use snarkvm_console_algorithms::{Poseidon2, Poseidon8};
use snarkvm_console_collections::merkle_tree::{MerklePath, MerkleTree};

// A dynamic record is a fixed-size representation of a record.
// Like static `Record`s, a dynamic record contains an owner, nonce, and a version.
// However, instead of storing the full data, it only stores the Merkle root of the data.
// This ensures that all dynamic records have a constant size, regardless of the amount of data they contain.
//
// Suppose we have the following record:
//
// record foo:
//     owner as address.private;
//     microcredits as u64.private;
//     memo as [u8; 32u32].public;
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
// L_0 := HashPSD8(microcredits || ToFields(entry_0))
// L_1 := HashPSD8(memo || ToFields(entry_1))
// P_0 := HashPSD2(L_0, L_1)
// P_1 := HashPSD2(P_0, ZERO)
// P_2 := HashPSD2(P_1, ZERO)
// P_3 := HashPSD2(P_2, ZERO)
//   R := HashPSD2(P_3, ZERO)
//
// Note that:
//  - `ZERO` is defined by the `PathHash` implementation for `HashPSD2`.
//  - `ToFields` encodes the entry's mode and plaintext variant.
pub struct DynamicRecord<N: Network, Private: Visibility> {
    /// The owner of the program record.
    owner: Owner<N, Private>,
    /// The Merkle root of the program data.
    root: Field<N>,
    /// The nonce of the program record.
    nonce: Group<N>,
    /// The version of the program record.
    version: U8<N>,
    /// The Merkle tree of the program data, if it has been provided.
    tree: Option<RecordDataTree<N>>,
}

impl<N: Network, Private: Visibility> DynamicRecord<N, Private> {
    /// Creates a new dynamic record.
    pub fn new(
        owner: Owner<N, Private>,
        root: Field<N>,
        nonce: Group<N>,
        version: U8<N>,
        tree: Option<RecordDataTree<N>>,
    ) -> Self {
        Self { owner, root, nonce, version, tree }
    }
}

impl<N: Network, Private: Visibility> DynamicRecord<N, Private> {
    /// Returns the owner of the record.
    pub fn owner(&self) -> &Owner<N, Private> {
        &self.owner
    }

    /// Returns the Merkle root of the record data.
    pub fn data(&self) -> &Field<N> {
        &self.root
    }

    /// Returns the nonce of the record.
    pub fn nonce(&self) -> &Group<N> {
        &self.nonce
    }

    /// Returns the version of the record.
    pub fn version(&self) -> &U8<N> {
        &self.version
    }

    /// Returns the Merkle tree of the record data.
    pub fn tree(&self) -> Option<&RecordDataTree<N>> {
        self.tree.as_ref()
    }
}
