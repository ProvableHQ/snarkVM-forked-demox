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

use snarkvm_synthesizer_snark::ProvingKey;

use super::*;

// TODO (dynamic_dispatch) re-introduce
/* 
macro_rules! prepare_impl {
    ($self:ident, $transitions:ident, $call_graph:ident) => {{

        // Initialize a vector for the batch verifier inputs.
        let mut batch_prover_inputs: HashMap<(ProgramID<N>, Identifier<N>), Vec<TranslationAssignment<N>>> = HashMap::new();

        let network_id = U16::new(N::ID);

        let mut translation_count = 0;

        for (parent, children) in $call_graph.iter() {
            let (parent_transition, parent_function) = $transitions.get(parent).ok_or_else(|| 
                anyhow!("Transition not found in the call graph")
            )?;

            let record_translation_arguments = parent_transition.record_translation_args().cloned().unwrap_or_default();
            let mut record_translation_arguments_iter = record_translation_arguments.iter();

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
                for ((call_operand_type, child_input), child_input_type) in dynamic_call.operand_types().iter().zip(child_transition.inputs().iter()).zip(child_function.input_types().iter()) {
                    match (call_operand_type, child_input_type) {
                        (ValueType::DynamicRecord, ValueType::ExternalRecord(locator)) => {
                            let dynamic_record_fid = *parent_function.name().to_field()?;
                            let dynamic_record_id = if let Some(record_translation_argument) = record_translation_arguments_iter.next() {
                                **record_translation_argument
                            } else {
                                bail!("No record translation argument found for the parent input");
                            };
                            let static_record_id = **child_input.id();
                            let to_static_record = N::Field::one();
                            let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                            
                            batch_verifier_inputs.entry((*child_program_id, *locator.resource())).or_default().push(
                                vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
                            );
                            translation_count += 1;
                        }
                        (ValueType::DynamicRecord, ValueType::Record(record_identifier)) => {
                            let dynamic_record_fid = *parent_function.name().to_field()?;
                            let dynamic_record_id = if let Some(record_translation_argument) = record_translation_arguments_iter.next() {
                                **record_translation_argument
                            } else {
                                bail!("No record translation argument found for the parent input");
                            };
                            let static_record_id = **child_input.id();
                            let to_static_record = N::Field::one();
                            let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                            
                            batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
                                vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
                            );
                            translation_count += 1;
                        }
                        (ValueType::ExternalRecord(locator), ValueType::DynamicRecord) => {
                            let dynamic_record_fid = *child_function.name().to_field()?;
                            let dynamic_record_id = **child_input.id();
                            let static_record_id = if let Some(record_translation_argument) = record_translation_arguments_iter.next() {
                                **record_translation_argument
                            } else {
                                bail!("No record translation argument found for the parent input");
                            };
                            let to_static_record = N::Field::zero();
                            let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                            
                            batch_verifier_inputs.entry((*child_program_id, *locator.resource())).or_default().push(
                                vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
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
                            let to_static_record = N::Field::zero();
                            let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                            
                            batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
                               vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
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
                for ((call_destination_type, child_output), child_output_type) in dynamic_call.destination_types().iter().zip(child_transition.outputs().iter()).zip(child_function.output_types().iter()) {
                    match (call_destination_type, child_output_type) {
                        (ValueType::ExternalRecord(locator), ValueType::DynamicRecord) => {
                            let dynamic_record_fid = *child_function.name().to_field()?;
                            let dynamic_record_id = **child_output.id();
                            let static_record_id = if let Some(record_translation_argument) = record_translation_arguments_iter.next() {
                                **record_translation_argument
                            } else {
                                bail!("No record translation argument found for the parent output");
                            };
                            let to_static_record = N::Field::zero();
                            let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                            
                            batch_verifier_inputs.entry((*child_program_id, *locator.resource())).or_default().push(
                                vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
                            );
                            translation_count += 1;
                        }
                        (ValueType::Record(record_identifier), ValueType::DynamicRecord) => {
                            let dynamic_record_fid = *child_function.name().to_field()?;
                            let dynamic_record_id = **child_output.id();
                            let static_record_id = if let Some(record_translation_argument) = record_translation_arguments_iter.next() {
                                **record_translation_argument
                            } else {
                                bail!("No record translation argument found for the parent output");
                            };
                            let to_static_record = N::Field::one();
                            let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;

                            batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
                                vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
                            );
                            translation_count += 1;
                        }
                        (ValueType::DynamicRecord, ValueType::ExternalRecord(locator)) => {
                            let dynamic_record_fid = *parent_function.name().to_field()?;
                            let dynamic_record_id = if let Some(record_translation_argument) = record_translation_arguments_iter.next() {
                                **record_translation_argument
                            } else {
                                bail!("No record translation argument found for the parent output");
                            };
                            let static_record_id = **child_output.id();
                            let to_static_record = N::Field::zero();
                            let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                            
                            batch_verifier_inputs.entry((*child_program_id, *locator.resource())).or_default().push(
                                vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
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
                            let to_static_record = N::Field::zero();
                            let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                            
                            batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
                                vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
                            );
                            translation_count += 1;
                        }
                        _ => { } // No translation to perform.
                    }
                }
            }

            ensure!(record_translation_arguments_iter.next().is_none(), "Extra record translation argument found for the parent transition");
        }

        let batch_with_proving_keys = batch_prover_inputs.into_iter().map(|(key, inputs)| {
            let proving_key = proving_keys.get(&key).ok_or_else(||
                anyhow!("Translation proving key not found for {}/{}", key.0, key.1)
            )?;
            Ok((proving_key.clone(), inputs))
        }).collect::<Result<Vec<(ProvingKey<N>, Vec<TranslationAssignment<N>>)>>>()?;

        Ok(batch_with_proving_keys)
    }};





        // for transition in $transitions.iter() {
        //     // Process the input tasks.
        //     match $self.input_tasks.get(transition.id()) {
        //         Some(tasks) => {
        //             for task in tasks {
        //                 // TODO (dynamic_dispatch) implement this.
        //             }
        //         },
        //         None => { bail!("Missing input tasks for transition {} in translation", transition.id()) }
        //     }

        //     let program_id = transition.program_id().clone();

        //     // TODO this needs to be accessible.
        //     let tvk = Field::<N>::zero();

        //     // TODO (dynamic_dispatch) easier way to access this?
        //     // TODO (dynamic_dispatch) should we distinguish caller/callee?
        //     let function_id = compute_function_id(&network_id, &program_id, transition.function_name(), transition.is_dynamic())?;
    
            // for (register_index, input) in transition.inputs().iter().enumerate() {
            //     match input {
            //         Input::Record(serial_number, tag) => {
            //             // TODO (dynamic_dispatch) construct the translations object; it could contain pointerse to records in other transactions if we're okay with introducing a lifetime
            //             let Some((
            //                 record_static,
            //                 static_record_name,
            //                 record_view_key,
            //                 gamma_option
            //             )) = transition.translations().get(translation_count) else {
            //                 bail!("Translation data {translation_count} for transition {} (case 1) not found", transition.id());
            //             };

            //             let to_static_record = true;
                        
            //             let commitment = record_static.to_commitment(&program_id, &static_record_name, &record_view_key).unwrap();

            //             let Some(gamma) = gamma_option else {
            //                 bail!("Translation {translation_count} for transition {} (case 1) consumes a static record, but no gamma was supplied", transition.id());
            //             };

            //             ensure!(
            //                 Record::<N, Plaintext<N>>::serial_number_from_gamma(&gamma, commitment).unwrap() == *serial_number,
            //                 "In translation {translation_count} for transition {} (case 1), the serial number of the static record does not match the expected value", transition.id());

            //             let record_dynamic = DynamicRecord::<N>::from_record(&record_static)?;

            //             let id_dynamic = record_dynamic.to_id(function_id, tvk, U16::new(register_index as u16)).unwrap();

            //             // TODO (dynamic_dispatch) fix this if necessary
            //             let child_program_id = program_id.clone();
                        
            //             batch_prover_inputs.entry((child_program_id, *static_record_name)).or_default().push(TranslationAssignment::new(
            //                 record_static,
            //                 program_id,
            //                 function_id,
            //                 static_record_name,
            //                 record_dynamic,
            //                 to_static_record,
            //                 translation_count,
            //                 tvk,
            //                 // TODO (dynamic_dispatch) is this the correct register index?
            //                 register_index as u16,
            //                 id_dynamic,
            //                 *serial_number,
            //                 record_view_key,
            //                 gamma,
            //             ));

            //             translation_count += 1;
            //         }
            //     }
            // }

            //         (ValueType::DynamicRecord, ValueType::Record(record_identifier)) => {
            //             // Case 1: the dynamic call passes in a dynamic record which becomes a static record in the callee.
            //             // TODO (dynamic_dispatch): decide whether this should be the child function in some cases.
            //             let dynamic_record_fid = *parent_function.name().to_field()?;
            //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
            //             let dynamic_record_id = N::Field::zero(); // ID of the input
            //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
            //             let static_record_id = N::Field::zero(); // ID of the output
            //             let to_static_record = N::Field::one();
            //             let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                        
            //             batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
            //                 vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
            //             );

            //             translation_assignments.push(TranslationAssignment::new(
            //                 record_static,
            //                 program_id,
            //                 function_id,
            //                 record_name,
            //                 record_dynamic,
            //                 to_static_record,
            //                 translation_count,
            //                 tvk,
            //                 register_index,
            //                 id_dynamic,
            //                 id_static,
            //                 record_view_key,
            //                 gamma,
            //             ));

            //             translation_count += 1;
            //         }
            //         (ValueType::Record(record_identifier), ValueType::DynamicRecord) => {
            //             // Case 2: the dynamic call passes in a static record which becomes a dynamic record.
            //             // TODO (dynamic_dispatch): decide whether this should be the child function in some cases.
            //             let dynamic_record_fid = *child_function.name().to_field()?;
            //             // TODO (dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
            //             let dynamic_record_id = N::Field::zero(); // ID of the output
            //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
            //             let static_record_id = N::Field::zero(); // ID of the input
            //             let to_static_record = N::Field::zero();
            //             let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                        
            //             batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
            //                 vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
            //             );
            //             translation_count += 1;
            //         }
            //         _ => { } // No translation to perform.
            //     }
            // }
            // ensure!(
            //     dynamic_call.destination_types().len() == child_function.outputs().len(),
            //     "The number of call destinations {} does not match the number of function outputs {}",
            //     dynamic_call.destination_types().len(),
            //     child_function.outputs().len()
            // );
            // // TODO (dynamic_dispatch): is it okay that call_destionation is not used?
            // for ((call_destination, call_destination_type), output) in dynamic_call.destinations().iter()
            //     .zip(dynamic_call.destination_types().iter())
            //     .zip(child_function.outputs().iter()) {

            //     match (call_destination_type, output.value_type()) {
            //         (ValueType::Record(record_identifier), ValueType::DynamicRecord) => {
            //             // Case 3: the dynamic call returns a dynamic record which becomes a static record.
            //             // TODO (dynamic_dispatch): decide whether this should be the child function in some cases.
            //             let dynamic_record_fid = *child_function.name().to_field()?;
            //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
            //             let dynamic_record_id = N::Field::zero(); // ID of the output
            //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
            //             let static_record_id = N::Field::zero(); // ID of the input
            //             let to_static_record = N::Field::one();
            //             let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;

            //             batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
            //                 vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
            //             );
            //             translation_count += 1;
            //         }
            //         (ValueType::DynamicRecord, ValueType::Record(record_identifier)) => {
            //             // Case 4: the dynamic call returns a concrete record which becomes a static record.
            //             let dynamic_record_fid = *parent_function.name().to_field()?;
            //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
            //             let dynamic_record_id = N::Field::zero(); // ID of the input
            //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
            //             let static_record_id = N::Field::zero(); // ID of the output
            //             let to_static_record = N::Field::zero();
            //             let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                        
            //             batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
            //                 vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
            //             );
            //             translation_count += 1;
}
 */

