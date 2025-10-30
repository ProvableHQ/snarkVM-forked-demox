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

/// An argument passed into a future.
#[derive(Clone)]
pub enum Argument<N: Network> {
    /// A plaintext value.
    Plaintext(Plaintext<N>),
    /// A future.
    Future(Future<N>),
    /// A dynamic future.
    DynamicFuture(DynamicFuture<N>),
}

impl<N: Network> Equal<Self> for Argument<N> {
    type Output = Boolean<N>;

    /// Returns `true` if `self` and `other` are equal.
    fn is_equal(&self, other: &Self) -> Self::Output {
        match (self, other) {
            (Self::Plaintext(a), Self::Plaintext(b)) => a.is_equal(b),
            (Self::Future(a), Self::Future(b)) => a.is_equal(b),
            (Self::DynamicFuture(a), Self::DynamicFuture(b)) => a.is_equal(b),
            (Self::Plaintext(..), _) | (Self::Future(..), _) | (Self::DynamicFuture(..), _) => Boolean::new(false),
        }
    }

    /// Returns `true` if `self` and `other` are *not* equal.
    fn is_not_equal(&self, other: &Self) -> Self::Output {
        match (self, other) {
            (Self::Plaintext(a), Self::Plaintext(b)) => a.is_not_equal(b),
            (Self::Future(a), Self::Future(b)) => a.is_not_equal(b),
            (Self::DynamicFuture(a), Self::DynamicFuture(b)) => a.is_not_equal(b),
            (Self::Plaintext(..), _) | (Self::Future(..), _) | (Self::DynamicFuture(..), _) => Boolean::new(true),
        }
    }
}

impl<N: Network> ToBits for Argument<N> {
    /// Returns the argument as a list of **little-endian** bits.
    #[inline]
    fn write_bits_le(&self, vec: &mut Vec<bool>) {
        match self {
            Self::Plaintext(plaintext) => {
                vec.push(false);
                plaintext.write_bits_le(vec);
            }
            Self::Future(future) => {
                vec.push(true);
                future.write_bits_le(vec);
            }
            Self::DynamicFuture(dynamic_future) => {
                vec.push(true);
                // Note. This encoding is needed to uniquely disambiguate dynamic futures from static futures.
                // This is sound because:
                //  - a static future expects the program ID bits after the initial tag bit
                //  - a program ID contains two `Identifier`s
                //  - an `Identifier` cannot lead with a zero byte, since a leading zero byte implies an empty string.
                vec.extend(0u8.to_bits_le());
                dynamic_future.write_bits_le(vec);
            }
        }
    }

    /// Returns the argument as a list of **big-endian** bits.
    #[inline]
    fn write_bits_be(&self, vec: &mut Vec<bool>) {
        match self {
            Self::Plaintext(plaintext) => {
                vec.push(false);
                plaintext.write_bits_be(vec);
            }
            Self::Future(future) => {
                vec.push(true);
                future.write_bits_be(vec);
            }
            Self::DynamicFuture(dynamic_future) => {
                vec.push(true);
                // Note. This encoding is needed to uniquely disambiguate dynamic futures from static futures.
                // This is sound because:
                //  - a static future expects the program ID bits after the initial tag bit
                //  - a program ID contains two `Identifier`s
                //  - an `Identifier` cannot lead with a zero byte, since a leading zero byte implies an empty string.
                vec.extend(0u8.to_bits_be());
                dynamic_future.write_bits_be(vec);
            }
        }
    }
}

impl<N: Network> ToFields for Argument<N> {
    type Field = Field<N>;

    /// Returns this plaintext as a list of field elements.
    fn to_fields(&self) -> Result<Vec<Self::Field>> {
        // Encode the data as little-endian bits.
        let mut bits_le = self.to_bits_le();
        // Adds one final bit to the data, to serve as a terminus indicator.
        // During decryption, this final bit ensures we've reached the end.
        bits_le.push(true);
        // Pack the bits into field elements.
        let fields = bits_le
            .chunks(Field::<N>::size_in_data_bits())
            .map(Field::<N>::from_bits_le)
            .collect::<Result<Vec<_>>>()?;
        // Ensure the number of field elements does not exceed the maximum allowed size.
        match fields.len() <= N::MAX_DATA_SIZE_IN_FIELDS as usize {
            true => Ok(fields),
            false => bail!("Argument exceeds maximum allowed size"),
        }
    }
}
