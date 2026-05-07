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

use crate::{FinalizeRegisters, FinalizeTypes, Process, Stack};
use console::{
    network::prelude::*,
    program::{Identifier, ProgramID, Value},
};
use snarkvm_ledger_store::{FinalizeStorage, FinalizeStore};
use snarkvm_synthesizer_program::{Command, FinalizeGlobalState, RegistersTrait, StackTrait};

impl<N: Network> Process<N> {
    /// Evaluates a query function in the program identified by `program_id` and returns its outputs.
    ///
    /// This is the public-facing query API: callers pass a program ID and query function name,
    /// not a `&Stack<N>`. It mirrors the shape of `Process::authorize` / `Process::execute`,
    /// minus the proof stage.
    #[inline]
    pub fn evaluate_query<P: FinalizeStorage<N>>(
        &self,
        state: FinalizeGlobalState,
        store: &FinalizeStore<N, P>,
        program_id: impl TryInto<ProgramID<N>>,
        query_name: impl TryInto<Identifier<N>>,
        inputs: Vec<Value<N>>,
    ) -> Result<Vec<Value<N>>> {
        let program_id = program_id.try_into().map_err(|_| anyhow!("Invalid program ID"))?;
        let query_name = query_name.try_into().map_err(|_| anyhow!("Invalid query function name"))?;
        let stack = self.get_stack(program_id)?;
        evaluate_query(state, store, &stack, &query_name, inputs)
    }
}

