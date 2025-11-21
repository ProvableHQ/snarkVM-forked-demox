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

use crate::vm::test_helpers::{sample_vm_at_height, *};

use anyhow::Result;
use console::{
    network::ConsensusVersion,
    program::{DynamicRecord, Identifier, OutputID, Value, Entry},
    account::{ViewKey, Address}
};
use snarkvm_synthesizer_program::Program;
use snarkvm_utilities::TestRng;

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
    // Expected output IDsand public-ouput values
    expected_output_ids: Option<Vec<OutputID<CurrentNetwork>>>,
    expected_public_outputs: Option<Vec<Plaintext<CurrentNetwork>>>,
    rng: &mut TestRng,
) {

    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(caller_private_key).unwrap();
    let caller_address = Address::<CurrentNetwork>::try_from(caller_private_key).unwrap();

    // Various parameters for dynamic.call instructions.
    let program_a_name_str = "flow";
    let program_a_name_as_field =
        Identifier::<CurrentNetwork>::from_str(program_a_name_str).unwrap().to_field().unwrap();
    let program_b_name_str = "gas_manager";
    let program_b_name_as_field =
        Identifier::<CurrentNetwork>::from_str(program_b_name_str).unwrap().to_field().unwrap();
    let network_as_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();

    let get_liquid_liters_function_name = Identifier::<CurrentNetwork>::from_str("get_liquid_liters").unwrap();
    let get_gas_liters_function_name = Identifier::<CurrentNetwork>::from_str("get_gas_liters").unwrap();
    let consume_dynamic_blob_function_name = Identifier::<CurrentNetwork>::from_str("consume_dynamic_blob").unwrap();
    let nitrogen_pump_function_name = Identifier::<CurrentNetwork>::from_str("nitrogen_pump").unwrap();

    let get_liquid_liters_function_field = get_liquid_liters_function_name.to_field().unwrap();
    let get_gas_liters_function_field = get_gas_liters_function_name.to_field().unwrap();
    let consume_dynamic_blob_function_field = consume_dynamic_blob_function_name.to_field().unwrap();
    let nitrogen_pump_function_field = nitrogen_pump_function_name.to_field().unwrap();

    let program_a_string = format!(
        r"
    program {program_a_name_str}.aleo;

    // Tries to consume a container passed as dynamic as a specifically liquid one
    function get_dynamic_liters_from_liquid:
        input r0 as dynamic.record;
        call.dynamic {program_b_name_as_field} {network_as_field} {get_liquid_liters_function_field} with r0 (as dynamic.record) into r1 (as u64.public);
        output r1 as u64.public;
    
    function get_dynamic_liters_from_gas:
        input r0 as dynamic.record;
        call.dynamic {program_b_name_as_field} {network_as_field} {get_gas_liters_function_field} with r0 (as dynamic.record) into r1 (as u64.public);
        output r1 as u64.public;

    function consume_dynamic_blob:
        input r0 as dynamic.record;
        output true as boolean.private;

    function dynamic_pump:
        call.dynamic {program_b_name_as_field} {network_as_field} {nitrogen_pump_function_field} into r0 (as dynamic.record);
        output r0 as dynamic.record;

    constructor:
        assert.eq true true;
    "
    );

    // Preparing the record values for the hardcoded gas_record minter
    let (gas_owner, gas_liters, gas_flammable) = if let Some(gas_to_mint_record) = &gas_to_mint {
        let liters_entry = gas_to_mint_record.data().get(&Identifier::<CurrentNetwork>::from_str("liters").unwrap()).unwrap();
        let flammable_entry = gas_to_mint_record.data().get(&Identifier::<CurrentNetwork>::from_str("flammable").unwrap()).unwrap();
        let liters_value = match liters_entry {
            Entry::Public(plaintext) => plaintext.to_string(),
            _ => panic!("`liters` entry should be public"),
        };
        let flammable_value = match flammable_entry {
            Entry::Private(plaintext) => plaintext.to_string(),
            _ => panic!("`flammable` entry should be private"),
        };
        (
            caller_address.to_string(),
            liters_value,
            flammable_value,
        )
    } else {
        (
            "0field".to_string(),
            "100u64".to_string(),
            "false".to_string()
        )
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
        call.dynamic {program_a_name_as_field} {network_as_field} {consume_dynamic_blob_function_field} with r0 (as gas_container.record) into r1 (as boolean.private);
        output r0.liters as u64.public;

    function get_liquid_liters:
        input r0 as liquid_container.record;
        output r0.liters as u64.public;

    function get_gas_liters:
        input r0 as gas_container.record;
        output r0.liters as u64.public;

    function nitrogen_pump:
        input r0 as u64.public;
        cast self.caller r0 false into r1 as gas_container.record;
        output r1 as gas_container.record;

    function hardcoded_gas_pump:
        cast {gas_owner} {gas_liters} {gas_flammable} into r0 as gas_container.record;
        output r0 as gas_container.record;

    constructor:
        assert.eq true true;
    "
    );

    // Initialize a new program.
    let program_a = Program::<CurrentNetwork>::from_str(&program_a_string).unwrap();
    let program_b = Program::<CurrentNetwork>::from_str(&program_b_string).unwrap();

    // Initialize the VM.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V12).unwrap(), rng);

    // Deploy the program.
    println!("Deploying program {program_a_name_str}...");
    let transaction_a = vm.deploy(&caller_private_key, &program_a, None, 0, None, rng).unwrap();
    let block_a = sample_next_block(&vm, &caller_private_key, &[transaction_a], rng).unwrap();
    assert_eq!(block_a.transactions().num_accepted(), 1);
    assert_eq!(block_a.transactions().num_rejected(), 0);
    assert_eq!(block_a.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block_a).unwrap();

    println!("Deploying program {program_b_name_str}...");
    let transaction_b = vm.deploy(&caller_private_key, &program_b, None, 0, None, rng).unwrap();
    let block_b = sample_next_block(&vm, &caller_private_key, &[transaction_b], rng).unwrap();
    assert_eq!(block_b.transactions().num_accepted(), 1);
    assert_eq!(block_b.transactions().num_rejected(), 0);
    assert_eq!(block_b.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block_b).unwrap();

    // TODO (Antonio) reintroduce
    // ensure!(
    //     input_values.is_none() || gas_to_mint.is_none(),
    //     "When gas_to_mint is provided, the resulting static input is converted to dynamic record is used instead of input_values, which should be None",
    // );

    // ensure!(
    //     input_values.is_some() || gas_to_mint.is_some(),
    //     "Exactly one of input_values or gas_to_mint must be provided",
    // );

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

    vm.check_transaction(&transaction, None, rng).unwrap();

    println!("Sampling final block...");

    let block = sample_next_block(&vm, &caller_private_key, &[transaction.clone()], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 1);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();

    println!("Asserting output correctness...");

    let output_ids = transaction.transitions().last().unwrap().output_ids().collect_vec();

    let public_outputs = transaction
        .transitions()
        .last()
        .unwrap()
        .outputs()
        .iter()
        .filter_map(|output| match output {
            Output::Public(_, Some(plaintext)) => Some(plaintext),
            _ => None,
        })
        .collect_vec();

    if let Some(expected_public_outputs) = expected_public_outputs {
        assert_eq!(public_outputs.into_iter().cloned().collect_vec(), expected_public_outputs);
    }

    if let Some(expected_output_ids) = expected_output_ids {
        assert_eq!(
            output_ids.into_iter().cloned().collect_vec(),
            expected_output_ids.into_iter().map(get_main_field).collect_vec()
        );
    }
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
    let network_as_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();

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
            "call.dynamic {program_0_name_field} {network_as_field} {function_c_name_field} with r0 r1 (as u8.private u8.private) into r2 (as u8.private);"
        )
    } else {
        "call zero.aleo/c r0 r1 into r2;".to_string()
    };

    let call_b_d_str = if call_b_d_dynamic {
        format!(
            "call.dynamic {program_1_name_field} {network_as_field} {function_d_name_field} with r1 r2 (as u8.private u8.private) into r3 (as u8.private);"
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
            "call.dynamic {program_2_name_field} {network_as_field} {function_b_name_field} with r0 r1 (as u8.private u8.private) into r2 (as u8.private);"
        )
    } else {
        "call two.aleo/b r0 r1 into r2;".to_string()
    };

    let call_e_d_str = if call_e_d_dynamic {
        format!(
            "call.dynamic {program_1_name_field} {network_as_field} {function_d_name_field} with r1 r2 (as u8.private u8.private) into r3 (as u8.private);"
        )
    } else {
        "call one.aleo/d r1 r2 into r3;".to_string()
    };

    let call_e_c_str = if call_e_c_dynamic {
        format!(
            "call.dynamic {program_0_name_field} {network_as_field} {function_c_name_field} with r1 r2 (as u8.private u8.private) into r4 (as u8.private);"
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
            "call.dynamic {program_2_name_field} {network_as_field} {function_b_name_field} with r0 r1 (as u8.private u8.private) into r2 (as u8.private);"
        )
    } else {
        "call two.aleo/b r0 r1 into r2;".to_string()
    };

    let call_a_e_str = if call_a_e_dynamic {
        format!(
            "call.dynamic {program_3_name_field} {network_as_field} {function_e_name_field} with r1 r2 (as u8.private u8.private) into r3 (as u8.private);"
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
        let block = sample_next_block(&vm, &caller_private_key, &[transaction], rng).unwrap();

        assert_eq!(block.transactions().num_accepted(), 1);
        assert_eq!(block.transactions().num_rejected(), 0);
        assert_eq!(block.aborted_transaction_ids().len(), 0);
        vm.add_next_block(&block).unwrap();
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

    println!("\n\n\n\n\n\n\n\n\nVerifying transaction...\n\n\n\n\n\n\n\n\n");

    vm.check_transaction(&transaction, None, rng).unwrap();

    let block = sample_next_block(&vm, &caller_private_key, &[transaction.clone()], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 1);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();
}

/************************ Dynamic call-graph recovery ************************/

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

/************************** Other dynamic-call tests **************************/

// This test verifiers that a dynamic call to the `credits.transfer_public` function works as expected.
#[test]
fn test_dynamic_call_to_transfer_public() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;

    // Initialize the VM.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V12)?, rng);

    // Define the program to be executed.
    let program = Program::from_str(
        r"
program test_dcall.aleo;

//function static:
//    input r0 as address.public;
//    input r1 as u64.public;
//    dcall credits transfer_public with r0 r1 (as address.public u64.public) into r2 (as dynamic.future);
//    async static r2 into r3;
//    output r3 as test_dcall.aleo/static.future;
//finalize static:
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

function dynamic_transfer_private:
    input r0 as field.public;
    input r1 as field.public;
    input r2 as field.public;
    input r3 as dynamic.record;
    input r4 as address.public;
    input r5 as u64.public;
    call.dynamic r0 r1 r2 with r3 r4 r5 (as dynamic.record address.public u64.public) into r6 r7 (as dynamic.record dynamic.record);
    output r6 as dynamic.record;
    output r7 as dynamic.record;

constructor:
    assert.eq true true;
    ",
    )?;

    // Deploy the program.
    println!("Deploying program: {}", program.id());
    let transaction = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block)?;

    // Execute the "static" function.
    //println!("Executing the `static` function...");
    //let transaction = vm.execute(
    //    &caller_private_key,
    //    ("test_dcall_to_transfer_public.aleo", "static"),
    //    vec![Value::from_str(&format!("{caller_address}"))?, Value::from_str("1234u64")?].into_iter(),
    //    None,
    //    0,
    //    None,
    //    rng,
    //)?;
    //vm.check_transaction(&transaction, None, rng)?;
    //let block = sample_next_block(&vm, &caller_private_key, &[transaction], rng)?;
    //assert_eq!(block.transactions().num_accepted(), 1);
    //assert_eq!(block.transactions().num_rejected(), 0);
    //assert_eq!(block.aborted_transaction_ids().len(), 0);
    //vm.add_next_block(&block)?;

    // Get the program and function identifiers as fields and check that they are expected.
    println!("Executing the `dynamic` function...");
    let credits_as_field = Identifier::<CurrentNetwork>::from_str("credits")?.to_field()?;
    let aleo_as_field = Identifier::<CurrentNetwork>::from_str("aleo")?.to_field()?;
    let transfer_public_as_field = Identifier::<CurrentNetwork>::from_str("transfer_public")?.to_field()?;
    println!("credits_as_field: {credits_as_field}");
    println!("aleo_as_field: {aleo_as_field}");
    println!("transfer_public_as_field: {transfer_public_as_field}");

    let program_id_fields = ProgramID::<CurrentNetwork>::from_str("credits.aleo")?.to_fields()?;
    assert_eq!(program_id_fields.len(), 2);
    assert_eq!(program_id_fields[0], credits_as_field);
    assert_eq!(program_id_fields[1], aleo_as_field);

    // Execute the "dynamic" function.
    let transaction = vm.execute(
        &caller_private_key,
        ("test_dcall.aleo", "two_transfer_publics"),
        vec![
            Value::from_str(&format!("{credits_as_field}"))?,
            Value::from_str(&format!("{aleo_as_field}"))?,
            Value::from_str(&format!("{transfer_public_as_field}"))?,
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

    Ok(())
}

/************************** Translation test cases ***************************/

// TODO (dynamic_dispatch) remove the legend once working
// Single-translation test cases (O: coded, P: passing)
// O input static -> dynamic
// P input dynamic -> static
// O output static -> dynamic
// x output dynamic -> static ! Cannot be tested directly since dynamic records cannot be directly instantiated. Tested as part of multi-translation tests below.
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
// fn test_translation_input_static_dynamic() {
//     let rng = &mut TestRng::default();

//     let caller_private_key = sample_genesis_private_key(rng);
//     let caller_address = Address::try_from(&caller_private_key).unwrap();

//     let record_static_str = format!(
//         r#"{{
//         owner: {}.private,
//         liters: 22u64.public,
//         flammable: false.private,
//         _nonce: 0group.public,
//         _version: 1u8.public
//     }}"#,
//         caller_address
//     );

//     // Construct the static and dynamic records.
//     let r0_static = Record::<CurrentNetwork, Plaintext<CurrentNetwork>>::from_str(&record_static_str).unwrap();

//     // Input and expected output
//     let r0_value = Value::<CurrentNetwork>::Record(r0_static.clone());

//     test_translation(&caller_private_key, "gas_manager.aleo", "consume_gas", &[r0_value], Some(r0_static), None, None, rng);
// }

#[test]
fn test_translation_input_dynamic_static() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    let record_static_str = format!(
        r#"{{
        owner: {}.private,
        liters: 1888u64.public,
        flammable: false.private,
        _nonce: 0group.public,
        _version: 1u8.public
    }}"#,
        caller_address
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
        None,
        Some(vec![expected_output]),
        rng,
    );
}

#[test]
fn test_translation_output_static_dynamic() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);

    let record_static_str = r#"{
        owner: 0group.private,
        liters: 10u64.public,
        flammable: false.private,
        _nonce: 0group.public,
        _version: 1u8.public
    }"#;

    // Construct the static and dynamic records.
    let r0_static = Record::<CurrentNetwork, Plaintext<CurrentNetwork>>::from_str(&record_static_str).unwrap();
    let r0_dynamic = DynamicRecord::<CurrentNetwork>::from_record(&r0_static).unwrap();

    // Input and expected output
    let caller_function_name = Identifier::<CurrentNetwork>::from_str("nitrogen_pump").unwrap();
    let caller_function_field = caller_function_name.to_field().unwrap();
    let input_output_index = U16::<CurrentNetwork>::from_str("0").unwrap();
    let tvk = None::<Field<CurrentNetwork>>.unwrap();

    let r0_dynamic_id = r0_dynamic.to_id(caller_function_field, tvk, input_output_index).unwrap();

    test_translation(
        &caller_private_key,
        "flow.aleo",
        "dynamic_pump",
        Some(vec![]),
        None,
        Some(vec![OutputID::DynamicRecord(r0_dynamic_id)]),
        None,
        rng,
    );
}
