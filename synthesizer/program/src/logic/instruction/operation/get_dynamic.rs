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

use crate::{Opcode, Operand, RegistersCircuit, RegistersTrait, StackTrait};
use circuit::{Inject, Mode, Eject, traits::{ToField, ToFields}};
use console::{
    network::prelude::*,
    program::{Access, Entry, EntryType, Identifier, Plaintext, PlaintextType, Register, RegisterType, Value, ToFields as ConsoleToFields, ToField as ConsoleToField},
};

type CircuitLH<A> = circuit::Poseidon8<A>;
type CircuitPH<A> = circuit::Poseidon2<A>;
type ConsoleLH<N> = console::algorithms::Poseidon8<N>;
type ConsolePH<N> = console::algorithms::Poseidon2<N>;

/// Retrieves the value of an entry in a dynamic record.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct GetDynamicRecordInstruction<N: Network> {
    /// The register and entry containing the dynamic record being read.
    // It is always of the form Register(Register::Access(u64, [Access::Member(Identifier)]))
    operands: [Operand<N>; 1],
    /// The destination register to store the value of the entry.
    // The variant is always Register::Locator
    destination: Register<N>,
    /// The type of the entry being read.t
    entry_type: PlaintextType<N>,
}

impl<N: Network> GetDynamicRecordInstruction<N> {
    /// Initializes a new `get.dynamic.record` instruction.
    #[inline]
    pub fn new(source_operand: Operand<N>, destination: Register<N>, entry_type: PlaintextType<N>) -> Result<Self> {
        if let Operand::Register(Register::Access(_, accesses)) = &source_operand {
            if let [Access::Member(_)] = accesses.as_slice() {
                if let Register::Locator(_) = destination {
                    return Ok(Self { 
                        operands: [source_operand],
                        destination, 
                        entry_type
                    })
                } else {
                    bail!("Expected destination of the form r<i>, where <i> is the index of a register. Found {}", destination)
                }
            }
        }
        bail!("Expected source operand of the form r<i>.<name>, where <i> is the index of a register and <name> is the identifier of an entry. Found {}", source_operand)
    }

    /// Returns the opcode.
    #[inline]
    pub const fn opcode() -> Opcode {
        Opcode::GetDynamicRecord("get.dynamic.record")
    }

    /// Returns the operands in the operation.
    #[inline]
    pub fn operands(&self) -> &[Operand<N>] {
        &self.operands
    }

    /// Returns the destination register.
    #[inline]
    pub fn destinations(&self) -> Vec<Register<N>> {
        vec![self.destination.clone()]
    }
}

impl<N: Network> GetDynamicRecordInstruction<N> {
    /// Evaluates the instruction.
    pub fn evaluate(&self, stack: &impl StackTrait<N>, registers: &mut impl RegistersTrait<N>) -> Result<()> {

        // TODO (Antonio) operand and destination checks
        let (source_record, entry_identifier) = if let [Operand::Register(Register::Access(source_index, accesses))] = &self.operands {
            if let [Access::Member(identifier)] = accesses.as_slice() {
                (Operand::Register(Register::Locator(*source_index)), identifier.clone())
            } else {
                bail!("Expected single access of type Member found {:?}", accesses);
            }
        } else {
            bail!("Expected operand of the form r<i>.<name>, found {:?}", self.operands);
        };

        // Retrieve the dynamic record
        let dynamic_record = {
            let value = registers.load(stack, &source_record)?;
            if let Value::DynamicRecord(dynamic_record) = value {
                dynamic_record
            } else {
                bail!("Expected DynamicRecord, found {value}")
            }
        };

        let entry = if let Some(data) = dynamic_record.data() {
            if let Some(entry) = data.get(&entry_identifier) {
                entry
            } else {
                bail!("Entry {} not found in DynamicRecord", entry_identifier)
            }
        } else {
            bail!("DynamicRecord has no data")
        };

        // TODO (Antonio) publicness and plaintext type is correct
/*         match (&self.entry_type, entry) {
            (EntryType::Constant(constant_type), Entry::Constant(plaintext)) => {},
            (EntryType::Public(public_type), Entry::Public(plaintext)) => {},
            (EntryType::Private(private_type), Entry::Private(plaintext)) => {},
            _ => bail!("Expected entry of variant {:?}, found {:?}", self.entry_type, entry),
        }
 */
        let plaintext = match entry {
            Entry::Constant(plaintext) => plaintext,
            Entry::Public(plaintext) => plaintext,
            Entry::Private(plaintext) => plaintext,
        };

        // Store the output.
        registers.store(stack, &self.destination, Value::Plaintext(plaintext.clone()))
    }

