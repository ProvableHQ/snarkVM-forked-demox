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

mod helpers;
use helpers::{LeafHash, PathHash};

mod leaf_index;

mod verify;

#[cfg(test)]
use snarkvm_circuit_types::environment::assert_scope;

use snarkvm_circuit_types::{Boolean, Field, U64, environment::prelude::*};

pub struct MerklePath<E: Environment, const DEPTH: u8> {
    /// The leaf index for the path.
    leaf_index: U64<E>,
    /// The `siblings` contains a list of sibling hashes from the leaf to the root.
    siblings: Vec<Field<E>>,
}

impl<E: Environment, const DEPTH: u8> Inject for MerklePath<E, DEPTH> {
    type Primitive = console::merkle_tree::MerklePath<E::Network, DEPTH>;

    /// Initializes a Merkle path from the given mode and native Merkle path.
    fn new(mode: Mode, merkle_path: Self::Primitive) -> Self {
        // Initialize the leaf index.
        let leaf_index = U64::new(mode, merkle_path.leaf_index());
        // Initialize the Merkle path siblings.
        let siblings: Vec<_> = merkle_path.siblings().iter().map(|node| Field::new(mode, *node)).collect();
        // Ensure the Merkle path is the correct depth.
        match siblings.len() == DEPTH as usize {
            // Return the Merkle path.
            true => Self { leaf_index, siblings },
            false => E::halt("Merkle path is not the correct depth"),
        }
    }
}

impl<E: Environment, const DEPTH: u8> Eject for MerklePath<E, DEPTH> {
    type Primitive = console::merkle_tree::MerklePath<E::Network, DEPTH>;

    /// Ejects the mode of the Merkle path.
    fn eject_mode(&self) -> Mode {
        (&self.leaf_index, &self.siblings).eject_mode()
    }

    /// Ejects the Merkle path.
    fn eject_value(&self) -> Self::Primitive {
        match Self::Primitive::try_from((&self.leaf_index, &self.siblings).eject_value()) {
            Ok(merkle_path) => merkle_path,
            Err(error) => E::halt(format!("Failed to eject the Merkle path: {error}")),
        }
    }
}

#[derive(Clone)]
pub struct MerkleTree<E: Environment, LH: LeafHash<E, Hash = PH::Hash>, PH: PathHash<E, Hash = Field<E>>, const DEPTH: u8> {
    /// The leaf hasher for the Merkle tree.
    leaf_hasher: LH,
    /// The path hasher for the Merkle tree.
    path_hasher: PH,
    /// The computed root of the full Merkle tree.
    root: PH::Hash,
    /// The internal hashes, from root to hashed leaves, of the full Merkle tree.
    tree: Vec<PH::Hash>,
    /// The canonical empty hash.
    empty_hash: Field<E>,
    /// The number of hashed leaves in the tree.
    number_of_leaves: usize,
}

impl<E: Environment, LH: LeafHash<E, Hash = PH::Hash>, PH: PathHash<E, Hash = Field<E>>, const DEPTH: u8> MerkleTree<E, LH, PH, DEPTH> {

    #[inline]
    /// Initializes a new Merkle tree with the given leaves.
    pub fn new(leaf_hasher: LH, path_hasher: PH, leaves: &[LH::Leaf]) -> Result<Self> {

        // Ensure the Merkle tree depth is greater than 0.
        ensure!(DEPTH > 0, "Merkle tree depth must be greater than 0");
        // Ensure the Merkle tree depth is less than or equal to 64.
        ensure!(DEPTH <= 64u8, "Merkle tree depth must be less than or equal to 64");

        // Compute the maximum number of leaves.
        let max_leaves = match leaves.len().checked_next_power_of_two() {
            Some(num_leaves) => num_leaves,
            None => bail!("Integer overflow when computing the maximum number of leaves in the Merkle tree"),
        };

        // Compute the number of nodes.
        let num_nodes = max_leaves - 1;
        // Compute the tree size as the maximum number of leaves plus the number of nodes.
        let tree_size = max_leaves + num_nodes;
        // Compute the number of levels in the Merkle tree (i.e. log2(tree_size)).
        let tree_depth = tree_depth::<DEPTH>(tree_size)?;
        // Compute the number of padded levels.
        let padding_depth = DEPTH - tree_depth;

        // Compute the empty hash.
        let empty_hash = path_hasher.hash_empty();

        // Calculate the size of the tree which excludes leafless nodes.
        // The minimum tree size is either a single root node or the calculated number of nodes plus
        // the supplied leaves; if the number of leaves is odd, an empty hash is added for padding.
        let minimum_tree_size =
            std::cmp::max(1, num_nodes + leaves.len() + if leaves.len() > 1 { leaves.len() % 2 } else { 0 });

        // Initialize the Merkle tree.
        let mut tree = vec![empty_hash.clone(); minimum_tree_size];

        // Compute and store each leaf hash.
        for (tree_leaf, provided_leaf) in tree[num_nodes..num_nodes + leaves.len()].iter_mut().zip(leaves.iter()) {
            *tree_leaf = leaf_hasher.hash_leaf(provided_leaf);
        }

        // Compute and store the hashes for each level, iterating from the penultimate level to the root level.
        let mut start_index = num_nodes;
        // Compute the start index of the current level.
        while let Some(start) = parent(start_index) {
            // Compute the end index of the current level.
            let end = left_child(start);
            // Construct the children for each node in the current level; the leaves are padded, which means
            // that there either are 2 children, or there are none, at which point we may stop iterating.
            let tuples = (start..end)
                .take_while(|&i| tree.get(left_child(i)).is_some())
                .map(|i| (tree[left_child(i)].clone(), tree[right_child(i)].clone()))
                .collect::<Vec<_>>();
            // Compute and store the hashes for each node in the current level.
            let num_full_nodes = tuples.len();
            for (tree_node, (left, right)) in tree[start..][..num_full_nodes].iter_mut().zip(tuples.iter()) {
                *tree_node = path_hasher.hash_children(left, right);
            }
            // Use the precomputed empty node hash for every empty node, if there are any.
            if start + num_full_nodes < end {
                let empty_node_hash = path_hasher.hash_children(&empty_hash, &empty_hash);
                for node in tree.iter_mut().take(end).skip(start + num_full_nodes) {
                    *node = empty_node_hash.clone();
                }
            }
            // Update the start index for the next level.
            start_index = start;
        }

        // Compute the root hash, by iterating from the root level up to `DEPTH`.
        let mut root_hash = tree[0].clone();
        for _ in 0..padding_depth {
            // Update the root hash, by hashing the current root hash with the empty hash.
            root_hash = path_hasher.hash_children(&root_hash, &empty_hash);
        }

        Ok(Self {
            leaf_hasher,
            path_hasher,
            root: root_hash,
            tree,
            empty_hash,
            number_of_leaves: leaves.len(),
        })
    }

