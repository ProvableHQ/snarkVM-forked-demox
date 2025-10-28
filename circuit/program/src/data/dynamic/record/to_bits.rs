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

impl<A: Aleo> ToBits for DynamicRecord<A> {
    type Boolean = Boolean<A>;

    /// Returns the circuit dynamic record as a list of **little-endian** bits.
    fn write_bits_le(&self, vec: &mut Vec<Self::Boolean>) {
        // Construct the owner visibility bit.
        vec.push(self.owner.is_private());

        // Construct the owner bits.
        match &self.owner {
            Owner::Public(public) => public.write_bits_le(vec),
            Owner::Private(Plaintext::Literal(Literal::Address(address), ..)) => address.write_bits_le(vec),
            _ => A::halt("Internal error: plaintext to_bits_le corrupted in record owner"),
        };

        // Construct the root bits.
        self.root.write_bits_le(vec);

        // Construct the nonce bits.
        self.nonce.write_bits_le(vec);

        // Construct the version bits.
        self.version.write_bits_le(vec);
    }

    /// Returns the circuit dynamic record as a list of **big-endian** bits.
    fn write_bits_be(&self, vec: &mut Vec<Self::Boolean>) {
        // Construct the owner visibility bit.
        vec.push(self.owner.is_private());

        // Construct the owner bits.
        match &self.owner {
            Owner::Public(public) => public.write_bits_be(vec),
            Owner::Private(Plaintext::Literal(Literal::Address(address), ..)) => address.write_bits_be(vec),
            _ => A::halt("Internal error: plaintext to_bits_be corrupted in record owner"),
        };

        // Construct the root bits.
        self.root.write_bits_be(vec);

        // Construct the nonce bits.
        self.nonce.write_bits_be(vec);

        // Construct the version bits.
        self.version.write_bits_be(vec);
    }
}
