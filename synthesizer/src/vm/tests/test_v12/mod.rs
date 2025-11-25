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

mod recursion;

use super::*;

use crate::vm::test_helpers::{sample_vm_at_height, *};

use anyhow::Result;
use console::{
    account::{Address, ViewKey},
    network::ConsensusVersion,
    program::{DynamicRecord, Entry, Identifier, OutputID, Value},
};
use snarkvm_synthesizer_program::Program;
use snarkvm_utilities::TestRng;

// TODOs:
// - test the case with the interface of a dynamic call doesn't match the mode.

fn get_main_field(output_id: OutputID<CurrentNetwork>) -> Field<CurrentNetwork> {
    match output_id {
        OutputID::Constant(field)
        | OutputID::Public(field)
        | OutputID::Private(field)
        | OutputID::Record(field, _, _)
        | OutputID::ExternalRecord(field)
        | OutputID::Future(field)
        | OutputID::DynamicRecord(field)
        | OutputID::DynamicFuture(field) => field,
    }
}

fn add_and_test(
    vm: &VM<CurrentNetwork, LedgerType>,
    caller_private_key: &PrivateKey<CurrentNetwork>,
    transactions: &[Transaction<CurrentNetwork>],
    rng: &mut TestRng,
) {
    for (index, transaction) in transactions.iter().enumerate() {
        vm.check_transaction(transaction, None, rng).map_err(|e| {
            anyhow!("Transaction {index} check failed: {e}")
        }).unwrap();
    }
    let block = sample_next_block(vm, caller_private_key, transactions, rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), transactions.len());
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();
}

