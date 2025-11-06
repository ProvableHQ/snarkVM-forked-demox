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

use console::Network;
use console_root::prelude::MainnetV0;
use snarkvm_circuit_network::AleoV0;
use snarkvm_console_account::{ToField as ConsoleToField, ToFields as ConsoleToFields};

type CurrentNetwork = MainnetV0;
type CurrentAleo = AleoV0;

#[test]
fn test_merkleize_simple() {

    let psd2 = CurrentNetwork::hash_psd2;

    let record_str = r#"{
    owner: aleo1d5hg2z3ma00382pngntdp68e74zv54jdxy249qhaujhks9c72yrs33ddah.private,
    location_x: 100field.public,
    location_y: 243field.public,
    has_allies: false.public,
    codename: "morningstar".public,
    interstellar_signing_key: 2group.private,
    _nonce: 0group.public,
    _version: 1u8.public
}"#;
    
    let record = console::Record::<CurrentNetwork, console::Plaintext<CurrentNetwork>>::from_str(record_str).unwrap();

    // This padding value should be consistent with Record::merkleize()
    let path_hasher = Poseidon2::<CurrentNetwork>::setup("DynamicRecordPathHasher").unwrap();
    let padding_hash = path_hasher.hash_empty().unwrap();
        
    // Computing the data root by manually using console types
    let leaves = record.data().iter().map(|(identifier, entry)| {
        let mut hash_input = vec![identifier.to_field().unwrap()];
        hash_input.extend(entry.to_fields().unwrap());
        CurrentNetwork::hash_psd8(hash_input.as_slice()).unwrap()
    }).collect_vec();

    let depth = CurrentNetwork::MAX_DATA_ENTRIES.ilog2();
    assert!(depth >= 3, "For this concrete test, the depth must be at least 3 (4 < 5 = num entries <= 8), got: {depth}");

    // We pad and hash each level by hand
    let lvl_d_minus_1 = vec![
        psd2(&[leaves[0], leaves[1]]).unwrap(),
        psd2(&[leaves[2], leaves[3]]).unwrap(),
        psd2(&[leaves[4], padding_hash]).unwrap(),
        padding_hash,
    ];

    let lvl_d_minus_2 = vec![
        psd2(&[lvl_d_minus_1[0], lvl_d_minus_1[1]]).unwrap(),
        psd2(&[lvl_d_minus_1[2], lvl_d_minus_1[3]]).unwrap(),
    ];

    let lvl_d_minus_3 = vec![
        psd2(&[lvl_d_minus_2[0], lvl_d_minus_2[1]]).unwrap(),
    ];

    // Compute remaining levels up to the root
    let mut data_root = lvl_d_minus_3[0];
    
    for _ in 0..depth - 3 {
        data_root = psd2(&[data_root, padding_hash]).unwrap();
    }

    // Merkleizing the data in-circuit

    let circuit_record = Record::<CurrentAleo, Plaintext<CurrentAleo>>::new(Mode::Private, record);

    let actual_data_root = circuit_record.merkleize().unwrap();

    // Checking correctness

    assert_eq!(actual_data_root.eject_value(), data_root);
}


#[test]
fn test_merkleize_with_structs() {

    let psd2 = CurrentNetwork::hash_psd2;

    let record_str = r#"{
        owner: aleo1d5hg2z3ma00382pngntdp68e74zv54jdxy249qhaujhks9c72yrs33ddah.private,
        location_x: 100field.public,
        location_y: 243field.public,
        has_allies: false.public,
        codename: "morningstar".public,
        num_crew: 9u64.public,
        stealth_mode: false.private,
        resources: {
            food: 90u32.private,
            spice: 23918u32.private
        },
        targets: {
            main: {
                name: "AlphaCentauri".private,
                star: true.private,
                interconnected: true.private
            },
            secondary: {
                name: "Earth".private,
                star: false.private,
                interconnected: false.private
            }
        },
        interstellar_signing_key: 2group.private,
        _nonce: 0group.public,
        _version: 1u8.public
    }"#;
    
    let record = console::Record::<CurrentNetwork, console::Plaintext<CurrentNetwork>>::from_str(record_str).unwrap();

    // Computing the data root by manually using console types

    // This padding value should be consistent with Record::merkleize()
    let path_hasher = Poseidon2::<CurrentNetwork>::setup("DynamicRecordPathHasher").unwrap();
    let padding_hash = path_hasher.hash_empty().unwrap();

    let mut level = record.data().iter().map(|(identifier, entry)| {
        let mut hash_input = vec![identifier.to_field().unwrap()];
        hash_input.extend(entry.to_fields().unwrap());
        CurrentNetwork::hash_psd8(hash_input.as_slice()).unwrap()
    }).collect_vec();

    assert_eq!(level.len(), 9, "Structure entries should only result in one leaf");

    let depth = CurrentNetwork::MAX_DATA_ENTRIES.ilog2();
    assert!(depth >= 4, "For this concrete test, the depth must be at least 4 (8 < 9 = num entries <= 16), got: {depth}");

    // We pad and hash each level in a loop
    for _ in 0..depth {
        if level.len() % 2 == 1 {
            level.push(padding_hash.clone());
        }

        let next_level = level.chunks_exact(2).map(|left_and_right| {
            psd2(left_and_right).unwrap()
        }).collect_vec();
        level = next_level;
    }

    let data_root = level[0];

    // Merkleizing the data in-circuit

    let circuit_record = Record::<CurrentAleo, Plaintext<CurrentAleo>>::new(Mode::Private, record);

    let actual_data_root = circuit_record.merkleize().unwrap();

    // Checking correctness

    assert_eq!(actual_data_root.eject_value(), data_root);
}

// Checks consistency between console and circuit implementations of merkleization
#[test]
fn test_merkleize_consistency() {

    let record_str = r#"{
        owner: aleo1d5hg2z3ma00382pngntdp68e74zv54jdxy249qhaujhks9c72yrs33ddah.private,
        location_x: 100field.public,
        location_y: 243field.public,
        has_allies: false.public,
        codename: "morningstar".public,
        num_crew: 9u64.public,
        stealth_mode: false.private,
        resources: {
            food: 90u32.private,
            spice: 23918u32.private
        },
        targets: {
            main: {
                name: "AlphaCentauri".private,
                star: true.private,
                interconnected: true.private
            },
            secondary: {
                name: "Earth".private,
                star: false.private,
                interconnected: false.private
            }
        },
        interstellar_signing_key: 2group.private,
        _nonce: 0group.public,
        _version: 1u8.public
    }"#;

    let console_record = console::Record::<CurrentNetwork, console::Plaintext<CurrentNetwork>>::from_str(record_str).unwrap();
    let console_data_root = console_record.merkleize().unwrap();
    
    let circuit_record = Record::<CurrentAleo, Plaintext<CurrentAleo>>::new(Mode::Private, console_record);    
    let circuit_data_root = circuit_record.merkleize().unwrap();
    
    assert_eq!(circuit_data_root.eject_value(), console_data_root);
}