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

mod assignment;
pub use assignment::{TranslationAssignment, compute_console_external_record_id};

mod prepare;

#[cfg(test)]
mod tests;

use crate::Stack;

use circuit::{Inject, traits::ToGroup};

use console::{
    network::prelude::*,
    program::{DynamicRecord, Identifier, Plaintext, ProgramID, Record, U16, ValueType, compute_function_id},
    types::{Field, Group},
};
use snarkvm_ledger_block::{Input, Output, Transition};
use snarkvm_synthesizer_program::Function;
use snarkvm_synthesizer_snark::VerifyingKey;

use std::collections::HashMap;

/// Data collected during execution to prove record translation in dynamic calls.
/// It largely mirrors the `TranslationAssignment` struct in this module.
#[derive(Clone, Debug)]
pub struct RecordTranslationData<N: Network> {
    /// The static record.
    pub record_static: Record<N, Plaintext<N>>,
    /// The dynamic record.
    pub record_dynamic: DynamicRecord<N>,
    /// The ID of the program where the static record is defined (whether external or not).
    pub program_id: ProgramID<N>,
    /// The function ID of the callee in the dynamic call.
    pub function_id: Field<N>,
    /// The name of the static record.
    pub record_name: Identifier<N>,
    /// True if translation is happening for an input to `dynamic.call` (static record is being produced)
    /// or an output of `dynamic.call` (static record is being consumed).
    pub is_input: bool,
    /// Whether the value type corresponding to the static record is `Record` or `ExternalRecord`.
    pub static_is_external: bool,
    /// The view key of the transition containing the dynamic call.
    pub tvk: Field<N>,
    /// The record view key of the static record. Irrelevant if `static_is_external` is true.
    pub record_view_key: Option<Field<N>>,
    /// The additional point used to produce the serial number.
    /// Irrelevant if `is_input` is false or `static_is_external` is true.
    pub gamma: Option<Group<N>>,
    /// Index of the input operand or output destination that contains the (dynamic and static) record.
    /// Note: The first three dynamic.call operands are reserved for call-related data,
    /// however this operand index still starts at 0 and is the same for caller and callee.
    pub input_output_index: u16,
    /// The ID of the dynamic record.
    pub id_dynamic: Field<N>,
    /// The ID of the static record:
    /// - If the static record is external, this is its `InputID` = `OutputID`.
    /// - If the static record is not external, this is:
    ///   - Its `InputID`, i.e. its serial number, if the record is an input.
    ///   - Its `OutputID`, i.e. its commitment, if the record is an output.
    pub id_static: Field<N>,
}