fn test_translation(
    caller_private_key: &PrivateKey<CurrentNetwork>,
    // Program and function to call
    root_program_name: &str,
    root_function_name: &str,
    // Inputs to the root call; if None gas_to_mint is used as explained below.
    input_values: Option<Vec<Value<CurrentNetwork>>>,
    // If Some, precedes the root call with a transaction that mints the given
    // gas_container record and uses the corresponding dynamic record as input
    // to the root call.
    gas_to_mint: Option<Record<CurrentNetwork, Plaintext<CurrentNetwork>>>,
    // The expected outputs.
    expected_public_outputs: Option<Vec<Plaintext<CurrentNetwork>>>,
    rng: &mut TestRng,
) {
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(caller_private_key).unwrap();
    let caller_address = Address::<CurrentNetwork>::try_from(caller_private_key).unwrap();

    // Various parameters for dynamic.call instructions.
    let program_a_name_str = "flow";
    let program_a_name_field = Identifier::<CurrentNetwork>::from_str(program_a_name_str).unwrap().to_field().unwrap();
    let program_b_name_str = "gas_manager";
    let program_b_name_field = Identifier::<CurrentNetwork>::from_str(program_b_name_str).unwrap().to_field().unwrap();
    let network_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();

    let get_liquid_liters_function_name = Identifier::<CurrentNetwork>::from_str("get_liquid_liters").unwrap();
    let get_gas_liters_function_name = Identifier::<CurrentNetwork>::from_str("get_gas_liters").unwrap();
    let consume_dynamic_blob_function_name = Identifier::<CurrentNetwork>::from_str("consume_dynamic_blob").unwrap();
    let nitrogen_pump_function_name = Identifier::<CurrentNetwork>::from_str("nitrogen_pump").unwrap();
    let get_external_liters_function_name = Identifier::<CurrentNetwork>::from_str("get_external_liters").unwrap();
    let gas_pipe_function_name = Identifier::<CurrentNetwork>::from_str("gas_pipe").unwrap();

    let get_liquid_liters_function_field = get_liquid_liters_function_name.to_field().unwrap();
    let get_gas_liters_function_field = get_gas_liters_function_name.to_field().unwrap();
    let consume_dynamic_blob_function_field = consume_dynamic_blob_function_name.to_field().unwrap();
    let nitrogen_pump_function_field = nitrogen_pump_function_name.to_field().unwrap();
    let get_external_liters_function_field = get_external_liters_function_name.to_field().unwrap();
    let gas_pipe_function_field = gas_pipe_function_name.to_field().unwrap();

    let program_a_string = format!(
        r"
    import {program_b_name_str}.aleo;

    program {program_a_name_str}.aleo;

    // Tries to consume a container passed as dynamic as a specifically liquid one
    function get_dynamic_liters_from_liquid:
        input r0 as dynamic.record;
        call.dynamic {program_b_name_field} {network_field} {get_liquid_liters_function_field} with r0 (as dynamic.record) into r1 (as u64.public);
        output r1 as u64.public;
    
    function get_dynamic_liters_from_gas:
        input r0 as dynamic.record;
        call.dynamic {program_b_name_field} {network_field} {get_gas_liters_function_field} with r0 (as dynamic.record) into r1 (as u64.public);
        output r1 as u64.public;

    function consume_dynamic_blob:
        input r0 as dynamic.record;
        output true as boolean.private;

    function dynamic_pump:
        call.dynamic {program_b_name_field} {network_field} {nitrogen_pump_function_field} with 1u64 (as u64.public) into r0 (as dynamic.record);
        output r0 as dynamic.record;

    // Get the liters in an external liquid record
    function {get_external_liters_function_name}:
        input r0 as {program_b_name_str}.aleo/gas_container.record;
        output r0.liters as u64.public;

    // Input and output the same gas record
    function {gas_pipe_function_name}:
        input r0 as {program_b_name_str}.aleo/gas_container.record;
        output r0 as {program_b_name_str}.aleo/gas_container.record;

    constructor:
        assert.eq true true;
    "
    );

    // Preparing the record values for the hardcoded gas_record minter
    let (gas_owner, gas_liters, gas_flammable) = if let Some(gas_to_mint_record) = &gas_to_mint {
        let liters_entry =
            gas_to_mint_record.data().get(&Identifier::<CurrentNetwork>::from_str("liters").unwrap()).unwrap();
        let flammable_entry =
            gas_to_mint_record.data().get(&Identifier::<CurrentNetwork>::from_str("flammable").unwrap()).unwrap();
        let liters_value = match liters_entry {
            Entry::Public(plaintext) => plaintext.to_string(),
            _ => panic!("`liters` entry should be public"),
        };
        let flammable_value = match flammable_entry {
            Entry::Private(plaintext) => plaintext.to_string(),
            _ => panic!("`flammable` entry should be private"),
        };
        (caller_address.to_string(), liters_value, flammable_value)
    } else {
        (caller_address.to_string(), "100u64".to_string(), "false".to_string())
    };

    let program_b_string = format!(
        r"
    program {program_b_name_str}.aleo;

    record liquid_container:
        owner as address.private;
        liters as u64.public;

    record gas_container:
        owner as address.private;
        liters as u64.public;
        flammable as boolean.private;

    function consume_gas:
        input r0 as gas_container.record;
        call.dynamic {program_a_name_field} {network_field} {consume_dynamic_blob_function_field} with r0 (as gas_container.record) into r1 (as boolean.private);
        output r0.liters as u64.public;

    // function {get_liquid_liters_function_name}:
    //     input r0 as liquid_container.record;
    //     output r0.liters as u64.public;

    // function get_gas_liters_externally:
    //     input r0 as dynamic.record;
    //     call.dynamic {program_a_name_field} {network_field} {get_external_liters_function_field} with r0 (as dynamic.record) into r1 (as u64.public);
    //     output r1 as u64.public;

    function {get_gas_liters_function_name}:
        input r0 as gas_container.record;
        output r0.liters as u64.public;

    // function {nitrogen_pump_function_name}:
    //     input r0 as u64.public;
    //     cast self.caller r0 false into r1 as gas_container.record;
    //     output r1 as gas_container.record;

    // function hardcoded_gas_pump:
    //     cast {gas_owner} {gas_liters} {gas_flammable} into r0 as gas_container.record;
    //     output r0 as gas_container.record;

    // function pump_and_send_through_pipe:
    //     cast {gas_owner} {gas_liters} {gas_flammable} into r0 as gas_container.record;
    //     call.dynamic {program_a_name_field} {network_field} {gas_pipe_function_field} with r0 (as gas_container.record) into r1 (as dynamic.record);

    constructor:
        assert.eq true true;
    "
    );

    // Initialize a new program.
    let program_a = Program::<CurrentNetwork>::from_str(&program_a_string).unwrap();
    let program_b = Program::<CurrentNetwork>::from_str(&program_b_string).unwrap();

    // Initialize the VM.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V12).unwrap(), rng);

    // Deploy the programs.
    println!("Deploying program {program_b_name_str}...");
    let transaction_b = vm.deploy(&caller_private_key, &program_b, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction_b], rng);

    println!("Deploying program {program_a_name_str}...");
    let transaction_a = vm.deploy(&caller_private_key, &program_a, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction_a], rng);

    assert!(
        input_values.is_none() || gas_to_mint.is_none(),
        "When gas_to_mint is provided, the resulting static input is converted to dynamic record is used instead of input_values, which should be None",
    );

    assert!(
        input_values.is_some() || gas_to_mint.is_some(),
        "Exactly one of input_values or gas_to_mint must be provided",
    );

    let computed_input_values = input_values.unwrap_or_else(|| {
        println!("Minting gas_container record...");
        let transaction_mint = vm
            .execute(
                &caller_private_key,
                (format!("{program_b_name_str}.aleo"), "hardcoded_gas_pump"),
                Vec::<Value<CurrentNetwork>>::new().iter(),
                None,
                0,
                None,
                rng,
            )
            .unwrap();

        let mint_output = transaction_mint.transitions().next().unwrap().outputs().iter().next().unwrap();

        let output_gas_record = match mint_output {
            Output::Record(_, _, record_ciphertext, _) => {
                record_ciphertext.as_ref().unwrap().decrypt(&caller_view_key).unwrap()
            }
            _ => panic!("Minted record is not a record"),
        };

        let block_mint = sample_next_block(&vm, &caller_private_key, &[transaction_mint], rng).unwrap();
        assert_eq!(block_mint.transactions().num_accepted(), 1);
        assert_eq!(block_mint.transactions().num_rejected(), 0);
        assert_eq!(block_mint.aborted_transaction_ids().len(), 0);
        vm.add_next_block(&block_mint).unwrap();

        let dynamic_record = DynamicRecord::from_record(&output_gas_record).unwrap();
        vec![Value::<CurrentNetwork>::DynamicRecord(dynamic_record)]
    });

    println!("Executing root function {root_program_name}/{root_function_name}...");

    // Execute the root function.
    let transaction = vm
        .execute(
            &caller_private_key,
            (root_program_name, root_function_name),
            computed_input_values.into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    println!("Verifying transaction...");
    add_and_test(&vm, &caller_private_key, &[transaction.clone()], rng);

    println!("Asserting output correctness...");

    let output_ids = transaction.transitions().last().unwrap().output_ids().collect_vec();

    // TODO (dynamic_dispatch) reintroduce and fix
    // let public_outputs = transaction
    //     .transitions()
    //     .last()
    //     .unwrap()
    //     .outputs()
    //     .iter()
    //     .filter_map(|output| match output {
    //         Output::Public(_, Some(plaintext)) => Some(plaintext),
    //         _ => None,
    //     })
    //     .collect_vec();

    // if let Some(expected_public_outputs) = expected_public_outputs {
    //     assert_eq!(public_outputs.into_iter().cloned().collect_vec(), expected_public_outputs);
    // }
}

