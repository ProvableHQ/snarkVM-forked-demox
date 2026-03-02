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
    assert!(err.to_string().contains("record input at r0 of function program_b.aleo/read_external_val is not known to correspond to a record on the ledger"));

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

    let program_base = Program::<CurrentNetwork>::from_str(&format!(
        r"
        program base.aleo;

        record rover:
            owner as address.private;
            planet_code as u8.private;
            active as boolean.private;

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


        constructor:
            assert.eq true true;
        ",
    ))
    .unwrap();

    let program_extension = Program::<CurrentNetwork>::from_str(
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

        constructor:
            assert.eq true true;
        ",
    ).unwrap();

    let program_exploration = Program::<CurrentNetwork>::from_str(
        r"
        import extension.aleo;

        program exploration.aleo;

        // This function passes the local check: if call_base_closure does not output non-existent records, neither does call_base_next_planet
        function call_base_next_planet:
            input r0 as u8.private;
            add r0 1u8 into r1;
            call extension.aleo/call_base_closure r1 into r2;
            output r2 as dynamic.record;

        constructor:
            assert.eq true true;
        ",
    ).unwrap();

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    println!("Deploying programs...");

    let deploy_base = vm.deploy(&caller_private_key, &program_base, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deploy_base], rng);

    let deploy_extension = vm.deploy(&caller_private_key, &program_extension, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deploy_extension], rng);

    let deploy_exploration = vm.deploy(&caller_private_key, &program_exploration, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deploy_exploration], rng);

    // Test 1: A child function of the root transition breaks the (function version of the) local check (process_transition cases 3 and 5)
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

    // Test 2: An external closure call in a child of the root transition's child breaks the (closure version of the) local check (process_closure cases 1 and 2)
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
}

// TODO test cases
// - Local check satisfied at the start but broken after program update to program that contains a closure externally called from the original one
