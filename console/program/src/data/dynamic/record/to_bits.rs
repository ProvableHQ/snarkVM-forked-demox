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

impl<N: Network> ToBits for DynamicRecord<N, Plaintext<N>> {
    /// Returns this data as a list of **little-endian** bits.
    fn write_bits_le(&self, vec: &mut Vec<bool>) {
        // Construct the owner visibility bit.
        vec.push(self.owner.is_private());

        // Construct the owner bits.
        match &self.owner {
            Owner::Public(public) => public.write_bits_le(vec),
            Owner::Private(Plaintext::Literal(Literal::Address(address), ..)) => address.write_bits_le(vec),
            _ => N::halt("Internal error: plaintext to_bits_le corrupted in record owner"),
        };

        // Construct the root bits.
        self.root.write_bits_le(vec);

        // Construct the nonce bits.
        self.nonce.write_bits_le(vec);

        // Construct the version bits.
        self.version.write_bits_le(vec);
    }

    /// Returns this data as a list of **big-endian** bits.
    fn write_bits_be(&self, vec: &mut Vec<bool>) {
        // Construct the owner visibility bit.
        vec.push(self.owner.is_private());

        // Construct the owner bits.
        match &self.owner {
            Owner::Public(public) => public.write_bits_be(vec),
            Owner::Private(Plaintext::Literal(Literal::Address(address), ..)) => address.write_bits_be(vec),
            _ => N::halt("Internal error: plaintext to_bits_be corrupted in record owner"),
        };

        // Construct the root bits.
        self.root.write_bits_be(vec);

        // Construct the nonce bits.
        self.nonce.write_bits_be(vec);

        // Construct the version bits.
        self.version.write_bits_be(vec);
    }
}

impl<N: Network> ToBits for DynamicRecord<N, Ciphertext<N>> {
    /// Returns this data as a list of **little-endian** bits.
    fn write_bits_le(&self, vec: &mut Vec<bool>) {
        // Construct the owner visibility bit.
        vec.push(self.owner.is_private());

        // Construct the owner bits.
        match &self.owner {
            Owner::Public(public) => public.write_bits_le(vec),
            Owner::Private(ciphertext) => {
                // Ensure there is exactly one field element in the ciphertext.
                match ciphertext.len() == 1 {
                    true => ciphertext[0].write_bits_le(vec),
                    false => N::halt("Internal error: ciphertext to_bits_le corrupted in record owner"),
                }
            }
        };

        // Construct the root bits.
        self.root.write_bits_le(vec);

        // Construct the nonce bits.
        self.nonce.write_bits_le(vec);

        // Construct the version bits.
        self.version.write_bits_le(vec);
    }

    /// Returns this data as a list of **big-endian** bits.
    fn write_bits_be(&self, vec: &mut Vec<bool>) {
        // Construct the owner visibility bit.
        vec.push(self.owner.is_private());

        // Construct the owner bits.
        match &self.owner {
            Owner::Public(public) => public.write_bits_be(vec),
            Owner::Private(ciphertext) => {
                // Ensure there is exactly one field element in the ciphertext.
                match ciphertext.len() == 1 {
                    true => ciphertext[0].write_bits_be(vec),
                    false => N::halt("Internal error: ciphertext to_bits_be corrupted in record owner"),
                }
            }
        };

        // Construct the root bits.

        // Construct the nonce bits.
        self.nonce.write_bits_be(vec);

        // Construct the version bits.
        self.version.write_bits_be(vec);
    }
}