impl<N: Network> Translation<N> {
    /// Returns the translation assignments for the given transitions.
    pub fn prepare(
        &self,
        transitions: &[Transition<N>],
        call_graph: &HashMap<N::TransitionID, Vec<N::TransitionID>>,
    ) -> Result<Vec<(ProvingKey<N>, Vec<TranslationAssignment<N>>)>> {
       // TODO (Antonio) switch to macro
       
       // Initialize a vector for the batch verifier inputs.
        let mut batch_prover_inputs: HashMap<(ProgramID<N>, Identifier<N>), Vec<TranslationAssignment<N>>> = HashMap::new();

        // TODO (Antonio)
        // let network_id = U16::new(N::ID);

        // let mut translation_count = 0;

        // for (parent_id, children_ids) in call_graph.iter() {
        //     let parent_transition = transitions.iter().find(|transition| transition.id() == parent_id).ok_or_else(|| 
        //         anyhow!("Transition {} not found in the call graph", parent_id)
        //     )?;

        //     let parent_function_id = compute_function_id(&network_id, &parent_transition.program_id(), parent_transition.function_name(), parent_transition.is_dynamic())?;

        //     let child_transitions = children_ids.iter().map(|child_id| transitions.iter().find(|transition| transition.id() == child_id).ok_or_else(|| 
        //         anyhow!("Transition {} not found in the call graph", child_id))
        //     ).collect::<Result<Vec<&Transition<N>>>>()?;

        //     let translation_tasks = self.translation_tasks.get(parent_transition.id()).unwrap_or(&Vec::new());
        //     let transation_data = parent_transition.record_translation_args().unwrap_or(&Vec::new());

        //     ensure!(
        //         transation_data.len() == translation_tasks.len(),
        //         "The number of translation tasks {} for transition {} does not match the number of translation data {}",
        //         translation_tasks.len(),
        //         parent_transition.id(),
        //         transation_data.len(),
        //     );

        //     for child_transition in child_transitions {
        //         for child_input in child_transition.inputs() {
        //             match  child_input {
        //                 // TODO (dynamic_dispatch) how do I know here that the caller has passed a dynamic record?
        //                 (Input::Record(input_serial_number, _)) => {

        //                     let TranslationTask { commitment, gamma, serial_number, record } = translation_tasks.pop().unwrap();
        //                     let id_dynamic_data = transation_data.pop().unwrap();

        //                     let program_id = *child_transition.program_id();
        //                     let record_static = record;
        //                     // TODO (dynamic_dispatch) get
        //                     let record_name = Identifier::<N>::from_str("record")?;
        //                     // TODO (dynamic_dispatch) make sure this should always be the parent function ID
        //                     let function_id = parent_function_id;
        //                     let record_dynamic = DynamicRecord::<N>::from_record(&record_static)?;

        //                     let to_static_record = true;

        //                     batch_prover_inputs.entry((parent_transition.program_id(), *locator.resource())).or_default().push(TranslationAssignment::new(
        //                         record_static,
        //                         program_id,
        //                         function_id,
        //                         record_name,
        //                         record_dynamic,
        //                         to_static_record,
        //                         translation_count,
        //                         tvk,
        //                         register_index,
        //                         id_dynamic,
        //                         id_static,
        //                         record_view_key,
        //                         gamma,
        //                     ));

        //                     translation_count += 1;
        //                 }
        //                 // TODO (dynamic_dispatch) how do I know here that the caller has passed a static record?
        //                 (Input::DynamicRecord(dynamic_record_id)) => {

        //                     let TranslationTask { commitment, gamma, serial_number, record } = translation_tasks.pop().unwrap();

        //                     assert_eq!(*child_input.id(), dynamic_record_id);

        //                     let to_static_record = false;

        //                     batch_prover_inputs.entry((parent_transition.program_id(), *locator.resource())).or_default().push(TranslationAssignment::new(
        //                         record_static,
        //                         program_id,
        //                         function_id,
        //                         record_name,
        //                         record_dynamic,
        //                         to_static_record,
        //                         translation_count,
        //                         tvk,
        //                         register_index,
        //                         id_dynamic,
        //                         id_static,
        //                         record_view_key,
        //                         gamma,
        //                     ));

        //                     translation_count += 1;
        //                 }
        //                 _ => { } // No translation to perform.
        //             }
        //         }
        //     }

            // for (translation_task, dynamic_record_id) in translation_tasks.iter().zip(transation_data.iter()) {
            //     // TODO (Antonio) fix
            //     let to_static_record = match translation_task.translation_case {
            //         TranslationCase::InputStaticToDynamic => true,
            //         TranslationCase::InputDynamicToStatic => false,
            //         TranslationCase::OutputStaticToDynamic => true,
            //         TranslationCase::OutputDynamicToStatic => false,
            //     };

            //     let TranslationTask { commitment, gamma, serial_number, record } = *translation_task;

            //     let record_static = record;

            //     if to_static_record {
            //         ensure!(
            //             Record::<N, Plaintext<N>>::serial_number_from_gamma(&gamma, commitment).unwrap() == serial_number,
            //             "In translation {translation_count} (transition id: {}), the serial number of the static record does not match the expected value",
            //             parent_transition.id(),
            //         );
            //     }

            //     let record_dynamic = DynamicRecord::<N>::from_record(&record_static)?;

            //     // TODO (Antonio) fix
            //     let tvk = Field::<N>::zero();
            //     let register_index = 0;

            //     let parent_function_id = compute_function_id(&network_id, &parent_transition.program_id(), parent_transition.function_name(), parent_transition.is_dynamic())?;

            //     ensure!(
            //         // TODO (Dynamic_dispatch) make sure this should always be the parent function ID
            //         *dynamic_record_id == record_dynamic.to_id(parent_function_id, tvk, U16::new(register_index as u16)).unwrap(),
            //         "In translation {translation_count} (transition id: {}) for transition {}, the dynamic record id does not match the expected value",
            //         parent_transition.id(),
            //         dynamic_record_id,
            //     );

            //     // TODO (dynamic_dispatch) fix this if necessary
            //     let child_program_id = parent_transition.program_id();
                
            //     batch_prover_inputs.entry((child_program_id, *static_record_name)).or_default().push(TranslationAssignment::new(
            //         record_static,
            //         program_id,
            //         function_id,
            //         static_record_name,
            //         record_dynamic,
            //         to_static_record,
            //         translation_count,
            //         tvk,
            //         // TODO (dynamic_dispatch) is this the correct register index?
            //         register_index as u16,
            //         id_dynamic,
            //         *serial_number,
            //         record_view_key,
            //         gamma,
            //     ));

            //     translation_count += 1;

        // let batch_with_proving_keys = batch_prover_inputs.into_iter().map(|(key, inputs)| {
        //     let proving_key = proving_keys.get(&key).ok_or_else(||
        //        anyhow!("Translation proving key not found for {}/{}", key.0, key.1)
        //     )?;
        //     Ok((proving_key.clone(), inputs))
        // }).collect::<Result<Vec<(ProvingKey<N>, Vec<TranslationAssignment<N>>)>>>()?;

       Ok(Vec::new())
   }





