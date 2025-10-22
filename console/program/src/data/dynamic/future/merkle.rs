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

pub const FUTURE_DATA_TREE_DEPTH: u8 = 4;

pub type FutureDataTree<E> = MerkleTree<E, Poseidon8<E>, Poseidon2<E>, FUTURE_DATA_TREE_DEPTH>;
pub type FutureDataPath<E> = MerklePath<E, FUTURE_DATA_TREE_DEPTH>;

impl<N: Network> DynamicFuture<N> {
    /// Creates a new dynamic future from a static future.
    pub fn from_future(future: &Future<N>) -> Result<Self> {
        // Prepare the leaves.
        let leaves = future.arguments().iter().map(|argument| argument.to_fields()).collect::<Result<Vec<_>>>()?;

        // Initalize the hashers.
        let leaf_hasher = Poseidon8::setup("DynamicFutureLeafHasher")?;
        let path_hasher = Poseidon2::setup("DynamicFuturePathHasher")?;

        // Construct the merkle root of the data.
        let tree = FutureDataTree::new(&leaf_hasher, &path_hasher, &leaves)?;

        Ok(Self::new(
            future.program_id().name().to_field()?,
            future.program_id().network().to_field()?,
            future.function_name().to_field()?,
            *tree.root(),
            Some(tree),
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
        assert_eq!(CurrentNetwork::MAX_INPUTS.ilog2(), FUTURE_DATA_TREE_DEPTH as u32);
    }

    // TODO: Test different future arguments.
    // TODO: Test that you can correctly prove membership of an argument.
    // TODO: Benchmark merkleization performance for futures of various sizes.
}
