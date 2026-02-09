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
        let (string, _) = tag("_program_name")(string)?;
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
        let (string, _) = tag("_program_network")(string)?;
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
        let (string, _) = tag("_function_name")(string)?;
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
        let (string, _) = tag("_root")(string)?;
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

        Ok((string, Self::new_unchecked(program_name, program_network, function_name, root, None)))
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
        writeln!(
            f,
            "{{ _program_name: {}, _program_network: {}, _function_name: {}, _root: {}, arguments: {:?} }}",
            self.program_name(),
            self.program_network(),
            self.function_name(),
            self.root(),
            self.arguments()
        )
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
        write!(
            f,
            "{{ _program_name: {}, _program_network: {}, _function_name: {}, _root: {} }}",
            self.program_name(),
            self.program_network(),
            self.function_name(),
            self.root(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Argument, Future, Plaintext};
    use snarkvm_console_network::MainnetV0;

    use core::str::FromStr;

    type CurrentNetwork = MainnetV0;

    #[test]
    fn test_parse_display_roundtrip() {
        // Create a static future.
        let future = Future::<CurrentNetwork>::new(
            crate::ProgramID::from_str("test.aleo").unwrap(),
            crate::Identifier::from_str("foo").unwrap(),
            vec![Argument::Plaintext(Plaintext::from_str("100u64").unwrap())],
        );

        // Convert to dynamic future.
        let expected = DynamicFuture::from_future(&future).unwrap();

        // Convert to string.
        let expected_string = expected.to_string();

        // Parse the string.
        let candidate = DynamicFuture::<CurrentNetwork>::from_str(&expected_string).unwrap();

        // Verify the fields match.
        assert_eq!(expected.program_name(), candidate.program_name());
        assert_eq!(expected.program_network(), candidate.program_network());
        assert_eq!(expected.function_name(), candidate.function_name());
        assert_eq!(expected.root(), candidate.root());
    }

    #[test]
    fn test_parse() {
        // Parse a dynamic future from a string.
        let string = "{ _program_name: 0field, _program_network: 0field, _function_name: 0field, _root: 0field }";
        let (remainder, candidate) = DynamicFuture::<CurrentNetwork>::parse(string).unwrap();
        assert!(remainder.is_empty());
        assert_eq!(*candidate.program_name(), Field::from_u64(0));
        assert_eq!(*candidate.program_network(), Field::from_u64(0));
        assert_eq!(*candidate.function_name(), Field::from_u64(0));
        assert_eq!(*candidate.root(), Field::from_u64(0));
    }

    #[test]
    fn test_display() {
        // Create a static future.
        let future = Future::<CurrentNetwork>::new(
            crate::ProgramID::from_str("credits.aleo").unwrap(),
            crate::Identifier::from_str("transfer").unwrap(),
            vec![],
        );

        // Convert to dynamic future.
        let dynamic = DynamicFuture::from_future(&future).unwrap();

        // Check that the display contains expected fields.
        let display = dynamic.to_string();
        assert!(display.contains("_program_name:"));
        assert!(display.contains("_program_network:"));
        assert!(display.contains("_function_name:"));
        assert!(display.contains("_root:"));
    }
}
