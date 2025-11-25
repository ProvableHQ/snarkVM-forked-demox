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

// This test tests self-recursive dynamic function calls for plaintext types by computing Fibonacci numbers.
#[test]
fn test_fibonacci() {
    let recursive_calls_program_name = Identifier::<CurrentNetwork>::from_str("recursive_calls").unwrap();
    let recursive_calls_program_field = recursive_calls_program_name.to_field().unwrap();

    let fibonacci_function_name = Identifier::<CurrentNetwork>::from_str("fibonacci").unwrap();
    let fibonacci_function_field = fibonacci_function_name.to_field().unwrap();

    let base_function_name = Identifier::<CurrentNetwork>::from_str("base").unwrap();
    let base_function_field = base_function_name.to_field().unwrap();

    let aleo_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();

    // Define the swap program.
    let recursive_calls_program_str = format!(
        r"
        program {recursive_calls_program_name}.aleo; 

        // The recursive case for Fibonacci numbers.
        function {fibonacci_function_name}:
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
        function {base_function_name}:
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
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(caller_private_key).unwrap();

    // Initialize the VM at the V12 height.
    let v12_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V12).unwrap();
    let vm = crate::vm::test_helpers::sample_vm_at_height(v12_height, rng);

    let fibonacci_index = 5;
    let expected_num_transitions = 15;

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
        println!("Executing {recursive_calls_program_name}.aleo/{fibonacci_function_name}...");
        let transaction = vm
            .execute(
                &caller_private_key,
                (format!("{recursive_calls_program_name}.aleo"), fibonacci_function_name),
                vec![Value::from_str(&format!("{fibonnaci_index}u64")).unwrap()].into_iter(),
                None,
                0,
                None,
                rng,
            )
            .unwrap();
        let execution = transaction.execution().unwrap();
        assert_eq!(execution.transitions().into_iter().count(), expected_num_transitions);
        println!("last transition outputs: {:?}", execution.transitions().into_iter().last().unwrap().outputs());
        assert_eq!(
            execution
                .transitions()
                .into_iter()
                .last()
                .unwrap()
                .outputs()
                .into_iter()
                .find_map(|output| match output {
                    Output::Public(_, Some(plaintext)) => Some(plaintext),
                    _ => None,
                })
                .unwrap(),
            &Plaintext::from_str(&format!("{expected_output}u64")).unwrap()
        );
        add_and_test(&vm, &caller_private_key, &[transaction], rng);
    }

    // TODO (dynamic_dispatch): do we have a way to check the output without finalize blocks?
}

// This test verifies that recursive double-spends fail as expected.
// In this test, we have:
// - a function `one` that takes in a static record, re-casts the record, and outputs the static record.
// - a function `two` that takes in a static record and returns nothing.
// - a function `three` that takes in a dynamic record and outputs the dynamic record.
// - a function `four` that takes in a dynamic record and returns nothing.
// - a function `five` that takes in a dynamic record and calls `two` twice. This should fail due to double-spend.
// - a function `six` that takes in a dynamic record and calls `four` twice. This should pass because the record is dynamic.
// - a function `seven` that takes in a dynamic record and index. If the index is zero it calls `two`, else it calls itself recursively with index - 1. This should pass until the index exceeds the maximum call depth.
// - a function `eight` that takes in a dynamic record and index. The function first calls `two`, then either calls `four` if index is zero, or calls itself recursively with index - 1. This should pass if the index is zero and fail otherwise due to double-spend.
// - a function `nine` that takes in a dynamic record and index. The function first calls `one`, then either calls `three` if the index is zero, or calls itself recursively with index - 1. This should pass if the index is zero and fail otherwise due to record not existing.
#[test]
fn test_recursive_dynamic_record_calls() {
    todo!()
}
