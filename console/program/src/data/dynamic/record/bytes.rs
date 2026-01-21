// Copyright (c) 2019-2026 Provable Inc.
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
        // Read and validate the version.
        match u8::read_le(&mut reader) {
            Ok(0) => {} // The only valid version for a dynamic record.
            _ => {
                return Err(error("Failed to deserialize dynamic record - invalid version"));
            }
        }

        // Read the owner.
        let owner = Address::read_le(&mut reader)?;

        // Read the root.
        let root = Field::read_le(&mut reader)?;

        // Read the nonce.
        let nonce = Group::read_le(&mut reader)?;

        // Read the version.
        let version = U8::read_le(&mut reader)?;

        Ok(Self::new_unchecked(owner, root, nonce, version, None))
    }
}

impl<N: Network> ToBytes for DynamicRecord<N> {
    /// Writes the record to a buffer.
    fn write_le<W: Write>(&self, mut writer: W) -> IoResult<()> {
        // Write the version.
        0u8.write_le(&mut writer)?;

        // Write the owner.
        self.owner.write_le(&mut writer)?;

        // Write the root.
        self.root.write_le(&mut writer)?;

        // Write the nonce.
        self.nonce.write_le(&mut writer)?;

        // Write the version.
        self.version.write_le(&mut writer)
    }
}
