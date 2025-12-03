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

// These tests mix translation, casting to dynamic.record and get.dynamic.record.

// This test checks that execution_cost_for_authorization() computes the correct
// cost in transactions involving inclusion and translation proofs. This is in
// addition to all test cases in synthesizer/tests/test_vm_execute_and_finalize.rs,
// which also assert the correctness of cost estimation.
#[test]
fn test_execution_cost_for_authorization() {

    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key).unwrap();
    let caller_address_str = caller_address.to_string();
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(caller_private_key).unwrap();

    let program_a_str = "welder";
    let program_b_str = "payment_checker";
    let network_str = "aleo";
    let weld_function_str = "weld";
    let check_tossed_coin_str = "check_tossed_coin";

    let program_a_field = Identifier::<CurrentNetwork>::from_str(program_a_str).unwrap().to_field().unwrap();
    let program_b_field = Identifier::<CurrentNetwork>::from_str(program_b_str).unwrap().to_field().unwrap();
    let network_field = Identifier::<CurrentNetwork>::from_str(network_str).unwrap().to_field().unwrap();
    let weld_function_field = Identifier::<CurrentNetwork>::from_str(weld_function_str).unwrap().to_field().unwrap();
    let check_tossed_coin_field = Identifier::<CurrentNetwork>::from_str(check_tossed_coin_str).unwrap().to_field().unwrap();
    
    let program_a_string = format!(
        r"
        program {program_a_str}.aleo;

        record base_metal:
            owner as address.private;
            metal_id as u16.public;
            grams as u32.private;
            weldable as boolean.public;
        
        record accessory_metal:
            owner as address.private;
            metal_id as u16.public;
            grams as u32.private;
            signature_val as group.public;
        
        struct welding_pair:
            base_metal_id as u16;
            accessory_metal_id as u16;

        record welding_metal:
            owner as address.private;
            welds as welding_pair.public;

        record welded_chunk:
            owner as address.private;
            grams as u32.private;
        
        function mint_base_metal:
            input r0 as address.private;
            input r1 as u16.public;
            input r2 as u32.private;
            input r3 as boolean.public;

            cast r0 r1 r2 r3 into r4 as base_metal.record;

            output r4 as base_metal.record;

        function mint_accessory_metal:
            input r0 as address.private;
            input r1 as u16.public;
            input r2 as u32.private;
            input r3 as group.public;

            cast r0 r1 r2 r3 into r4 as accessory_metal.record;

            output r4 as accessory_metal.record;

        function mint_welding_metal:
            input r0 as address.private;
            input r1 as u16.public;
            input r2 as u16.public;

            cast r1 r2 into r3 as welding_pair;

            cast r0 r3 into r4 as welding_metal.record;

            output r4 as welding_metal.record;

        function {weld_function_str}:
            input r0 as base_metal.record;
            input r1 as accessory_metal.record;
            input r2 as welding_metal.record;

            assert.eq r0.metal_id r2.welds.base_metal_id;
            assert.eq r1.metal_id r2.welds.accessory_metal_id;
            assert.eq r0.weldable true;

            add r0.grams r1.grams into r3;

            cast r0.owner r3 into r4 as welded_chunk.record;

            output r4 as welded_chunk.record;

        function weld_dynamically:
            // Expected type: base_metal.record
            input r0 as dynamic.record;
            // Expected type: base_metal.record
            input r1 as dynamic.record;
            // Expected type: accessory_metal.record
            input r2 as dynamic.record;
            // Expected type: welding_metal.record
            input r3 as dynamic.record;

            call.dynamic {program_a_field} {network_field} {weld_function_field}
                with r0 r2 r3 (as dynamic.record dynamic.record dynamic.record)
                into r4 (as dynamic.record);

            call.dynamic {program_b_field} {network_field} {check_tossed_coin_field}
                with r1 (as dynamic.record);

            get.dynamic.record r4.grams into r5 as u32;

            output r5 as u32.public;
        
        constructor:
            assert.eq true true;
        "
    );

    let program_b_string = format!(
        r"
        import {program_a_str}.aleo;

        program {program_b_str}.aleo;

        // Check that the metal used to pay for the welding in program a is not weldable
        function {check_tossed_coin_str}:
            input r0 as {program_a_str}.aleo/base_metal.record;
            assert.eq r0.weldable false;
        
        constructor:
            assert.eq true true;
        "
    );

    let program_a = Program::<CurrentNetwork>::from_str(&program_a_string).unwrap();
    let program_b = Program::<CurrentNetwork>::from_str(&program_b_string).unwrap();

    // Initialize the VM.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V12).unwrap(), rng);

    // Deploy the programs.
    println!("Deploying program {program_a_str}.aleo...");
    let transaction_deploy_a = vm.deploy(&caller_private_key, &program_a, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction_deploy_a], rng);

    println!("Deploying program {program_b_str}.aleo...");
    let transaction_deploy_b = vm.deploy(&caller_private_key, &program_b, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction_deploy_b], rng);

    let record_data = [
        ("base_metal",vec![
            &caller_address_str,
            "183u16",
            "999u32",
            "true",
        ]),
        ("base_metal",vec![
            &caller_address_str,
            "93u16",
            "20u32",
            "false",
        ]),
        ("accessory_metal",vec![
            &caller_address_str,
            "82u16",
            "27u32",
            "0group",
        ]),
        ("welding_metal",vec![
            &caller_address_str,
            "183u16",
            "82u16",
        ]),
    ];

    let mut transactions_and_records = record_data.into_iter().map(|(record_name, entry_values)| {

        let function_name = format!("mint_{record_name}");

        println!("Calling {program_a_str}.aleo/{function_name}...");

        let transaction_mint = vm.execute(
            &caller_private_key,
            ("welder.aleo", function_name),
            entry_values.into_iter(),
            None,
            0,
            None,
            rng,
        ).unwrap();

        let record = match &transaction_mint.transitions().next().unwrap().outputs()[0] {
            Output::Record(_, _, record_ciphertext, _) => {
                record_ciphertext.as_ref().unwrap().decrypt(&caller_view_key).unwrap()
            }
            _ => panic!("Expected output record is not a record"),
        };

        (transaction_mint, record)
    }).collect_vec();

    let (transactions_mint, records): (Vec<_>, Vec<_>) = transactions_and_records.into_iter().unzip();

    add_and_test(&vm, &caller_private_key, &transactions_mint, rng);

    let dynamic_records = records.into_iter().map(|record| Value::DynamicRecord(DynamicRecord::<CurrentNetwork>::from_record(&record).unwrap())).collect_vec();

    println!("Executing {program_a_str}.aleo/weld_dynamically...");
    
    let transaction = vm
        .execute(
            &caller_private_key,
            ("welder.aleo", "weld_dynamically"),
            dynamic_records.into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    let expected_output = Plaintext::<CurrentNetwork>::from_str("1026u32").unwrap();

    assert!(
        // The first two transition are weld and check_tossed_coin; the root transition we are interested in is at index 2.
        matches!(transaction.transitions().skip(2).next().unwrap().outputs(), [Output::Public(_, Some(plaintext))] if *plaintext == expected_output),
        "Expected output: {:?}, got: {:?}",
        expected_output,
        transaction.transitions().next().unwrap().outputs()
    );

    // Checking the cost-estimation function computes the correct cost
    let execution = transaction.execution().unwrap();

    let actual_cost = execution_cost(&vm.process().read(), execution, ConsensusVersion::V12).unwrap();

    let authorization =
        Authorization::from_unchecked((vec![], execution.transitions().cloned().collect()));

    // The batch sizes involved in the cost computation are [1, 1, 1, 3, 2, 1, 1], where:
    // - the first 3 ones come from the three distinct transitions
    // - the next three comes from the single batch of three inclusion proofs, one for each record consumed.
    //   Note the second base_metal record is never consumed, since it is received as a dynamic record by weld
    //   and as an external record by check_tossed_coin.
    // - the next two comes from the two (input) translations for base_metal.record
    //   - as a non-external static record in weld
    //   - as an external static record in check_tossed_coin
    // - the next 2 ones come from the (input) translations for accessory_metal.record and welding_metal.record
    // - the next one comes from the (output) translation for welded_chunk.record

    let expected_cost =
        execution_cost_for_authorization(&vm.process().read(), &authorization, ConsensusVersion::V12)
            .unwrap();

    assert_eq!(actual_cost, expected_cost);

    // Ensuring transaction verification passes
    add_and_test(&vm, &caller_private_key, &[transaction], rng);
}