    /// Returns `true` if the given Merkle path is valid for the given root and leaf.
    pub fn verify(&self, path: &MerklePath<E, DEPTH>, root: &PH::Hash, leaf: &LH::Leaf) -> Boolean<E> {
        path.verify(&self.leaf_hasher, &self.path_hasher, root, leaf)
    }

    /// Returns the leaf hasher of the Merkle tree.
    pub const fn leaf_hasher(&self) -> &LH {
        &self.leaf_hasher
    }

    /// Returns the path hasher of the Merkle tree.
    pub const fn path_hasher(&self) -> &PH {
        &self.path_hasher
    }

    /// Returns the Merkle root of the tree.
    pub const fn root(&self) -> &PH::Hash {
        &self.root
    }

    /// Returns the Merkle tree (excluding the hashes of the leaves).
    pub fn tree(&self) -> &[PH::Hash] {
        &self.tree
    }

    /// Returns the empty hash.
    pub const fn empty_hash(&self) -> &PH::Hash {
        &self.empty_hash
    }

    /// Returns the number of leaves in the Merkle tree.
    pub const fn number_of_leaves(&self) -> usize {
        self.number_of_leaves
    }
}

/// Returns the depth of the tree, given the size of the tree.
#[inline]
fn tree_depth<const DEPTH: u8>(tree_size: usize) -> Result<u8> {
    let tree_size = u64::try_from(tree_size)?;
    // Since we only allow tree sizes up to u64::MAX, the maximum possible depth is 63.
    let tree_depth = u8::try_from(tree_size.checked_ilog2().unwrap_or(0))?;
    // Ensure the tree depth is within the depth bound.
    match tree_depth <= DEPTH {
        // Return the tree depth.
        true => Ok(tree_depth),
        false => bail!("Merkle tree cannot exceed depth {DEPTH}: attempted to reach depth {tree_depth}"),
    }
}

/// Returns the index of the left child, given an index.
#[inline]
const fn left_child(index: usize) -> usize {
    2 * index + 1
}

/// Returns the index of the right child, given an index.
#[inline]
const fn right_child(index: usize) -> usize {
    2 * index + 2
}

