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

impl<N: Network> FromBytes for TransitionLeaf<N> {
    /// Reads the transition leaf from a buffer.
    fn read_le<R: Read>(mut reader: R) -> IoResult<Self> {
        // Read the version.
        let version = FromBytes::read_le(&mut reader)?;
        // Ensure the version is valid.
        if version != TRANSITION_LEAF_VERSION && version != TRANSITION_LEAF_VERSION_DYNAMIC {
            return Err(error("Invalid transition leaf version"));
        }
        // Read the index.
        let index = FromBytes::read_le(&mut reader)?;
        // Read the variant.
        let variant = FromBytes::read_le(&mut reader)?;
        // Ensure the version and variant are compatible.
        // Dynamic version (2) is only allowed for Record (3) and ExternalRecord (4) variants.
        if version == TRANSITION_LEAF_VERSION_DYNAMIC && variant != 3 && variant != 4 {
            return Err(error(
                "Dynamic transition leaf version is only allowed for Record and ExternalRecord variants",
            ));
        }
        // Read the ID.
        let id = FromBytes::read_le(&mut reader)?;
        // Return the transition leaf.
        Ok(Self::from(version, index, variant, id))
    }
}

impl<N: Network> ToBytes for TransitionLeaf<N> {
    /// Writes the transition leaf to a buffer.
    fn write_le<W: Write>(&self, mut writer: W) -> IoResult<()> {
        // Write the version.
        self.version.write_le(&mut writer)?;
        // Write the index.
        self.index.write_le(&mut writer)?;
        // Write the variant.
        self.variant.write_le(&mut writer)?;
        // Write the ID.
        self.id.write_le(&mut writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use snarkvm_console_network::MainnetV0;

    type CurrentNetwork = MainnetV0;

    const ITERATIONS: u64 = 1000;

    #[test]
    fn test_bytes() -> Result<()> {
        let mut rng = TestRng::default();

        for _ in 0..ITERATIONS {
            // Sample a static leaf (version 1).
            let expected = test_helpers::sample_leaf(&mut rng);

            // Check the byte representation.
            let expected_bytes = expected.to_bytes_le()?;
            assert_eq!(expected, TransitionLeaf::read_le(&expected_bytes[..])?);

            // Sample a dynamic leaf (version 2).
            let expected_dynamic = test_helpers::sample_dynamic_leaf(&mut rng);

            // Check the byte representation.
            let expected_dynamic_bytes = expected_dynamic.to_bytes_le()?;
            assert_eq!(expected_dynamic, TransitionLeaf::read_le(&expected_dynamic_bytes[..])?);
        }
        Ok(())
    }

    #[test]
    fn test_version_variant_validation() -> Result<()> {
        let mut rng = TestRng::default();
        let id = Uniform::rand(&mut rng);

        // Test that version 1 (static) works with any variant.
        for variant in 0..=7 {
            let leaf = TransitionLeaf::<CurrentNetwork>::new_with_version(0, variant, id);
            let bytes = leaf.to_bytes_le()?;
            assert!(TransitionLeaf::<CurrentNetwork>::read_le(&bytes[..]).is_ok());
        }

        // Test that version 2 (dynamic) only works with Record (3) and ExternalRecord (4).
        for variant in [3u8, 4u8] {
            let leaf = TransitionLeaf::<CurrentNetwork>::new_dynamic_with_version(0, variant, id);
            let bytes = leaf.to_bytes_le()?;
            assert!(TransitionLeaf::<CurrentNetwork>::read_le(&bytes[..]).is_ok());
        }

        // Test that version 2 (dynamic) fails with other variants.
        for variant in [0u8, 1, 2, 5, 6, 7] {
            let leaf = TransitionLeaf::<CurrentNetwork>::from(TRANSITION_LEAF_VERSION_DYNAMIC, 0, variant, id);
            let bytes = leaf.to_bytes_le()?;
            assert!(TransitionLeaf::<CurrentNetwork>::read_le(&bytes[..]).is_err());
        }

        Ok(())
    }
}
