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
        U16,
        Value,
        ValueType,
        compute_function_id,
    },
    types::{Field, Group},
};
use snarkvm_ledger_block::{Input, Output, Transition};
use snarkvm_synthesizer_program::{Function, Instruction, RecordTranslationData};
use snarkvm_synthesizer_snark::VerifyingKey;

use std::collections::HashMap;

use itertools::izip;

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
        transition_id: N::TransitionID,
        record_translation_data: Result<&Vec<RecordTranslationData<N>>>,
    ) -> Result<()> {
        // TODO (dynamic_dispatch): Result isn't a good interface; also, decide whether always having a value for a valid key = TransitionID (even if empty) is a good choice
        self.translation_tasks.insert(transition_id, record_translation_data.cloned().unwrap_or_default());

        Ok(())
    }

    /// Returns the verifier public inputs for the given call graph and transitions.
    pub fn prepare_verifier_inputs<'a>(
        transitions: impl ExactSizeIterator<Item = &'a Transition<N>>,
        // Used to retrieve record names
        transition_map: &HashMap<N::TransitionID, (&Transition<N>, Function<N>)>,
        translation_verifying_keys: &HashMap<(ProgramID<N>, Identifier<N>), VerifyingKey<N>>,
    ) -> Result<Vec<(VerifyingKey<N>, Vec<Vec<N::Field>>)>> {
        // Determine the number of transitions.
        let num_transitions = transitions.len();

        let mut batch_verifier_inputs: HashMap<(ProgramID<N>, Identifier<N>), Vec<Vec<N::Field>>> = HashMap::new();

        let mut translation_count: u16 = 0;

        // Traversal order only affects the translation count appearing as a public input in the translation circuit.
        // Order is irrelevant as long as it is consistent between the prover and verifier. (cf. Translation::prepare)
        
        // TODO (Antonio) remove enumerate
        for (i, transition) in transitions.enumerate() {

            // TODO (Antonio) remove
            println!(" - transition {i}: {}/{}", transition.program_id(), transition.function_name());

            let (_, callee_function_core) = transition_map
                .get(transition.id())
                .ok_or_else(|| anyhow!("Transition {} from execution not found transition map", transition.id()))?;
            let callee_function_id =
                compute_function_id(&U16::<N>::new(N::ID), transition.program_id(), transition.function_name())?;

            ensure!(
                transition.caller_inputs().is_some() == transition.caller_outputs().is_some(),
                "The caller inputs and caller outputs should either both be Some or both be None, but found a discrepancy in transition {}: caller inputs = {}, caller outputs = {}",
                transition.id(),
                if transition.caller_inputs().is_some() { "Some" } else { "None" },
                if transition.caller_outputs().is_some() { "Some" } else { "None" }
            );

            // Prepare the input translation tasks
            let num_inputs = if let Some(caller_inputs) = transition.caller_inputs() {
                // TODO (dynamic_dispatch): confirm the input types don't have to be matched against the function definiction, as we were doing before (e. g. because that's already checked elsewhere)
                // TODO (antonio): cf above
                // TODO (vicsn): cf above
                // TODO (d0cd): cf above

                ensure!(
                    caller_inputs.len() == transition.inputs().len(),
                    "The number of caller inputs does not match the number of inputs in transition {}: ({} vs. {})",
                    transition.id(),
                    caller_inputs.len(),
                    transition.inputs().len(),
                );

                let callee_input_types = callee_function_core.input_types();

                ensure!(
                    callee_input_types.len() == transition.inputs().len(),
                    "The number of input types does not match the number of inputs in transition {}: ({} vs. {})",
                    transition.id(),
                    callee_input_types.len(),
                    transition.inputs().len(),
                );

                for (input_output_index, (caller_input, callee_input, callee_input_type)) in
                    izip!(caller_inputs.iter(), transition.inputs().iter(), callee_input_types.iter()).enumerate()
                {
                    match (caller_input, callee_input, callee_input_type) {
                        (
                            Input::DynamicRecord(id_dynamic),
                            Input::Record(serial_number, _),
                            ValueType::Record(record_name),
                        ) => {
                            // true
                            let field_is_input = N::Field::one();
                            // false
                            let field_static_is_external = N::Field::zero();

                            let field_function_id = *callee_function_id;

                            // TODO (dynamic_dispatch) is there a better way to do this? .to_fields() yields one field element
                            // TODO (dynamic_dispatch) separately: should this be to_bits_le or to_bits_be?
                            // TODO (dynamic_dispatch) both TODOs are superseeded by the discussed optimization of making the translation count a field element
                            let fields_translation_count = translation_count
                                .to_bits_le()
                                .into_iter()
                                .map(|bit: bool| if bit { N::Field::one() } else { N::Field::zero() })
                                .collect_vec();
                            let fields_input_output_index = (input_output_index as u16)
                                .to_bits_le()
                                .into_iter()
                                .map(|bit: bool| if bit { N::Field::one() } else { N::Field::zero() })
                                .collect_vec();

                            let field_id_static = **serial_number;
                            let field_id_dynamic = **id_dynamic;

                            let verifier_inputs = [
                                vec![
                                    // Initial constant 1
                                    N::Field::one(),
                                    field_is_input,
                                    field_static_is_external,
                                    field_function_id,
                                ],
                                fields_translation_count,
                                fields_input_output_index,
                                vec![
                                    field_id_static,
                                    field_id_dynamic
                                ],
                            ]
                            .into_iter()
                            .flatten()
                            .collect_vec();

                            batch_verifier_inputs
                                .entry((*transition.program_id(), *record_name))
                                .or_default()
                                .push(verifier_inputs);

                            translation_count += 1;
                        }
                        (
                            Input::DynamicRecord(id_dynamic),
                            Input::ExternalRecord(id_static),
                            ValueType::ExternalRecord(record_locator),
                        ) => {
                            // true
                            let field_is_input = N::Field::one();
                            // true
                            let field_static_is_external = N::Field::one();

                            let field_function_id = *callee_function_id;

                            // TODO (dynamic_dispatch) is there a better way to do this? .to_fields() yields one field element
                            // TODO (dynamic_dispatch) separately: should this be to_bits_le or to_bits_be?
                            // TODO (dynamic_dispatch) both TODOs are superseeded by the discussed optimization of making the translation count a field element
                            let fields_translation_count = translation_count
                                .to_bits_le()
                                .into_iter()
                                .map(|bit: bool| if bit { N::Field::one() } else { N::Field::zero() })
                                .collect_vec();
                            let fields_input_output_index = (input_output_index as u16)
                                .to_bits_le()
                                .into_iter()
                                .map(|bit: bool| if bit { N::Field::one() } else { N::Field::zero() })
                                .collect_vec();

                            let field_id_static = **id_static;
                            let field_id_dynamic = **id_dynamic;

                            let verifier_inputs = [
                                vec![
                                    // Initial constant 1
                                    N::Field::one(),
                                    field_is_input,
                                    field_static_is_external,
                                    field_function_id,
                                ],
                                fields_translation_count,
                                fields_input_output_index,
                                vec![
                                    field_id_static,
                                    field_id_dynamic
                                ],
                            ]
                            .into_iter()
                            .flatten()
                            .collect_vec();

                            let program_id = record_locator.program_id();
                            let record_name = record_locator.resource();
                            
                            batch_verifier_inputs
                                .entry((*program_id, *record_name))
                                .or_default()
                                .push(verifier_inputs);

                            translation_count += 1;
                        }
                        (Input::Record(..), Input::DynamicRecord(..), ValueType::DynamicRecord) => {
                            bail!("Translation of (non-external) input records to dynamic records is not supported");
                        }
                        (Input::ExternalRecord(..), Input::DynamicRecord(..), ValueType::DynamicRecord) => {
                            bail!("Translation of (external) input records to dynamic records is not supported");
                        }
                        (
                            Input::ExternalRecord(..),
                            Input::Record(..),
                            ValueType::Record(..),
                        ) |
                        (
                            Input::Record(..),
                            Input::ExternalRecord(..),
                            ValueType::ExternalRecord(..),
                        ) => {
                            // This is an admissible type combination which requires no translation
                        }
                        // TODO (dynamic_dispatch): if this check is redundant with other ones already in place, remove it
                        // TODO (dynamic_dispatch): make sure this (and its output counterpart below) do not incorrectly reject valid type combinations; possibly do away with this check altogether
                        _ => {
                            ensure!(
                                Input::variants_match(caller_input, callee_input)
                                    && callee_input.is_type(callee_input_type),
                                "Mismatch between caller input {}, (callee) input {} and (callee) input type {} in transition {} (index: {})",
                                caller_input,
                                callee_input,
                                callee_input_type,
                                transition.id(),
                                input_output_index
                            )
                        }
                    }
                }

                caller_inputs.len()
            } else {
                0
            };

            // Prepare the output translation tasks.
            if let Some(caller_outputs) = transition.caller_outputs() {
                ensure!(
                    caller_outputs.len() == transition.outputs().len(),
                    "The number of caller outputs does not match the number of outputs in transition {}: ({} vs. {})",
                    transition.id(),
                    caller_outputs.len(),
                    transition.outputs().len(),
                );

                let callee_output_types = callee_function_core.output_types();

                ensure!(
                    callee_output_types.len() == transition.outputs().len(),
                    "The number of outputs types does not match the number of outputs in transition {}: ({} vs. {})",
                    transition.id(),
                    callee_output_types.len(),
                    transition.outputs().len(),
                );

                for (input_output_index, (caller_output, callee_output, callee_output_type)) in
                    izip!(caller_outputs.iter(), transition.outputs().iter(), callee_output_types.iter()).enumerate()
                {
                    match (caller_output, callee_output, callee_output_type) {
                        (
                            Output::DynamicRecord(id_dynamic),
                            Output::Record(commitment, _, _, _),
                            ValueType::Record(record_name),
                        ) => {
                            // false
                            let field_is_input = N::Field::zero();
                            // false
                            let field_static_is_external = N::Field::zero();

                            let field_function_id = *callee_function_id;

                            // TODO (dynamic_dispatch) is there a better way to do this? .to_fields() yields one field element
                            // TODO (dynamic_dispatch) separately: should this be to_bits_le or to_bits_be?
                            // TODO (dynamic_dispatch) both TODOs are superseeded by the discussed optimization of making the translation count a field element
                            let fields_translation_count = translation_count
                                .to_bits_le()
                                .into_iter()
                                .map(|bit: bool| if bit { N::Field::one() } else { N::Field::zero() })
                                .collect_vec();
                            let fields_input_output_index = ((input_output_index + num_inputs) as u16)
                                .to_bits_le()
                                .into_iter()
                                .map(|bit: bool| if bit { N::Field::one() } else { N::Field::zero() })
                                .collect_vec();

                            let field_id_static = **commitment;
                            let field_id_dynamic = **id_dynamic;

                            let verifier_inputs = [
                                vec![
                                    // Initial constant 1
                                    N::Field::one(),
                                    field_is_input,
                                    field_static_is_external,
                                    field_function_id,
                                ],
                                fields_translation_count,
                                fields_input_output_index,
                                vec![
                                    field_id_static,
                                    field_id_dynamic
                                ],
                            ]
                            .into_iter()
                            .flatten()
                            .collect_vec();

                            batch_verifier_inputs
                                .entry((*transition.program_id(), *record_name))
                                .or_default()
                                .push(verifier_inputs);

                            translation_count += 1;
                        }
                        (
                            Output::DynamicRecord(id_dynamic),
                            Output::ExternalRecord(id_static),
                            ValueType::ExternalRecord(record_locator),
                        ) => {
                            // false
                            let field_is_input = N::Field::zero();
                            // true
                            let field_static_is_external = N::Field::one();

                            let field_function_id = *callee_function_id;

                            // TODO (dynamic_dispatch) is there a better way to do this? .to_fields() yields one field element
                            // TODO (dynamic_dispatch) separately: should this be to_bits_le or to_bits_be?
                            // TODO (dynamic_dispatch) both TODOs are superseeded by the discussed optimization of making the translation count a field element
                            let fields_translation_count = translation_count
                                .to_bits_le()
                                .into_iter()
                                .map(|bit: bool| if bit { N::Field::one() } else { N::Field::zero() })
                                .collect_vec();
                            let fields_input_output_index = ((input_output_index + num_inputs) as u16)
                                .to_bits_le()
                                .into_iter()
                                .map(|bit: bool| if bit { N::Field::one() } else { N::Field::zero() })
                                .collect_vec();

                            let field_id_static = **id_static;
                            let field_id_dynamic = **id_dynamic;

                            let verifier_inputs = [
                                vec![
                                    // Initial constant 1
                                    N::Field::one(),
                                    field_is_input,
                                    field_static_is_external,
                                    field_function_id,
                                ],
                                fields_translation_count,
                                fields_input_output_index,
                                vec![
                                    field_id_static,
                                    field_id_dynamic
                                ],
                            ]
                            .into_iter()
                            .flatten()
                            .collect_vec();

                            let program_id = record_locator.program_id();
                            let record_name = record_locator.resource();

                            batch_verifier_inputs
                                .entry((*program_id, *record_name))
                                .or_default()
                                .push(verifier_inputs);

                            translation_count += 1;
                        }
                        (Output::Record(..), Output::DynamicRecord(..), ValueType::DynamicRecord) => {
                            bail!("Translation of output dynamic records to (non-external) records is not supported");
                        }
                        (Output::ExternalRecord(..), Output::DynamicRecord(..), ValueType::DynamicRecord) => {
                            bail!("Translation of output dynamic records to (external) records is not supported");
                        }
                        (
                            Output::ExternalRecord(..),
                            Output::Record(..),
                            ValueType::Record(..),
                        ) |
                        (
                            Output::Record(..),
                            Output::ExternalRecord(..),
                            ValueType::ExternalRecord(..),
                        ) => {
                            // This is an admissible type combination which requires no translation
                        }
                        // TODO (dynamic_dispatch): if this check is redundant with other ones already in place, remove it
                        // TODO (dynamic_dispatch): make sure this (and its input counterpart above) do not incorrectly reject valid type combinations; possibly do away with this check altogether
                        _ => {
                            ensure!(
                                Output::variants_match(caller_output, callee_output)
                                    && callee_output.is_type(callee_output_type),
                                "Mismatch between caller output {}, (callee) output {} and (callee) output type {} in transition {} (index: {})",
                                caller_output,
                                callee_output,
                                callee_output_type,
                                transition.id(),
                                input_output_index
                            )
                        }
                    }
                }
            }
        }

        let batch_with_verifying_keys = batch_verifier_inputs
            .into_iter()
            .map(|(key, inputs)| {
                let verifying_key = translation_verifying_keys
                    .get(&key)
                    .ok_or_else(|| anyhow!("Translation verifying key not found for {}/{}", key.0, key.1))?;
                Ok((verifying_key.clone(), inputs))
            })
            .collect::<Result<Vec<(VerifyingKey<N>, Vec<Vec<N::Field>>)>>>()?;

        Ok(batch_with_verifying_keys)
    }
}
