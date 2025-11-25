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

use console::types::U16;
use snarkvm_synthesizer_snark::ProvingKey;

use super::*;

impl<N: Network> Translation<N> {
    /// Returns the translation assignments for the given transitions.
    pub fn prepare(
        &self,
        transitions: &[Transition<N>],
        // TODO (dynamic_dispatch) Consider using pointers or Arcs to proving keys
    ) -> Result<Vec<(ProvingKey<N>, Vec<TranslationAssignment<N>>)>> {
        // Initialize a vector for the batched assignments.
        let mut batched_assignments: HashMap<(ProgramID<N>, Identifier<N>), Vec<TranslationAssignment<N>>> =
            HashMap::new();
        let mut proving_keys: HashMap<(ProgramID<N>, Identifier<N>), ProvingKey<N>> = HashMap::new();

        let mut translation_count = 0;

        // TODO (dynamic_dispatch) so far we only cover translation case 1: input dynamic -> static
        // Traversal order only affects the translation count appearing as a public input in the translation circuit.
        // Order is irrelevant as long as it is consistent between the prover and verifier. (cf. Translation::prepare_verifier_inputs)
        for transition in transitions {
            let transition_id = transition.id();

            let Some(translation_tasks) = self.translation_tasks.get(transition_id) else {
                bail!("Translation tasks not found for transition ID {}", transition_id);
            };

            for translation_task in translation_tasks {
                let RecordTranslationData {
                    translation_proving_key,
                    record_dynamic,
                    record_static,
                    program_id,
                    function_id,
                    record_name,
                    is_input,
                    static_is_external,
                    tvk,
                    record_view_key,
                    gamma,
                    id_static,
                    id_dynamic,
                    input_output_index,
                } = translation_task;

                // TODO (dynamic_dispatch) add here consistency checks with the Transition object?
                let Some(record_view_key_value) = record_view_key.as_ref() else {
                    bail!(
                        "record_view_key is None in record translation for transition ID {} and index {}",
                        transition_id,
                        input_output_index
                    );
                };

                // Checks associated to input-record translation
                let batch = &mut batched_assignments.entry((*program_id, *record_name)).or_insert(vec![]);

                if let Some(previous_key) = proving_keys.get(&(*program_id, *record_name)) {
                    ensure!(
                        previous_key == translation_proving_key,
                        "Proving key mismatch for record {}/{}",
                        program_id,
                        record_name
                    );
                } else {
                    proving_keys.insert((*program_id, *record_name), translation_proving_key.clone());
                }

                batch.push(TranslationAssignment::new(
                    record_static.clone(),
                    record_dynamic.clone(),
                    program_id.clone(),
                    function_id.clone(),
                    record_name.clone(),
                    *is_input,
                    *static_is_external,
                    translation_count,
                    tvk.clone(),
                    *input_output_index,
                    *id_dynamic,
                    *id_static,
                    record_view_key_value.clone(),
                    *gamma,
                ));

                translation_count += 1;
            }
        }

        // Replace program ID + record name by proving key
        Ok(batched_assignments
            .into_iter()
            .map(|(key, value)| (proving_keys.get(&key).unwrap().clone(), value))
            .collect())
    }

    // TODO (dynamic_dispatch) should this really be the same as prepare?
    /// Returns the inclusion assignments for the given transitions.
    #[cfg(feature = "async")]
    pub async fn prepare_async(
        &self,
        transitions: &[Transition<N>],
    ) -> Result<Vec<(ProvingKey<N>, Vec<TranslationAssignment<N>>)>> {
        self.prepare(transitions)
    }
}
