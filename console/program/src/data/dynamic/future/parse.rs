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

impl<N: Network> Parser for DynamicFuture<N> {
    /// Parses a string into a future value.
    #[inline]
    fn parse(string: &str) -> ParserResult<Self> {
        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the "{" from the string.
        let (string, _) = tag("{")(string)?;

        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the "program_name" from the string.
        let (string, _) = tag("program_name")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the ":" from the string.
        let (string, _) = tag(":")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the program name from the string.
        let (string, program_name) = Field::parse(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the "," from the string.
        let (string, _) = tag(",")(string)?;

        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the "program_network" from the string.
        let (string, _) = tag("program_network")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the ":" from the string.
        let (string, _) = tag(":")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the program network from the string.
        let (string, program_network) = Field::parse(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the "," from the string.
        let (string, _) = tag(",")(string)?;

        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the "function_name" from the string.
        let (string, _) = tag("function_name")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the ":" from the string.
        let (string, _) = tag(":")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the function name from the string.
        let (string, function_name) = Field::parse(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the "," from the string.
        let (string, _) = tag(",")(string)?;

        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the "root" from the string.
        let (string, _) = tag("root")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the ":" from the string.
        let (string, _) = tag(":")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the argument root from the string.
        let (string, root) = Field::parse(string)?;

        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the "}" from the string.
        let (string, _) = tag("}")(string)?;

        Ok((string, Self::new_unchecked(program_name, program_network, function_name, root, None, None)))
    }
}

impl<N: Network> FromStr for DynamicFuture<N> {
    type Err = Error;

    /// Returns a future from a string literal.
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

impl<N: Network> Debug for DynamicFuture<N> {
    /// Prints the future as a string.
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl<N: Network> Display for DynamicFuture<N> {
    /// Prints the future as a string.
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        self.fmt_internal(f)
    }
}

impl<N: Network> DynamicFuture<N> {
    /// Prints the dynamic future with the given indentation depth.
    fn fmt_internal(&self, f: &mut Formatter) -> fmt::Result {
        writeln!(
            f,
            "{{ program_name: {}, program_network: {}, function_name: {}, root: {} }}",
            self.program_name(),
            self.program_network(),
            self.function_name(),
            self.root(),
        )
    }
}
