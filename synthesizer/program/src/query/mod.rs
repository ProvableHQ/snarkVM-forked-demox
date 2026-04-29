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

use crate::{Command, Instruction};

mod input;
pub use input::*;

mod output;
pub use output::*;

mod bytes;
mod parse;

use console::{
    network::{error, prelude::*},
    program::{FinalizeType, Identifier, Register},
};

use indexmap::IndexSet;
use std::collections::HashMap;

/// A query function: a top-level, externally-callable, read-only block that returns typed values.
///
/// Queries share the finalize command set (so they can `get`, `get.or_use`, `contains` against
/// mappings), but cannot mutate state, schedule futures, or call other functions.
#[derive(Clone, PartialEq, Eq)]
pub struct QueryCore<N: Network> {
    /// The name of the query function.
    name: Identifier<N>,
    /// The input statements, added in order of the input registers.
    inputs: IndexSet<Input<N>>,
    /// The commands, in order of execution.
    commands: Vec<Command<N>>,
    /// The output statements, in order of the desired output.
    outputs: IndexSet<Output<N>>,
    /// A mapping from `Position`s to their index in `commands`.
    positions: HashMap<Identifier<N>, usize>,
}

impl<N: Network> QueryCore<N> {
    /// Initializes a new read with the given name.
    pub fn new(name: Identifier<N>) -> Self {
        Self {
            name,
            inputs: IndexSet::new(),
            commands: Vec::new(),
            outputs: IndexSet::new(),
            positions: HashMap::new(),
        }
    }

    /// Returns the name of the query function.
    pub const fn name(&self) -> &Identifier<N> {
        &self.name
    }

    /// Returns the query inputs.
    pub const fn inputs(&self) -> &IndexSet<Input<N>> {
        &self.inputs
    }

    /// Returns the query input types.
    pub fn input_types(&self) -> Vec<FinalizeType<N>> {
        self.inputs.iter().map(|input| input.finalize_type()).cloned().collect()
    }

    /// Returns the query commands.
    pub fn commands(&self) -> &[Command<N>] {
        &self.commands
    }

    /// Returns the query outputs.
    pub const fn outputs(&self) -> &IndexSet<Output<N>> {
        &self.outputs
    }

    /// Returns the query output types.
    pub fn output_types(&self) -> Vec<FinalizeType<N>> {
        self.outputs.iter().map(|output| output.finalize_type()).cloned().collect()
    }

    /// Returns the mapping of `Position`s to their index in `commands`.
    pub const fn positions(&self) -> &HashMap<Identifier<N>, usize> {
        &self.positions
    }
}

impl<N: Network> QueryCore<N> {
    /// Adds the input statement to the query.
    #[inline]
    fn add_input(&mut self, input: Input<N>) -> Result<()> {
        // Ensure there are no commands or outputs in memory.
        ensure!(self.commands.is_empty(), "Cannot add inputs after commands have been added");
        ensure!(self.outputs.is_empty(), "Cannot add inputs after outputs have been added");

        // Ensure the maximum number of inputs has not been exceeded.
        ensure!(self.inputs.len() < N::MAX_INPUTS, "Cannot add more than {} inputs", N::MAX_INPUTS);
        // Ensure the input statement was not previously added.
        ensure!(!self.inputs.contains(&input), "Cannot add duplicate input statement");

        // Queries are externally-callable; futures and dynamic futures are not meaningful here.
        ensure!(
            matches!(input.finalize_type(), FinalizeType::Plaintext(..)),
            "Query inputs must be plaintext (futures are forbidden)"
        );

        // Ensure the input register is a locator.
        ensure!(matches!(input.register(), Register::Locator(..)), "Input register must be a locator");

        self.inputs.insert(input);
        Ok(())
    }

