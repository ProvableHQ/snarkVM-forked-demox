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

use super::*;

pub const RECORD_DATA_TREE_DEPTH: u8 = 5;

pub type RecordDataTree<E> = MerkleTree<E, Poseidon8<E>, Poseidon2<E>, RECORD_DATA_TREE_DEPTH>;
pub type RecordDataPath<E> = MerklePath<E, RECORD_DATA_TREE_DEPTH>;

impl<N: Network> DynamicRecord<N, Plaintext<N>> {
    /// Creates a new dynamic record from a static record.
    pub fn from_record(record: &Record<N, Plaintext<N>>) -> Result<Self> {
        // Prepare the leaves.
        let leaves = record
            .data()
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

        // Construct the merkle root of the data.
        let tree = RecordDataTree::new(&leaf_hasher, &path_hasher, &leaves)?;

        Ok(Self::new(record.owner().clone(), *tree.root(), *record.nonce(), *record.version(), Some(tree)))
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