impl<N: Network> RecordTranslationData<N> {
    /// Creates a new `RecordTranslationData` instance.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        record_static: Record<N, Plaintext<N>>,
        record_dynamic: DynamicRecord<N>,
        program_id: ProgramID<N>,
        function_id: Field<N>,
        record_name: Identifier<N>,
        is_input: bool,
        static_is_external: bool,
        tvk: Field<N>,
        record_view_key: Option<Field<N>>,
        gamma: Option<Group<N>>,
        input_output_index: u16,
        id_dynamic: Field<N>,
        id_static: Field<N>,
    ) -> Self {
        Self {
            record_static,
            record_dynamic,
            program_id,
            function_id,
            record_name,
            is_input,
            static_is_external,
            tvk,
            record_view_key,
            gamma,
            input_output_index,
            id_dynamic,
            id_static,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Translation<N: Network> {
    /// A map of `transition IDs` to a list of `input tasks`. Only contains
    /// entries for transitions that involve translation.
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
        record_translation_data: Vec<RecordTranslationData<N>>,
    ) -> Result<()> {
        self.translation_tasks.insert(transition_id, record_translation_data);

        Ok(())
    }

    /// Returns the verifier public inputs for the given call graph and transitions.
    pub fn prepare_verifier_inputs<'a, F>(
        transitions: impl ExactSizeIterator<Item = &'a Transition<N>>,
        // Used to retrieve record names
        transition_map: &HashMap<N::TransitionID, (&Transition<N>, Function<N>)>,
        get_translation_verifying_key: &F,
    ) -> Result<Vec<(VerifyingKey<N>, Vec<Vec<N::Field>>)>>
    where
        F: Fn(&(ProgramID<N>, Identifier<N>)) -> Result<VerifyingKey<N>>,
    {
        let mut batch_verifier_inputs: HashMap<(ProgramID<N>, Identifier<N>), Vec<Vec<N::Field>>> = HashMap::new();

        let mut translation_index = 0;

        // Traversal order affects the translation count as well as the internal order of each batch input to proving/verification.
        // Order is irrelevant as long as it is consistent between the prover and verifier. (cf. Translation::prepare)
        for transition in transitions {
            let (_, callee_function_core) = transition_map
                .get(transition.id())
                .ok_or_else(|| anyhow!("Transition {} from execution not found transition map", transition.id()))?;
            let callee_function_id =
                compute_function_id(&U16::<N>::new(N::ID), transition.program_id(), transition.function_name())?;

            let callee_input_types = callee_function_core.input_types();
            let num_inputs = transition.inputs().len();

            // Prepare the input translation tasks.
            // Detect inputs that carry a dynamic_id (RecordWithDynamicID, ExternalRecordWithDynamicID).
            for (input_output_index, (input, callee_input_type)) in
                transition.inputs().iter().zip(callee_input_types.iter()).enumerate()
            {
                // Only process inputs that have a dynamic_id.
                let Some(dynamic_id) = input.dynamic_id() else { continue };

                // Construct the translation count as a field element.
                let field_translation_index = *Field::<N>::from_u128(translation_index as u128);
                // Construct the input output index as a field element.
                let field_input_output_index = *Field::<N>::from_u128(input_output_index as u128);

                let field_is_input = N::Field::one();
                let field_function_id = *callee_function_id;
                let field_id_static = **input.id();
                let field_id_dynamic = **dynamic_id;

                match (input, callee_input_type) {
                    (Input::RecordWithDynamicID(..), ValueType::Record(record_name)) => {
                        let field_static_is_external = N::Field::zero();
                        let verifier_inputs = vec![
                            N::Field::one(),
                            field_is_input,
                            field_static_is_external,
                            field_function_id,
                            field_translation_index,
                            field_input_output_index,
                            field_id_static,
                            field_id_dynamic,
                        ];
                        batch_verifier_inputs
                            .entry((*transition.program_id(), *record_name))
                            .or_default()
                            .push(verifier_inputs);
                        translation_index += 1;
                    }
                    (Input::ExternalRecordWithDynamicID(..), ValueType::ExternalRecord(record_locator)) => {
                        let field_static_is_external = N::Field::one();
                        let verifier_inputs = vec![
                            N::Field::one(),
                            field_is_input,
                            field_static_is_external,
                            field_function_id,
                            field_translation_index,
                            field_input_output_index,
                            field_id_static,
                            field_id_dynamic,
                        ];
                        batch_verifier_inputs
                            .entry((*record_locator.program_id(), *record_locator.resource()))
                            .or_default()
                            .push(verifier_inputs);
                        translation_index += 1;
                    }
                    _ => bail!(
                        "Unexpected input variant with dynamic_id in transition {} (index: {})",
                        transition.id(),
                        input_output_index
                    ),
                }
            }

            let callee_output_types = callee_function_core.output_types();

            // Prepare the output translation tasks.
            // Detect outputs that carry a dynamic_id (RecordWithDynamicID, ExternalRecordWithDynamicID).
            for (input_output_index, (output, callee_output_type)) in
                transition.outputs().iter().zip(callee_output_types.iter()).enumerate()
            {
                // Only process outputs that have a dynamic_id.
                let Some(dynamic_id) = output.dynamic_id() else { continue };

                // Construct the translation count as a field element.
                let field_translation_index = *Field::<N>::from_u128(translation_index as u128);
                // Construct the input output index as a field element.
                let field_input_output_index = *Field::<N>::from_u128((num_inputs + input_output_index) as u128);

                let field_is_input = N::Field::zero();
                let field_function_id = *callee_function_id;
                let field_id_static = **output.id();
                let field_id_dynamic = **dynamic_id;

                match (output, callee_output_type) {
                    (Output::RecordWithDynamicID(..), ValueType::Record(record_name)) => {
                        let field_static_is_external = N::Field::zero();
                        let verifier_inputs = vec![
                            N::Field::one(),
                            field_is_input,
                            field_static_is_external,
                            field_function_id,
                            field_translation_index,
                            field_input_output_index,
                            field_id_static,
                            field_id_dynamic,
                        ];
                        batch_verifier_inputs
                            .entry((*transition.program_id(), *record_name))
                            .or_default()
                            .push(verifier_inputs);
                        translation_index += 1;
                    }
                    (Output::ExternalRecordWithDynamicID(..), ValueType::ExternalRecord(record_locator)) => {
                        let field_static_is_external = N::Field::one();
                        let verifier_inputs = vec![
                            N::Field::one(),
                            field_is_input,
                            field_static_is_external,
                            field_function_id,
                            field_translation_index,
                            field_input_output_index,
                            field_id_static,
                            field_id_dynamic,
                        ];
                        batch_verifier_inputs
                            .entry((*record_locator.program_id(), *record_locator.resource()))
                            .or_default()
                            .push(verifier_inputs);
                        translation_index += 1;
                    }
                    _ => bail!(
                        "Unexpected output variant with dynamic_id in transition {} (index: {})",
                        transition.id(),
                        input_output_index
                    ),
                }
            }
        }

        let batch_with_verifying_keys = batch_verifier_inputs
            .into_iter()
            .map(|(key, inputs)| Ok((get_translation_verifying_key(&key)?, inputs)))
            .collect::<Result<Vec<(VerifyingKey<N>, Vec<Vec<N::Field>>)>>>()?;

        Ok(batch_with_verifying_keys)
    }
}