    /// Adds the given command to the query.
    #[inline]
    pub fn add_command(&mut self, command: Command<N>) -> Result<()> {
        // Ensure there are no outputs already.
        ensure!(self.outputs.is_empty(), "Cannot add commands after outputs have been added");

        // Ensure the maximum number of commands has not been exceeded.
        ensure!(self.commands.len() < N::MAX_COMMANDS, "Cannot add more than {} commands", N::MAX_COMMANDS);

        // Reject any state-mutating or non-deterministic command.
        ensure!(!command.is_write(), "Forbidden operation: query functions cannot use 'set' or 'remove'");
        ensure!(!command.is_async(), "Forbidden operation: query functions cannot invoke an 'async' instruction");
        ensure!(!command.is_await(), "Forbidden operation: query functions cannot 'await' a future");
        ensure!(!command.is_call(), "Forbidden operation: query functions cannot 'call' another function");
        ensure!(!command.is_cast_to_record(), "Forbidden operation: query functions cannot cast to a record");

        // Reject `rand.chacha`. It is only meaningful with finalize global state.
        if matches!(&command, Command::RandChaCha(_)) {
            bail!("Forbidden operation: query functions cannot use 'rand.chacha'");
        }

        // Reject `async` instructions explicitly (already covered by is_async, but be paranoid).
        if let Command::Instruction(Instruction::Async(_)) = &command {
            bail!("Forbidden operation: query functions cannot invoke 'async'");
        }

        // Check the destination registers.
        for register in command.destinations() {
            ensure!(matches!(register, Register::Locator(..)), "Destination register must be a locator");
        }

        // Branch target validation.
        if let Some(position) = command.branch_to() {
            ensure!(!self.positions.contains_key(position), "Cannot branch to an earlier position '{position}'");
        }

        if let Some(position) = command.position() {
            ensure!(!self.positions.contains_key(position), "Cannot redefine position '{position}'");
            ensure!(self.positions.len() < N::MAX_POSITIONS, "Cannot add more than {} positions", N::MAX_POSITIONS);
            self.positions.insert(*position, self.commands.len());
        }

        self.commands.push(command);
        Ok(())
    }

    /// Adds the output statement to the query.
    #[inline]
    fn add_output(&mut self, output: Output<N>) -> Result<()> {
        // Ensure the maximum number of outputs has not been exceeded.
        ensure!(self.outputs.len() < N::MAX_OUTPUTS, "Cannot add more than {} outputs", N::MAX_OUTPUTS);

        // Queries return plaintext only.
        ensure!(
            matches!(output.finalize_type(), FinalizeType::Plaintext(..)),
            "Query outputs must be plaintext (futures are forbidden)"
        );

        self.outputs.insert(output);
        Ok(())
    }
}

impl<N: Network> TypeName for QueryCore<N> {
    #[inline]
    fn type_name() -> &'static str {
        "query"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type CurrentNetwork = console::network::MainnetV0;

    #[test]
    fn test_add_input() {
        let name = Identifier::from_str("query_core_test").unwrap();
        let mut query = QueryCore::<CurrentNetwork>::new(name);

        let input = Input::<CurrentNetwork>::from_str("input r0 as field.public;").unwrap();
        assert!(query.add_input(input.clone()).is_ok());
        assert!(query.add_input(input).is_err());
    }

    #[test]
    fn test_reject_set_command() {
        let name = Identifier::from_str("query_core_test").unwrap();
        let mut query = QueryCore::<CurrentNetwork>::new(name);

        let cmd = Command::<CurrentNetwork>::from_str("set 1u64 into balances[0u64];").unwrap();
        let err = query.add_command(cmd).unwrap_err();
        assert!(err.to_string().contains("'set' or 'remove'"));
    }

    #[test]
    fn test_reject_remove_command() {
        let name = Identifier::from_str("query_core_test").unwrap();
        let mut query = QueryCore::<CurrentNetwork>::new(name);

        let cmd = Command::<CurrentNetwork>::from_str("remove balances[0u64];").unwrap();
        let err = query.add_command(cmd).unwrap_err();
        assert!(err.to_string().contains("'set' or 'remove'"));
    }

    #[test]
    fn test_reject_rand_chacha() {
        let name = Identifier::from_str("query_core_test").unwrap();
        let mut query = QueryCore::<CurrentNetwork>::new(name);

        let cmd = Command::<CurrentNetwork>::from_str("rand.chacha into r0 as u64;").unwrap();
        let err = query.add_command(cmd).unwrap_err();
        assert!(err.to_string().contains("rand.chacha"));
    }
}
