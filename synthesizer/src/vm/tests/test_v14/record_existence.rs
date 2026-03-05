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

use super::*;

// Checks that, if a (previously minted) static Record is converted into a
// DynamicRecord outside the VM, passed to the root call as a DynamicRecord and
// then translated to a static Record as an input to a callee, the
// record-existence check passes. However, if the callee receives it as a
// DynamicRecord, the record-existence check fails.
#[test]
fn test_input_dynamic_then_materialize() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(&caller_private_key).unwrap();
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    let program_name_str = "instruments";
    let network_str = "aleo";
    let program_name_field = Identifier::<CurrentNetwork>::from_str(program_name_str).unwrap().to_field().unwrap();
    let network_field = Identifier::<CurrentNetwork>::from_str(network_str).unwrap().to_field().unwrap();
    let mint_guitar_function_str = "mint_guitar";
    let read_num_strings_static_function_str = "read_num_strings_static";
    let read_num_strings_static_function_field =
        Identifier::<CurrentNetwork>::from_str(read_num_strings_static_function_str).unwrap().to_field().unwrap();
    let read_num_strings_dynamic_function_str = "read_num_strings_dynamic";
    let read_num_strings_dynamic_function_field =
        Identifier::<CurrentNetwork>::from_str(read_num_strings_dynamic_function_str).unwrap().to_field().unwrap();
    let root_function_str = "root";

    let program_str = format!(
        r"
        program {program_name_str}.aleo;

        record guitar:
            owner as address.private;
            num_strings as u8.public;

        function {mint_guitar_function_str}:
            cast {caller_address} 6u8 into r0 as guitar.record;
            output r0 as guitar.record;

        function {read_num_strings_static_function_str}:
            input r0 as guitar.record;
            output r0.num_strings as u8.public;

        function {read_num_strings_dynamic_function_str}:
            input r0 as dynamic.record;
            get.record.dynamic r0.num_strings into r1 as u8.public;
            output r1 as u8.public;

        function {root_function_str}:
            input r0 as dynamic.record;
            input r1 as boolean.public;

            ternary r1 {read_num_strings_static_function_field} {read_num_strings_dynamic_function_field} into r2;

            call.dynamic {program_name_field} {network_field} r2
                with r0 (as dynamic.record)
                into r3 (as u8.public);
            output r3 as u8.public;

        constructor:
            assert.eq true true;
        "
    );

    let program = Program::<CurrentNetwork>::from_str(&program_str).unwrap();
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    println!("Deploying program {program_name_str}.aleo...");
    let transaction_deploy = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction_deploy], rng);

    for materialize in [true, false] {
        // Mint a static record in a transaction.
        println!("Minting guitar record...");
        let transaction_mint = vm
            .execute(
                &caller_private_key,
                (format!("{program_name_str}.aleo"), mint_guitar_function_str),
                Vec::<Value<CurrentNetwork>>::new().into_iter(),
                None,
                0,
                None,
                rng,
            )
            .unwrap();

        let mint_output = transaction_mint.transitions().next().unwrap().outputs().iter().next().unwrap();
        let record_ciphertext = match mint_output {
            Output::Record(_, _, ciphertext, _) => ciphertext.as_ref().unwrap().clone(),
            _ => panic!("Mint output should be a record"),
        };

        let block_mint = sample_next_block(&vm, &caller_private_key, &[transaction_mint], rng).unwrap();
        assert_eq!(block_mint.transactions().num_accepted(), 1);
        vm.add_next_block(&block_mint).unwrap();

        // Outside the VM: decrypt and obtain the static record (plaintext).
        let record_static = record_ciphertext.decrypt(&caller_view_key).unwrap();

        // Convert to dynamic so we can call the root function
        let record_dynamic = DynamicRecord::<CurrentNetwork>::from_record(&record_static).unwrap();

        // Call the root function indicating it should in turn call the child which expects a static record.
        println!(
            "Calling {program_name_str}.aleo/{root_function_str} (callee materializes dynamic record? {materialize})..."
        );
        let transaction = vm.execute(
            &caller_private_key,
            (format!("{program_name_str}.aleo"), root_function_str),
            vec![
                Value::<CurrentNetwork>::DynamicRecord(record_dynamic),
                Value::<CurrentNetwork>::Plaintext(
                    Plaintext::<CurrentNetwork>::from_str(&materialize.to_string()).unwrap(),
                ),
            ]
            .into_iter(),
            None,
            0,
            None,
            rng,
        );

        if materialize {
            let transaction = transaction.unwrap();
            let expected_amount = Plaintext::<CurrentNetwork>::from_str("6u8").unwrap();
            assert!(
                matches!(transaction.transitions().next().unwrap().outputs(), [Output::Public(_, Some(plaintext))] if *plaintext == expected_amount),
                "Expected output equal to 6, got: {:?}",
                transaction.transitions().next().unwrap().outputs()
            );
            add_and_test(&vm, &caller_private_key, &[transaction], rng);
        } else {
            let err = transaction.unwrap_err();
            assert!(err.to_string().contains("r0"));
            assert!(err.to_string().contains("is not known to correspond to a record on the ledger"));
        }
    }
}

