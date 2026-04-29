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

impl<N: Network> Parser for QueryCore<N> {
    /// Parses a string into a query function.
    #[inline]
    fn parse(string: &str) -> ParserResult<Self> {
        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the 'query' keyword from the string.
        let (string, _) = tag(Self::type_name())(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the query name from the string.
        let (string, name) = Identifier::<N>::parse(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the colon ':' keyword from the string.
        let (string, _) = tag(":")(string)?;

        // Parse the inputs from the string.
        let (string, inputs) = many0(Input::parse)(string)?;
        // Parse the commands from the string.
        let (string, commands) = many1(Command::<N>::parse)(string)?;
        // Parse the outputs from the string.
        let (string, outputs) = many1(Output::parse)(string)?;

        map_res(take(0usize), move |_| {
            let mut query = Self::new(name);
            inputs.iter().cloned().try_for_each(|input| query.add_input(input))?;
            commands.iter().cloned().try_for_each(|command| query.add_command(command))?;
            outputs.iter().cloned().try_for_each(|output| query.add_output(output))?;
            Ok::<_, Error>(query)
        })(string)
    }
}

impl<N: Network> FromStr for QueryCore<N> {
    type Err = Error;

    fn from_str(string: &str) -> Result<Self> {
        match Self::parse(string) {
            Ok((remainder, object)) => {
                ensure!(remainder.is_empty(), "Failed to parse string. Found invalid character in: \"{remainder}\"");
                Ok(object)
            }
            Err(error) => bail!("Failed to parse string. {error}"),
        }
    }
}

impl<N: Network> Debug for QueryCore<N> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl<N: Network> Display for QueryCore<N> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{} {}:", Self::type_name(), self.name)?;
        self.inputs.iter().try_for_each(|input| write!(f, "\n    {input}"))?;
        self.commands.iter().try_for_each(|command| write!(f, "\n    {command}"))?;
        self.outputs.iter().try_for_each(|output| write!(f, "\n    {output}"))
    }
}
