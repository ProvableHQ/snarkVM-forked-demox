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

mod assignment;
pub use assignment::*;

mod prepare;
pub use prepare::*;

#[cfg(test)]
pub mod tests;

use crate::Stack;

use circuit::{Inject, traits::ToGroup};

use console::{
    network::prelude::*,
    program::{
        DynamicRecord,
        Identifier,
        InputID,
        Plaintext,
        ProgramID,
        RECORD_DATA_TREE_DEPTH,
        Record,
        Value,
        ValueType,
    },
    types::{Field, Group},
};
use snarkvm_ledger_block::{Transition, Input, Output};
use snarkvm_synthesizer_program::{Function, Instruction, RecordTranslationData};
use snarkvm_synthesizer_snark::VerifyingKey;

use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct Translation<N: Network> {
    /// A map of `transition IDs` to a list of `input tasks`.
    translation_tasks: HashMap<N::TransitionID, Vec<RecordTranslationData<N>>>,
}

impl<N: Network> Translation<N> {
    /// Initializes a new `Translation` instance.
    pub fn new() -> Self {
        Self { translation_tasks: HashMap::new() }
    }

    /// Inserts the transition to build state for the translation task.
    pub fn insert_transition(
        &mut self,
        // TODO (dynamic_dispatch) seemingly unnecessary
        input_ids: &[InputID<N>],
        // TODO (dynamic_dispatch) seemingly unnecessary
        input_values: &[Value<N>],
        // TODO (dynamic_dispatch) seemingly only ID needed
        transition: &Transition<N>,
        record_translation_data: Result<&Vec<RecordTranslationData<N>>>,
    ) -> Result<()> {
        // TODO (dynamic_dispatch): Result isn't a good interface; also, decide whether always having a value for a valid key = TransitionID (even if empty) is a good choice
        self.translation_tasks.insert(*transition.id(), record_translation_data.cloned().unwrap_or_default());

        Ok(())
    }

