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

// This test tests self-recursive dynamic function calls for plaintext types by computing Fibonacci numbers.
#[test]
fn test_fibonacci() {
    let recursive_calls_program_name = Identifier::<CurrentNetwork>::from_str("recursive_calls").unwrap();
    let recursive_calls_program_field = recursive_calls_program_name.to_field().unwrap();

    let fibonacci_name = Identifier::<CurrentNetwork>::from_str("fibonacci").unwrap();
    let fibonacci_function_field = fibonacci_name.to_field().unwrap();

    let base_name = Identifier::<CurrentNetwork>::from_str("base").unwrap();
    let base_function_field = base_name.to_field().unwrap();

    let aleo_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();

    // Define the swap program.
    let recursive_calls_program_str = format!(
        r"
        program {recursive_calls_program_name}.aleo; 

        // The recursive case for Fibonacci numbers.
        function {fibonacci_name}:
            input r0 as u64.public;

            // Determine whether the input is zero or one.
            is.eq r0 0u64 into r1;
            is.eq r0 1u64 into r2;
            or r1 r2 into r3;

            // Subtract 1 and 2 from the current index for the recursive cases.
            sub.w r0 1u64 into r4;
            sub.w r0 2u64 into r5;

            // Select the inputs and function based on whether we are handling the recursive case or base case.
            ternary r3 r0 r4 into r6;
            ternary r3 r0 r5 into r7;
            ternary r3 {base_function_field} {fibonacci_function_field} into r8;

            // Call fibonnaci(r0 - 1)
            call.dynamic {recursive_calls_program_field} {aleo_field} r8 with r6 (as u64.public) into r9 (as u64.public);

            // Call fibonacci(r0 - 2)
            call.dynamic {recursive_calls_program_field} {aleo_field} r8 with r7 (as u64.public) into r10 (as u64.public);

            // Return the sum.
            add r9 r10 into r11;
            ternary r3 r9 r11 into r12;
            output r12 as u64.public;

        // The base case for Fibonacci numbers.
        function {base_name}:
            input r0 as u64.public;
            output r0 as u64.public;

        constructor:
            assert.eq true true;
    "
    );

    // Parse program
    let recursive_calls_program = Program::<CurrentNetwork>::from_str(&recursive_calls_program_str).unwrap();

    // Initialize an RNG.
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = crate::vm::test_helpers::sample_genesis_private_key(rng);

    // Initialize the VM at the V12 height.
    let v12_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap();
    let vm = crate::vm::test_helpers::sample_vm_at_height(v12_height, rng);

    // Deploy the program
    println!("Deploying program {recursive_calls_program_name}.aleo...");
    let deployment = vm.deploy(&caller_private_key, &recursive_calls_program, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deployment], rng);

    // Execute the the fibonacci function for the given inputs, expected output, and expected number of transitions.
    #[rustfmt::skip]
    let test_cases = [
        (0, 0, 3), 
        (1, 1, 3), 
        (2, 1, 7), 
        (3, 2, 11), 
        (4, 3, 19)
    ];
    for (fibonnaci_index, expected_output, expected_num_transitions) in test_cases {
        println!("Executing {recursive_calls_program_name}.aleo/{fibonacci_name}...");
        let transaction = vm
            .execute(
                &caller_private_key,
                (format!("{recursive_calls_program_name}.aleo"), fibonacci_name),
                vec![Value::from_str(&format!("{fibonnaci_index}u64")).unwrap()].into_iter(),
                None,
                0,
                None,
                rng,
            )
            .unwrap();
        let execution = transaction.execution().unwrap();
        assert_eq!(execution.transitions().count(), expected_num_transitions);
        assert_eq!(
            execution
                .transitions()
                .last()
                .unwrap()
                .outputs()
                .iter()
                .find_map(|output| match output {
                    Output::Public(_, Some(plaintext)) => Some(plaintext),
                    _ => None,
                })
                .unwrap(),
            &Plaintext::from_str(&format!("{expected_output}u64")).unwrap()
        );
        add_and_test(&vm, &caller_private_key, &[transaction], rng);
    }
}

