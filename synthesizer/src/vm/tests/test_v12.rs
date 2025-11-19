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
    program::{DynamicRecord, Identifier, OutputID, Value},
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
    root_program_name: &str,
    root_function_name: &str,
    input_values: &[Value<CurrentNetwork>],
    expected_output_ids: Option<Vec<OutputID<CurrentNetwork>>>,
    expected_public_outputs: Option<Vec<Plaintext<CurrentNetwork>>>,
) {
    // Various parameters for dynamic.call instructions.
    let program_a_name_str = "flow";
    let program_a_name_as_field =
        Identifier::<CurrentNetwork>::from_str(program_a_name_str).unwrap().to_field().unwrap();
    let program_b_name_str = "gas_manager";
    let program_b_name_as_field =
        Identifier::<CurrentNetwork>::from_str(program_b_name_str).unwrap().to_field().unwrap();
    let network_as_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();

    let get_liquid_liters_function_name = Identifier::<CurrentNetwork>::from_str("get_liquid_liters").unwrap();
    let consume_dynamic_blob_function_name = Identifier::<CurrentNetwork>::from_str("consume_dynamic_blob").unwrap();
    let nitrogen_pump_function_name = Identifier::<CurrentNetwork>::from_str("nitrogen_pump").unwrap();

    let get_liquid_liters_function_field = get_liquid_liters_function_name.to_field().unwrap();
    let consume_dynamic_blob_function_field = consume_dynamic_blob_function_name.to_field().unwrap();
    let nitrogen_pump_function_field = nitrogen_pump_function_name.to_field().unwrap();

    let program_a_string = format!(
        r"
    program {program_a_name_str}.aleo;

    // Tries to consume a container passed as dynamic as a specifically liquid one
    function get_dynamic_liters:
        input r0 as dynamic.record;
        call.dynamic {program_b_name_as_field} {network_as_field} {get_liquid_liters_function_field} with r0 (as dynamic.record) into r1 (as u64.public);
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

    function nitrogen_pump:
        input r0 as u64.public;
        cast self.caller r0 false into r1 as gas_container.record;
        output r1 as gas_container.record;

    constructor:
        assert.eq true true;
    "
    );

    // Initialize a new program.
    let program_a = Program::<CurrentNetwork>::from_str(&program_a_string).unwrap();
    let program_b = Program::<CurrentNetwork>::from_str(&program_b_string).unwrap();

    let rng = &mut TestRng::default();

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

    println!("Executing function: {root_function_name}...");

    // TODO (dynamic_dispatch) remove
    println!("Executing {root_program_name}/{root_function_name}...");

    // Execute the "dynamic" function.
    let transaction = vm
        .execute(
            &caller_private_key,
            (root_program_name, root_function_name),
            input_values.into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

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

    println!("Verifying transaction...");

    vm.check_transaction(&transaction, None, rng).unwrap();

    let block = sample_next_block(&vm, &caller_private_key, &[transaction.clone()], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 1);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();
}

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
// O input dynamic -> static
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
fn test_translation_input_static_dynamic() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    let record_static_str = format!(
        r#"{{
        owner: {}.private,
        liters: 22u64.public,
        flammable: false.private,
        _nonce: 0group.public,
        _version: 1u8.public
    }}"#,
        caller_address
    );

    // Construct the static and dynamic records.
    let r0_static = Record::<CurrentNetwork, Plaintext<CurrentNetwork>>::from_str(&record_static_str).unwrap();

    // Input and expected output
    let r0_value = Value::<CurrentNetwork>::Record(r0_static);

    test_translation(&caller_private_key, "gas_manager.aleo", "consume_gas", &[r0_value], None, None);
}

#[test]
fn test_translation_input_dynamic_static() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    let record_static_str = format!(
        r#"{{
        owner: {}.private,
        liters: 97u64.public,
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
    let expected_output = Plaintext::<CurrentNetwork>::from_str("97u64").unwrap();

    test_translation(
        &caller_private_key,
        "flow.aleo",
        "get_dynamic_liters",
        &[r0_value],
        None,
        Some(vec![expected_output]),
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
        &[],
        Some(vec![OutputID::DynamicRecord(r0_dynamic_id)]),
        None,
    );
}

// TODO (Antonio) fix "Expected an dynamic record input..."
// TODO (Antonio) ask Victor if registers.record_translation_arguments() is still there, ow. fix everywhere
