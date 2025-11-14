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

#[cfg(feature = "async")]
use snarkvm_synthesizer_snark::ProvingKey;

use super::*;

macro_rules! prepare_impl {
    ($self:ident, $transitions:ident, $query:ident, $current_state_root:ident, $current_block_height:ident, $get_state_paths_for_commitments:ident $(, $await:ident)?) => {{

        // Initialize a vector for the batch verifier inputs.
        let mut batch_prover_inputs: HashMap<(ProgramID<N>, Identifier<N>), Vec<TranslationAssignment<N>>> = HashMap::new();

        let network_id = U16::new(N::ID);

        let mut translation_count = 0;

        // Initialize a vector for the assignments.
        let mut assignments = vec![];

        // Retrieve the current block height.
        let current_block_height = {
            $query.$current_block_height()
            $(.$await)?
        }?;

        // Determine which consensus version is being used.
        let consensus_version = N::CONSENSUS_VERSION(current_block_height)?;

        for transition in $transitions.iter() {

            let program_id = transition.program_id().clone();

            // TODO this needs to be accessible.
            let tvk = Field::<N>::zero();

            // TODO (dynamic_dispatch) easier way to access this?
            // TODO (dynamic_dispatch) should we distinguish caller/callee?
            let function_id = compute_function_id(&network_id, &program_id, transition.function_name(), transition.is_dynamic())?;
    
            for (register_index, input) in transition.inputs().iter().enumerate() {
                match input {
                    Input::Record(serial_number, tag) => {
                        // TODO (dynamic_dispatch) construct the translations object; it could contain pointerse to records in other transactions if we're okay with introducing a lifetime
                        let Some((
                            record_static,
                            static_record_name,
                            record_view_key,
                            gamma_option
                        )) = transition.translations().get(translation_count) else {
                            bail!("Translation data {translation_count} for transition {} (case 1) not found", transition.id());
                        };

                        let to_static_record = true;
                        
                        let commitment = record_static.to_commitment(&program_id, &static_record_name, &record_view_key).unwrap();

                        let Some(gamma) = gamma_option else {
                            bail!("Translation {translation_count} for transition {} (case 1) consumes a static record, but no gamma was supplied", transition.id());
                        };

                        ensure!(
                            Record::<N, Plaintext<N>>::serial_number_from_gamma(&gamma, commitment).unwrap() == *serial_number,
                            "In translation {translation_count} for transition {} (case 1), the serial number of the static record does not match the expected value", transition.id());

                        let record_dynamic = DynamicRecord::<N>::from_record(&record_static)?;

                        let id_dynamic = record_dynamic.to_id(function_id, tvk, U16::new(register_index as u16)).unwrap();

                        // TODO (dynamic_dispatch) fix this if necessary
                        let child_program_id = program_id.clone();
                        
                        batch_prover_inputs.entry((child_program_id, *static_record_name)).or_default().push(TranslationAssignment::new(
                            record_static,
                            program_id,
                            function_id,
                            static_record_name,
                            record_dynamic,
                            to_static_record,
                            translation_count,
                            tvk,
                            // TODO (dynamic_dispatch) is this the correct register index?
                            register_index as u16,
                            id_dynamic,
                            *serial_number,
                            record_view_key,
                            gamma,
                        ));

                        translation_count += 1;
                    }
                }
            }

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

        Ok(assignments)
    }};
}

impl<N: Network> Translation<N> {
    /// Returns the translation assignments for the given transitions.
    pub fn prepare(
        &self,
        transitions: &[Transition<N>],
        query: &dyn QueryTrait<N>,
    ) -> Result<Vec<(VerifyingKey<N>, Vec<TranslationAssignment<N>>)>> {
        prepare_impl!(
            self,
            transitions,
            query,
            current_state_root,
            current_block_height,
            get_state_paths_for_commitments
        )
    }

    /// Returns the inclusion assignments for the given transitions.
    #[cfg(feature = "async")]
    pub async fn prepare_async(
        &self,
        transitions: &[Transition<N>],
        query: &dyn QueryTrait<N>,
    ) -> Result<Vec<(VerifyingKey<N>, Vec<TranslationAssignment<N>>)>> {
        prepare_impl!(
            self,
            transitions,
            query,
            current_state_root_async,
            current_block_height_async,
            get_state_paths_for_commitments_async,
            await
        )
    }
}