    /// Returns the verifier public inputs for the given call graph and transitions.
    pub fn prepare_verifier_inputs<'a>(
        translation_verifying_keys: &HashMap<(ProgramID<N>, Identifier<N>), VerifyingKey<N>>,
        transitions: &HashMap<N::TransitionID, (&Transition<N>, Function<N>)>,
        _call_graph: &HashMap<N::TransitionID, Vec<N::TransitionID>>,
    ) -> Result<Vec<(VerifyingKey<N>, Vec<Vec<N::Field>>)>> {
        // Determine the number of transitions.
        let num_transitions = transitions.len();

        // Initialize a vector for the batch verifier inputs.
        /* 
            let circuit_record_consumed = circuit::Boolean::<A>::new(circuit::Mode::Public, self.record_consumed);
            let circuit_function_id = circuit::Field::<A>::new(circuit::Mode::Public, self.function_id);
            let _circuit_translation_count =
                circuit::U16::<A>::new(circuit::Mode::Public, console::types::U16::<N>::new(self.translation_count));
            let circuit_input_output_index =
                circuit::U16::<A>::new(circuit::Mode::Public, console::types::U16::<N>::new(self.input_output_index));
            let circuit_id_static = circuit::Field::<A>::new(circuit::Mode::Public, self.id_static);
            let circuit_id_dynamic = circuit::Field::<A>::new(circuit::Mode::Public, self.id_dynamic);
        */
        let mut batch_verifier_inputs: HashMap<(ProgramID<N>, Identifier<N>), Vec<Vec<N::Field>>> = HashMap::new();

        let mut translation_count = 0;

        for transition in transitions.values().rev() {
            if let Some(caller_inputs) = transition.caller_inputs() {

                let child_program_id = transition.program_id();
                let child_function_name = transition.function_name();
                let child_function_id = crate::compute_function_id(&console::types::U16::new(N::ID), child_program_id, child_function_name)?;

                // TODO: fix the next 6 lines
                ensure!(dynamic_call.operand_types().len() == child_transition.inputs().len(), "The number of call operands {} does not match the number of function inputs {}", dynamic_call.operand_types().len(), child_transition.inputs().len());
                ensure!(dynamic_call.operand_types().len() == child_function.input_types().len(), "The number of call operands {} does not match the number of function input types {}", dynamic_call.operand_types().len(), child_function.input_types().len());
                ensure!(dynamic_call.operand_types().len() == caller_inputs.len(), "The number of call operands {} does not match the number of parent caller inputs {}", dynamic_call.operand_types().len(), caller_inputs.len());
                for (io_index, (call_operand_type, child_input, child_input_type, caller_input)) in itertools::izip!(dynamic_call.operand_types(), child_transition.inputs(), child_function.input_types(), caller_inputs).enumerate() {
                    match (call_operand_type, child_input_type, child_input) {
                        (ValueType::DynamicRecord, ValueType::Record(record_identifier), Input::Record(serial_number, _)) => {
                            let dynamic_record_fid = *child_function_id;
                            let dynamic_record_id = **caller_input;
                            let static_record_id = **serial_number;
                            let record_consumed = N::Field::one();
                            let translation_count_field = *console::types::U16::<N>::new(translation_count).to_field()?;
                            let io_index_field = *console::types::U16::<N>::new(io_index as u16).to_field()?;

                            batch_verifier_inputs.entry((*child_program_id, record_identifier)).or_default().push(
                                vec![record_consumed, dynamic_record_fid, translation_count_field, io_index_field, static_record_id, dynamic_record_id]
                            );
                            translation_count += 1;
                        }
                        _ => { } // No translation to perform.
                    }
                }
            }
        }

        // for (parent, children) in call_graph.iter() {
        //     let (parent_transition, parent_function) = transitions.get(parent).ok_or_else(||
        //         anyhow!("Transition not found in the call graph")
        //     )?;

        //     let parent_program_id = parent_transition.program_id();
        //     let parent_function_name = parent_transition.function_name();

        //     let call_instructions = parent_function.instructions().iter().filter(|instruction| {
        //         matches!(instruction, Instruction::Call(_) | Instruction::CallDynamic(_))
        //     }).collect_vec();

        //     ensure!(
        //         call_instructions.len() == children.len(),
        //         "The number of call instructions {} does not match the number of children {}",
        //         call_instructions.len(),
        //         children.len()
        //     );

        //     for (child, call_instruction) in children.iter().zip(call_instructions.iter()) {
        //         let (child_transition, child_function) = transitions.get(child).ok_or_else(||
        //             anyhow!("Transition not found in the call graph")
        //         )?;

        //         let parent_caller_inputs = parent_transition.caller_inputs().unwrap_or_default().iter().map(|input| input.id());
        //         let parent_caller_outputs = parent_transition.caller_outputs().unwrap_or_default().iter().map(|output| output.id());

        //         let child_program_id = child_transition.program_id();
        //         let child_function_name = child_transition.function_name();
        //         let child_function_id = crate::compute_function_id(&console::types::U16::new(N::ID), child_program_id, child_function_name)?;

        //         let Instruction::CallDynamic(dynamic_call) = call_instruction else {
        //             // Only dynamic calls can invoke a translation from a dynamic to a static record.
        //             continue;
        //         };

        //         // Determine if any record translation proofs are required.
        //         ensure!(dynamic_call.operand_types().len() == child_transition.inputs().len(), "The number of call operands {} does not match the number of function inputs {}", dynamic_call.operand_types().len(), child_transition.inputs().len());
        //         ensure!(dynamic_call.operand_types().len() == child_function.input_types().len(), "The number of call operands {} does not match the number of function input types {}", dynamic_call.operand_types().len(), child_function.input_types().len());
        //         ensure!(dynamic_call.operand_types().len() == parent_caller_inputs.len(), "The number of call operands {} does not match the number of parent caller inputs {}", dynamic_call.operand_types().len(), parent_caller_inputs.len());
        //         for (io_index, (call_operand_type, child_input, child_input_type, parent_caller_input)) in itertools::izip!(dynamic_call.operand_types(), child_transition.inputs(), child_function.input_types(), parent_caller_inputs).enumerate() {
        //             match (call_operand_type, child_input_type, child_input) {
        //                 (ValueType::DynamicRecord, ValueType::Record(record_identifier), Input::Record(serial_number, _)) => {
        //                     let dynamic_record_fid = *child_function_id;
        //                     let dynamic_record_id = **parent_caller_input;
        //                     let static_record_id = **serial_number;
        //                     let record_consumed = N::Field::one();
        //                     let translation_count_field = *console::types::U16::<N>::new(translation_count).to_field()?;
        //                     let io_index_field = *console::types::U16::<N>::new(io_index as u16).to_field()?;

        //                     batch_verifier_inputs.entry((*child_program_id, record_identifier)).or_default().push(
        //                         vec![record_consumed, dynamic_record_fid, translation_count_field, io_index_field, static_record_id, dynamic_record_id]
        //                     );
        //                     translation_count += 1;
        //                 }
        //                 _ => { } // No translation to perform.
        //             }
        //         }
        //         ensure!(dynamic_call.destination_types().len() == child_transition.outputs().len(), "The number of call destinations {} does not match the number of function outputs {}", dynamic_call.destination_types().len(), child_transition.outputs().len());
        //         ensure!(dynamic_call.destination_types().len() == child_function.output_types().len(), "The number of call destinations {} does not match the number of function output types {}", dynamic_call.destination_types().len(), child_function.output_types().len());
        //         ensure!(dynamic_call.destination_types().len() == parent_caller_outputs.len(), "The number of call destinations {} does not match the number of parent caller outputs {}", dynamic_call.destination_types().len(), parent_caller_outputs.len());
        //         for (io_index, (call_destination_type, child_output, child_output_type, parent_caller_output)) in itertools::izip!(dynamic_call.destination_types(), child_transition.outputs(), child_function.output_types(), parent_caller_outputs).enumerate() {
        //             match (call_destination_type, child_output_type, child_output) {
        //                 (ValueType::DynamicRecord, ValueType::Record(record_identifier), Output::Record(commitment, ..)) => {
        //                     let dynamic_record_fid = *child_function_id;
        //                     let dynamic_record_id = **parent_caller_output;
        //                     let static_record_id = **commitment;
        //                     let record_consumed = N::Field::one();
        //                     let translation_count_field = *console::types::U16::<N>::new(translation_count).to_field()?;
        //                     let io_index_field = *console::types::U16::<N>::new((child_transition.inputs().len() + io_index) as u16).to_field()?;

        //                     batch_verifier_inputs.entry((*child_program_id, record_identifier)).or_default().push(
        //                         vec![record_consumed, dynamic_record_fid, translation_count_field, io_index_field, static_record_id, dynamic_record_id]
        //                     );
        //                     translation_count += 1;
        //                 }
        //                 _ => { } // No translation to perform.
        //             }
        //         }
        //     }
        // }

        let batch_with_verifying_keys = batch_verifier_inputs.into_iter().map(|(key, inputs)| {
            let verifying_key = translation_verifying_keys.get(&key).ok_or_else(||
                anyhow!("Translation verifying key not found for {}/{}", key.0, key.1)
            )?;
            Ok((verifying_key.clone(), inputs))
        }).collect::<Result<Vec<(VerifyingKey<N>, Vec<Vec<N::Field>>)>>>()?;

        Ok(batch_with_verifying_keys)
    }
}
