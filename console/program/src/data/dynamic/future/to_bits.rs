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

impl<N: Network> ToBits for DynamicFuture<N> {
    /// Returns the dynamic future as a list of **little-endian** bits.
    #[inline]
    fn write_bits_le(&self, vec: &mut Vec<bool>) {
        // Write the bits for the program name.
        vec.extend_from_slice(&self.program_name.to_bits_le());

        // Write the bits for the program network.
        vec.extend_from_slice(&self.program_network.to_bits_le());

        // Write the bits for the function name.
        vec.extend_from_slice(&self.function_name.to_bits_le());

        // Write the bits for the root.
        vec.extend_from_slice(&self.root.to_bits_le());
    }

    /// Returns the dynamic future as a list of **big-endian** bits.
    #[inline]
    fn write_bits_be(&self, vec: &mut Vec<bool>) {
        // Write the bits for the program name.
        vec.extend_from_slice(&self.program_name.to_bits_be());

        // Write the bits for the function network.
        vec.extend_from_slice(&self.program_network.to_bits_be());

        // Write the bits for the function name.
        vec.extend_from_slice(&self.function_name.to_bits_be());

        // Write the bits for the root.
        vec.extend_from_slice(&self.root.to_bits_be());
    }
}

impl<N: Network> DynamicFuture<N> {
    /// Returns the number of bits in a dynamic future.
    #[inline]
    pub fn size_in_bits() -> Result<usize> {
        Field::<N>::size_in_bits().checked_mul(4).ok_or_else(|| anyhow!("`size_in_bits` overflowed"))
    }

    /// Returns the number of raw bits in a dynamic future.
    #[inline]
    pub fn size_in_bits_raw() -> Result<usize> {
        Self::size_in_bits()
    }
}