    /// Executes the instruction.
    pub fn execute<A: circuit::Aleo<Network = N>>(
        &self,
        stack: &impl StackTrait<N>,
        registers: &mut impl RegistersCircuit<N, A>,
    ) -> Result<()> {
        let (source_record, entry_identifier) = if let [Operand::Register(Register::Access(source_index, accesses))] = &self.operands {
            if let [Access::Member(identifier)] = accesses.as_slice() {
                (Operand::Register(Register::Locator(*source_index)), identifier.clone())
            } else {
                bail!("Expected single access of type Member found {:?}", accesses);
            }
        } else {
            bail!("Expected operand of the form r<i>.<name>, found {:?}", self.operands);
        };

        // Retrieve the dynamic record
        let circuit_dynamic_record = {
            let value = registers.load_circuit(stack, &source_record)?;
            if let circuit::Value::DynamicRecord(dynamic_record) = value {
                dynamic_record
            } else {
                bail!("Expected DynamicRecord, found {:?}", value.eject_value())
            }
        };

        // TODO (Antonio) syntax
        let tree = if let Some(tree) = circuit_dynamic_record.tree() {
            tree
        } else {
            bail!("DynamicRecord has no tree")
        };

        let data = if let Some(data) = circuit_dynamic_record.data() {
            data
        } else {
            bail!("DynamicRecord has no data")
        };

        let (index, entry) = if let Some((index, _, entry)) = data.get_full(&entry_identifier) {
            (index, entry)
        } else {
            bail!("Entry {} not found in DynamicRecord", entry_identifier)
        };

        // TODO (Antonio) assert mode, variant, type

        // Constructing the leaf of the merkleized-data tree
        let mut console_leaf = vec![entry_identifier.to_field()?];
        console_leaf.extend(entry.to_fields()?);

        // Computing the path (i. e. Merkle proof) with native objects
        let console_path = tree.prove(index, &console_leaf)?;

        // Loading the root of the merkleized-data tree
        let root = circuit_dynamic_record.root();

        // Constructing the in-circuit leaf in Private mode
        let circuit_identifier = circuit::Identifier::constant(entry_identifier.clone());
        let circuit_entry = circuit::Entry::new(Mode::Private, entry.clone());
        let mut circuit_leaf = vec![circuit_identifier.to_field()];
        circuit_leaf.extend(circuit_entry.to_fields());

        // Loading the in-circuit hashers
        let console_leaf_hasher = ConsoleLH::<A::Network>::setup("DynamicRecordLeafHasher").unwrap();
        let console_path_hasher = ConsolePH::<A::Network>::setup("DynamicRecordPathHasher").unwrap();
        let circuit_leaf_hasher = CircuitLH::<A>::constant(console_leaf_hasher.clone());
        let circuit_path_hasher = CircuitPH::<A>::constant(console_path_hasher.clone());

        // Constructing the in-circuit path (i. e. Merkle proof) in Private mode
        let circuit_path = circuit::merkle_tree::MerklePath::new(Mode::Private, console_path);

        // Verifying the path inside the circuit
        circuit_path.verify(&circuit_leaf_hasher, &circuit_path_hasher, &root, &circuit_leaf);

        let circuit_entry_plaintext = match circuit_entry {
            circuit::Entry::Constant(plaintext) => plaintext,
            circuit::Entry::Public(plaintext) => plaintext,
            circuit::Entry::Private(plaintext) => plaintext,
        };

        registers.store_circuit(stack, &self.destination, circuit::Value::Plaintext(circuit_entry_plaintext))?;

        Ok(())
    }

    /// Finalizes the instruction.
    #[inline]
    pub fn finalize(&self, stack: &impl StackTrait<N>, registers: &mut impl RegistersTrait<N>) -> Result<()> {
        // TODO (Antonio) what should this do?
        bail!("Forbidden operation: Finalize cannot invoke 'get.dynamic.record'.")
    }

