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

mod bytes;
mod parse;
mod serialize;
mod size_in_bits;

use crate::{ArrayType, Identifier, LiteralType, StructType};
use snarkvm_console_network::prelude::*;

/// A `PlaintextType` defines the type parameter for a literal, struct, or array.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum PlaintextType<N: Network> {
    /// A literal type contains its type name.
    /// The format of the type is `<type_name>`.
    Literal(LiteralType),
    /// An struct type contains its identifier.
    /// The format of the type is `<identifier>`.
    Struct(Identifier<N>),
    /// An array type contains its element type and length.
    /// The format of the type is `[<element_type>; <length>]`.
    Array(ArrayType<N>),
}

impl<N: Network> From<LiteralType> for PlaintextType<N> {
    /// Initializes a plaintext type from a literal type.
    fn from(literal: LiteralType) -> Self {
        PlaintextType::Literal(literal)
    }
}

impl<N: Network> From<Identifier<N>> for PlaintextType<N> {
    /// Initializes a plaintext type from a struct type.
    fn from(struct_: Identifier<N>) -> Self {
        PlaintextType::Struct(struct_)
    }
}

impl<N: Network> From<ArrayType<N>> for PlaintextType<N> {
    /// Initializes a plaintext type from an array type.
    fn from(array: ArrayType<N>) -> Self {
        PlaintextType::Array(array)
    }
}

impl<N: Network> PlaintextType<N> {
    // Prefix bits for (de)serializing the `Array` variant.
    pub const ARRAY_PREFIX_BITS: [bool; 2] = [true, false];
    /// Prefix bits for (de)serializing the `Literal` variant.
    pub const LITERAL_PREFIX_BITS: [bool; 2] = [false, false];
    /// Prefix bits for (de)serializing the `Struct` variant.
    pub const STRUCT_PREFIX_BITS: [bool; 2] = [false, true];

    /// Returns `true` if the `PlaintextType` contains a string type.
    pub fn contains_string_type(&self) -> bool {
        match self {
            Self::Literal(LiteralType::String) => true,
            Self::Array(array_type) => array_type.contains_string_type(),
            _ => false, // Structs are checked in their definition.
        }
    }

    /// Returns `true` if the `PlaintextType` is an array and the size exceeds the given maximum.
    pub fn exceeds_max_array_size(&self, max_array_size: u32) -> bool {
        match self {
            Self::Literal(_) | Self::Struct(_) => false,
            Self::Array(array_type) => array_type.exceeds_max_array_size(max_array_size),
        }
    }
}
