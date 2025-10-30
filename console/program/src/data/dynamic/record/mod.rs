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

use crate::{
    Address,
    Boolean,
    Entry,
    Field,
    Group,
    Identifier,
    Literal,
    Network,
    Owner,
    Plaintext,
    Record,
    Result,
    ToField,
    ToFields,
    U8,
};

use snarkvm_console_algorithms::{Poseidon2, Poseidon8};
use snarkvm_console_collections::merkle_tree::{MerklePath, MerkleTree};
use snarkvm_console_network::*;

use indexmap::IndexMap;

/// The depth of the record data tree.
pub const RECORD_DATA_TREE_DEPTH: u8 = 5;

/// The record data tree.
pub type RecordDataTree<E> = MerkleTree<E, Poseidon8<E>, Poseidon2<E>, RECORD_DATA_TREE_DEPTH>;
/// The record data path.
pub type RecordDataPath<E> = MerklePath<E, RECORD_DATA_TREE_DEPTH>;

/// A dynamic record is a fixed-size representation of a record.
/// Like static `Record`s, a dynamic record contains an owner, nonce, and a version.
/// However, instead of storing the full data, it only stores the Merkle root of the data.
/// This ensures that all dynamic records have a constant size, regardless of the amount of data they contain.
///
/// Suppose we have the following record:
///
/// record foo:
///     owner as address.private;
///     microcredits as u64.private;
///     memo as [u8; 32u32].public;
///
/// It's merkle-ization is as follows:
///
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
///
/// Note that:
///  - `ZERO` is defined by the `PathHash` implementation for `HashPSD2`.
///  - `ToFields` encodes the entry's mode and plaintext variant.
#[derive(Clone)]
pub struct DynamicRecord<N: Network> {
    /// The owner of the record.
    owner: Owner<N, Plaintext<N>>,
    /// The Merkle root of the record data.
    root: Field<N>,
    /// The nonce of the record.
    nonce: Group<N>,
    /// The version of the record.
    version: U8<N>,
    /// The optional Merkle tree of the record data.
    tree: Option<RecordDataTree<N>>,
    /// The optional program data.
    data: Option<IndexMap<Identifier<N>, Entry<N, Plaintext<N>>>>,
}

impl<N: Network> DynamicRecord<N> {
    /// Initializes a dynamic record without checking that the root, tree, and data are consistent.
    pub const fn new_unchecked(
        owner: Owner<N, Plaintext<N>>,
        root: Field<N>,
        nonce: Group<N>,
        version: U8<N>,
        tree: Option<RecordDataTree<N>>,
        data: Option<IndexMap<Identifier<N>, Entry<N, Plaintext<N>>>>,
    ) -> Self {
        Self { owner, root, nonce, version, tree, data }
    }
}

impl<N: Network> DynamicRecord<N> {
    /// Returns the owner of the record.
    pub const fn owner(&self) -> &Owner<N, Plaintext<N>> {
        &self.owner
    }

    /// Returns the Merkle root of the record data.
    pub const fn root(&self) -> &Field<N> {
        &self.root
    }

    /// Returns the nonce of the record.
    pub const fn nonce(&self) -> &Group<N> {
        &self.nonce
    }

    /// Returns the version of the record.
    pub const fn version(&self) -> &U8<N> {
        &self.version
    }

    /// Returns the optional Merkle tree of the record data.
    pub const fn tree(&self) -> &Option<RecordDataTree<N>> {
        &self.tree
    }

    /// Returns the optional record data.
    pub const fn data(&self) -> &Option<IndexMap<Identifier<N>, Entry<N, Plaintext<N>>>> {
        &self.data
    }

    /// Returns `true` if the program record is a hiding variant.
    pub fn is_hiding(&self) -> bool {
        !self.version.is_zero()
    }
}

impl<N: Network> DynamicRecord<N> {
    /// Creates a dynamic record from a static record.
    pub fn from_record(record: &Record<N, Plaintext<N>>) -> Result<Self> {
        // Get the owner.
        let owner = record.owner().clone();
        // Get the program data.
        let data = record.data().clone();
        // Get the nonce.
        let nonce = *record.nonce();
        // Get the version.
        let version = *record.version();

        // Prepare the leaves.
        let leaves = data
            .iter()
            .map(|(name, entry)| {
                // Initialize the leaf.
                let mut leaf = vec![];
                // Add the entry name.
                leaf.push(name.to_field()?);
                // Add the entry data.
                leaf.extend(entry.to_fields()?);

                Ok(leaf)
            })
            .collect::<Result<Vec<_>>>()?;

        // Initalize the hashers.
        let leaf_hasher = Poseidon8::setup("DynamicRecordLeafHasher")?;
        let path_hasher = Poseidon2::setup("DynamicRecordPathHasher")?;

        // Construct the merkle tree.
        let tree = RecordDataTree::new(&leaf_hasher, &path_hasher, &leaves)?;

        // Get the root.
        let root = *tree.root();

        Ok(Self::new_unchecked(owner, root, nonce, version, Some(tree), Some(data)))
    }

    /// Creates a static record from this dynamic record.
    pub fn to_record(&self) -> Result<Record<N, Plaintext<N>>> {
        // Ensure that the data is present.
        let Some(data) = &self.data else {
            bail!("Cannot convert a dynamic record to static record without the underlying data");
        };
        Record::<N, Plaintext<N>>::from_plaintext(self.owner.clone(), data.clone(), self.nonce, self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snarkvm_console_network::MainnetV0;

    type CurrentNetwork = MainnetV0;

    #[test]
    fn test_data_depth() {
        assert_eq!(CurrentNetwork::MAX_DATA_ENTRIES.ilog2(), RECORD_DATA_TREE_DEPTH as u32);
    }

    // TODO: Test different record formats.
    // TODO: Test that you can correctly prove membership of an entry.
    // TODO: Benchmark merkleization performance for records of various sizes.
}