    /// Returns the output type from the given program and input types.
    pub fn output_types(
        &self,
        _stack: &impl StackTrait<N>,
        input_types: &[RegisterType<N>],
    ) -> Result<Vec<RegisterType<N>>> {
        // TODO (Antonio) checks
        // Ensure the number of input types is correct.
        ensure!(input_types.len() == 1, "Instruction '{}' expects 1 input, found {} inputs", Self::opcode(), input_types.len());

        // Ensure the source operand is a dynamic record.
        ensure!(input_types[0] != RegisterType::DynamicRecord, "Input type {} should be DynamicRecord", input_types[0]);

        // TODO (Antonio) intra-instruction checks
        // Ensure the number of operands is correct.
        // if self.operands.len() != 2 {
        //     bail!("Instruction '{}' expects 2 operands, found {} operands", Self::opcode(), self.operands.len())
        // }

        // TODO (Antonio) should I read the dynamic record here and assert the type coincides with that stored internally?
        Ok(vec![RegisterType::Plaintext(self.entry_type.clone())])
    }
}

impl<N: Network> Parser for GetDynamicRecordInstruction<N> {
    /// Parses a string into an operation.
    fn parse(string: &str) -> ParserResult<Self> {

        // TODO(Antonio) remove
        // parse instruction of the form: get.dynamic.record r<i>.<entry_name> into r<j> as <entry_type>;
        // Parse the whitespace and comments from the string.
        let (string, _) = Sanitizer::parse(string)?;
        // Parse the opcode from the string.
        let (string, _) = tag(*Self::opcode())(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the source operand from the string.
        let (string, source_operand) = Operand::parse(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the 'into' from the string.
        let (string, _) = tag("into")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the destination register from the string.
        let (string, destination) = Register::parse(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the 'as' from the string.
        let (string, _) = tag("as")(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;
        // Parse the entry type from the string.
        let (string, entry_type) = PlaintextType::parse(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;

        // TODO (Antonio) remove
        println!("GETS HERE, source_operand: {source_operand}");

        match Self::new(source_operand, destination, entry_type) {
            Ok(instruction) => Ok((string, instruction)),
            Err(e) => map_res(fail, |_: ParserResult<Self>| {
                Err(error(format!("Failed to parse '{}' instruction: {e}", Self::opcode())))
            })(string),
            Err(e) => map_res(fail, |_: ParserResult<Self>| {
                Err(error(format!("Failed to parse '{}' instruction: {e}", Self::opcode())))
            })(string),
        }
    }
}

impl<N: Network> FromStr for GetDynamicRecordInstruction<N> {
    type Err = Error;

    /// Parses a string into an operation.
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

impl<N: Network> Debug for GetDynamicRecordInstruction<N> {
    /// Prints the operation as a string.
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl<N: Network> Display for GetDynamicRecordInstruction<N> {
    /// Prints the operation to a string.
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        // Print the operation.
        write!(f, "{} {} into {} as {}", Self::opcode(), self.operands[0], self.destination, self.entry_type)
    }
}

impl<N: Network> FromBytes for GetDynamicRecordInstruction<N> {
    /// Reads the operation from a buffer.
    fn read_le<R: Read>(mut reader: R) -> IoResult<Self> {
        let source_operand = Operand::read_le(&mut reader)?;
        let destination = Register::read_le(&mut reader)?;
        let entry_type = PlaintextType::read_le(&mut reader)?;

        // Return the operation.
        Self::new(source_operand, destination, entry_type).map_err(error)
    }
}

impl<N: Network> ToBytes for GetDynamicRecordInstruction<N> {
    /// Writes the operation to a buffer.
    fn write_le<W: Write>(&self, mut writer: W) -> IoResult<()> {
        if self.operands.len() != 1 {
            return Err(error(format!("Expected one operand, found {}", self.operands.len())));
        }

        // Write the source operand.
        self.operands[0].write_le(&mut writer)?;
        // Write the destination register.
        self.destination.write_le(&mut writer)?;
        // Write the entry type.
        self.entry_type.write_le(&mut writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::network::MainnetV0;

    type CurrentNetwork = MainnetV0;

    fn assert_err_contains(instruction_str: &str, error_msg: &str) {
        println!("ERROR: {}", convert_result(GetDynamicRecordInstruction::<CurrentNetwork>::parse(instruction_str), instruction_str));
    }

    #[test]
    fn test_parse() {

        // Correct cases
        assert!(GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.entry into r1 as bool").is_ok());

        // TODO remove
        println!("CORRECT: {}", GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.entry into r1 as bool").unwrap().1);
        
        // Incorrect source: no identifier
        
        // Incorrect source: type index
        assert_err_contains("get.dynamic.record r0 into r1 as bool", "Expected source operand of the form");

        // Incorrect: several source operands

        // Incorrect: several target operands

        // Incorrect: no "into"

        // Incorrect: no "as"

        // Incorrect: no entry type
        
    }
}