// This test checks that the execution graph computed from an execution
// involving dynamic calls is correct. The functions are invoked in the
// following order:
// "four::a"
//   --> "two::b"
//        --> "zero::c"
//        --> "one::d"
//   --> "three::e"
//        --> "two::b"
//             --> "zero::c"
//             --> "one::d"
//        --> "one::d"
//        --> "zero::c"
//
// Each of the call instructions can be static or dynamic depending on the
// boolean inputs to the test function.
//
// The linearized order is:
//  - [a, b, c, d, e, b, c, d, d, c]
// The transitions must be included in the `Execution` in the order they finish.
// The execution order is:
//  - [c, d, b, c, d, b, d, c, e, a]
fn test_complex_dynamic_graph_construction_internal(
    // In each of the arguments, call_X_Y_dynamic indicates whether the call to
    // function Y inside function X should be static or dynamic.
    call_a_b_dynamic: bool,
    call_a_e_dynamic: bool,
    call_b_c_dynamic: bool,
    call_b_d_dynamic: bool,
    call_e_b_dynamic: bool,
    call_e_d_dynamic: bool,
    call_e_c_dynamic: bool,
) {
    let network_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();

    let program_0_name_str = "zero";
    let program_1_name_str = "one";
    let program_2_name_str = "two";
    let program_3_name_str = "three";
    let program_4_name_str = "four";

    let program_0_name_field = Identifier::<CurrentNetwork>::from_str(program_0_name_str).unwrap().to_field().unwrap();
    let program_1_name_field = Identifier::<CurrentNetwork>::from_str(program_1_name_str).unwrap().to_field().unwrap();
    let program_2_name_field = Identifier::<CurrentNetwork>::from_str(program_2_name_str).unwrap().to_field().unwrap();
    let program_3_name_field = Identifier::<CurrentNetwork>::from_str(program_3_name_str).unwrap().to_field().unwrap();

    let function_b_name_id = Identifier::<CurrentNetwork>::from_str("b").unwrap();
    let function_c_name_id = Identifier::<CurrentNetwork>::from_str("c").unwrap();
    let function_d_name_id = Identifier::<CurrentNetwork>::from_str("d").unwrap();
    let function_e_name_id = Identifier::<CurrentNetwork>::from_str("e").unwrap();

    let function_b_name_field = function_b_name_id.to_field().unwrap();
    let function_c_name_field = function_c_name_id.to_field().unwrap();
    let function_d_name_field = function_d_name_id.to_field().unwrap();
    let function_e_name_field = function_e_name_id.to_field().unwrap();

    /******************************* program 0 *******************************/
    let (string, program0) = Program::<CurrentNetwork>::parse(
        r"
    program zero.aleo;

    function c:
        input r0 as u8.private;
        input r1 as u8.private;
        add r0 r1 into r2;
        output r2 as u8.private;
        
    constructor:
        assert.eq true true;",
    )
    .unwrap();
    assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");

    /******************************* program 1 *******************************/
    let (string, program1) = Program::<CurrentNetwork>::parse(
        r"
    program one.aleo;

    function d:
        input r0 as u8.private;
        input r1 as u8.private;
        add r0 r1 into r2;
        output r2 as u8.private;
        
    constructor:
        assert.eq true true;",
    )
    .unwrap();
    assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");

    /******************************* program 2 *******************************/
    let call_b_c_str = if call_b_c_dynamic {
        format!(
            "call.dynamic {program_0_name_field} {network_field} {function_c_name_field} with r0 r1 (as u8.private u8.private) into r2 (as u8.private);"
        )
    } else {
        "call zero.aleo/c r0 r1 into r2;".to_string()
    };

    let call_b_d_str = if call_b_d_dynamic {
        format!(
            "call.dynamic {program_1_name_field} {network_field} {function_d_name_field} with r1 r2 (as u8.private u8.private) into r3 (as u8.private);"
        )
    } else {
        "call one.aleo/d r1 r2 into r3;".to_string()
    };

    let program2_string = format!(
        r"
        import zero.aleo;
        import one.aleo;

        program two.aleo;

        function b:
            input r0 as u8.private;
            input r1 as u8.private;
            {call_b_c_str}
            {call_b_d_str}
            output r3 as u8.private;
            
        constructor:
            assert.eq true true;",
    );
    let (string, program2) = Program::<CurrentNetwork>::parse(program2_string.as_str()).unwrap();
    assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");

    /******************************* program 3 *******************************/

    let call_e_b_str = if call_e_b_dynamic {
        format!(
            "call.dynamic {program_2_name_field} {network_field} {function_b_name_field} with r0 r1 (as u8.private u8.private) into r2 (as u8.private);"
        )
    } else {
        "call two.aleo/b r0 r1 into r2;".to_string()
    };

    let call_e_d_str = if call_e_d_dynamic {
        format!(
            "call.dynamic {program_1_name_field} {network_field} {function_d_name_field} with r1 r2 (as u8.private u8.private) into r3 (as u8.private);"
        )
    } else {
        "call one.aleo/d r1 r2 into r3;".to_string()
    };

    let call_e_c_str = if call_e_c_dynamic {
        format!(
            "call.dynamic {program_0_name_field} {network_field} {function_c_name_field} with r1 r2 (as u8.private u8.private) into r4 (as u8.private);"
        )
    } else {
        "call zero.aleo/c r1 r2 into r4;".to_string()
    };

    let program3_string = format!(
        r"
        import zero.aleo;
        import one.aleo;
        import two.aleo;

        program three.aleo;

        function e:
            input r0 as u8.private;
            input r1 as u8.private;
            {call_e_b_str}
            {call_e_d_str}
            {call_e_c_str}
            output r4 as u8.private;
            
        constructor:
            assert.eq true true;",
    );

    let (string, program3) = Program::<CurrentNetwork>::parse(program3_string.as_str()).unwrap();
    assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");

    /******************************* program 4 *******************************/

    let call_a_b_str = if call_a_b_dynamic {
        format!(
            "call.dynamic {program_2_name_field} {network_field} {function_b_name_field} with r0 r1 (as u8.private u8.private) into r2 (as u8.private);"
        )
    } else {
        "call two.aleo/b r0 r1 into r2;".to_string()
    };

    let call_a_e_str = if call_a_e_dynamic {
        format!(
            "call.dynamic {program_3_name_field} {network_field} {function_e_name_field} with r1 r2 (as u8.private u8.private) into r3 (as u8.private);"
        )
    } else {
        "call three.aleo/e r1 r2 into r3;".to_string()
    };

    let program4_string = format!(
        r"
    import two.aleo;
    import three.aleo;

    program four.aleo;

    function a:
        input r0 as u8.private;
        input r1 as u8.private;
        {call_a_b_str}
        {call_a_e_str}
        output r3 as u8.private;
        
    constructor:
        assert.eq true true;",
    );

    let (string, program4) = Program::<CurrentNetwork>::parse(program4_string.as_str()).unwrap();
    assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");

    // Initialize the RNG.
    let rng = &mut TestRng::default();

    // Initialize caller.
    let caller_private_key = sample_genesis_private_key(rng);

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V12).unwrap(), rng);

    for (program, program_name_str) in [
        (program0, program_0_name_str),
        (program1, program_1_name_str),
        (program2, program_2_name_str),
        (program3, program_3_name_str),
        (program4, program_4_name_str),
    ] {
        // Deploy the program.
        println!("Deploying program {program_name_str}...");
        let transaction = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
        add_and_test(&vm, &caller_private_key, &[transaction], rng);
    }

    println!("Executing program four::a...");

    // Declare the input value.
    let r0 = Value::<CurrentNetwork>::from_str("1u8").unwrap();
    let r1 = Value::<CurrentNetwork>::from_str("2u8").unwrap();

    // Execute the "dynamic" function.
    let transaction =
        vm.execute(&caller_private_key, ("four.aleo", "a"), [r0, r1].into_iter(), None, 0, None, rng).unwrap();

    println!("Reconstructing call graph...");

    let transitions = transaction.transitions().collect_vec();
    let tids = transitions.iter().map(|transition| transition.id()).collect_vec();

    // Call tree                    transition index
    // "four::a"                    (9)
    //   --> "two::b"               (2)
    //        --> "zero::c"         (0)
    //        --> "one::d"          (1)
    //   --> "three::e"             (8)
    //        --> "two::b"          (5)
    //             --> "zero::c"    (3)
    //             --> "one::d"     (4)
    //        --> "one::d"          (6)
    //        --> "zero::c"         (7)
    //
    // The expected call graph is:
    // 9 -> [2, 8]
    // 8 -> [5, 6, 7]
    // 5 -> [3, 4]
    // 2 -> [0, 1]
    // 0 -> []
    // 1 -> []
    // 3 -> []
    // 4 -> []
    // 6 -> []
    // 7 -> []

    let graph = vm.process().read().construct_call_graph(transitions.into_iter()).unwrap();
    println!("First check");
    assert_eq!(graph[tids[9]], &[*tids[2], *tids[8]]);
    println!("Second check");
    assert_eq!(graph[tids[8]], &[*tids[5], *tids[6], *tids[7]]);
    println!("Third check");
    assert_eq!(graph[tids[5]], &[*tids[3], *tids[4]]);
    println!("Fourth check");
    assert_eq!(graph[tids[2]], &[*tids[0], *tids[1]]);
    println!("Fifth check");
    assert_eq!(graph[tids[0]], &[]);
    println!("Sixth check");
    assert_eq!(graph[tids[1]], &[]);
    println!("Seventh check");
    assert_eq!(graph[tids[3]], &[]);
    println!("Eighth check");
    assert_eq!(graph[tids[4]], &[]);
    println!("Ninth check");
    assert_eq!(graph[tids[6]], &[]);
    println!("Tenth check");
    assert_eq!(graph[tids[7]], &[]);

    let block = sample_next_block(&vm, &caller_private_key, &[transaction.clone()], rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction.clone()], rng);
}

