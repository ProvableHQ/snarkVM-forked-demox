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

use crate::{FinalizeStoreTrait, Opcode, Operand, RegistersTrait, StackTrait};
use console::{
    network::prelude::*,
    program::{Identifier, Literal, Plaintext, ProgramID, Register, Value},
};

/// A dynamic get command that uses the provided default in case of failure, e.g. `get.or_use.dynamic r0.r1/r2[r3] r4 into r5;`.
/// Resolves the `program` and `mapping` operands, gets the value stored at the `key` operand in `mapping`, and stores the result into `destination`.
/// If the key is not present, `default` is stored in `destination`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct GetOrUseDynamic<N: Network> {
    /// The operands.
    operands: [Operand<N>; 5],
    /// The destination register.
    destination: Register<N>,
}

impl<N: Network> GetOrUseDynamic<N> {
    /// Returns the opcode.
    #[inline]
    pub const fn opcode() -> Opcode {
        Opcode::Command("get.or_use.dynamic")
    }

    /// Returns the operands in the operation.
    #[inline]
    pub fn operands(&self) -> &[Operand<N>] {
        &self.operands
    }

    /// Returns the operand containing the program name.
    #[inline]
    pub const fn program_name(&self) -> &Operand<N> {
        &self.operands[0]
    }

    /// Returns the operand containing the program network.
    #[inline]
    pub const fn program_network(&self) -> &Operand<N> {
        &self.operands[1]
    }

    /// Returns the operand containing the mapping name.
    #[inline]
    pub const fn mapping_name(&self) -> &Operand<N> {
        &self.operands[2]
    }

    /// Returns the operand containing the key.
    #[inline]
    pub const fn key(&self) -> &Operand<N> {
        &self.operands[3]
    }

    /// Returns the operand containing the default value.
    #[inline]
    pub const fn default(&self) -> &Operand<N> {
        &self.operands[4]
    }

    /// Returns the destination register.
    #[inline]
    pub const fn destination(&self) -> &Register<N> {
        &self.destination
    }
}

impl<N: Network> GetOrUseDynamic<N> {
    /// Finalizes the command.
    #[inline]
    pub fn finalize(
        &self,
        stack: &impl StackTrait<N>,
        store: &impl FinalizeStoreTrait<N>,
        registers: &mut impl RegistersTrait<N>,
    ) -> Result<()> {
        // Get the program name.
        let program_name = match registers.load(stack, self.program_name())? {
            Value::Plaintext(Plaintext::Literal(Literal::Field(field), _)) => Identifier::from_field(&field)?,
            _ => bail!("Expected the first operand of `get.or_use.dynamic` to be a field literal."),
        };

        // Get the program network.
        let program_network = match registers.load(stack, self.program_network())? {
            Value::Plaintext(Plaintext::Literal(Literal::Field(field), _)) => Identifier::from_field(&field)?,
            _ => bail!("Expected the second operand of `get.or_use.dynamic` to be a field literal."),
        };

        // Construct the program ID.
        let program_id = ProgramID::try_from((program_name, program_network))?;

        // Get the mapping name.
        let mapping_name = match registers.load(stack, self.mapping_name())? {
            Value::Plaintext(Plaintext::Literal(Literal::Field(field), _)) => Identifier::from_field(&field)?,
            _ => bail!("Expected the third operand of `call.dynamic` to be a field literal."),
        };

        // Ensure the mapping exists.
        if !store.contains_mapping_speculative(&program_id, &mapping_name)? {
            bail!("Mapping '{program_id}/{mapping_name}' does not exist");
        }

        // Load the operand as a plaintext.
        let key = registers.load_plaintext(stack, self.key())?;

        // Retrieve the value from storage as a literal.
        let value = match store.get_value_speculative(program_id, mapping_name, &key)? {
            Some(Value::Plaintext(plaintext)) => Value::Plaintext(plaintext),
            Some(Value::Record(..)) => bail!("Cannot 'get.or_use.dynamic' a 'record'"),
            Some(Value::Future(..)) => bail!("Cannot 'get.or_use.dynamic' a 'future'"),
            Some(Value::DynamicRecord(..)) => bail!("Cannot 'get.or_use.dynamic' a 'dynamic.record'"),
            Some(Value::DynamicFuture(..)) => bail!("Cannot 'get.or_use.dynamic' a 'dynamic.future'"),
            // If a key does not exist, then use the default value.
            None => Value::Plaintext(registers.load_plaintext(stack, self.default())?),
        };

        // Assign the value to the destination register.
        registers.store(stack, &self.destination, value)?;

        Ok(())
    }
}

