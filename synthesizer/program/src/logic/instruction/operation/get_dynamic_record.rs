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
    /// The type of the entry being read.
    plaintext_type: PlaintextType<N>,
}

impl<N: Network> GetDynamicRecordInstruction<N> {
    /// Initializes a new `get.dynamic.record` instruction.
    #[inline]
    pub fn new(operand: Operand<N>, destination: Register<N>, plaintext_type: PlaintextType<N>) -> Result<Self> {
        
        Self::check_and_get_input_output(&[operand.clone()], &destination)?;
        
        Ok(Self { 
            operands: [operand],
            destination, 
            plaintext_type
        })
    }

    // Internal function which checks the source operand and destination
    // register have the expected shape and splits the former into a
    // Record::Locator and an Identifier.
    fn check_and_get_input_output(
        source_operands: &[Operand<N>],
        destination: &Register<N>,
    ) -> Result<(Operand<N>, Identifier<N>)> {

        ensure!(
            matches!(destination, Register::Locator(_)),
            "Expected destination of the form r<i>, found {}",
            destination
        );
        
        if [Operand::Register(Register::Access(source_index, accesses))] = source_operands {
            if let [Access::Member(identifier)] = accesses.as_slice() {
                Ok((Operand::Register(Register::Locator(*source_index)), identifier.clone()))
            } else {
                Err(anyhow!("Expected single Access of type Member in source operand, found {:?}", accesses))
            }
        } else {
            Err(anyhow!("Expected source operand of the form r<i>.<name>, found {:?}", source_operands))
        }
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

        let (source_record, entry_identifier) = Self::check_and_get_input_output(&self.operands, &self.destination)?;

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
            bail!("DynamicRecord data has not been populated")
        };

        let plaintext = match entry {
            Entry::Constant(plaintext) => plaintext,
            Entry::Public(plaintext) => plaintext,
            Entry::Private(plaintext) => plaintext,
        };

        ensure!(
            stack.matches_plaintext(&plaintext, &self.plaintext_type).is_ok(),
            "Type mismatch in DynamicRecord entry {:?}: expected {:?}, found {:?}",
            entry_identifier,
            self.plaintext_type,
            entry
        );

        // Store the output.
        registers.store(stack, &self.destination, Value::Plaintext(plaintext.clone()))
    }

    /// Executes the instruction.
    pub fn execute<A: circuit::Aleo<Network = N>>(
        &self,
        stack: &impl StackTrait<N>,
        registers: &mut impl RegistersCircuit<N, A>,
    ) -> Result<()> {
        let (source_record, entry_identifier) = Self::check_and_get_input_output(&self.operands, &self.destination)?;

        // Retrieve the dynamic record
        let circuit_dynamic_record = {
            let value = registers.load_circuit(stack, &source_record)?;
            if let circuit::Value::DynamicRecord(dynamic_record) = value {
                dynamic_record
            } else {
                bail!("Expected DynamicRecord, found {:?}", value.eject_value())
            }
        };

        let tree = circuit_dynamic_record.tree().as_ref().ok_or_else(|| anyhow!("DynamicRecord tree has not been populated"))?;
        let data = circuit_dynamic_record.data().ok_or_else(|| anyhow!("DynamicRecord data has not been populated"))?;

        let (index, _, entry) = data.get_full(&entry_identifier).ok_or_else(|| anyhow!("Entry {} not found in DynamicRecord", entry_identifier))?;

        
        // This verification is only a sanity check and not performed
        // in-circuit. The fact that the in-circuit entry has the correct type
        // is encoded into the circuit structure (and therefore the proving and
        // verifying keys).
        {
            let plaintext = match entry {
                Entry::Constant(plaintext) => plaintext,
                Entry::Public(plaintext) => plaintext,
                Entry::Private(plaintext) => plaintext,
            };
            ensure!(
                stack.matches_plaintext(&plaintext, &self.plaintext_type).is_ok(),
                "Type mismatch in DynamicRecord entry {:?}: expected {:?}, found {:?}",
                entry_identifier,
                self.plaintext_type,
                entry
            );
        }

        // Constructing the leaf of the merkleized-data tree
        let mut console_leaf = vec![entry_identifier.to_field()?];
        console_leaf.extend(entry.to_fields()?);

        // Computing the path (i. e. Merkle proof) with native objects
        let console_path = tree.prove(index, &console_leaf)?;

        // Loading the root of the merkleized-data tree
        let root = circuit_dynamic_record.root();

        // Constructing the in-circuit leaf in Private mode. An entry is
        // described by:
        // - its identifier, which is injected into the circuit as a constant
        //   Field element
        // - its visibility (Constant, Public or Private) which is injected as
        //   two constant Boolean
        // - its plaintext, whose variant and relevant identifiers (e. g. those
        //   inside structures) are injected as constants
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
        bail!("Forbidden operation: Finalize cannot invoke 'get.dynamic.record'.")
    }

    /// Returns the output type from the given program and input types.
    pub fn output_types(
        &self,
        _stack: &impl StackTrait<N>,
        _input_types: &[RegisterType<N>],
    ) -> Result<Vec<RegisterType<N>>> {
        // Ensure the instruction is correctly defined.
        Self::check_and_get_input_output(&self.operands, &self.destination)?;
        
        // TODO (Antonio) should this return <TYPE> or r<j> if the instruction
        // is of the form
        //    get.dynamic.record r<k>.<name> into r<j> as <TYPE>
        // ? Implemented the former for now.
        Ok(vec![RegisterType::Plaintext(self.plaintext_type.clone())])
    }
}

