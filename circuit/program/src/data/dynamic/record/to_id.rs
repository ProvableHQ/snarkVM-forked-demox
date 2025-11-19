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

use super::*;

impl<A: Aleo> DynamicRecord<A> {
    /// Returns the ID of the dynamic record.
    pub fn to_id(&self, function_id: Field<A>, tvk: Field<A>, index: U16<A>) -> Field<A> {
        // Construct the preimage as `(function ID || self || tvk || index)`.
        let mut preimage = Vec::new();
        preimage.push(function_id);
        preimage.extend(self.to_fields());
        preimage.push(tvk);
        preimage.push(index.to_field());

        A::hash_psd8(&preimage)
    }
}

#[cfg(test)]
mod tests {

    use console::{TestRng, Uniform};
    use snarkvm_circuit_network::AleoV0 as CurrentAleo;
    use snarkvm_circuit_types::environment::UpdatableCount;

    use super::*;

    type CurrentNetwork = <CurrentAleo as Environment>::Network;

    const ITERATIONS: usize = 50;

    fn test_to_id_with_mode(mode: Mode, count: UpdatableCount, rng: &mut TestRng) {
        for _ in 0..ITERATIONS {
            CurrentAleo::reset();

            // Dynamic record fields
            let owner = console::Address::<CurrentNetwork>::rand(rng);
            let root = console::Field::<CurrentNetwork>::rand(rng);
            let nonce = console::Group::<CurrentNetwork>::rand(rng);
            let version = console::U8::<CurrentNetwork>::rand(rng);

            let console_record =
                console::DynamicRecord::<CurrentNetwork>::new_unchecked(owner, root, nonce, version, None, None);

            // Extra fields when computing a Dynamic record's ID
            let function_id = console::Field::<CurrentNetwork>::rand(rng);
            let tvk = console::Field::<CurrentNetwork>::rand(rng);
            let index = console::U16::<CurrentNetwork>::rand(rng);

            // Circuit record
            let circuit_record = DynamicRecord::<CurrentAleo>::new(mode, console_record.clone());

            // In-circuit extra fields when computing a Dynamic record's ID
            let circuit_function_id = Field::<CurrentAleo>::new(mode, function_id);
            let circuit_tvk = Field::<CurrentAleo>::new(mode, tvk);
            let circuit_index = U16::<CurrentAleo>::new(mode, index);

            let circuit_id = circuit_record.to_id(circuit_function_id, circuit_tvk, circuit_index);

            // Comparing IDs
            let console_id = console_record.to_id(function_id, tvk, index).unwrap();
            assert_eq!(circuit_id.eject_value(), console_id);

            // Checking the count
            count.assert_matches(
                CurrentAleo::num_constants(),
                CurrentAleo::num_public(),
                CurrentAleo::num_private(),
                CurrentAleo::num_constraints(),
            );
        }
    }

    #[test]
    fn test_to_id() {
        let mut rng = TestRng::default();
        test_to_id_with_mode(Mode::Constant, count_is!(27, 1, 2042, 2045), &mut rng);
        test_to_id_with_mode(Mode::Public, count_is!(9, 19, 2057, 2076), &mut rng);
        test_to_id_with_mode(Mode::Private, count_is!(9, 1, 2075, 2076), &mut rng);
    }
}
