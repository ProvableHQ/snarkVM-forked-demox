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