       // for transition in $transitions.iter() {
       //     // Process the input tasks.
       //     match $self.input_tasks.get(transition.id()) {
       //         Some(tasks) => {
       //             for task in tasks {
       //                 // TODO (dynamic_dispatch) implement this.
       //             }
       //         },
       //         None => { bail!("Missing input tasks for transition {} in translation", transition.id()) }
       //     }

       //     let program_id = transition.program_id().clone();

       //     // TODO this needs to be accessible.
       //     let tvk = Field::<N>::zero();

       //     // TODO (dynamic_dispatch) easier way to access this?
       //     // TODO (dynamic_dispatch) should we distinguish caller/callee?
       //     let function_id = compute_function_id(&network_id, &program_id, transition.function_name(), transition.is_dynamic())?;
   
           // for (register_index, input) in transition.inputs().iter().enumerate() {
           //     match input {
           //         Input::Record(serial_number, tag) => {
           //             // TODO (dynamic_dispatch) construct the translations object; it could contain pointerse to records in other transactions if we're okay with introducing a lifetime
           //             let Some((
           //                 record_static,
           //                 static_record_name,
           //                 record_view_key,
           //                 gamma_option
           //             )) = transition.translations().get(translation_count) else {
           //                 bail!("Translation data {translation_count} for transition {} (case 1) not found", transition.id());
           //             };