// Tests recursive double-spend detection with static/dynamic records across various call patterns.
//
// This test defines the following functions:
// - `one`: Takes a static record, re-casts it, and outputs the static record.
// - `two`: Takes a static record and returns nothing.
// - `three`: Takes a dynamic record and outputs the dynamic record.
// - `four`: Takes a dynamic record and returns nothing.
// - `five`: Takes a dynamic record and calls `two` twice. This should fail due to double-spend.
// - `six`: Takes a dynamic record and calls `four` twice. This should pass because the record is dynamic.
// - `seven`: Takes a dynamic record and index. If index is zero it calls `two`, else it calls itself recursively with index - 1. This should pass until the index exceeds the maximum call depth.
// - `eight`: Takes a dynamic record and index. First calls `two`, then either calls `four` if index is zero, or calls itself recursively with index - 1. This should pass if index is zero and fail otherwise due to double-spend.
// - `nine`: Takes a dynamic record and index. First calls `one`, then either calls `three` if index is zero, or calls itself recursively with index - 1 and the new record. This should pass as long as transitions do not exceed the maximum allowed.
//
// TODO (@reviewers): Verify that consumption of local records is expected behavior in recursive calls.
#[test]
fn test_recursive_dynamic_record_calls() {
    // Initialize an RNG.
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = crate::vm::test_helpers::sample_genesis_private_key(rng);
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(caller_private_key).unwrap();
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    // Initialize the VM at the V12 height.
    let v12_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap();
    let vm = crate::vm::test_helpers::sample_vm_at_height(v12_height, rng);

    // Define the first program that defines a record and functions `one`, `two`, `three`, and `four`.
    let basic_records_ops_program_name = Identifier::<CurrentNetwork>::from_str("basic_record_ops").unwrap();

    let one_name = Identifier::<CurrentNetwork>::from_str("one").unwrap();
    let one_field = one_name.to_field().unwrap();

    let two_name = Identifier::<CurrentNetwork>::from_str("two").unwrap();
    let two_field = two_name.to_field().unwrap();

    let three_name = Identifier::<CurrentNetwork>::from_str("three").unwrap();

    let four_name = Identifier::<CurrentNetwork>::from_str("four").unwrap();
    let four_field = four_name.to_field().unwrap();

    let two_indexed_name = Identifier::<CurrentNetwork>::from_str("two_indexed").unwrap();
    let two_indexed_field = two_indexed_name.to_field().unwrap();

    let three_indexed_name = Identifier::<CurrentNetwork>::from_str("three_indexed").unwrap();
    let three_indexed_field = three_indexed_name.to_field().unwrap();

    let four_indexed_name = Identifier::<CurrentNetwork>::from_str("four_indexed").unwrap();
    let four_indexed_field = four_indexed_name.to_field().unwrap();

    let basic_records_ops_program_str = format!(
        r"
program {basic_records_ops_program_name}.aleo;

record Data:
    owner as address.private;
    data as u64.private;

function mint:
    input r0 as address.private;
    input r1 as u64.private;
    cast r0 r1 into r2 as Data.record;
    output r2 as Data.record;

function {one_name}:
    input r0 as Data.record;
    cast r0.owner r0.data into r1 as Data.record;
    output r1 as Data.record;

function {two_name}:
    input r0 as Data.record;

function {three_name}:
    input r0 as record.dynamic;
    output r0 as record.dynamic;

function {four_name}:
    input r0 as record.dynamic;

function {two_indexed_name}:
    input r0 as record.dynamic;
    input r1 as u8.public;

function {three_indexed_name}:
    input r0 as record.dynamic;
    input r1 as u8.public;
    output r0 as record.dynamic;

function {four_indexed_name}:
    input r0 as record.dynamic;
    input r1 as u8.public;

constructor:
    assert.eq true true;
"
    );

    // Define the second program that defines functions `five`, `six`, `seven`, `eight`, and `nine`.
    let test_functions_program_name = Identifier::<CurrentNetwork>::from_str("test_functions").unwrap();

    let five_name = Identifier::<CurrentNetwork>::from_str("five").unwrap();

    let six_name = Identifier::<CurrentNetwork>::from_str("six").unwrap();

    let seven_name = Identifier::<CurrentNetwork>::from_str("seven").unwrap();
    let seven_field = seven_name.to_field().unwrap();

    let eight_name = Identifier::<CurrentNetwork>::from_str("eight").unwrap();
    let eight_field = eight_name.to_field().unwrap();

    let nine_name = Identifier::<CurrentNetwork>::from_str("nine").unwrap();
    let nine_field = nine_name.to_field().unwrap();

    let aleo_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();
    let basic_records_ops_program_field = basic_records_ops_program_name.to_field().unwrap();
    let test_functions_program_field = test_functions_program_name.to_field().unwrap();

    let test_functions_program_str = format!(
        r"
program {test_functions_program_name}.aleo;

function {five_name}:
    input r0 as record.dynamic;
    call.dynamic {basic_records_ops_program_field} {aleo_field} {two_field} with r0 (as record.dynamic);
    call.dynamic {basic_records_ops_program_field} {aleo_field} {two_field} with r0 (as record.dynamic);

function {six_name}:
    input r0 as record.dynamic;
    call.dynamic {basic_records_ops_program_field} {aleo_field} {four_field} with r0 (as record.dynamic);
    call.dynamic {basic_records_ops_program_field} {aleo_field} {four_field} with r0 (as record.dynamic);

function {seven_name}:
    input r0 as record.dynamic;
    input r1 as u8.public;
    is.eq r1 0u8 into r2;
    sub.w r1 1u8 into r3;
    ternary r2 {two_indexed_field} {seven_field} into r4;
    ternary r2 {basic_records_ops_program_field} {test_functions_program_field} into r5;
    call.dynamic r5 {aleo_field} r4 with r0 r3 (as record.dynamic u8.public);

function {eight_name}:
    input r0 as record.dynamic;
    input r1 as u8.public;
    call.dynamic {basic_records_ops_program_field} {aleo_field} {two_field} with r0 (as record.dynamic);
    is.eq r1 0u8 into r2;
    sub.w r1 1u8 into r3;
    ternary r2 {four_indexed_field} {eight_field} into r4;
    ternary r2 {basic_records_ops_program_field} {test_functions_program_field} into r5;
    call.dynamic r5 {aleo_field} r4 with r0 r3 (as record.dynamic u8.public);

function {nine_name}:
    input r0 as record.dynamic;
    input r1 as u8.public;
    call.dynamic {basic_records_ops_program_field} {aleo_field} {one_field} with r0 (as record.dynamic) into r2 (as record.dynamic);
    is.eq r1 0u8 into r3;
    sub.w r1 1u8 into r4;
    ternary r3 {three_indexed_field} {nine_field} into r5;
    ternary r3 {basic_records_ops_program_field} {test_functions_program_field} into r6;
    call.dynamic r6 {aleo_field} r5 with r2 r4 (as record.dynamic u8.public) into r7 (as record.dynamic);
    output r7 as record.dynamic;

constructor:
    assert.eq true true;
    "
    );

    // Deploy the programs.
    let basic_records_ops_program = Program::<CurrentNetwork>::from_str(&basic_records_ops_program_str).unwrap();
    let test_functions_program = Program::<CurrentNetwork>::from_str(&test_functions_program_str).unwrap();

    println!("Deploying program {basic_records_ops_program_name}.aleo...");
    let deployment1 = vm.deploy(&caller_private_key, &basic_records_ops_program, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deployment1], rng);

    println!("Deploying program {test_functions_program_name}.aleo...");
    let deployment2 = vm.deploy(&caller_private_key, &test_functions_program, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deployment2], rng);

    // A helper function to mint a record for the caller.
    let mint_record = |rng: &mut TestRng| {
        println!("Minting record...");
        let mint_transaction = vm
            .execute(
                &caller_private_key,
                (
                    format!("{basic_records_ops_program_name}.aleo"),
                    Identifier::<CurrentNetwork>::from_str("mint").unwrap(),
                ),
                vec![Value::from_str(&caller_address.to_string()).unwrap(), Value::from_str("100u64").unwrap()]
                    .into_iter(),
                None,
                0,
                None,
                rng,
            )
            .unwrap();
        let execution = mint_transaction.execution().unwrap();
        let minted_record = execution
            .transitions()
            .last()
            .unwrap()
            .outputs()
            .iter()
            .find_map(|output| match output {
                Output::Record(_, _, Some(record), _) => Some(record.decrypt(&caller_view_key).unwrap()),
                _ => None,
            })
            .unwrap();
        add_and_test(&vm, &caller_private_key, &[mint_transaction], rng);

        minted_record
    };

    // A helper function to execute a function and check if it succeeds or fails as expected.
    let execute_and_check = |function_name: Identifier<CurrentNetwork>,
                             inputs: Vec<Value<CurrentNetwork>>,
                             should_succeed: bool,
                             test_description: &str,
                             rng: &mut TestRng| {
        println!("{test_description}");
        let result = vm.execute(
            &caller_private_key,
            (format!("{test_functions_program_name}.aleo"), function_name),
            inputs.into_iter(),
            None,
            0,
            None,
            rng,
        );

        if should_succeed {
            let transaction = result.unwrap_or_else(|_| panic!("Expected {function_name} to succeed"));
            add_and_test(&vm, &caller_private_key, &[transaction], rng);
        } else if let Ok(transaction) = result {
            // Check that the transaction fails during addition to the ledger.
            let block = sample_next_block(&vm, &caller_private_key, &[transaction], rng).unwrap();
            assert_eq!(block.transactions().num_accepted(), 0);
            assert_eq!(block.transactions().num_rejected(), 0);
            assert_eq!(block.aborted_transaction_ids().len(), 1);
            vm.add_next_block(&block).unwrap();
        }
    };

    // Test function `five` which should fail due to double-spend.
    execute_and_check(
        five_name,
        vec![Value::DynamicRecord(DynamicRecord::from_record(&mint_record(rng)).unwrap())],
        false,
        &format!("Testing function {five_name} which should fail due to double-spend"),
        rng,
    );

    // Test function `six` which should pass because the record is dynamic.
    execute_and_check(
        six_name,
        vec![Value::DynamicRecord(DynamicRecord::from_record(&mint_record(rng)).unwrap())],
        true,
        &format!("Testing function {six_name} which should pass because the record is dynamic"),
        rng,
    );

    // Test function `seven` at a valid depth which should pass.
    {
        let test_index = 5u8;
        execute_and_check(
            seven_name,
            vec![
                Value::DynamicRecord(DynamicRecord::from_record(&mint_record(rng)).unwrap()),
                Value::from_str(&format!("{test_index}u8")).unwrap(),
            ],
            true,
            &format!("Testing function {seven_name} at index {test_index} which should pass"),
            rng,
        );
    }

    // Test function `seven` at the maximum valid depth which should pass.
    {
        let test_index = Transaction::<CurrentNetwork>::MAX_TRANSITIONS - 3; // Account for the fee transition and zero indexing.
        execute_and_check(
            seven_name,
            vec![
                Value::DynamicRecord(DynamicRecord::from_record(&mint_record(rng)).unwrap()),
                Value::from_str(&format!("{test_index}u8")).unwrap(),
            ],
            true,
            &format!("Testing function {seven_name} at index {test_index} which should pass"),
            rng,
        );
    }

    // Test function `seven` at the maximum call depth which should fail.
    {
        let test_index = Transaction::<CurrentNetwork>::MAX_TRANSITIONS - 2; // Account for the fee transition and zero indexing.
        execute_and_check(
            seven_name,
            vec![
                Value::DynamicRecord(DynamicRecord::from_record(&mint_record(rng)).unwrap()),
                Value::from_str(&format!("{test_index}u8")).unwrap(),
            ],
            false,
            &format!(
                "Testing function {seven_name} at index {test_index} which should fail due to exceeding max call depth"
            ),
            rng,
        );
    }

    // Test function `eight` at index zero which should pass.
    execute_and_check(
        eight_name,
        vec![
            Value::DynamicRecord(DynamicRecord::from_record(&mint_record(rng)).unwrap()),
            Value::from_str("0u8").unwrap(),
        ],
        true,
        &format!("Testing function {eight_name} at index 0 which should pass"),
        rng,
    );

    // Test function `eight` at index one which should fail due to double-spend.
    execute_and_check(
        eight_name,
        vec![
            Value::DynamicRecord(DynamicRecord::from_record(&mint_record(rng)).unwrap()),
            Value::from_str("1u8").unwrap(),
        ],
        false,
        &format!("Testing function {eight_name} at index 1 which should fail due to double-spend"),
        rng,
    );

    // Test function `nine` at index zero which should pass.
    execute_and_check(
        nine_name,
        vec![
            Value::DynamicRecord(DynamicRecord::from_record(&mint_record(rng)).unwrap()),
            Value::from_str("0u8").unwrap(),
        ],
        true,
        &format!("Testing function {nine_name} at index 0 which should pass"),
        rng,
    );

    // Test function `nine` at index one which should pass.
    execute_and_check(
        nine_name,
        vec![
            Value::DynamicRecord(DynamicRecord::from_record(&mint_record(rng)).unwrap()),
            Value::from_str("1u8").unwrap(),
        ],
        true,
        &format!("Testing function {nine_name} at index 1 which should pass"),
        rng,
    );

    // Test function `nine` at index two which should pass.
    execute_and_check(
        nine_name,
        vec![
            Value::DynamicRecord(DynamicRecord::from_record(&mint_record(rng)).unwrap()),
            Value::from_str("2u8").unwrap(),
        ],
        true,
        &format!("Testing function {nine_name} at index 2 which should pass"),
        rng,
    );

    // Test function `nine` at index 15 which should fail due to exceeding the maximum number of transitions.
    execute_and_check(
        nine_name,
        vec![
            Value::DynamicRecord(DynamicRecord::from_record(&mint_record(rng)).unwrap()),
            Value::from_str("15u8").unwrap(),
        ],
        false,
        &format!(
            "Testing function {nine_name} at index 15 which should fail due to exceeding the maximum number of transitions"
        ),
        rng,
    );
}