impl<N: Network> Parser for GetDynamicRecordInstruction<N> {
    /// Parses a string into an operation.
    fn parse(string: &str) -> ParserResult<Self> {

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
        let (string, plaintext_type) = PlaintextType::parse(string)?;
        // Parse the whitespace from the string.
        let (string, _) = Sanitizer::parse_whitespaces(string)?;

        match Self::new(source_operand, destination, plaintext_type) {
            Ok(instruction) => Ok((string, instruction)),
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
        write!(f, "{} {} into {} as {}", Self::opcode(), self.operands[0], self.destination, self.plaintext_type)
    }
}

impl<N: Network> FromBytes for GetDynamicRecordInstruction<N> {
    /// Reads the operation from a buffer.
    fn read_le<R: Read>(mut reader: R) -> IoResult<Self> {
        let source_operand = Operand::read_le(&mut reader)?;
        let destination = Register::read_le(&mut reader)?;
        let plaintext_type = PlaintextType::read_le(&mut reader)?;

        // Return the operation.
        Self::new(source_operand, destination, plaintext_type).map_err(error)
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
        self.plaintext_type.write_le(&mut writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use console::{network::MainnetV0, program::ArrayType};
    type CurrentNetwork = MainnetV0;

    #[test]
    fn test_parse() {

        // ************ Literal types ************

        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.outdated into r1 as bool").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands().len() == 1, "The number of operands is incorrect");
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(0, vec![Access::Member(Identifier::from_str("outdated").unwrap())])), "The source operand is incorrect");
        assert!(instruction.destination == Register::Locator(1), "The destination register is incorrect");
        assert!(instruction.plaintext_type == PlaintextType::from_str("bool").unwrap(), "The plaintext type is incorrect");
        
        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.middleman into r1 as address").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(0, vec![Access::Member(Identifier::from_str("middleman").unwrap())])));
        assert!(instruction.destination == Register::Locator(1));
        assert!(instruction.plaintext_type == PlaintextType::from_str("address").unwrap());
        
        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.sk into r1 as field").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(0, vec![Access::Member(Identifier::from_str("sk").unwrap())])));
        assert!(instruction.destination == Register::Locator(1));
        assert!(instruction.plaintext_type == PlaintextType::from_str("field").unwrap());
        
        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.pk into r1 as group").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(0, vec![Access::Member(Identifier::from_str("pk").unwrap())])));
        assert!(instruction.destination == Register::Locator(1));
        assert!(instruction.plaintext_type == PlaintextType::from_str("group").unwrap());
        
        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.crs_byte into r1 as u8").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(0, vec![Access::Member(Identifier::from_str("crs_byte").unwrap())])));
        assert!(instruction.destination == Register::Locator(1));
        assert!(instruction.plaintext_type == PlaintextType::from_str("u8").unwrap());
        
        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.size into r1 as u16").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(0, vec![Access::Member(Identifier::from_str("size").unwrap())])));
        assert!(instruction.destination == Register::Locator(1));
        assert!(instruction.plaintext_type == PlaintextType::from_str("u16").unwrap());
        
        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.register into r1 as u32").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(0, vec![Access::Member(Identifier::from_str("register").unwrap())])));
        assert!(instruction.destination == Register::Locator(1));
        assert!(instruction.plaintext_type == PlaintextType::from_str("u32").unwrap());

        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.crs_byte into r1 as u8").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(0, vec![Access::Member(Identifier::from_str("crs_byte").unwrap())])));
        assert!(instruction.destination == Register::Locator(1));
        assert!(instruction.plaintext_type == PlaintextType::from_str("u8").unwrap());

        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.size into r1 as u16").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(0, vec![Access::Member(Identifier::from_str("size").unwrap())])));
        assert!(instruction.destination == Register::Locator(1));
        assert!(instruction.plaintext_type == PlaintextType::from_str("u16").unwrap());
        
        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.register into r1 as u32").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(0, vec![Access::Member(Identifier::from_str("register").unwrap())])));
        assert!(instruction.destination == Register::Locator(1));
        assert!(instruction.plaintext_type == PlaintextType::from_str("u32").unwrap());

        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.usize into r1 as u64").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(0, vec![Access::Member(Identifier::from_str("usize").unwrap())])));
        assert!(instruction.destination == Register::Locator(1));
        assert!(instruction.plaintext_type == PlaintextType::from_str("u64").unwrap());
        
        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r0.long into r1 as u128").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(0, vec![Access::Member(Identifier::from_str("long").unwrap())])));
        assert!(instruction.destination == Register::Locator(1));
        assert!(instruction.plaintext_type == PlaintextType::from_str("u128").unwrap());

        // ************ Other correct cases ************
        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r3.banana into r3 as fruit_struct").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(3, vec![Access::Member(Identifier::from_str("banana").unwrap())])));
        assert!(instruction.destination == Register::Locator(3));
        assert!(instruction.plaintext_type == PlaintextType::Struct(Identifier::from_str("fruit_struct").unwrap()));

        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r1.apples into r1 as [fruit_struct; 20u32]").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(1, vec![Access::Member(Identifier::from_str("apples").unwrap())])));
        assert!(instruction.destination == Register::Locator(1));
        assert!(instruction.plaintext_type == PlaintextType::Array(ArrayType::from_str("[fruit_struct; 20u32]").unwrap()));

        let (remainder, instruction) = GetDynamicRecordInstruction::<CurrentNetwork>::parse("get.dynamic.record r1.dragonfruit_matrix into r0 as [[fruit_struct; 20u32]; 10u32]").unwrap();
        assert!(remainder.is_empty());
        assert!(instruction.operands()[0] == Operand::Register(Register::Access(1, vec![Access::Member(Identifier::from_str("dragonfruit_matrix").unwrap())])));
        assert!(instruction.destination == Register::Locator(0));
        assert!(instruction.plaintext_type == PlaintextType::Array(ArrayType::from_str("[[fruit_struct; 20u32]; 10u32]").unwrap()));

        // ************ Incorrect cases ************
        let incorrect_cases = [
            // Incorrect source: no identifier
            "get.dynamic.record r1 into r0 as field",
            // Incorrect: several source operands
            "get.dynamic.record r1.apples r2.banana into r0 as field",
            // Incorrect: several target operands
            "get.dynamic.record r1.apples into r0 r1 as field",
            // Incorrect: no "into"
            "get.dynamic.record r1.apples as field",
            // Incorrect: no "as"
            "get.dynamic.record r1.apples into r0 field",
            // Incorrect: no entry type
            "get.dynamic.record r1.apples into r0 as",
        ];
        
        for case in incorrect_cases {
            assert!(GetDynamicRecordInstruction::<CurrentNetwork>::parse(case).is_err());
        }
    }
}