/// Returns the index of the parent, given the index of a child.
#[inline]
const fn parent(index: usize) -> Option<usize> {
    if index > 0 { Some((index - 1) >> 1) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snarkvm_circuit_network::AleoV0 as Circuit;
    use snarkvm_circuit_types::environment::UpdatableCount;
    use snarkvm_console_collections::merkle_tree::MerkleTree as ConsoleMerkleTree;
    use snarkvm_utilities::{TestRng, Uniform};
    use snarkvm_circuit_algorithms::{Poseidon2, Poseidon8};

    use anyhow::Result;

    type CurrentNetwork = <Circuit as Environment>::Network;
    type NativeLH = console::algorithms::Poseidon8<CurrentNetwork>;
    type NativePH = console::algorithms::Poseidon2<CurrentNetwork>;
    type CircuitLH =Poseidon8<Circuit>;
    type CircuitPH = Poseidon2<Circuit>;

    const ITERATIONS: u128 = 100;

    // The minimum and maximum number of field elements a leaf can contain.
    const MIN_LEAF_LENGTH: usize = 1;
    const MAX_LEAF_LENGTH: usize = 12;

    fn check_new<const DEPTH: u8>(
        mode: Mode,
        num_constants: u64,
        num_public: u64,
        num_private: u64,
        num_constraints: u64,
    ) -> Result<()> {
        let mut rng = TestRng::default();

        let mut create_leaves = |num_leaves| {
            (0..num_leaves)
                .map(|_| console::Field::<<Circuit as Environment>::Network>::rand(&mut rng).to_bits_le())
                .collect::<Vec<_>>()
        };

        for i in 0..ITERATIONS {
            // Determine the number of leaves.
            let num_leaves = core::cmp::min(2u128.pow(DEPTH as u32), i);
            // Compute the leaves.
            let leaves = create_leaves(num_leaves);
            // Compute the Merkle tree.
            let merkle_tree = <<Circuit as Environment>::Network as snarkvm_console_network::Network>::merkle_tree_bhp::<
                DEPTH,
            >(&leaves)?;

            for (index, leaf) in leaves.iter().enumerate() {
                // Compute the Merkle path.
                let merkle_path = merkle_tree.prove(index, leaf)?;

                // // Initialize the Merkle leaf.
                // let leaf: Vec<Boolean<_>> = Inject::new(mode, leaf.clone());

                Circuit::scope(format!("New {mode}"), || {
                    let candidate = MerklePath::<Circuit, DEPTH>::new(mode, merkle_path.clone());
                    assert_eq!(merkle_path, candidate.eject_value());
                    assert_scope!(num_constants, num_public, num_private, num_constraints);
                });
                Circuit::reset();
            }
        }
        Ok(())
    }

    #[test]
    fn test_new_constant() -> Result<()> {
        check_new::<32>(Mode::Constant, 96, 0, 0, 0)
    }

    #[test]
    fn test_new_public() -> Result<()> {
        check_new::<32>(Mode::Public, 0, 96, 0, 64)
    }

    #[test]
    fn test_new_private() -> Result<()> {
        check_new::<32>(Mode::Private, 0, 0, 96, 64)
    }

    fn test_compatibility<const DEPTH: u8>(mode: Mode, count: UpdatableCount, rng: &mut TestRng) {
    
        for num_leaves in 1..=1 << DEPTH {

            Circuit::reset();
            
            // **** Console tree
            let console_leaf_hasher = NativeLH::setup("AleoMerklePathTest0").unwrap();
            let console_path_hasher = NativePH::setup("AleoMerklePathTest1").unwrap();

            let circuit_leaf_hasher = CircuitLH::constant(console_leaf_hasher.clone());
            let circuit_path_hasher = CircuitPH::constant(console_path_hasher.clone());

            let console_leaves = (0..num_leaves).map(|i| {
                // We want to generate a random-looking leaf length for testing
                // purposes but need to do so in a deterministic way due to
                // constant/public/private-variable count checks. One has
                // MAX_LEAF_LENGTH - MIN_LEAF_LENGTH = 11, which is coprime with
                // 7 * DEPTH * i * num_leaves in the tested cases except
                // whenever num_leaves is a multiple of 11.
                let leaf_length = MIN_LEAF_LENGTH + (7usize * DEPTH as usize * i * num_leaves) % (MAX_LEAF_LENGTH - MIN_LEAF_LENGTH);
                (0..leaf_length).map(|_| console::Field::<CurrentNetwork>::rand(rng)).collect_vec()
            }).collect_vec();

            let console_tree = ConsoleMerkleTree::<CurrentNetwork, NativeLH, NativePH, DEPTH>::new(
                &console_leaf_hasher,
                &console_path_hasher,
                &console_leaves,
            ).unwrap();

            // **** Circuit tree
            let circuit_leaves = console_leaves.iter().map(|leaf| {
                leaf.iter().map(|leaf_element| Field::new(mode, *leaf_element)).collect_vec()
            }).collect_vec();

            let circuit_tree = MerkleTree::<Circuit, CircuitLH, CircuitPH, DEPTH>::new(
                circuit_leaf_hasher,
                circuit_path_hasher,
                &circuit_leaves,
            ).unwrap();

            // Check the count
            count.assert_matches(
                Circuit::num_constants(),
                Circuit::num_public(),
                Circuit::num_private(),
                Circuit::num_constraints(),
            );

            assert_eq!(*console_tree.root(), circuit_tree.root().eject_value());
        }
    }

    #[test]
    fn test_merkle_tree_compatibility_circuit_console() {
        let mut rng = TestRng::default();
        test_compatibility::<1>(Mode::Public, count_is!(1072, 6, 1505, 1505), &mut rng);
        test_compatibility::<2>(Mode::Private, count_is!(1075, 1, 4154, 4135), &mut rng);
        test_compatibility::<3>(Mode::Public, count_is!(1083, 37, 8255, 8255), &mut rng);
        test_compatibility::<4>(Mode::Private, count_is!(1099, 1, 17652, 17570), &mut rng);
        test_compatibility::<5>(Mode::Public, count_is!(1132, 196, 38785, 38785), &mut rng);
        test_compatibility::<6>(Mode::Private, count_is!(1196, 1, 77954, 77570), &mut rng);
    }
}
