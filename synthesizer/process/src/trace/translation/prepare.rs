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

use circuit::Assignment;
use snarkvm_synthesizer_program::Program;
use snarkvm_synthesizer_snark::ProvingKey;

use super::*;

impl<N: Network> Translation<N> {
    /// Returns the translation assignments for the given transitions.
    pub fn prepare(
        &self,
        transitions: &[Transition<N>],
        call_graph: &HashMap<N::TransitionID, Vec<N::TransitionID>>,
    ) -> Result<Vec<(ProvingKey<N>, Vec<TranslationAssignment<N>>)>> {

        // Initialize a vector for the batched assignments.
        let mut batched_inputs_inputs: HashMap<(ProgramID<N>, Identifier<N>), (ProvingKey<N>, Vec<TranslationAssignment<N>>)> = HashMap::new();

        let mut translation_count = 0;

        // TODO (dynamic_dispatch) so far we only cover translation case 1: input dynamic -> static
        for ((transition_id, caller_dynamic_record_id), index) in self.transition_indices.iter() {
            if let Some((translation_tasks, record_translation_data)) = self.translation_tasks.get(transition_id) {
                ensure!(
                    translation_tasks.len() == record_translation_data.len(),
                    "The number of translation tasks for transition {} does not match the number of record translation data ({} vs. {})", transition_id, translation_tasks.len(),
                    record_translation_data.len()
                );

                // Identify which translation task corresopnds to the marked translation in translation_indices
                let mut found = translation_tasks.iter().zip(record_translation_data.iter()).filter(|(_, data)| data.input_output_index == *index).collect_vec();
            
                ensure!(found.len() != 0, "No translation task and data found for transition {} marked for translation", transition_id);
                ensure!(found.len() <= 1, "Multiple translation tasks and data found for transition {} marked for translation", transition_id);

                let (identified_task, identified_data) = found.pop().unwrap();

                let TranslationTask { commitment, gamma, serial_number, record } = identified_task;
                let RecordTranslationData { record_static, program_id, function_id, record_name, record_consumed, tvk, record_view_key, gamma: gamma_data, static_record_id, input_output_index, proving_key } = identified_data;
                
                // Checks associated to translation case 1
                ensure!(gamma_data.as_ref() == Some(gamma), "gamma value in translation task does not that in translation data for transition ID {} and register index {}", transition_id, index);
                ensure!(!record_consumed, "Expected record_consumed = false in translation data for transition ID {} and register index {}", transition_id, index);
                ensure!(input_output_index == index, "Expected register index in translation data to be the same as the index in translation tasks for transition ID {} and register index {}", transition_id, index);

                // Preparing the TranslationAssignment data
                let record_dynamic = DynamicRecord::<N>::from_record(&record_static)?;
                let record_consumed = false;
                let input_output_index = *index;
                let id_static = static_record_id;
                let id_dynamic = caller_dynamic_record_id;

                // TODO (dynamic_dispatch): is the clone cheap?
                let batch = batched_inputs_inputs.entry((*program_id, *record_name)).or_insert((proving_key.clone(), vec![]));

                batch.1.push(TranslationAssignment::new(
                    record_static.clone(),
                    program_id.clone(),
                    function_id.clone(),
                    record_name.clone(),
                    record_dynamic.clone(),
                    record_consumed,
                    translation_count,
                    tvk.clone(),
                    input_output_index,
                    id_dynamic.clone(),
                    id_static.clone(),
                    record_view_key.clone(),
                    gamma.clone(),
                ));

                translation_count += 1;
            } else {
                bail!("Translation tasks and data not found for transition {} marked for translation", transition_id);
            }
        }

        // Discard the program_id and record_name and return the results.
        Ok(batched_inputs_inputs.into_iter().map(|(_, value)| (value.0, value.1)).collect())
    }

    // TODO (dynamic_dispatch) should this really be the same as prepare?
    /// Returns the inclusion assignments for the given transitions.
    #[cfg(feature = "async")]
    pub async fn prepare_async(
        &self,
        transitions: &[Transition<N>],
        call_graph: &HashMap<N::TransitionID, Vec<N::TransitionID>>,
    ) -> Result<Vec<(ProvingKey<N>, Vec<TranslationAssignment<N>>)>> {
        self.prepare(transitions, call_graph)
    }
}