#[test]
fn test_complex_dynamic_graph_construction() {
    let num_random_mixes = 3;

    // All static calls
    test_complex_dynamic_graph_construction_internal(false, false, false, false, false, false, false);
    // All dynamic calls
    test_complex_dynamic_graph_construction_internal(true, true, true, true, true, true, true);

    // Random static-/dynamic-call mixes
    let rng = &mut TestRng::default();
    for _ in 0..num_random_mixes {
        let mix: [bool; 7] = rng.r#gen();
        test_complex_dynamic_graph_construction_internal(mix[0], mix[1], mix[2], mix[3], mix[4], mix[5], mix[6]);
    }
}

// This test verifiers that a dynamic call to the `credits` functions work as expected.
#[test]
fn test_dynamic_calls_to_credits_aleo() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_view_key = ViewKey::try_from(&caller_private_key)?;
    let caller_address = Address::try_from(&caller_private_key)?;

    let test_dcall_program_id = ProgramID::<CurrentNetwork>::from_str("test_dcall.aleo").unwrap();
    let test_dcall_program_address = test_dcall_program_id.to_address()?;

    // Define the program to be executed.
    let program = Program::from_str(
        r"
            program test_dcall.aleo;

            // This static variant fails to parse because we can't parse identifiers as literals yet
            // function static:
            //    input r0 as address.public;
            //    input r1 as u64.public;
            //    call.dynamic credits aleo transfer_public_as_signer with r0 r1 (as address.public u64.public) into r2 (as dynamic.future);
            //    async static r2 into r3;
            //    output r3 as test_dcall.aleo/static.future;
            // finalize static:
            //    input r0 as dynamic.future;
            //    await r0; 
                    
            function two_transfer_publics:
                input r0 as field.public;
                input r1 as field.public;
                input r2 as field.public;
                input r3 as address.public;
                input r4 as u64.public;
                call.dynamic r0 r1 r2 with r3 r4 (as address.public u64.public) into r5 (as dynamic.future);
                call.dynamic r0 r1 r2 with r3 r4 (as address.public u64.public) into r6 (as dynamic.future);
                async two_transfer_publics r5 r6 into r7;
                output r7 as test_dcall.aleo/two_transfer_publics.future;
            finalize two_transfer_publics:
                input r0 as dynamic.future;
                input r1 as dynamic.future;
                await r1;
                await r0;

            function dynamic_transfer_pub_to_priv:
                input r0 as field.public;
                input r1 as field.public;
                input r2 as field.public;
                input r3 as address.private;
                input r4 as u64.public;
                call.dynamic r0 r1 r2 with r3 r4 (as address.private u64.public) into r5 r6 (as dynamic.record dynamic.future);
                async dynamic_transfer_pub_to_priv r6 into r7;
                output r5 as dynamic.record;
                output r7 as test_dcall.aleo/dynamic_transfer_pub_to_priv.future;
            finalize dynamic_transfer_pub_to_priv:
                input r0 as dynamic.future;
                await r0;

            function dynamic_transfer_private:
                input r0 as field.public;
                input r1 as field.public;
                input r2 as field.public;
                input r3 as dynamic.record;
                input r4 as address.private;
                input r5 as u64.private;
                call.dynamic r0 r1 r2 with r3 r4 r5 (as dynamic.record address.private u64.private) into r6 r7 (as dynamic.record dynamic.record);
                output r6 as dynamic.record;
                output r7 as dynamic.record;

            constructor:
                assert.eq true true;
                ",
    )?;

    // Initialize the VM.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V12)?, rng);

    // Deploy the program.
    println!("Deploying program: {}", program.id());
    let transaction = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block)?;

    // TODO: Uncomment this once we can parse identifiers as literals.
    // Execute the "static" function.
    // println!("Executing the `static` function...");
    // let transaction = vm.execute(
    //    &caller_private_key,
    //    ("test_dcall_to_transfer_public.aleo", "static"),
    //    vec![Value::from_str(&format!("{caller_address}"))?, Value::from_str("1234u64")?].into_iter(),
    //    None,
    //    0,
    //    None,
    //    rng,
    // )?;
    // vm.check_transaction(&transaction, None, rng)?;
    // let block = sample_next_block(&vm, &caller_private_key, &[transaction], rng)?;
    // assert_eq!(block.transactions().num_accepted(), 1);
    // assert_eq!(block.transactions().num_rejected(), 0);
    // assert_eq!(block.aborted_transaction_ids().len(), 0);
    // vm.add_next_block(&block)?;

    // Get the program and function identifiers as fields and check that they are expected.
    println!("Executing the `dynamic` function...");
    let credits_field = Identifier::<CurrentNetwork>::from_str("credits")?.to_field()?;
    let aleo_field = Identifier::<CurrentNetwork>::from_str("aleo")?.to_field()?;
    let transfer_public_as_signer_field =
        Identifier::<CurrentNetwork>::from_str("transfer_public_as_signer")?.to_field()?;
    let transfer_public_to_private_field =
        Identifier::<CurrentNetwork>::from_str("transfer_public_to_private")?.to_field()?;
    let transfer_private_field = Identifier::<CurrentNetwork>::from_str("transfer_private")?.to_field()?;
    println!("credits_field: {credits_field}");
    println!("aleo_field: {aleo_field}");
    println!("transfer_public_as_signer_field: {transfer_public_as_signer_field}");
    println!("transfer_public_to_private_field: {transfer_public_to_private_field}");

    let program_id_fields = ProgramID::<CurrentNetwork>::from_str("credits.aleo")?.to_fields()?;
    assert_eq!(program_id_fields.len(), 2);
    assert_eq!(program_id_fields[0], credits_field);
    assert_eq!(program_id_fields[1], aleo_field);

    // Execute 'two_transfer_publics'.
    let transaction = vm.execute(
        &caller_private_key,
        ("test_dcall.aleo", "two_transfer_publics"),
        vec![
            Value::from_str(&format!("{credits_field}"))?,
            Value::from_str(&format!("{aleo_field}"))?,
            Value::from_str(&format!("{transfer_public_as_signer_field}"))?,
            Value::from_str(&format!("{test_dcall_program_address}"))?,
            Value::from_str("1000000u64")?,
        ]
        .into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    vm.check_transaction(&transaction, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block)?;

    // Execute 'dynamic_transfer_public_to_private'.
    let transaction = vm.execute(
        &caller_private_key,
        ("test_dcall.aleo", "dynamic_transfer_pub_to_priv"),
        vec![
            Value::from_str(&format!("{credits_field}"))?,
            Value::from_str(&format!("{aleo_field}"))?,
            Value::from_str(&format!("{transfer_public_to_private_field}"))?,
            Value::from_str(&format!("{caller_address}"))?,
            Value::from_str("1234u64")?,
        ]
        .into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    vm.check_transaction(&transaction, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block)?;

    // Collect the record
    ensure!(block.records().collect_vec().len() == 1, "Expected 1 record, got {}", block.records().collect_vec().len());
    let record = block.records().collect_vec().last().unwrap().1.decrypt(&caller_view_key).unwrap();
    let dynamic_record = DynamicRecord::<CurrentNetwork>::from_record(&record).unwrap();

    // Execute 'dynamic_transfer_private'.
    println!("Executing 'dynamic_transfer_private'...");
    let transaction = vm.execute(
        &caller_private_key,
        ("test_dcall.aleo", "dynamic_transfer_private"),
        vec![
            Value::from_str(&format!("{credits_field}"))?,
            Value::from_str(&format!("{aleo_field}"))?,
            Value::from_str(&format!("{transfer_private_field}"))?,
            Value::<CurrentNetwork>::DynamicRecord(dynamic_record),
            Value::from_str(&format!("{caller_address}"))?,
            Value::from_str("1u64")?,
        ]
        .into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    vm.check_transaction(&transaction, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block)?;

    // Collect the record
    let records = block.records().collect_vec();
    println!("records: {records:?}");

    Ok(())
}

#[test]
fn test_universal_swap() {
    // Turn on trace logging.
    tracing_subscriber::fmt::init();
    // Define a mint_private function and constructor.
    let mint_private_function = r"
        function mint_private:
            input r0 as u64.private;
            cast self.caller r0 into r1 as credits.record;
            cast self.caller r0 into r2 as credits.record;
            output r1 as credits.record;
            output r2 as credits.record;
        constructor:
            assert.eq true true;
        ";

    // Define the credits programs.
    let credits_program = Program::<CurrentNetwork>::credits().unwrap().to_string();
    let mut credits_a_program = credits_program.replace("credits.aleo", "credits_a.aleo");
    credits_a_program.push_str(mint_private_function);
    let credits_a_program = Program::from_str(&credits_a_program).unwrap();
    let mut credits_b_program = credits_program.replace("credits.aleo", "credits_b.aleo");
    credits_b_program.push_str(mint_private_function);
    let credits_b_program = Program::from_str(&credits_b_program).unwrap();

    // Define the swap program.
    let amm_program = Program::from_str(r"
        program amm.aleo;

        struct reserves:
            // corresponds to credits_a.aleo
            token_a as u64;
            // corresponds to credits_b.aleo
            token_b as u64;

        mapping reserves_mapping:
            key as address.public;
            value as reserves.public;

        function buy_token_b:
            // credits_a
            input r0 as field.public;
            // credits_b
            input r1 as field.public;
            // aleo
            input r2 as field.public;
            // transfer_private_to_public function
            input r3 as field.public;
            // transfer_public_to_private function
            input r4 as field.public;
            // credits_a record
            input r5 as dynamic.record;
            // Token a amount to send
            input r6 as u64.public;
            // Token b amount to receive
            input r7 as u64.public;
            cast r6 r7 into r8 as reserves;
            call.dynamic r0 r2 r3 with r5 aleo1rrj2mgall8mw57lcpkkvkxwqkawpc5rjarqm57w8gux2ahnt9sxqf0md56 r6 (as dynamic.record address.public u64.public) into r9 r10 (as dynamic.record dynamic.future);
            call.dynamic r1 r2 r4 with self.signer r6 (as address.private u64.public) into r11 r12 (as dynamic.record dynamic.future);
            async buy_token_b r6 r7 r10 r12 into r13;
            // token_a change record
            output r9 as dynamic.record;
            // token_b receiver record
            output r11 as dynamic.record;
            output r13 as amm.aleo/buy_token_b.future;

        finalize buy_token_b:
            // token_a amount
            input r0 as u64.public;
            // token_b amount
            input r1 as u64.public;
            input r2 as dynamic.future;
            input r3 as dynamic.future;
            await r2;
            await r3;
            // TODO: implement reserve update logic here.

        constructor:
            assert.eq true true;
        ",
    ).unwrap();

    // Initialize an RNG.
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = crate::vm::test_helpers::sample_genesis_private_key(rng);
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(caller_private_key).unwrap();

    // Initialize the VM at the V12 height.
    let v12_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V12).unwrap();
    let vm = crate::vm::test_helpers::sample_vm_at_height(v12_height, rng);

    // Deploy the program - one at a time so as not to surpass public payer limits.
    for program in [credits_a_program, credits_b_program, amm_program] {
        let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
        let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng).unwrap();
        assert_eq!(block.transactions().num_accepted(), 1);
        assert_eq!(block.transactions().num_rejected(), 0);
        assert_eq!(block.aborted_transaction_ids().len(), 0);
        vm.add_next_block(&block).unwrap();
    }

    // Execute credits_a.aleo/mint_private to mint a few credits_a records.
    let execute_mint_a = vm
        .execute(
            &caller_private_key,
            ("credits_a.aleo", "mint_private"),
            vec![Value::from_str("100u64")].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();
    // Execute credits_b.aleo/mint_private to mint a few credits_b records.
    let execute_mint_b = vm
        .execute(
            &caller_private_key,
            ("credits_b.aleo", "mint_private"),
            vec![Value::from_str("100u64")].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();
    let block = sample_next_block(&vm, &caller_private_key, &[execute_mint_a, execute_mint_b], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 2);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();

    // Obtain the credits records.
    let records =
        block.records().map(|(_, record)| record.decrypt(&caller_view_key)).collect::<Result<Vec<_>>>().unwrap();
    // Split the records into credits_a and credits_b records.
    let (records_a, records_b) = records.split_at(2);

    // Create the AMM program address.
    let amm_address: Address<CurrentNetwork> = ProgramID::from_str("amm.aleo").unwrap().to_address().unwrap();
    let amm_address_value = Value::from_str(&amm_address.to_string()).unwrap();

    // Execute credits_a.aleo/transfer_private_to_public to give amm.aleo an initial balance of credits_a.
    let execute_transfer_a = vm
        .execute(
            &caller_private_key,
            ("credits_a.aleo", "transfer_private_to_public"),
            vec![Value::Record(records_a[0].clone()), amm_address_value.clone(), Value::from_str("100u64").unwrap()]
                .into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();
    // Execute credits_b.aleo/transfer_private_to_public to give amm.aleo an initial balance of credits_b.
    let execute_transfer_b = vm
        .execute(
            &caller_private_key,
            ("credits_b.aleo", "transfer_private_to_public"),
            vec![Value::Record(records_b[0].clone()), amm_address_value.clone(), Value::from_str("100u64").unwrap()]
                .into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();
    let block = sample_next_block(&vm, &caller_private_key, &[execute_transfer_a, execute_transfer_b], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 2);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();

    let dynamic_record_a = DynamicRecord::<CurrentNetwork>::from_record(&records_a[1].clone()).unwrap();
    let credits_a_field = Identifier::<CurrentNetwork>::from_str("credits_a").unwrap().to_field().unwrap();
    let credits_b_field = Identifier::<CurrentNetwork>::from_str("credits_b").unwrap().to_field().unwrap();
    let aleo_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();
    let transfer_private_to_public_field =
        Identifier::<CurrentNetwork>::from_str("transfer_private_to_public").unwrap().to_field().unwrap();
    let transfer_public_to_private_field =
        Identifier::<CurrentNetwork>::from_str("transfer_public_to_private").unwrap().to_field().unwrap();

    // Execute amm.aleo/buy_token_b to buy token_b.
    let execute_buy_token_b = vm
        .execute(
            &caller_private_key,
            ("amm.aleo", "buy_token_b"),
            vec![
                Value::from_str(&format!("{credits_a_field}")).unwrap(),
                Value::from_str(&format!("{credits_b_field}")).unwrap(),
                Value::from_str(&format!("{aleo_field}")).unwrap(),
                Value::from_str(&format!("{transfer_private_to_public_field}")).unwrap(),
                Value::from_str(&format!("{transfer_public_to_private_field}")).unwrap(),
                Value::<CurrentNetwork>::DynamicRecord(dynamic_record_a),
                Value::from_str("100u64").unwrap(),
                Value::from_str("100u64").unwrap(),
            ]
            .into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();
    let block = sample_next_block(&vm, &caller_private_key, &[execute_buy_token_b], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 1);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();

    // Obtain the credits_a change and credits_b receiver records.
    let (_change_record, _receiver_record) = block
        .records()
        .map(|(_, record)| record.decrypt(&caller_view_key))
        .collect::<Result<Vec<_>>>()
        .unwrap()
        .split_at(1);
}

#[test]
fn test_conditional_execution() {
    let constants_program_name = Identifier::<CurrentNetwork>::from_str("constants").unwrap();
    let constants_program_field = constants_program_name.to_field().unwrap();

    let other_constants_program_name = Identifier::<CurrentNetwork>::from_str("other_constants").unwrap();
    let other_constants_program_field = other_constants_program_name.to_field().unwrap();

    let three_function_name = Identifier::<CurrentNetwork>::from_str("three").unwrap();
    let three_function_field = three_function_name.to_field().unwrap();

    let four_function_name = Identifier::<CurrentNetwork>::from_str("four").unwrap();
    let four_function_field = four_function_name.to_field().unwrap();

    let five_function_name = Identifier::<CurrentNetwork>::from_str("five").unwrap();
    let five_function_field = five_function_name.to_field().unwrap();

    let aleo_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();

    // Define the swap program.
    let constants_program_str = format!(
        r"
        program constants.aleo;

        function {three_function_name}:
            output 3u128 as u128.private;

        function {four_function_name}:
            output 4u128 as u128.private;

        constructor:
            assert.eq true true;
        ",
    );

    // Define the swap program.
    let other_constants_program_str = format!(
        r"
        program other_constants.aleo;

        function {five_function_name}:
            output 5u128 as u128.private;

        constructor:
            assert.eq true true;
        ",
    );

    // Define the swap program.
    let conditional_program_str = format!(
        r"
        import constants.aleo;

        program conditional_program.aleo;

        function conditional_function:
            input r0 as boolean.private; // flag
            input r1 as field.public;    // custom program
            input r2 as field.public;    // custom function
            
            ternary r0 r1 {other_constants_program_field} into r3;
            ternary r0 r2 {five_function_field} into r4;

            call.dynamic r3 {aleo_field} r4 into r5 (as u128.private);

            add r5 1u128 into r6;
            
            output r6 as u128.public;

        constructor:
            assert.eq true true;
        ",
    );

    // Parse programs
    let constants_program = Program::<CurrentNetwork>::from_str(&constants_program_str).unwrap();
    let other_constants_program = Program::<CurrentNetwork>::from_str(&other_constants_program_str).unwrap();
    let conditional_program = Program::<CurrentNetwork>::from_str(&conditional_program_str).unwrap();

    // Initialize an RNG.
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = crate::vm::test_helpers::sample_genesis_private_key(rng);
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(caller_private_key).unwrap();

    // Initialize the VM at the V12 height.
    let v12_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V12).unwrap();
    let vm = crate::vm::test_helpers::sample_vm_at_height(v12_height, rng);

    // Deploy the program - one at a time so as not to surpass public payer limits.
    for program in [
        ("constants.aleo", constants_program),
        ("other_constants.aleo", other_constants_program),
        ("conditional_execution.aleo", conditional_program),
    ] {
        println!("Deploying program {}...", program.0);

        let deployment = vm.deploy(&caller_private_key, &program.1, None, 0, None, rng).unwrap();
        add_and_test(&vm, &caller_private_key, &[deployment], rng);
    }

    println!("Executing (custom) conditional_program.aleo/conditional_function -> constants/three.aleo...");
    let execute_1 = vm
        .execute(
            &caller_private_key,
            ("conditional_program.aleo", "conditional_function"),
            vec![
                Value::from_str("true").unwrap(),
                Value::from_str(&format!("{constants_program_field}")).unwrap(),
                Value::from_str(&format!("{three_function_field}")).unwrap(),
            ]
            .into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    println!("Executing (custom) conditional_program.aleo/conditional_function -> constants/four.aleo...");
    let execute_2 = vm
        .execute(
            &caller_private_key,
            ("conditional_program.aleo", "conditional_function"),
            vec![
                Value::from_str("true").unwrap(),
                Value::from_str(&format!("{constants_program_field}")).unwrap(),
                Value::from_str(&format!("{four_function_field}")).unwrap(),
            ]
            .into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    println!("Executing (fallback) conditional_program.aleo/conditional_function -> other_constants/five.aleo...");
    let execute_3 = vm
        .execute(
            &caller_private_key,
            ("conditional_program.aleo", "conditional_function"),
            vec![
                Value::from_str("false").unwrap(),
                Value::from_str(&format!("{constants_program_field}")).unwrap(),
                Value::from_str(&format!("{four_function_field}")).unwrap(),
            ]
            .into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    add_and_test(&vm, &caller_private_key, &[execute_1, execute_2, execute_3], rng);

    // TODO (dynamic_dispatch): do we have a way to check the output without finalize blocks?
}

// TODO Missing test cases from the design doc:
// - Conditional execution with finalize scopes

/************************** Translation test cases ***************************/

// TODO (dynamic_dispatch) remove the legend once working
// 
// Single-translation test cases (O: coded, P: passing)
// P input dynamic -> static external
// O input dynamic -> static non-external
// P output static non-external -> dynamic
// O output static external -> dynamic
// 
// Double-translation test cases
// - input dynamic -> dynamic (no translation; check dynamic-record InputID changes as expected)
// - input static -> static (no translation)
// Double-translation test cases (non-exhaustive)
// - input static -> dynamic subsequently passed as input dynamic -> static
// - output static -> dynamic subsequently passed as output dynamic -> static
// Polimorphy
// - input static-type-1 -> dynamic, then static-type-2 -> dynamic (e. g. controlled by a boolean private input)
// - input static-type-1 + static-type2 -> dynamic, dynamic
// Other chained cases (non-exhaustive)
// - input static -> dynamic passed as static -> dynamic, output as dynamic -> static
// - input static -> dynamic passed as static -> dynamic, output as dynamic (check dynamic-record OutputID changes as expected)
// Key-fetching
// - input static -> dynamic, input dynamic -> static, output static -> dynamic, output dynamic -> static all witht he same static definition: only one translation proving key should be fetched
// - static {program_1 - record_name_1, program_1 - record_name_1, program_1 - record_name_2, program_2 - record_name_1, program_2 - record_name_2}: 4 translation proving keys should be fetched
// Signature consistency
// - test involve translation of the output of a call from a preexisting program to ensure signature-verification circuit hasn't changed
// More

#[test]
fn test_translation_input_dynamic_non_external() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    let record_static_str = format!(
        r#"{{
        owner: {caller_address}.private,
        liters: 1888u64.public,
        flammable: false.private,
        _nonce: 0group.public,
        _version: 1u8.public
    }}"#
    );

    // Construct the static and dynamic records.
    let r0_static = Record::<CurrentNetwork, Plaintext<CurrentNetwork>>::from_str(&record_static_str).unwrap();
    let r0_dynamic = DynamicRecord::<CurrentNetwork>::from_record(&r0_static).unwrap();

    // Input and expected output
    let r0_value = Value::<CurrentNetwork>::DynamicRecord(r0_dynamic);
    let expected_output = Plaintext::<CurrentNetwork>::from_str("1888u64").unwrap();

    test_translation(
        &caller_private_key,
        "flow.aleo",
        "get_dynamic_liters_from_gas",
        None,
        Some(r0_static),
        Some(vec![expected_output]),
        rng,
    );
}

#[test]
fn test_translation_output_non_external_dynamic() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);

    test_translation(&caller_private_key, "flow.aleo", "dynamic_pump", Some(vec![]), None, None, rng);
}

#[test]
fn test_translation_input_dynamic_external() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    let record_static_str = format!(
        r#"{{
        owner: {caller_address}.private,
        liters: 292u64.public,
        flammable: true.private,
        _nonce: 0group.public,
        _version: 1u8.public
    }}"#
    );

    // Construct the static and dynamic records.
    let r0_static = Record::<CurrentNetwork, Plaintext<CurrentNetwork>>::from_str(&record_static_str).unwrap();
    let r0_dynamic = DynamicRecord::<CurrentNetwork>::from_record(&r0_static).unwrap();

    // Input and expected output
    let r0_value = Value::<CurrentNetwork>::DynamicRecord(r0_dynamic);
    let expected_output = Plaintext::<CurrentNetwork>::from_str("292u64").unwrap();

    test_translation(
        &caller_private_key,
        "gas_manager.aleo",
        "get_gas_liters_externally",
        None,
        Some(r0_static),
        Some(vec![expected_output]),
        rng,
    );
}

#[test]
fn test_translation_output_external_dynamic() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);

    test_translation(&caller_private_key, "gas_manager.aleo", "pump_and_send_through_pipe", Some(vec![]), None, None, rng);
}