           //             let to_static_record = true;
                       
           //             let commitment = record_static.to_commitment(&program_id, &static_record_name, &record_view_key).unwrap();

           //             let Some(gamma) = gamma_option else {
           //                 bail!("Translation {translation_count} for transition {} (case 1) consumes a static record, but no gamma was supplied", transition.id());
           //             };

           //             ensure!(
           //                 Record::<N, Plaintext<N>>::serial_number_from_gamma(&gamma, commitment).unwrap() == *serial_number,
           //                 "In translation {translation_count} for transition {} (case 1), the serial number of the static record does not match the expected value", transition.id());

           //             let record_dynamic = DynamicRecord::<N>::from_record(&record_static)?;

           //             let id_dynamic = record_dynamic.to_id(function_id, tvk, U16::new(register_index as u16)).unwrap();

           //             // TODO (dynamic_dispatch) fix this if necessary
           //             let child_program_id = program_id.clone();
                       
           //             batch_prover_inputs.entry((child_program_id, *static_record_name)).or_default().push(TranslationAssignment::new(
           //                 record_static,
           //                 program_id,
           //                 function_id,
           //                 static_record_name,
           //                 record_dynamic,
           //                 to_static_record,
           //                 translation_count,
           //                 tvk,
           //                 // TODO (dynamic_dispatch) is this the correct register index?
           //                 register_index as u16,
           //                 id_dynamic,
           //                 *serial_number,
           //                 record_view_key,
           //                 gamma,
           //             ));

