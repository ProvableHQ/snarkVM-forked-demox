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

impl<N: Network> FromBytes for DynamicRecord<N> {
    /// Reads the dynamic record from a buffer.
    fn read_le<R: Read>(mut reader: R) -> IoResult<Self> {
        // Read the variant.
        let variant = U8::<N>::new(u8::read_le(&mut reader)?);

        // Set the version based on the variant.
        let version = match *variant {
            0 | 1 => U8::zero(),
            2 | 3 => U8::one(),
            4.. => return Err(error(format!("Failed to decode record variant ({variant}) for the version"))),
        };

        // Read the owner.
        let owner = match *variant {
            0 | 2 => Owner::Public(Address::read_le(&mut reader)?),
            1 | 3 => Owner::Private(Plaintext::read_le(&mut reader)?),
            4.. => return Err(error(format!("Failed to decode record variant ({variant}) for the owner"))),
        };

        // Read the root.
        // TODO (@d0cd) check that this encoding is differentiated from static records.
        let root = Field::read_le(&mut reader)?;

        // Read the nonce.
        let nonce = Group::read_le(&mut reader)?;

        Ok(Self::new_unchecked(owner, root, nonce, version, None, None))
    }
}

impl<N: Network> ToBytes for DynamicRecord<N> {
    /// Writes the record to a buffer.
    fn write_le<W: Write>(&self, mut writer: W) -> IoResult<()> {
        // Set the variant.
        let variant = match (*self.version, self.owner.is_public()) {
            (0, true) => 0u8,
            (0, false) => 1u8,
            (1, true) => 2u8,
            (1, false) => 3u8,
            (_, _) => {
                return Err(error(format!(
                    "Failed to encode record - variant mismatch (version = {}, hiding = {}, owner = {})",
                    self.version,
                    self.is_hiding(),
                    self.owner.is_public()
                )));
            }
        };

        #[cfg(debug_assertions)]
        {
            // Ensure the version is correct.
            let is_version_correct = match (!self.is_hiding(), self.owner.is_public()) {
                (true, true) => variant == 0,
                (true, false) => variant == 1,
                (false, true) => variant == 2,
                (false, false) => variant == 3,
            };
            if !is_version_correct {
                return Err(error(format!(
                    "Failed to encode record - version mismatch (version = {}, hiding = {}, owner = {})",
                    self.version,
                    self.is_hiding(),
                    self.owner.is_public()
                )));
            }
        }

        // Write the variant.
        variant.write_le(&mut writer)?;

        // Write the owner.
        match &self.owner {
            Owner::Public(owner) => owner.write_le(&mut writer)?,
            Owner::Private(owner) => owner.write_le(&mut writer)?,
        };

        // Write the root.
        self.root.write_le(&mut writer)?;

        // Write the nonce.
        self.nonce.write_le(&mut writer)
    }
}