/// Evaluates a query function on a populated finalize store and returns the declared outputs.
///
/// This is the prototype query path. It is invoked by external callers only — query functions
/// cannot be called from inside other program components in this prototype.
pub fn evaluate_query<N: Network, P: FinalizeStorage<N>>(
    state: FinalizeGlobalState,
    store: &FinalizeStore<N, P>,
    stack: &Stack<N>,
    query_name: &Identifier<N>,
    inputs: Vec<Value<N>>,
) -> Result<Vec<Value<N>>> {
    // Resolve the query function in the stack's program.
    let query = stack.program().get_query_ref(query_name)?;

    // Compute the register types for the query body.
    let types = FinalizeTypes::from_query(stack, query)?;

    // Queries are read-only and externally-callable: no transition is associated. Use a default
    // transition ID and a zero nonce. The block height in `state` is the only state-binding.
    let transition_id = N::TransitionID::default();
    let mut registers = FinalizeRegisters::new(state, transition_id, *query.name(), types, 0);

    // Validate the input arity.
    ensure!(
        query.inputs().len() == inputs.len(),
        "Query '{}' expects {} inputs, got {}",
        query.name(),
        query.inputs().len(),
        inputs.len(),
    );

    // Store the inputs.
    for (input_stmt, value) in query.inputs().iter().zip(inputs.into_iter()) {
        registers.store(stack, input_stmt.register(), value)?;
    }

    // Evaluate the commands. Queries cannot await; branches reuse the finalize-side
    // `branch_to` helper so the two execution paths cannot drift.
    let mut counter = 0;
    while counter < query.commands().len() {
        let command = &query.commands()[counter];
        match command {
            Command::Await(_) => bail!("'await' is forbidden in a query function"),
            Command::BranchEq(branch) => {
                counter = crate::finalize::branch_to(counter, branch, query.positions(), stack, &registers)?;
            }
            Command::BranchNeq(branch) => {
                counter = crate::finalize::branch_to(counter, branch, query.positions(), stack, &registers)?;
            }
            other => {
                other.finalize(stack, store, &mut registers)?;
                counter += 1;
            }
        }
    }

    // Load the outputs.
    let mut outputs = Vec::with_capacity(query.outputs().len());
    for output in query.outputs() {
        outputs.push(registers.load(stack, output.operand())?);
    }
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Process;
    use console::{
        account::PrivateKey,
        network::MainnetV0,
        program::{Literal, Plaintext},
        types::U64,
    };
    use snarkvm_ledger_store::helpers::memory::FinalizeMemory;
    use snarkvm_synthesizer_program::{FinalizeStoreTrait, Program};

    type CurrentNetwork = MainnetV0;

    fn sample_finalize_state(block_height: u32) -> FinalizeGlobalState {
        FinalizeGlobalState::from(block_height as u64, block_height, None, [0u8; 32])
    }

    #[test]
    fn test_evaluate_query_simple() -> Result<()> {
        // A program with a mapping and a query function that sums two mappings for an address.
        let program = Program::<CurrentNetwork>::from_str(
            r"
program token_with_query.aleo;

mapping balances:
    key as address.public;
    value as u64.public;

mapping staked:
    key as address.public;
    value as u64.public;

function noop:
    input r0 as u64.private;
    output r0 as u64.private;

query total_balance:
    input r0 as address.public;
    get.or_use balances[r0] 0u64 into r1;
    get.or_use staked[r0] 0u64 into r2;
    add r1 r2 into r3;
    output r3 as u64.public;",
        )?;

        // Initialize a process and a stack for this program (no deployment needed here).
        let process = Process::<CurrentNetwork>::load()?;
        let stack = Stack::new(&process, &program)?;

        // Initialize the finalize store and seed mapping values.
        let finalize_store = FinalizeStore::<_, FinalizeMemory<_>>::open(aleo_std::StorageMode::new_test(None))?;

        let program_id = *program.id();
        finalize_store.initialize_mapping(program_id, Identifier::from_str("balances")?)?;
        finalize_store.initialize_mapping(program_id, Identifier::from_str("staked")?)?;

        // Pick a deterministic address.
        let mut rng = console::prelude::TestRng::default();
        let private_key = PrivateKey::<CurrentNetwork>::new(&mut rng)?;
        let address = console::account::Address::try_from(&private_key)?;
        let address_key = Plaintext::from(Literal::Address(address));

        finalize_store.update_key_value(
            program_id,
            Identifier::from_str("balances")?,
            address_key.clone(),
            Value::Plaintext(Plaintext::from(Literal::U64(U64::new(40)))),
        )?;
        finalize_store.update_key_value(
            program_id,
            Identifier::from_str("staked")?,
            address_key.clone(),
            Value::Plaintext(Plaintext::from(Literal::U64(U64::new(2)))),
        )?;

        // Evaluate the query.
        let outputs = evaluate_query(
            sample_finalize_state(1),
            &finalize_store,
            &stack,
            &Identifier::from_str("total_balance")?,
            vec![Value::Plaintext(address_key.clone())],
        )?;

        // Expect a single u64 output equal to 42.
        assert_eq!(outputs.len(), 1);
        match &outputs[0] {
            Value::Plaintext(Plaintext::Literal(Literal::U64(v), _)) => assert_eq!(**v, 42),
            other => panic!("unexpected output: {other}"),
        }

        Ok(())
    }

    #[test]
    fn test_evaluate_query_uses_or_default_when_key_missing() -> Result<()> {
        // Same program, but query an address that's never been stored.
        let program = Program::<CurrentNetwork>::from_str(
            r"
program token_with_query.aleo;

mapping balances:
    key as address.public;
    value as u64.public;

function noop:
    input r0 as u64.private;
    output r0 as u64.private;

query fetch_balance:
    input r0 as address.public;
    get.or_use balances[r0] 7u64 into r1;
    output r1 as u64.public;",
        )?;

        let process = Process::<CurrentNetwork>::load()?;
        let stack = Stack::new(&process, &program)?;

        let finalize_store = FinalizeStore::<_, FinalizeMemory<_>>::open(aleo_std::StorageMode::new_test(None))?;
        finalize_store.initialize_mapping(*program.id(), Identifier::from_str("balances")?)?;

        let mut rng = console::prelude::TestRng::default();
        let private_key = PrivateKey::<CurrentNetwork>::new(&mut rng)?;
        let address = console::account::Address::try_from(&private_key)?;
        let address_key = Plaintext::from(Literal::Address(address));

        let outputs = evaluate_query(
            sample_finalize_state(1),
            &finalize_store,
            &stack,
            &Identifier::from_str("fetch_balance")?,
            vec![Value::Plaintext(address_key)],
        )?;

        assert_eq!(outputs.len(), 1);
        match &outputs[0] {
            Value::Plaintext(Plaintext::Literal(Literal::U64(v), _)) => assert_eq!(**v, 7),
            other => panic!("unexpected output: {other}"),
        }
        Ok(())
    }

    #[test]
    fn test_evaluate_query_errors_when_mapping_not_initialized() -> Result<()> {
        // Same shape of program as the other tests, but the finalize store is intentionally not
        // initialized for `balances`. The runtime path must surface the existing
        // "Mapping ... does not exist" error rather than panic or silently succeed.
        let program = Program::<CurrentNetwork>::from_str(
            r"
program token_with_query.aleo;

mapping balances:
    key as address.public;
    value as u64.public;

function noop:
    input r0 as u64.private;
    output r0 as u64.private;

query fetch_balance:
    input r0 as address.public;
    get.or_use balances[r0] 7u64 into r1;
    output r1 as u64.public;",
        )?;

        let process = Process::<CurrentNetwork>::load()?;
        let stack = Stack::new(&process, &program)?;

        // Open an empty finalize store. Note: `initialize_mapping` is deliberately NOT called.
        let finalize_store = FinalizeStore::<_, FinalizeMemory<_>>::open(aleo_std::StorageMode::new_test(None))?;

        let mut rng = console::prelude::TestRng::default();
        let private_key = PrivateKey::<CurrentNetwork>::new(&mut rng)?;
        let address = console::account::Address::try_from(&private_key)?;
        let address_key = Plaintext::from(Literal::Address(address));

        let result = evaluate_query(
            sample_finalize_state(1),
            &finalize_store,
            &stack,
            &Identifier::from_str("fetch_balance")?,
            vec![Value::Plaintext(address_key)],
        );

        let err = result.expect_err("expected error when mapping is not initialized").to_string();
        assert!(err.contains("does not exist"), "unexpected error message: {err}");
        Ok(())
    }
}
