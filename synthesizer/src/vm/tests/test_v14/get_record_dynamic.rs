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

#[test]
fn test_get_record_dynamic() {
    // Parameters for dynamic function calls
    let program_name_str = "warehouse";
    let network_str = "aleo";
    let mint_nineties_bleach_function_str = "mint_nineties_bleach";
    let mint_fake_compliance_cert_function_str = "mint_fake_compliance_cert";

    let program_name_field = Identifier::<CurrentNetwork>::from_str(program_name_str).unwrap().to_field().unwrap();
    let network_field = Identifier::<CurrentNetwork>::from_str(network_str).unwrap().to_field().unwrap();
    let mint_nineties_bleach_function_field =
        Identifier::<CurrentNetwork>::from_str(mint_nineties_bleach_function_str).unwrap().to_field().unwrap();
    let mint_fake_compliance_cert_function_field =
        Identifier::<CurrentNetwork>::from_str(mint_fake_compliance_cert_function_str).unwrap().to_field().unwrap();

    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    // Initialize a new program.
    let program_str = format!(
        r"
        program {program_name_str}.aleo;

        struct safety_struct:
            first as field;
            second as field;

        record consumable:
            owner as address.private;
            // D, M, Y
            expiry_date as [u8; 3u32].private;
            critical as boolean.public;
            // D, M, Y
            production_date as [u8; 3u32].public;

        record non_consumable:
            owner as address.private;
            amount as u64.private;
            producer_country_code as u16.public;
            producer_pk as group.private;
            id as field.public;
            production_date as [u8; 3u32].public;
            safety as safety_struct.public;

        function production_month:
            input r0 as dynamic.record;
            get.record.dynamic r0.production_date into r1 as [u8; 3u32];
            output r1[1u32] as u8.public;

        function production_month_as_u16:
            input r0 as dynamic.record;
            get.record.dynamic r0.production_date into r1 as [u16; 3u32];
            output r1[1u32] as u16.public;
        
        function production_year_difference:
            call.dynamic {program_name_field} {network_field} {mint_nineties_bleach_function_field} into r0 (as dynamic.record);
            call.dynamic {program_name_field} {network_field} {mint_fake_compliance_cert_function_field} into r1 (as dynamic.record);
            
            get.record.dynamic r0.production_date into r2 as [u8; 3u32];
            get.record.dynamic r1.production_date into r3 as [u8; 3u32];

            sub r2[2u32] r3[2u32] into r4;

            output r4 as u8.public;

        function {mint_nineties_bleach_function_str}:
            cast 10u8 9u8 92u8 into r0 as [u8; 3u32];
            cast 10u8 9u8 42u8 into r1 as [u8; 3u32];

            cast {caller_address} r0 true r1 into r2 as consumable.record;

            output r2 as consumable.record;

        function {mint_fake_compliance_cert_function_str}:
            cast 11u8 7u8 17u8 into r0 as [u8; 3u32];
            cast 10field 13field into r1 as safety_struct;

            cast {caller_address} 2u64 91u16 2group 1field r0 r1 into r2 as non_consumable.record;

            output r2 as non_consumable.record;

        function read_producer_country:
            input r0 as dynamic.record;
            get.record.dynamic r0.producer_country_code into r1 as u16;
            output r1 as u16.public;

        constructor:
            assert.eq true true;
        "
    );

    let program = Program::<CurrentNetwork>::from_str(&program_str).unwrap();

    // Initialize the VM.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    // Deploy the program.
    println!("Deploying program warehouse.aleo...");
    let transaction_deploy = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction_deploy], rng);

    /************** Case 1: Simple read **************/

    let record_static_str = format!(
        r#"{{
        owner: {caller_address}.private,
        expiry_date: [29u8.private, 2u8.private, 25u8.private],
        critical: false.public,
        production_date: [10u8.private, 7u8.private, 87u8.private],
        _nonce: 0group.public,
        _version: 1u8.public
    }}"#
    );

    let record_static = Record::<CurrentNetwork, Plaintext<CurrentNetwork>>::from_str(&record_static_str).unwrap();
    let record_dynamic = DynamicRecord::<CurrentNetwork>::from_record(&record_static).unwrap();

    println!("Executing root function warehouse.aleo/production_month...");
    let transaction_1 = vm
        .execute(
            &caller_private_key,
            ("warehouse.aleo", "production_month"),
            vec![Value::<CurrentNetwork>::DynamicRecord(record_dynamic.clone())].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    let expected_output = Plaintext::<CurrentNetwork>::from_str("7u8").unwrap();

    assert!(
        matches!(transaction_1.transitions().next().unwrap().outputs(), [Output::Public(_, Some(plaintext))] if *plaintext == expected_output),
        "Expected output: {:?}, got: {:?}",
        expected_output,
        transaction_1.transitions().next().unwrap().outputs()
    );

    add_and_test(&vm, &caller_private_key, &[transaction_1], rng);

    /************** Case 2: Read from minted records (using polymorphy) **************/

    // In this case a function outputs two static records of different types that are received as
    // dynamic by the caller; and the caller then proceeds to field two fields with the same name
    // that the two static-record types happen to have.

    println!("Executing root function warehouse.aleo/production_year_difference...");
    let transaction_2 = vm
        .execute(
            &caller_private_key,
            ("warehouse.aleo", "production_year_difference"),
            Vec::<Value<CurrentNetwork>>::new().into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    let expected_output = Plaintext::<CurrentNetwork>::from_str("25u8").unwrap();

    assert!(
        // The first two transactions correspond to the two minting operations
        matches!(transaction_2.transitions().nth(2).unwrap().outputs(), [Output::Public(_, Some(plaintext))] if *plaintext == expected_output),
        "Expected output: {:?}, got: {:?}",
        expected_output,
        transaction_2.transitions().next().unwrap().outputs()
    );

    add_and_test(&vm, &caller_private_key, &[transaction_2], rng);

    /************** Case 3: Various incorrect readings **************/

    // We trigger get.record.dynamic failures in various ways

    // Case 3.1: We attempt to read the field "producer_country_code" from a
    // dynamic record derived from a static consumable.record, which does not
    // have one.

    println!("Executing root function warehouse.aleo/read_producer_country (should fail)...");

    assert!(
        vm.execute(
            &caller_private_key,
            ("warehouse.aleo", "read_producer_country"),
            vec![Value::<CurrentNetwork>::DynamicRecord(record_dynamic.clone())].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap_err()
        .to_string()
        .contains("does not contain entry entry producer_country_code")
    );

    // Case 3.2: We manipulate the root of the already created dynamic record,
    // which will cause the Merkle root to fail in a read which would otherwise
    // succeed. Note that failure already occurs at the (honest) prover side.
    let manipulated_record_dynamic = DynamicRecord::new_unchecked(
        *record_dynamic.owner(),
        Uniform::rand(rng),
        *record_dynamic.nonce(),
        *record_dynamic.version(),
        record_dynamic.data().clone(),
    );

    println!("Executing root function warehouse.aleo/production_month (should fail)...");

    assert!(
        vm.execute(
            &caller_private_key,
            ("warehouse.aleo", "production_month"),
            vec![Value::<CurrentNetwork>::DynamicRecord(manipulated_record_dynamic)].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap_err()
        .to_string()
        .contains("root in the dynamic record does not match")
    );

    // Case 3.3: We attempt to read the field "production_date" as an array of
    // u16 instead of the actual u8.
    println!("Executing root function warehouse.aleo/production_month_as_u16 (should fail)...");

    assert!(
        vm.execute(
            &caller_private_key,
            ("warehouse.aleo", "production_month_as_u16"),
            vec![Value::<CurrentNetwork>::DynamicRecord(record_dynamic.clone())].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap_err()
        .to_string()
        .contains("Type mismatch")
    );

    // Case 3.4: We attempt to read the field "production_date" from a different
    // leaf index than it had when the Merkle root was computed.

    let mut manipulated_record_data = record_dynamic.data().clone().unwrap();
    assert!(
        manipulated_record_data
            .get_index_of(&Identifier::<CurrentNetwork>::from_str("production_date").unwrap())
            .unwrap()
            == 2
    );
    manipulated_record_data.swap_indices(1, 2);

    let manipulated_record_dynamic_2 = DynamicRecord::new_unchecked(
        *record_dynamic.owner(),
        Uniform::rand(rng),
        *record_dynamic.nonce(),
        *record_dynamic.version(),
        Some(manipulated_record_data),
    );

    println!("Executing root function warehouse.aleo/production_month (should fail)...");

    assert!(
        vm.execute(
            &caller_private_key,
            ("warehouse.aleo", "production_month"),
            vec![Value::<CurrentNetwork>::DynamicRecord(manipulated_record_dynamic_2)].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap_err()
        .to_string()
        .contains("root in the dynamic record does not match")
    );
}

// Translates the output of a call to credits.aleo/transfer_public_to_private
// into a dynamic record to ensure signature verification has not been broken by
// the new caller metadata
#[test]
fn translate_transfer_public_to_private() {
    let credits_program_str = Identifier::<CurrentNetwork>::from_str("credits").unwrap();
    let network_str = "aleo";
    let transfer_function_name = Identifier::<CurrentNetwork>::from_str("transfer_public_to_private").unwrap();
    let credits_field = credits_program_str.to_field().unwrap();
    let network_field = Identifier::<CurrentNetwork>::from_str(network_str).unwrap().to_field().unwrap();
    let transfer_function_field = transfer_function_name.to_field().unwrap();

    let program_str = format!(
        r"
        program dynamic_credits.aleo;

        // Calls transfer_public_to_private and publicly outputs amount
        function transfer_pub_priv_and_inform:
            input r0 as u64.private;

            call.dynamic {credits_field} {network_field} {transfer_function_field}
                with self.caller r0 (as address.private u64.public)
                into r1 r2 (as dynamic.record dynamic.future);

            get.record.dynamic r1.microcredits into r3 as u64;

            async transfer_pub_priv_and_inform r2 into r4;

            output r3 as u64.public;
            output r4 as dynamic_credits.aleo/transfer_pub_priv_and_inform.future;

        finalize transfer_pub_priv_and_inform:
            input r0 as dynamic.future;
            await r0;

        constructor:
            assert.eq true true;
    "
    );

    let program = Program::<CurrentNetwork>::from_str(&program_str).unwrap();

    let rng = &mut TestRng::default();

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    let caller_private_key = sample_genesis_private_key(rng);
    let address = Address::try_from(&caller_private_key).unwrap();
    println!("Caller address: {address}");

    // Deploy the program
    println!("Deploying program dynamic_credits.aleo...");
    let transaction_deploy = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction_deploy], rng);

    // Print the initial balance
    let Some(Value::Plaintext(Plaintext::Literal(Literal::U64(initial_balance), _))) = vm
        .finalize_store()
        .get_value_confirmed(
            ProgramID::<CurrentNetwork>::from_str("credits.aleo").unwrap(),
            Identifier::from_str("account").unwrap(),
            &Plaintext::from_str(&address.to_string()).unwrap(),
        )
        .unwrap()
    else {
        panic!("Failed to get initial balance");
    };
    println!("Initial balance: {initial_balance}");

    // Deposit some credits to the program.
    let transaction = vm
        .execute(
            &caller_private_key,
            ("credits.aleo", "transfer_public"),
            vec![
                Value::from_str(
                    &ProgramID::<CurrentNetwork>::from_str("dynamic_credits.aleo")
                        .unwrap()
                        .to_address()
                        .unwrap()
                        .to_string(),
                )
                .unwrap(),
                Value::<CurrentNetwork>::from_str("57u64").unwrap(),
            ]
            .into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    // Execute the dynamic call.
    println!("Executing root function dynamic_credits.aleo/transfer_pub_priv_and_inform...");
    let transaction_transfer = vm
        .execute(
            &caller_private_key,
            ("dynamic_credits.aleo", "transfer_pub_priv_and_inform"),
            vec![Value::<CurrentNetwork>::from_str("57u64").unwrap()].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction_transfer], rng);
}
