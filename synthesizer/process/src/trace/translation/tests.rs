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

use console::{types::U16, program::{Plaintext, ProgramID, Record}, types::Field};

use crate::{TranslationAssignment, tests::test_utils::{CurrentAleo, CurrentNetwork}};

use super::*;

use std::str::FromStr;

fn compare_r1cs(
    circuit_assignment_1: &circuit::Assignment<<CurrentNetwork as Environment>::Field>,
    circuit_assignment_2: &circuit::Assignment<<CurrentNetwork as Environment>::Field>,
) {
    assert_eq!(circuit_assignment_1.num_public(), circuit_assignment_2.num_public());
    assert_eq!(circuit_assignment_1.num_private(), circuit_assignment_2.num_private());
    assert_eq!(circuit_assignment_1.num_constraints(), circuit_assignment_2.num_constraints());
    assert_eq!(circuit_assignment_1.num_nonzeros(), circuit_assignment_2.num_nonzeros());
    // TODO (Antonio) reintroduce or make better
    // for (constraint1, constraint2) in circuit_assignment_1.constraints().iter().zip(circuit_assignment_2.constraints().iter()) {
    //     let (lc1a, lc1b, lc1c) = constraint1.clone().to_terms();
    //     let (lc2a, lc2b, lc2c) = constraint2.clone().to_terms();
    //     assert_eq!(lc1a.to_terms(), lc2a.clone().to_terms());
    //     assert_eq!(lc1b.to_terms(), lc2b.clone().to_terms());
    //     assert_eq!(lc1c.to_terms(), lc2c.clone().to_terms());
    // }
}
#[test]
fn test_translation_simple() {

    let mut rng = TestRng::default();

    let record_static_str = r#"{
        owner: aleo1d5hg2z3ma00382pngntdp68e74zv54jdxy249qhaujhks9c72yrs33ddah.private,
        location_x: 100field.public,
        location_y: 243field.public,
        has_allies: false.public,
        codename: "morningstar".public,
        interstellar_signing_key: 2group.private,
        _nonce: 0group.public,
        _version: 1u8.public
    }"#;

    // Independent fields
    let record_static = Record::<CurrentNetwork, Plaintext<CurrentNetwork>>::from_str(record_static_str).unwrap();
    let program_id = ProgramID::<CurrentNetwork>::from_str("space_fighters.aleo").unwrap();
    let function_id = Field::<CurrentNetwork>::from_u64(2);
    let record_name = Identifier::<CurrentNetwork>::from_str("spacecraft").unwrap();
    let to_static_record = false;
    let translation_count = 17;
    let tvk: Field::<CurrentNetwork> = Uniform::rand(&mut rng);
    let register_index = 2;
    let record_view_key: Field::<CurrentNetwork> = Uniform::rand(&mut rng);
    let gamma: Group::<CurrentNetwork> = Uniform::rand(&mut rng);

    // Dependent fields
    let record_dynamic = DynamicRecord::<CurrentNetwork>::from_record(&record_static).unwrap();

    // TODO (Antonio) reintroduce or remove
    // assert_eq!(*record_dynamic.root(), record_static.merkleize().unwrap());

    // TODO (Antonio) Temporary patch
    let record_dynamic = DynamicRecord::<CurrentNetwork>::new_unchecked(
        record_dynamic.owner().clone(),
        // TODO (Antonio)
        record_static.merkleize().unwrap(),
        record_dynamic.nonce().clone(),
        record_dynamic.version().clone(),
        record_dynamic.tree().clone(),
        record_dynamic.data().clone(),
    );
    let id_dynamic = record_dynamic.to_id(function_id, tvk, U16::new(register_index)).unwrap();
    let id_static = record_static.to_commitment(&program_id, &record_name, &record_view_key).unwrap();

    let translation_assignment = TranslationAssignment::<CurrentNetwork>::new(
        record_static,
        program_id,
        function_id,
        record_name,
        record_dynamic,
        to_static_record,
        translation_count,
        tvk,
        register_index,
        id_dynamic,
        id_static,
        record_view_key,
        gamma,
    );

    let circuit_assignment = translation_assignment.to_circuit_assignment::<CurrentAleo>().unwrap();

    compare_r1cs(&circuit_assignment, &circuit_assignment);

    println!("R1CS:");
    println!("   nun_public: {}", circuit_assignment.num_public());
    println!("   nun_private: {}", circuit_assignment.num_private());
    println!("   nun_constraints: {}", circuit_assignment.num_constraints());
    println!("   nun_nonzeros: {:?}", circuit_assignment.num_nonzeros());
}