           //             translation_count += 1;
           //         }
           //     }
           // }

           //         (ValueType::DynamicRecord, ValueType::Record(record_identifier)) => {
           //             // Case 1: the dynamic call passes in a dynamic record which becomes a static record in the callee.
           //             // TODO (dynamic_dispatch): decide whether this should be the child function in some cases.
           //             let dynamic_record_fid = *parent_function.name().to_field()?;
           //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
           //             let dynamic_record_id = N::Field::zero(); // ID of the input
           //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
           //             let static_record_id = N::Field::zero(); // ID of the output
           //             let to_static_record = N::Field::one();
           //             let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                       
           //             batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
           //                 vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
           //             );

           //             translation_assignments.push(TranslationAssignment::new(
           //                 record_static,
           //                 program_id,
           //                 function_id,
           //                 record_name,
           //                 record_dynamic,
           //                 to_static_record,
           //                 translation_count,
           //                 tvk,
           //                 register_index,
           //                 id_dynamic,
           //                 id_static,
           //                 record_view_key,
           //                 gamma,
           //             ));

           //             translation_count += 1;
           //         }
           //         (ValueType::Record(record_identifier), ValueType::DynamicRecord) => {
           //             // Case 2: the dynamic call passes in a static record which becomes a dynamic record.
           //             // TODO (dynamic_dispatch): decide whether this should be the child function in some cases.
           //             let dynamic_record_fid = *child_function.name().to_field()?;
           //             // TODO (dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
           //             let dynamic_record_id = N::Field::zero(); // ID of the output
           //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
           //             let static_record_id = N::Field::zero(); // ID of the input
           //             let to_static_record = N::Field::zero();
           //             let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                       
           //             batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
           //                 vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
           //             );
           //             translation_count += 1;
           //         }
           //         _ => { } // No translation to perform.
           //     }
           // }
           // ensure!(
           //     dynamic_call.destination_types().len() == child_function.outputs().len(),
           //     "The number of call destinations {} does not match the number of function outputs {}",
           //     dynamic_call.destination_types().len(),
           //     child_function.outputs().len()
           // );
           // // TODO (dynamic_dispatch): is it okay that call_destionation is not used?
           // for ((call_destination, call_destination_type), output) in dynamic_call.destinations().iter()
           //     .zip(dynamic_call.destination_types().iter())
           //     .zip(child_function.outputs().iter()) {

