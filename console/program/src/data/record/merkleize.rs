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

use snarkvm_console_algorithms::Poseidon2;
use snarkvm_console_collections::merkle_tree::PathHash;

// TODO (Antonio) Add consistency check with circuit/program/src/data/record/merkleize/mod.rs
use super::*;

impl<N: Network> Record<N, Plaintext<N>> {
    /// Returns the root of the merkleized record data as defined in `DynamicRecord`
    pub fn merkleize(
        &self,
    ) -> Result<Field<N>> {
        let depth = N::MAX_DATA_ENTRIES.ilog2();

        ensure!(self.data.len() > 0, "A Record must have at least one entry in order to be merkleized");
        ensure!(
            self.data.len() <= N::MAX_DATA_ENTRIES,
            "The record exceeds the maximum allowed size ({} > {})",
            self.data.len(),
            N::MAX_DATA_ENTRIES
        );

        // Initialize the padding value
        let path_hasher = Poseidon2::<N>::setup("DynamicRecordPathHasher")?;
        let padding_hash = path_hasher.hash_empty()?;
        
        let mut level = self.data.iter().map(|(identifier, entry)| {
            // TODO (Antonio) Make sure when we add the identifier is grabbed here, it has been loaded as a constant
            let mut hash_input = vec![identifier.to_field()?];
            // TODO (Antonio) secure? length check?
            hash_input.extend(entry.to_fields()?);
            // TODO (Antonio) Print the hash input
            println!("   hash_input: {:?}", hash_input);
            N::hash_psd8(hash_input.as_slice())
        }).collect::<Result<Vec<Field<N>>>>()?;
        
        // TODO (Antonio) Print the leaves
        println!("\n**** In Record::merkleize");
        for (i, e) in level.iter().enumerate() {
            println!("   leaf {}: {:?}", i, e);
        }
        println!("****\n");        

        for _ in 0..depth {
            //Padding the level to even length
            if level.len() % 2 == 1 {
                level.push(padding_hash.clone());
            }

            // Hashing pairs of nodes
            let next_level = level.chunks_exact(2).map(|left_and_right| {
                N::hash_psd2(left_and_right)
            }).collect::<Result<Vec<Field<N>>>>()?;
            level = next_level;
        }

        ensure!(level.len() == 1, "Root level of Merkle tree has {} nodes, expected 1", level.len());
        Ok(level.pop().unwrap())
    }
}

impl<N: Network> Record<N, Ciphertext<N>> {
    /// Returns the record commitment.
    pub fn merkleize(
        &self,
        _program_id: &ProgramID<N>,
        _record_name: &Identifier<N>,
        _record_view_key: &Field<N>,
    ) -> Result<Field<N>> {
        bail!("Illegal operation: Record::merkleize() cannot be invoked on the `Ciphertext` variant.")
    }
}
