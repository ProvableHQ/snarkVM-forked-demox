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

use circuit::AleoV0;
use console::{
    network::MainnetV0,
    prelude::*,
    program::{Identifier, Plaintext, ProgramID, Register, Value},
};
use snarkvm_synthesizer_process::{Authorization, CallStack, FinalizeRegisters, Registers, Stack};
use snarkvm_synthesizer_program::{
    FinalizeGlobalState,
    FinalizeOperation,
    FinalizeStoreTrait,
    RegistersCircuit as _,
    RegistersTrait as _,
};

type CurrentNetwork = MainnetV0;
type CurrentAleo = AleoV0;

/// Samples the registers. Note: Do not replicate this for real program use, it is insecure.
pub fn sample_registers(
    stack: &Stack<CurrentNetwork>,
    function_name: &Identifier<CurrentNetwork>,
    values: &[(Value<CurrentNetwork>, Option<circuit::Mode>)],
) -> Result<Registers<CurrentNetwork, CurrentAleo>> {
    // Initialize the registers.
    let mut registers = Registers::<CurrentNetwork, CurrentAleo>::new(
        CallStack::evaluate(Authorization::try_from((vec![], vec![]))?)?,
        stack.get_register_types(function_name)?.clone(),
    );

    // For each value, store the register and value.
    for (index, (value, mode)) in values.iter().enumerate() {
        // Initialize the register.
        let register = Register::Locator(index as u64);
        // Store the value in the console registers.
        registers.store(stack, &register, value.clone())?;
        // If the mode is not `None`,
        if let Some(mode) = mode {
            use circuit::Inject;

            // Initialize the circuit value.
            let circuit_value = circuit::Value::new(*mode, value.clone());
            // Store the value in the circuit registers.
            registers.store_circuit(stack, &register, circuit_value)?;
        }
    }
    Ok(registers)
}

/// Samples the finalize registers. Note: Do not replicate this for real program use, it is insecure.
pub fn sample_finalize_registers(
    stack: &Stack<CurrentNetwork>,
    function_name: &Identifier<CurrentNetwork>,
    plaintexts: &[Plaintext<CurrentNetwork>],
) -> Result<FinalizeRegisters<CurrentNetwork>> {
    // Initialize the registers.
    let mut finalize_registers = FinalizeRegisters::<CurrentNetwork>::new(
        FinalizeGlobalState::from(1, 1, None, [0; 32]),
        Some(<CurrentNetwork as Network>::TransitionID::default()),
        *function_name,
        stack.get_finalize_types(function_name)?.clone(),
        Some(0u64),
    );

    // For each literal,
    for (index, plaintext) in plaintexts.iter().enumerate() {
        // Initialize the register
        let register = Register::Locator(index as u64);
        // Store the value in the console registers.
        finalize_registers.store(stack, &register, Value::Plaintext(plaintext.clone()))?;
    }

    Ok(finalize_registers)
}

/// Bailing `FinalizeStoreTrait` for tests of instructions that don't touch the finalize store.
/// `Instruction::finalize` requires a store argument; instructions like `assert`, `cast`, `hash`,
/// etc. ignore it, so the harness can use this no-op shim instead of building a real store.
pub struct NoopFinalizeStore;

impl FinalizeStoreTrait<CurrentNetwork> for NoopFinalizeStore {
    fn contains_mapping_confirmed(
        &self,
        _program_id: &ProgramID<CurrentNetwork>,
        _mapping_name: &Identifier<CurrentNetwork>,
    ) -> Result<bool> {
        bail!("NoopFinalizeStore: contains_mapping_confirmed is unsupported")
    }

    fn contains_mapping_speculative(
        &self,
        _program_id: &ProgramID<CurrentNetwork>,
        _mapping_name: &Identifier<CurrentNetwork>,
    ) -> Result<bool> {
        bail!("NoopFinalizeStore: contains_mapping_speculative is unsupported")
    }

    fn contains_key_speculative(
        &self,
        _program_id: ProgramID<CurrentNetwork>,
        _mapping_name: Identifier<CurrentNetwork>,
        _key: &Plaintext<CurrentNetwork>,
    ) -> Result<bool> {
        bail!("NoopFinalizeStore: contains_key_speculative is unsupported")
    }

    fn get_value_speculative(
        &self,
        _program_id: ProgramID<CurrentNetwork>,
        _mapping_name: Identifier<CurrentNetwork>,
        _key: &Plaintext<CurrentNetwork>,
    ) -> Result<Option<Value<CurrentNetwork>>> {
        bail!("NoopFinalizeStore: get_value_speculative is unsupported")
    }

    fn insert_key_value(
        &self,
        _program_id: ProgramID<CurrentNetwork>,
        _mapping_name: Identifier<CurrentNetwork>,
        _key: Plaintext<CurrentNetwork>,
        _value: Value<CurrentNetwork>,
    ) -> Result<FinalizeOperation<CurrentNetwork>> {
        bail!("NoopFinalizeStore: insert_key_value is unsupported")
    }

    fn update_key_value(
        &self,
        _program_id: ProgramID<CurrentNetwork>,
        _mapping_name: Identifier<CurrentNetwork>,
        _key: Plaintext<CurrentNetwork>,
        _value: Value<CurrentNetwork>,
    ) -> Result<FinalizeOperation<CurrentNetwork>> {
        bail!("NoopFinalizeStore: update_key_value is unsupported")
    }

    fn remove_key_value(
        &self,
        _program_id: ProgramID<CurrentNetwork>,
        _mapping_name: Identifier<CurrentNetwork>,
        _key: &Plaintext<CurrentNetwork>,
    ) -> Result<Option<FinalizeOperation<CurrentNetwork>>> {
        bail!("NoopFinalizeStore: remove_key_value is unsupported")
    }
}
