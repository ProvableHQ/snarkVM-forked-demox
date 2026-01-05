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

/// Computes the ID of a record (dynamic or external) given its field representation.
/// The ID is computed as `hash_psd8(function_id || record_fields || tvk || index)`.
/// This is shared by dynamic record IDs and external record IDs.
pub fn compute_record_id<A: Aleo>(
    function_id: Field<A>,
    record_fields: Vec<Field<A>>,
    tvk: Field<A>,
    index: Field<A>,
) -> Field<A> {
    // Construct the preimage as `(function ID || record_fields || tvk || index)`.
    let mut preimage = Vec::new();
    preimage.push(function_id);
    preimage.extend(record_fields);
    preimage.push(tvk);
    preimage.push(index);

    A::hash_psd8(&preimage)
}

impl<A: Aleo> DynamicRecord<A> {
    /// Returns the ID of the dynamic record.
    pub fn to_id(&self, function_id: Field<A>, tvk: Field<A>, index: U16<A>) -> Field<A> {
        compute_record_id(function_id, self.to_fields(), tvk, index.to_field())
    }
}

#[cfg(test)]
mod tests {

    use console::{TestRng, Uniform};
    use snarkvm_circuit_network::AleoV0 as CurrentAleo;

    use super::*;

    type CurrentNetwork = <CurrentAleo as Environment>::Network;

    const ITERATIONS: usize = 50;

    fn test_to_id_with_mode(mode: Mode) {
        let mut rng = TestRng::default();

        for _ in 0..ITERATIONS {
            // Dynamic record fields
            let owner_address = console::Address::<CurrentNetwork>::rand(&mut rng);
            let owner = console::Owner::<CurrentNetwork, console::Plaintext<CurrentNetwork>>::Public(owner_address);
            let root = console::Field::<CurrentNetwork>::rand(&mut rng);
            let nonce = console::Group::<CurrentNetwork>::rand(&mut rng);
            let version = console::U8::<CurrentNetwork>::rand(&mut rng);

            let console_record =
                console::DynamicRecord::<CurrentNetwork>::new_unchecked(*owner, root, nonce, version, None);

            // Extra fields when computing a Dynamic record's ID
            let function_id = console::Field::<CurrentNetwork>::rand(&mut rng);
            let tvk = console::Field::<CurrentNetwork>::rand(&mut rng);
            let index = console::U16::<CurrentNetwork>::rand(&mut rng);

            // Circuit record
            let circuit_record = DynamicRecord::<CurrentAleo>::new(mode, console_record.clone());

            // In-circuit extra fields when computing a Dynamic record's ID
            let circuit_function_id = Field::<CurrentAleo>::new(mode, function_id);
            let circuit_tvk = Field::<CurrentAleo>::new(mode, tvk);
            let circuit_index = U16::<CurrentAleo>::new(mode, index);

            let circuit_id = circuit_record.to_id(circuit_function_id.clone(), circuit_tvk.clone(), circuit_index);

            // Comparing IDs
            let console_id = console_record.to_id(function_id, tvk, index).unwrap();
            assert_eq!(circuit_id.eject_value(), console_id);

            // Test compute_record_id produces the same result.
            let index_field = console::Field::<CurrentNetwork>::from_u16(*index);
            let circuit_index_field = Field::<CurrentAleo>::new(mode, index_field);
            let circuit_id_field =
                compute_record_id(circuit_function_id, circuit_record.to_fields(), circuit_tvk, circuit_index_field);
            assert_eq!(circuit_id_field.eject_value(), console_id);
        }
    }

    #[test]
    fn test_to_id() {
        test_to_id_with_mode(Mode::Constant);
        test_to_id_with_mode(Mode::Public);
        test_to_id_with_mode(Mode::Private);
    }
}
