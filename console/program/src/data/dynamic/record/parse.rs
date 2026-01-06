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

impl<N: Network> Parser for DynamicRecord<N> {
    /// Parses a string as a dynamic record: `{ owner: address, _root: field, _nonce: field, _version: u8 }`.
    #[inline]
    fn parse(string: &str) -> ParserResult<Self> {
        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the "{" from the string.
        let (string, _) = tag("{")(string)?;

        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the "owner" tag from the string.
        let (string, _) = tag("owner")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the ":" from the string.
        let (string, _) = tag(":")(string)?;
        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the owner from the string.
        let (string, owner) = Address::parse(string)?;
        // Parse the "," from the string.
        let (string, _) = tag(",")(string)?;

        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the "_root" tag from the string.
        let (string, _) = tag("_root")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the ":" from the string.
        let (string, _) = tag(":")(string)?;
        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the nonce from the string.
        let (string, (root, _)) = pair(Field::parse, tag(".private"))(string)?;
        // Parse the "," from the string.
        let (string, _) = tag(",")(string)?;

        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the "_nonce" tag from the string.
        let (string, _) = tag("_nonce")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the ":" from the string.
        let (string, _) = tag(":")(string)?;
        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the nonce from the string.
        let (string, (nonce, _)) = pair(Group::parse, tag(".public"))(string)?;

        // There may be an optional "_version" tag. Parse the "," from the string if it exists.
        let string = match opt(tag(","))(string)? {
            // If there is a version, then parse the "," from the string.
            (string, Some(_)) => string,
            // If there is no version, then keep the string as is.
            (string, None) => string,
        };

        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the optional "_version" tag from the string.
        let (string, version) = match opt(tag("_version"))(string)? {
            // If there is no version, then set the version to zero.
            (string, None) => (string, U8::zero()),
            // If there is a version, then parse the version from the string.
            (string, Some(_)) => {
                // Parse the whitespace from the string.
                let (string, _) = Sanitizer::parse_whitespaces(string)?;
                // Parse the ":" from the string.
                let (string, _) = tag(":")(string)?;
                // Parse the whitespace and comments from the string.
                let (string, _) = Sanitizer::parse(string)?;
                // Parse the version from the string.
                terminated(U8::parse, tag(".public"))(string)?
            }
        };

        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the '}' from the string.
        let (string, _) = tag("}")(string)?;
        // Output the dynamic record.
        Ok((string, DynamicRecord::new_unchecked(owner, root, nonce, version, None)))
    }
}

impl<N: Network> FromStr for DynamicRecord<N> {
    type Err = Error;

    /// Returns a dynamic record from a string literal.
    fn from_str(string: &str) -> Result<Self> {
        match Self::parse(string) {
            Ok((remainder, object)) => {
                // Ensure the remainder is empty.
                ensure!(remainder.is_empty(), "Failed to parse string. Found invalid character in: \"{remainder}\"");
                // Return the object.
                Ok(object)
            }
            Err(error) => bail!("Failed to parse string. {error}"),
        }
    }
}

impl<N: Network> Debug for DynamicRecord<N> {
    /// Prints the dynamic record as a string.
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl<N: Network> Display for DynamicRecord<N> {
    /// Prints the dynamic record as a string.
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.fmt_internal(f, 0)
    }
}

impl<N: Network> DynamicRecord<N> {
    /// Prints the dynamic record with the given indentation depth.
    fn fmt_internal(&self, f: &mut Formatter, depth: usize) -> fmt::Result {
        /// The number of spaces to indent.
        const INDENT: usize = 2;

        // Print the opening brace.
        write!(f, "{{")?;
        // Print the owner with a comma.
        write!(f, "\n{:indent$}owner: {},", "", self.owner, indent = (depth + 1) * INDENT)?;
        // Print the root woth a comma.
        write!(f, "\n{:indent$}_root: {}.private,", "", self.root, indent = (depth + 1) * INDENT)?;
        // Print the nonce with a comma.
        write!(f, "\n{:indent$}_nonce: {}.public,", "", self.nonce, indent = (depth + 1) * INDENT)?;
        // Print the version without a comma.
        write!(f, "\n{:indent$}_version: {}.public", "", self.version, indent = (depth + 1) * INDENT)?;
        // Print the closing brace.
        write!(f, "\n{:indent$}}}", "", indent = depth * INDENT)
    }
}