           //     match (call_destination_type, output.value_type()) {
           //         (ValueType::Record(record_identifier), ValueType::DynamicRecord) => {
           //             // Case 3: the dynamic call returns a dynamic record which becomes a static record.
           //             // TODO (dynamic_dispatch): decide whether this should be the child function in some cases.
           //             let dynamic_record_fid = *child_function.name().to_field()?;
           //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
           //             let dynamic_record_id = N::Field::zero(); // ID of the output
           //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
           //             let static_record_id = N::Field::zero(); // ID of the input
           //             let to_static_record = N::Field::one();
           //             let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;

           //             batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
           //                 vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
           //             );
           //             translation_count += 1;
           //         }
           //         (ValueType::DynamicRecord, ValueType::Record(record_identifier)) => {
           //             // Case 4: the dynamic call returns a concrete record which becomes a static record.
           //             let dynamic_record_fid = *parent_function.name().to_field()?;
           //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
           //             let dynamic_record_id = N::Field::zero(); // ID of the input
           //             // TODO(dynamic_dispatch): this is a placeholder. How does the verifier obtain this...?
           //             let static_record_id = N::Field::zero(); // ID of the output
           //             let to_static_record = N::Field::zero();
           //             let translation_count_field = *Field::<N>::from_bits_le(&translation_count.to_bits_le())?;
                       
           //             batch_verifier_inputs.entry((*child_program_id, *record_identifier)).or_default().push(
           //                 vec![translation_count_field, dynamic_record_fid, dynamic_record_id, static_record_id, to_static_record]
           //             );
           //             translation_count += 1;

    /// Returns the inclusion assignments for the given transitions.
    #[cfg(feature = "async")]
    pub async fn prepare_async(
        &self,
        transitions: &[Transition<N>],
        call_graph: &HashMap<N::TransitionID, Vec<N::TransitionID>>,
    ) -> Result<Vec<(ProvingKey<N>, Vec<TranslationAssignment<N>>)>> {
        // TODO (Antonio) switch to macro
        Ok(vec![])
        /* prepare_impl!(
            self,
            transitions,
            call_graph
        ) */
    }
}
