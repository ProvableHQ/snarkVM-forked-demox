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
use snarkvm_ledger_block::Transition;
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
        // TODO (dynamic_dispatch) remove
        println!("\n\n ********* INSERTING TRANSLATION TASK \n\n *********");
        // TODO (dynamic_dispatch): Result isn't a good interface; also, decide whether always having a value for a valid key = TransitionID (even if empty) is a good choice
        self.translation_tasks.insert(*transition.id(), record_translation_data.cloned().unwrap_or_default());

        Ok(())
    }

    /// Returns the verifier public inputs for the given call graph and transitions.
    pub fn prepare_verifier_inputs<'a>(
        translation_verifying_keys: &HashMap<(ProgramID<N>, Identifier<N>), VerifyingKey<N>>,
        transitions: &HashMap<N::TransitionID, (&Transition<N>, Function<N>)>,
        call_graph: &HashMap<N::TransitionID, Vec<N::TransitionID>>,
    ) -> Result<Vec<(VerifyingKey<N>, Vec<Vec<N::Field>>)>> {
        // Determine the number of transitions.
        let num_transitions = transitions.len();

        // Initialize a vector for the batch verifier inputs.
        let mut batch_verifier_inputs: HashMap<(ProgramID<N>, Identifier<N>), Vec<Vec<N::Field>>> = HashMap::new();

        let mut translation_count = 0;

        /*  for (parent, children) in call_graph.iter() {
            let (parent_transition, parent_function) = transitions.get(parent).ok_or_else(||
                anyhow!("Transition not found in the call graph")
            )?;

            let record_translation_arguments = parent_transition.caller_inputs().unwrap_or_default().iter().map(|input| input.id());

            let parent_program_id = parent_transition.program_id();
            let parent_function_name = parent_transition.function_name();

            let call_instructions = parent_function.instructions().iter().filter(|instruction| {
                matches!(instruction, Instruction::Call(_) | Instruction::CallDynamic(_))
            }).collect_vec();

            ensure!(
                call_instructions.len() == children.len(),
                "The number of call instructions {} does not match the number of children {}",
                call_instructions.len(),
                children.len()
            );

            for (child, call_instruction) in children.iter().zip(call_instructions.iter()) {
                let (child_transition, child_function) = transitions.get(child).ok_or_else(||
                    anyhow!("Transition not found in the call graph")
                )?;

                let child_program_id = child_transition.program_id();
                let child_function_name = child_transition.function_name();

                let Instruction::CallDynamic(dynamic_call) = call_instruction else {
                    // Only dynamic calls can invoke a translation from a dynamic to a static record.
                    continue;
                };

                // Determine if any record translation proofs are required.
                ensure!(
                    dynamic_call.operand_types().len() == child_function.inputs().len(),
                    "The number of call operands does not match the number of function inputs"
                );
                for (io_index, ((call_operand_type, child_input), child_input_type)) in dynamic_call.operand_types().iter().skip(3).zip(child_transition.inputs().iter()).zip(child_function.input_types().iter()).enumerate() {
                    match (call_operand_type, child_input_type) {
                        (ValueType::DynamicRecord, ValueType::Record(record_identifier)) => {
                            let dynamic_record_fid = *parent_function.name().to_field()?;
                            let dynamic_record_id = if let Some(record_translation_argument) = record_translation_arguments_iter.next() {
                                **record_translation_argument
                            } else {
                                bail!("No record translation argument found for the parent input");
                            };
                            let static_record_id = **child_input.id();
                            let record_consumed = N::Field::one();
                            let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                            let io_index_field = *Field::<N>::from_bits_le(&(io_index as u8).to_bits_le())?;

                            batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
                                vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, record_consumed]
                            );
                            translation_count += 1;
                        }
                        (ValueType::Record(record_identifier), ValueType::DynamicRecord) => {
                            let dynamic_record_fid = *child_function.name().to_field()?;
                            let dynamic_record_id = **child_input.id();
                            let static_record_id = if let Some(record_translation_argument) = record_translation_arguments_iter.next() {
                                **record_translation_argument
                            } else {
                                bail!("No record translation argument found for the parent input");
                            };
                            let record_consumed = N::Field::zero();
                            let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                            let io_index_field = *Field::<N>::from_bits_le(&(io_index as u8).to_bits_le())?;

                            batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
                               vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, record_consumed]
                            );
                            translation_count += 1;
                        }
                        _ => { } // No translation to perform.
                    }
                }
                ensure!(
                    dynamic_call.destination_types().len() == child_function.outputs().len(),
                    "The number of call destinations {} does not match the number of function outputs {}",
                    dynamic_call.destination_types().len(),
                    child_function.outputs().len()
                );
                for (io_index, ((call_destination_type, child_output), child_output_type)) in dynamic_call.destination_types().iter().skip(3).zip(child_transition.outputs().iter()).zip(child_function.output_types().iter()).enumerate() {
                    match (call_destination_type, child_output_type) {
                        (ValueType::Record(record_identifier), ValueType::DynamicRecord) => {
                            let dynamic_record_fid = *child_function.name().to_field()?;
                            let dynamic_record_id = **child_output.id();
                            let static_record_id = if let Some(record_translation_argument) = record_translation_arguments_iter.next() {
                                **record_translation_argument
                            } else {
                                bail!("No record translation argument found for the parent output");
                            };
                            let record_consumed = N::Field::one();
                            let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                            let io_index_field = *Field::<N>::from_bits_le(&(io_index as u8).to_bits_le())?;

                            batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
                                vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, record_consumed]
                            );
                            translation_count += 1;
                        }
                        (ValueType::DynamicRecord, ValueType::Record(record_identifier)) => {
                            let dynamic_record_fid = *parent_function.name().to_field()?;
                            let dynamic_record_id = if let Some(record_translation_argument) = record_translation_arguments_iter.next() {
                                **record_translation_argument
                            } else {
                                bail!("No record translation argument found for the parent output");
                            };
                            let static_record_id = **child_output.id();
                            let record_consumed = N::Field::zero();
                            let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                            let io_index_field = *Field::<N>::from_bits_le(&(io_index as u8).to_bits_le())?;

                            batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
                                vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, record_consumed]
                            );
                            translation_count += 1;
                        }
                        _ => { } // No translation to perform.
                    }
                }
            }

            ensure!(record_translation_arguments_iter.next().is_none(), "Extra record translation argument found for the parent transition");
        }

        let batch_with_verifying_keys = batch_verifier_inputs.into_iter().map(|(key, inputs)| {
            let verifying_key = translation_verifying_keys.get(&key).ok_or_else(||
                anyhow!("Translation verifying key not found for {}/{}", key.0, key.1)
            )?;
            Ok((verifying_key.clone(), inputs))
        }).collect::<Result<Vec<(VerifyingKey<N>, Vec<Vec<N::Field>>)>>>()?;

        Ok(batch_with_verifying_keys) */

        Ok(vec![])
    }
}