// Checks that closures can receive Records as ExternalRecords and vice-versa;
// and they can output ExternalRecords received as either ExternalRecords or
// Records.
#[test]
fn test_external_record_and_closure_call() -> Result<()> {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(&caller_private_key)?;

    // Use program names with 10+ characters so deployment namespace cost stays within MAX_FEE.
    let program_a = Program::<CurrentNetwork>::from_str(
        r"
        program program_a.aleo;

        record a_record:
            owner as address.private;
            val as u8.private;

        closure read_val:
            input r0 as a_record.record;
            add r0.val 0u8 into r1;
            output r1 as u8;

        function mint_record:
            input r0 as address.private;
            input r1 as u8.private;
            cast r0 r1 into r2 as a_record.record;
            output r2 as a_record.record;

        function consume_record:
            input r0 as a_record.record;

        constructor:
            assert.eq true true;
        ",
    )?;

    let program_b = Program::<CurrentNetwork>::from_str(
        r"
        import program_a.aleo;

        program program_b.aleo;

        record record_b:
            owner as address.private;
            truth as boolean.private;

        function mint_record_b:
            input r0 as address.private;
            input r1 as boolean.private;
            cast r0 r1 into r2 as record_b.record;
            output r2 as record_b.record;

        function read_external_val:
            input r0 as program_a.aleo/a_record.record;
            call program_a.aleo/read_val r0 into r1;
            output r1 as u8.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?, rng);

    println!("Deploying programs...");

    let deploy_a = vm.deploy(&caller_private_key, &program_a, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[deploy_a], rng);

    let deploy_b = vm.deploy(&caller_private_key, &program_b, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[deploy_b], rng);

    println!("Minting record...");

    let tx_mint = vm.execute(
        &caller_private_key,
        ("program_a.aleo", "mint_record"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("42u8")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    let record_ciphertext = match tx_mint.transitions().next().unwrap().outputs().first().unwrap() {
        Output::Record(_, _, ct, _) => ct.as_ref().unwrap().clone(),
        _ => panic!("expected record output"),
    };

    add_and_test(&vm, &caller_private_key, &[tx_mint], rng);

    let a_record_plaintext = record_ciphertext.decrypt(&caller_view_key)?;

    println!("Reading record through external closure call...");

    let tx_read = vm.execute(
        &caller_private_key,
        ("program_b.aleo", "read_external_val"),
        [Value::<CurrentNetwork>::Record(a_record_plaintext.clone())].into_iter(),
        None,
        0,
        None,
        rng,
    );

    let err = tx_read.unwrap_err();
    assert!(err.to_string().contains("record input at r0 of the root function program_b.aleo/read_external_val is not known to correspond to a record on the ledger"));

    // Check that the record has not been consumed
    println!("Consuming record...");

    let tx_consume = vm.execute(
        &caller_private_key,
        ("program_a.aleo", "consume_record"),
        [Value::<CurrentNetwork>::Record(a_record_plaintext)].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    add_and_test(&vm, &caller_private_key, &[tx_consume], rng);

    // Upgrade program_a with a closure that reads the "truth" field from program_b's record_b.
    let program_a_upgraded = Program::<CurrentNetwork>::from_str(
        r"
        import program_b.aleo;

        program program_a.aleo;

            record a_record:
                owner as address.private;
                val as u8.private;

            closure read_val:
                input r0 as a_record.record;
                add r0.val 0u8 into r1;
                output r1 as u8;

            closure read_truth:
                input r0 as program_b.aleo/record_b.record;
                or r0.truth false into r1;
                output r1 as boolean;

            function mint_record:
                input r0 as address.private;
                input r1 as u8.private;
                cast r0 r1 into r2 as a_record.record;
                output r2 as a_record.record;

            function consume_record:
                input r0 as a_record.record;

            constructor:
                assert.eq true true;
            ",
    )?;

    println!("Upgrading program_a...");

    let deploy_upgrade = vm.deploy(&caller_private_key, &program_a_upgraded, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[deploy_upgrade], rng);

    let program_b_upgraded = Program::<CurrentNetwork>::from_str(
        r"
        import program_a.aleo;

        program program_b.aleo;

        record record_b:
            owner as address.private;
            truth as boolean.private;

        function mint_record_b:
            input r0 as address.private;
            input r1 as boolean.private;
            cast r0 r1 into r2 as record_b.record;
            output r2 as record_b.record;

        function read_external_val:
            input r0 as program_a.aleo/a_record.record;
            call program_a.aleo/read_val r0 into r1;
            output r1 as u8.public;

        function read_in_a_closure:
            input r0 as record_b.record;
            call program_a.aleo/read_truth r0 into r1;
            output r1 as boolean.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    println!("Upgrading program_b...");

    let deploy_upgrade_b = vm.deploy(&caller_private_key, &program_b_upgraded, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[deploy_upgrade_b], rng);

    // First transaction: mint a record_b.
    let tx_mint_b = vm.execute(
        &caller_private_key,
        ("program_b.aleo", "mint_record_b"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("true")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    let record_b_ciphertext = match tx_mint_b.transitions().next().unwrap().outputs().first().unwrap() {
        Output::Record(_, _, ct, _) => ct.as_ref().unwrap().clone(),
        _ => panic!("expected record output from mint_record_b"),
    };

    add_and_test(&vm, &caller_private_key, &[tx_mint_b], rng);

    let record_b_plaintext = record_b_ciphertext.decrypt(&caller_view_key)?;

    // Second transaction: call read_in_a_closure with the minted record.
    let tx_read_in_a = vm.execute(
        &caller_private_key,
        ("program_b.aleo", "read_in_a_closure"),
        [Value::<CurrentNetwork>::Record(record_b_plaintext)].into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    add_and_test(&vm, &caller_private_key, &[tx_read_in_a], rng);

    Ok(())
}

#[test]
fn test_existence_check() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key).unwrap();
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(&caller_private_key).unwrap();

    let network_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();
    let program_base_field = Identifier::<CurrentNetwork>::from_str("base").unwrap().to_field().unwrap();
    let decomission_function_field = Identifier::<CurrentNetwork>::from_str("decomission").unwrap().to_field().unwrap();
    let decomission_reversed_function_field =
        Identifier::<CurrentNetwork>::from_str("decomission_reversed").unwrap().to_field().unwrap();
    let decomission_two_function_field =
        Identifier::<CurrentNetwork>::from_str("decomission_two").unwrap().to_field().unwrap();
    let mint_rover_function_field = Identifier::<CurrentNetwork>::from_str("mint_rover").unwrap().to_field().unwrap();
    let program_frontier_field = Identifier::<CurrentNetwork>::from_str("frontier").unwrap().to_field().unwrap();
    let program_remapper_field = Identifier::<CurrentNetwork>::from_str("remapper").unwrap().to_field().unwrap();
    let consume_map_function_field = Identifier::<CurrentNetwork>::from_str("consume_map").unwrap().to_field().unwrap();
    let consume_dynamic_map_function_field =
        Identifier::<CurrentNetwork>::from_str("consume_dynamic_map").unwrap().to_field().unwrap();
    let remap_dynamic_function_field =
        Identifier::<CurrentNetwork>::from_str("remap_dynamic_function").unwrap().to_field().unwrap();
    let do_not_consume_function_field =
        Identifier::<CurrentNetwork>::from_str("do_not_consume").unwrap().to_field().unwrap();

    let program_base = Program::<CurrentNetwork>::from_str(&format!(
        r"
        program base.aleo;

        record rover:
            owner as address.private;
            planet_code as u8.private;
            active as boolean.private;

        function mint_rover:
            input r0 as u8.private;
            input r1 as boolean.private;

            cast self.signer r0 r1 into r2 as rover.record;
            output r2 as rover.record;

        // This function breaks the local check
        function dynamic_mint:
            input r0 as u8.private;
            input r1 as boolean.private;

            cast self.signer r0 r1 into r2 as rover.record;
            cast r2 into r3 as dynamic.record;

            output r3 as dynamic.record;

        // This closure breaks the local check
        closure dynamic_mint_closure:
            input r0 as u8;
            input r1 as boolean;

            cast {caller_address} r0 r1 into r2 as rover.record;
            cast r2 into r3 as dynamic.record;

            output r3 as dynamic.record;

        function call_closure_mint:
            input r0 as u8.public;
            call dynamic_mint_closure r0 false into r1;
            output 2u8 as u8.public;

        // Consumes a rover Record and outputs whether its planet was the same as that of a DynamicRecord received separately
        function decomission:
            input r0 as rover.record;
            input r1 as dynamic.record;

            get.record.dynamic r1.planet_code into r2 as u8.private;

            is.eq r2 r0.planet_code into r3;

            output r3 as boolean.public;

        // Does the same as decomission but receives the DynamicRecord and Record in the opposite order
        function decomission_reversed:
            input r0 as dynamic.record;
            input r1 as rover.record;

            get.record.dynamic r0.planet_code into r2 as u8.private;

            is.eq r2 r1.planet_code into r3;

            output r3 as boolean.public;

        // Consumes two rover Records and outputs whether their planets coincide
        function decomission_two:
            input r0 as rover.record;
            input r1 as rover.record;

            is.eq r0.planet_code r1.planet_code into r2;

            output r2 as boolean.public;

        constructor:
            assert.eq true true;
        ",
    ))
    .unwrap();

    let program_extension = Program::<CurrentNetwork>::from_str(&format!(
        r"
        import base.aleo;

        program extension.aleo;

        // This function passes the local check: if dynamic_mint does not output non-existent records, neither does call_base
        function call_base:
            input r0 as boolean.private;
            call base.aleo/dynamic_mint 3u8 r0 into r1;
            output r1 as dynamic.record;

        // This function passes the local check: if dynamic_mint does not output non-existent records, neither does call_base
        function call_base_closure:
            input r0 as u8.private;
            call base.aleo/dynamic_mint_closure r0 false into r1;
            output r1 as dynamic.record;

        function check_decomission_same:
            input r0 as base.aleo/rover.record;

            assert.eq r0.active true;
            cast r0 into r1 as dynamic.record;
            cast r0 into r2 as dynamic.record;

            call dummy 10u16 false into r3 r4 r5;
            call dynamic_pass_through_closure 10u64 r2 into r6 r7;

            call.dynamic {program_base_field} {network_field} {decomission_function_field}
                with r1 r6 (as dynamic.record dynamic.record)
                into r8 (as boolean.public);

            output r8 as boolean.public;

        function mint_and_decomission_same:
            input r0 as u8.private;
            input r1 as boolean.private;

            call base.aleo/mint_rover r0 r1 into r2;

        closure dummy:
            input r0 as u16;
            input r1 as boolean;

            add r0 r0 into r2;

            output r0 as u16;
            output r2 as u16;
            output r1 as boolean;
        
        closure dynamic_pass_through_closure:
            input r0 as u64;
            input r1 as dynamic.record;

            assert.eq r0 r0;

            output r1 as dynamic.record;
            output r0 as u64;

        function dynamic_pass_through_function:
            input r0 as u64.private;
            input r1 as dynamic.record;

            assert.eq r0 r0;

            output r1 as dynamic.record;
            output r0 as u64.private;

        // Calls a function to mint a Record (received as a DynamicRecord), casts the input ExternalRecord
        // to a DynamicRecord and passes both DynamicRecords to a callee controlled by the input flags
        function mint_own_and_decom_int:
            input r0 as base.aleo/rover.record;
            input r1 as u8.private;
            input r2 as boolean.private;
            // Flag controlling whether the base.aleo function called takes two static Records (true) or one static Record and one DynamicRecord (false)
            input r3 as boolean.private;
            // Flag controlling whether the function called at the end takes in a (Record, DynamicRecord) (true) or (DynamicRecord, Record) (false) assuming r3 = false.
            input r4 as boolean.private;

            call.dynamic {program_base_field} {network_field} {mint_rover_function_field}
                with r1 r2 (as u8.private boolean.private)
                into r5 (as dynamic.record);

            cast r0 into r6 as dynamic.record;

            // Shuffling DynamicRecords for good measure

            call dynamic_pass_through_closure 11u64 r5 into r7 r8;
            call dynamic_pass_through_closure 1u64 r6 into r9 r10;

            ternary r4 {decomission_function_field} {decomission_reversed_function_field} into r11;
            ternary r3 {decomission_two_function_field} r11 into r12;

            call.dynamic {program_base_field} {network_field} r12
                with r7 r9 (as dynamic.record dynamic.record)
                into r13 (as boolean.public);

            output r13 as boolean.public;

        constructor:
            assert.eq true true;
        ",
    )).unwrap();

    let program_exploration = Program::<CurrentNetwork>::from_str(
        r"
        import base.aleo;
        import extension.aleo;

        program exploration.aleo;

        // This function passes the local check: if call_base_closure does not output non-existent records, neither does call_base_next_planet
        function call_base_next_planet:
            input r0 as u8.private;
            add r0 1u8 into r1;
            call extension.aleo/call_base_closure r1 into r2;
            output r2 as dynamic.record;

        // Remapping to complicate register tracking
        closure remap_external:
            input r0 as base.aleo/rover.record;

            assert.eq true true;

            output r0 as base.aleo/rover.record;

        // Wrapper around mint_own_and_decom_int to complicate register tracking
        function mint_own_and_decom_wrapper:
            input r0 as u8.private;
            input r1 as boolean.private;
            input r2 as base.aleo/rover.record;
            input r3 as boolean.private;
            input r4 as boolean.private;

            call remap_external r2 into r5;

            call extension.aleo/mint_own_and_decom_int r5 r0 r1 r3 r4 into r6;

            output r6 as boolean.public;

        constructor:
            assert.eq true true;
        ",
    ).unwrap();

    let program_frontier = Program::<CurrentNetwork>::from_str(
        r"
        program frontier.aleo;

        record map:
            owner as address.private;
            capital_coordinate_x as u16.private;
            capital_coordinate_y as u16.private;
            orography as boolean.private;

        function mint_map:
            cast self.signer 111u16 222u16 true into r0 as map.record;
            output r0 as map.record;

        function consume_map:
            input r0 as map.record;
        
        constructor:
            assert.eq true true;
        ",
    )
    .unwrap();

    let program_mini_remapper = Program::<CurrentNetwork>::from_str(
        r"
        import frontier.aleo;
        program mini_remapper.aleo;

        function remap_external_function:
            input r0 as frontier.aleo/map.record;

            assert.eq true true;

            output r0 as frontier.aleo/map.record;
        
        constructor:
            assert.eq true true;
        ",
    )
    .unwrap();

    let program_remapper = Program::<CurrentNetwork>::from_str(&format!(
        r"
        import base.aleo;
        import mini_remapper.aleo;
        import frontier.aleo;

        program remapper.aleo;
        
        closure remap:
            input r0 as frontier.aleo/map.record;

            cast r0 into r1 as dynamic.record;

            output r1 as dynamic.record;

        closure remap_external:
            input r0 as frontier.aleo/map.record;

            assert.eq true true;
            
            output r0 as frontier.aleo/map.record;

        closure remap_dynamic:
            input r0 as dynamic.record;

            assert.eq true true;

            output r0 as dynamic.record;

        function remap_dynamic_function:
            input r0 as dynamic.record;

            assert.eq true true;

            output r0 as dynamic.record;

        function do_not_consume:
            input r0 as dynamic.record;

            get.record.dynamic r0.orography into r1 as boolean.private;
        
        function growing_family:
            input r0 as frontier.aleo/map.record;
            // Flag which determines whether the last function called will mark the family as existing or not
            input r1 as boolean.private;

            call mini_remapper.aleo/remap_external_function r0 into r2;
            
            cast r2 into r3 as dynamic.record;
            call remap_external r2 into r4;
            call remap r4 into r5;
            
            call.dynamic {program_remapper_field} {network_field} {consume_dynamic_map_function_field}
                with r5 r1 (as dynamic.record boolean.private);
                
        function consume_dynamic_map:
            input r0 as dynamic.record;
            input r1 as boolean.private;

            ternary r1 {program_frontier_field} {program_remapper_field} into r2;
            ternary r1 {consume_map_function_field} {do_not_consume_function_field} into r3;

            call remap_dynamic r0 into r4;

            call.dynamic {program_remapper_field} {network_field} {remap_dynamic_function_field}
                with r4 (as dynamic.record)
                into r5 (as dynamic.record);

            // This call materialises r5 only if r1 is true. By the time the global check gets here, the family contains nine members
            call.dynamic r2 {network_field} r3
                with r5 (as dynamic.record);

        constructor:
            assert.eq true true;
        "),
    ).unwrap();

    let program_frontier_upgraded = Program::<CurrentNetwork>::from_str(
        r"
        import extension.aleo;
        import remapper.aleo;

        program frontier.aleo;

        record map:
            owner as address.private;
            capital_coordinate_x as u16.private;
            capital_coordinate_y as u16.private;
            orography as boolean.private;

        function mint_map:
            cast self.signer 111u16 222u16 true into r0 as map.record;
            output r0 as map.record;

        function consume_map:
            input r0 as map.record;

        function simple_cast_closure:
            input r0 as boolean.private;
            
            cast self.signer 1u16 2u16 r0 into r1 as map.record;

            cast r1 into r2 as dynamic.record;

            // This is okay: even though the locally minted DynamicRecord is not known to materialize, it is only passed to a closure (not to a function)
            call extension.aleo/dynamic_pass_through_closure 3u64 r2 into r3 r4;

        function simple_cast_function:
            input r0 as boolean.private;
            
            cast self.signer 1u16 2u16 r0 into r1 as map.record;

            cast r1 into r2 as dynamic.record;

            // This is not okay: the locally minted DynamicRecord is not known to materialize and it is passed to an actual function
            call extension.aleo/dynamic_pass_through_function 3u64 r2 into r3 r4;

        function tricky_cast_closure:
            input r0 as boolean.private;
            
            cast self.signer 1u16 2u16 r0 into r1 as map.record;

            // Here we are effectively performing a static-to-dynamic cast, but through a closure (where it is in fact external-to-dynamic)
            call remapper.aleo/remap r1 into r2;

            // This is okay: even though the locally minted DynamicRecord is not known to materialize, it is only passed to a closure (not to a function)
            call extension.aleo/dynamic_pass_through_closure 3u64 r2 into r3 r4;

        function tricky_cast_function:
            input r0 as boolean.private;
            
            cast self.signer 1u16 2u16 r0 into r1 as map.record;

            // Here we are effectively performing a static-to-dynamic cast, but through a closure (where it is in fact external-to-dynamic)
            call remapper.aleo/remap r1 into r2;

            // This is not okay: the locally minted DynamicRecord is not known to materialize and it is passed to an actual function
            call extension.aleo/dynamic_pass_through_function 3u64 r2 into r3 r4;

        function tricky_cast_function_saved:
            input r0 as boolean.private;
            
            cast self.signer 1u16 2u16 r0 into r1 as map.record;

            // Here we are effectively performing a static-to-dynamic cast, but through a closure (where it is in fact external-to-dynamic)
            call remapper.aleo/remap r1 into r2;

            call extension.aleo/dynamic_pass_through_function 3u64 r2 into r3 r4;

            // This saves the call above from breaking the locak check
            output r1 as map.record;
        

        function tricky_cast_closure_ruined:
            input r0 as boolean.private;
            
            cast self.signer 1u16 2u16 r0 into r1 as map.record;

            // Here we are effectively performing a static-to-dynamic cast, but through a closure (where it is in fact external-to-dynamic)
            call remapper.aleo/remap r1 into r2;

            // This is okay: even though the locally minted DynamicRecord is not known to materialize, it is only passed to a closure (not to a function)
            call extension.aleo/dynamic_pass_through_closure 3u64 r2 into r3 r4;

            // This is not okay: we are outputting the locally minted DynamicRecord. The local check should remember this comes from the locally minted static Record at r1
            output r3 as dynamic.record;

        constructor:
            assert.eq true true;
        "
    ).unwrap();

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    println!("Deploying program base...");
    let deploy_base = vm.deploy(&caller_private_key, &program_base, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deploy_base], rng);

    println!("Deploying program extension...");
    let deploy_extension = vm.deploy(&caller_private_key, &program_extension, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deploy_extension], rng);

    println!("Deploying program exploration...");
    let deploy_exploration = vm.deploy(&caller_private_key, &program_exploration, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deploy_exploration], rng);

    println!("Deploying program frontier...");
    let deploy_frontier = vm.deploy(&caller_private_key, &program_frontier, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deploy_frontier], rng);

    println!("Deploying program mini_remapper...");
    let deploy_mini_remapper = vm.deploy(&caller_private_key, &program_mini_remapper, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deploy_mini_remapper], rng);

    println!("Deploying program remapper...");
    let deploy_remapper = vm.deploy(&caller_private_key, &program_remapper, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deploy_remapper], rng);

    println!("Upgrading program frontier...");
    let deploy_frontier_upgraded =
        vm.deploy(&caller_private_key, &program_frontier_upgraded, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deploy_frontier_upgraded], rng);

    // Test 1: A child function of the root transition breaks the (function version of the) local check
    // Involves process_transition cases 3, 5
    println!("Test 1: Calling extension.aleo/call_base...");

    let tx_base_function = vm.execute(
        &caller_private_key,
        ("extension.aleo", "call_base"),
        [Value::from_str("true").unwrap()].into_iter(),
        None,
        0,
        None,
        rng,
    );

    let err = tx_base_function.unwrap_err();
    assert!(err.to_string().contains("r2"));
    assert!(err.to_string().contains("base.aleo/dynamic_mint does not pass the local record-existence check"));

    // Test 2: An external closure call in a child of the root transition's child breaks the (closure version of the) local check
    // Involves process_transition case 3 and process_closure cases 1, 2
    println!("Test 2: Calling exploration.aleo/call_base_next_planet...");

    let tx_base_closure = vm.execute(
        &caller_private_key,
        ("exploration.aleo", "call_base_next_planet"),
        [Value::from_str("3u8").unwrap()].into_iter(),
        None,
        0,
        None,
        rng,
    );

    let err = tx_base_closure.unwrap_err();
    assert!(err.to_string().contains("Closure dynamic_mint_closure attempts to output DynamicRecord at r3 cast from locally minted static Record at r2"));

    // Test 3: A static record cast to dynamic twice and passed to a callee once translated and once as dynamic does not break the global or local checks.
    // It involves dynamic-record-register remapping through a closure call.
    // Involves process_transition cases 1, 2, 4 and process_closure case 5
    println!("Test 3: extension.aleo/check_decomission_same...");

    let mint_planet_4_tx = vm
        .execute(
            &caller_private_key,
            ("base.aleo", "mint_rover"),
            [Value::from_str("4u8").unwrap(), Value::from_str("true").unwrap()].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    add_and_test(&vm, &caller_private_key, &[mint_planet_4_tx.clone()], rng);

    let mint_planet_4_output = mint_planet_4_tx.transitions().next().unwrap().outputs().first().unwrap();
    let mint_planet_4_record = match mint_planet_4_output {
        Output::Record(_, _, ct, _) => ct.as_ref().unwrap().decrypt(&caller_view_key).unwrap(),
        _ => panic!("expected record output from mint_rover"),
    };

    let check_decomission_same_tx = vm
        .execute(
            &caller_private_key,
            ("extension.aleo", "check_decomission_same"),
            [Value::<CurrentNetwork>::Record(mint_planet_4_record)].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    add_and_test(&vm, &caller_private_key, &[check_decomission_same_tx], rng);

    // Test 4: a root function receives a DynamicRecord R_d1 and calls a
    // function that mints a static Record, receiving it as a DynamicRecord
    // R_d2. The two records are eventually passed
    //  - 4.1) to a function receiving a DynamicRecord and a static Record. This
    //    fails since R_d1 is not known to materialize.
    //  - 4.2) to a function receiving a static Record and a DynamicRecord. This
    //         passes since R_d1 is known to materialize by the call and R_d2 is
    //         known to materialize by the local-check guarantee.
    //  - 4.3) to a function receiving two static Records. This passes for the
    // same reason as above Although both 4.2 and 4.3 pass, 4.2 nets 0 unspent
    // Records and 4.3 nets -1 unspent Records on the ledger.
    println!("Test 4: mint_own_and_decom_wrapper...");

    let three_mint_txs = (0..3)
        .map(|_| {
            vm.execute(
                &caller_private_key,
                ("base.aleo", "mint_rover"),
                [Value::from_str("5u8").unwrap(), Value::from_str("true").unwrap()].into_iter(),
                None,
                0,
                None,
                rng,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();

    let three_records = three_mint_txs
        .iter()
        .map(|tx| match tx.transitions().next().unwrap().outputs().first().unwrap() {
            Output::Record(_, _, ct, _) => ct.as_ref().unwrap().decrypt(&caller_view_key).unwrap(),
            _ => panic!("expected record output from mint_rover"),
        })
        .collect::<Vec<_>>();

    add_and_test(&vm, &caller_private_key, &three_mint_txs, rng);

    // Involves process_transition cases 1, 2, 3, 4 and process_closure case 5
    println!("    4.1) Final function receives (DynamicRecord, Record)...");

    let mint_own_and_decom_4_1_tx = vm.execute(
        &caller_private_key,
        ("exploration.aleo", "mint_own_and_decom_wrapper"),
        [
            Value::from_str("6u8").unwrap(),
            Value::from_str("true").unwrap(),
            Value::<CurrentNetwork>::Record(three_records[0].clone()),
            Value::from_str("false").unwrap(),
            Value::from_str("true").unwrap(), // Together with the previous flag: select the function that receives (Record, DynamicRecord)
        ]
        .into_iter(),
        None,
        0,
        None,
        rng,
    );

    assert!(mint_own_and_decom_4_1_tx.unwrap_err().to_string().contains(
        "record input at r2 of the root function exploration.aleo/mint_own_and_decom_wrapper is not known to correspond"
    ));

    let num_unspent_records_1 =
        vm.transition_store().records().count() - vm.transition_store().serial_numbers().count();

    // Involves process_transition cases 1, 2, 3, 4 and process_closure case 5
    println!("    4.2) Final function receives (Record, DynamicRecord)...");

    let mint_own_and_decom_4_2_tx = vm
        .execute(
            &caller_private_key,
            ("exploration.aleo", "mint_own_and_decom_wrapper"),
            [
                Value::from_str("6u8").unwrap(),
                Value::from_str("true").unwrap(),
                Value::<CurrentNetwork>::Record(three_records[1].clone()),
                Value::from_str("false").unwrap(),
                Value::from_str("false").unwrap(), // Together with the previous flag: select the function that receives (DynamicRecord, Record)
            ]
            .into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    add_and_test(&vm, &caller_private_key, &[mint_own_and_decom_4_2_tx], rng);

    let num_unspent_records_2 =
        vm.transition_store().records().count() - vm.transition_store().serial_numbers().count();

    assert_eq!(num_unspent_records_2, num_unspent_records_1);

    // Involves process_transition cases 1, 2, 3, 4 and process_closure case 5
    println!("    4.3) Final function receives (Record, Record)...");

    let mint_own_and_decom_4_3_tx = vm
        .execute(
            &caller_private_key,
            ("exploration.aleo", "mint_own_and_decom_wrapper"),
            [
                Value::from_str("6u8").unwrap(),
                Value::from_str("true").unwrap(),
                Value::<CurrentNetwork>::Record(three_records[2].clone()),
                Value::from_str("true").unwrap(), // Select the function that receives (Record, Record)
                Value::from_str("false").unwrap(),
            ]
            .into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    add_and_test(&vm, &caller_private_key, &[mint_own_and_decom_4_3_tx], rng);

    let num_unspent_records_3 =
        vm.transition_store().records().count() - vm.transition_store().serial_numbers().count();

    assert_eq!(num_unspent_records_3, num_unspent_records_2 - 1);

    // Test 5: four tests on the local check involving cast-to-dynamic (both from static Records and, in external closures, from ExternalRecords)

    println!("Test 5: Calling frontier.aleo performing various types of casts to DynamicRecords...");

    // Involves process_transition cases 3, 5 and process_closure case 6
    println!("    5.1) Passing a locally minted DynamicRecord to a closure...");

    let test_case_5_1_tx = vm
        .execute(
            &caller_private_key,
            ("frontier.aleo", "simple_cast_closure"),
            [Value::from_str("true").unwrap()].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    add_and_test(&vm, &caller_private_key, &[test_case_5_1_tx], rng);

    // Involves process_transition cases 2, 3, 5, 6, 8
    println!("    5.2) Attempting to pass a locally minted DynamicRecord to a function...");

    let test_case_5_2_tx = vm.execute(
        &caller_private_key,
        ("frontier.aleo", "simple_cast_function"),
        [Value::from_str("true").unwrap()].into_iter(),
        None,
        0,
        None,
        rng,
    );

    let err = test_case_5_2_tx.unwrap_err().to_string();
    assert!(err.contains("frontier.aleo/simple_cast_function does not pass the local record-existence check"));
    assert!(err.contains("The following registers violate this condition: \"r1\""));

    // Involves process_transition cases 3 and process_closure cases 3, 6
    println!("    5.3) Passing a locally minted DynamicRecord via external-closure cast to a closure...");

    let test_case_5_3_tx = vm
        .execute(
            &caller_private_key,
            ("frontier.aleo", "tricky_cast_closure"),
            [Value::from_str("true").unwrap()].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    add_and_test(&vm, &caller_private_key, &[test_case_5_3_tx], rng);

    // Involves process_transition cases 2, 3, 6, 8 and process_closure case 3
    println!("    5.4) Attempting to pass a locally minted DynamicRecord via external-closure cast to a function...");

    let test_case_5_4_tx = vm.execute(
        &caller_private_key,
        ("frontier.aleo", "tricky_cast_function"),
        [Value::from_str("true").unwrap()].into_iter(),
        None,
        0,
        None,
        rng,
    );

    let err = test_case_5_4_tx.unwrap_err().to_string();
    assert!(err.contains("frontier.aleo/tricky_cast_function does not pass the local record-existence check"));
    assert!(err.contains("The following registers violate this condition: \"r1\""));

    // Involves process_transition cases 2, 3, 6, 8 and process_closure case 3
    println!("    5.5) Saving case 5.4 by outputting the original static Record...");

    let test_case_5_5_tx = vm
        .execute(
            &caller_private_key,
            ("frontier.aleo", "tricky_cast_function_saved"),
            [Value::from_str("true").unwrap()].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    add_and_test(&vm, &caller_private_key, &[test_case_5_5_tx], rng);

    // Involves process_transition case 3 and process_closure cases 3, 6
    println!("    5.6) Ruining case 5.3 by outputting the locally minted DynamicRecord (after two remappings)...");

    let test_case_5_6_tx = vm.execute(
        &caller_private_key,
        ("frontier.aleo", "tricky_cast_closure_ruined"),
        [Value::from_str("true").unwrap()].into_iter(),
        None,
        0,
        None,
        rng,
    );

    let err = test_case_5_6_tx.unwrap_err().to_string();
    assert!(err.contains("frontier.aleo/tricky_cast_closure_ruined does not pass the local record-existence check"));
    assert!(err.contains("The following registers violate this condition: \"r1\""));

    // Test 6: global check where a large family is constructed. It checks families are updated correctly throughout function and closure calls.

    println!("Test 6: growing_family...");

    let map_record_tx = vm
        .execute(
            &caller_private_key,
            ("frontier.aleo", "mint_map"),
            Vec::<Value<CurrentNetwork>>::new().into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    let map_record = match map_record_tx.transitions().next().unwrap().outputs().first().unwrap() {
        Output::Record(_, _, ct, _) => ct.as_ref().unwrap().decrypt(&caller_view_key).unwrap(),
        _ => panic!("expected record output from mint_map"),
    };

    add_and_test(&vm, &caller_private_key, &[map_record_tx], rng);

    // Involves process_transition cases 1, 2, 4, 8 and process_closure cases 4, 5
    println!("    6.1) Calling remapper.aleo/growing_family and making the family materialize at the end...");

    let growing_family_tx = vm
        .execute(
            &caller_private_key,
            ("remapper.aleo", "growing_family"),
            [Value::<CurrentNetwork>::Record(map_record.clone()), Value::from_str("true").unwrap()].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    add_and_test(&vm, &caller_private_key, &[growing_family_tx], rng);

    let other_map_record_tx = vm
        .execute(
            &caller_private_key,
            ("frontier.aleo", "mint_map"),
            Vec::<Value<CurrentNetwork>>::new().into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    let other_map_record = match other_map_record_tx.transitions().next().unwrap().outputs().first().unwrap() {
        Output::Record(_, _, ct, _) => ct.as_ref().unwrap().decrypt(&caller_view_key).unwrap(),
        _ => panic!("expected record output from mint_map"),
    };

    add_and_test(&vm, &caller_private_key, &[other_map_record_tx], rng);

    // Involves process_transition cases 2, 4, 8 and process_closure cases 4, 5
    println!("    6.2) Calling remapper.aleo/growing_family and not making the family materialize at the end...");

    let growing_family_tx = vm.execute(
        &caller_private_key,
        ("remapper.aleo", "growing_family"),
        [Value::<CurrentNetwork>::Record(other_map_record.clone()), Value::from_str("false").unwrap()].into_iter(),
        None,
        0,
        None,
        rng,
    );

    let err = growing_family_tx.unwrap_err().to_string();
    assert!(err.contains("Non-static record input at r0"));
    assert!(err.contains("not known to correspond to a record on the ledger"));
}