impl<N: Network> Parser for GetOrUseDynamic<N> {
    /// Parses a string into an operation.
    #[inline]
    fn parse(string: &str) -> ParserResult<Self> {
        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the opcode from the string.
        let (string, _) = tag(*Self::opcode())(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;

        // TODO (@d0cd) Verify that the grammar does not have ambiguities.

        // Parse the program name operand from the string.
        let (string, program_name) = Operand::parse(string)?;
        // Parse the "." from the string.
        let (string, _) = tag(".")(string)?;
        // Parse the program network operand from the string.
        let (string, program_network) = Operand::parse(string)?;
        // Parse the "/" from the string.
        let (string, _) = tag("/")(string)?;
        // Parse the mapping name operand from the string.
        let (string, mapping_name) = Operand::parse(string)?;

        // Parse the "[" from the string.
        let (string, _) = tag("[")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the key operand from the string.
        let (string, key) = Operand::parse(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the "]" from the string.
        let (string, _) = tag("]")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the default value from the string.
        let (string, default) = Operand::parse(string)?;

        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the "into" keyword from the string.
        let (string, _) = tag("into")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the destination register from the string.
        let (string, destination) = Register::parse(string)?;

        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the ";" from the string.
        let (string, _) = tag(";")(string)?;

        Ok((string, Self { operands: [program_name, program_network, mapping_name, key, default], destination }))
    }
}

impl<N: Network> FromStr for GetOrUseDynamic<N> {
    type Err = Error;

    /// Parses a string into the command.
    #[inline]
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

impl<N: Network> Debug for GetOrUseDynamic<N> {
    /// Prints the command as a string.
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl<N: Network> Display for GetOrUseDynamic<N> {
    /// Prints the command to a string.
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        // Print the command.
        write!(f, "{} ", Self::opcode())?;
        // Print the program name, program network, mapping and key operand.
        write!(
            f,
            "{}.{}/{}[{}] {} into ",
            self.program_name(),
            self.program_network(),
            self.mapping_name(),
            self.key(),
            self.default()
        )?;
        // Print the destination register.
        write!(f, "{};", self.destination)
    }
}

impl<N: Network> FromBytes for GetOrUseDynamic<N> {
    /// Reads the command from a buffer.
    fn read_le<R: Read>(mut reader: R) -> IoResult<Self> {
        // Read the program name.
        let program_name = Operand::read_le(&mut reader)?;
        // Read the program network.
        let program_network = Operand::read_le(&mut reader)?;
        // Read the mapping name.
        let mapping_name = Operand::read_le(&mut reader)?;
        // Read the key operand.
        let key = Operand::read_le(&mut reader)?;
        // Read the default operand.
        let default = Operand::read_le(&mut reader)?;
        // Read the destination register.
        let destination = Register::read_le(&mut reader)?;
        // Return the command.
        Ok(Self { operands: [program_name, program_network, mapping_name, key, default], destination })
    }
}

impl<N: Network> ToBytes for GetOrUseDynamic<N> {
    /// Writes the command to a buffer.
    fn write_le<W: Write>(&self, mut writer: W) -> IoResult<()> {
        // Write the program name.
        self.program_name().write_le(&mut writer)?;
        // Write the program network.
        self.program_network().write_le(&mut writer)?;
        // Write the mapping name.
        self.mapping_name().write_le(&mut writer)?;
        // Write the key operand.
        self.key().write_le(&mut writer)?;
        // Write the default operand.
        self.default().write_le(&mut writer)?;
        // Write the destination register.
        self.destination.write_le(&mut writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::{network::MainnetV0, program::Register};

    type CurrentNetwork = MainnetV0;

    #[test]
    fn test_parse() {
        let (string, get) =
            GetOrUseDynamic::<CurrentNetwork>::parse("get.or_use.dynamic r0.r1/r2[r3] r4 into r5;").unwrap();
        assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");
        assert_eq!(get.operands().len(), 5, "The number of operands is incorrect");
        assert_eq!(get.program_name(), &Operand::Register(Register::Locator(0)), "The first operand is incorrect");
        assert_eq!(get.program_network(), &Operand::Register(Register::Locator(1)), "The second operand is incorrect");
        assert_eq!(get.mapping_name(), &Operand::Register(Register::Locator(2)), "The third operand is incorrect");
        assert_eq!(get.key(), &Operand::Register(Register::Locator(3)), "The fourth operand is incorrect");
        assert_eq!(get.default(), &Operand::Register(Register::Locator(4)), "The fifth operand is incorrect");
        assert_eq!(get.destination, Register::Locator(5), "The destination register is incorrect");
    }

    #[test]
    fn test_from_bytes() {
        let (string, get) =
            GetOrUseDynamic::<CurrentNetwork>::parse("get.or_use.dynamic r0.r1/r2[r3] r4 into r5;").unwrap();
        assert!(string.is_empty());
        let bytes_le = get.to_bytes_le().unwrap();
        let result = GetOrUseDynamic::<CurrentNetwork>::from_bytes_le(&bytes_le[..]);
        assert!(result.is_ok())
    }
}
