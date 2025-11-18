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
    account::ViewKey,
    network::ConsensusVersion,
    program::{Identifier, Value},
};
use snarkvm_synthesizer_program::Program;
use snarkvm_utilities::TestRng;

// fn test_translation(root_function_name: &str, inputs: &[Value<CurrentNetwork>], expected_outputs: &[Value<CurrentNetwork>]) {

//     // Declare function names of dynamic.call instructions.
//     let get_liquid_liters_function_name = Identifier::<CurrentNetwork>::from_str("get_liquid_liters").unwrap();

//     let program_name_str = "liter_fetcher";
//     let program_name_as_field = Identifier::<CurrentNetwork>::from_str(program_name_str).unwrap().to_field().unwrap();
//     let network_as_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();
//     let get_liquid_liters_function_name_as_field = get_liquid_liters_function_name.to_field().unwrap();

//     let program_string = format!(r"
//     program liter_fetcher.aleo;
    
//     record liquid_container:
//       owner as address.private;
//       liters as u64.public;
    
//     record gas_container:
//       owner as address.private;
//       liters as u64.public;

//     // Tries to consume a container passed as dynamic as a specifically liquid one
//     function get_dynamic_liters:
//       input r0 as dynamic.record;
//       call.dynamic {program_name_as_field} {network_as_field} {get_liquid_liters_function_name_as_field} with r0 (as dynamic.record) into r1 (as u64.public);
//       output r1 as u64.public;
    
//     function get_liquid_liters:
//       input r0 as liquid_container.record;
//       output r0.liters as u64.public;

//     constructor:
//         assert.eq true true;
//     ");

//     // Initialize a new program.
//     let program = Program::<CurrentNetwork>::from_str(&program_string).unwrap();

//     // Construct the process.
//     let mut process = crate::test_helpers::sample_process(&program);

//     // Initialize the RNG.
//     let rng = &mut TestRng::default();

//     // Initialize the caller.
//     let caller_private_key = PrivateKey::<CurrentNetwork>::new(rng).unwrap();
//     let caller = Address::try_from(&caller_private_key).unwrap();

//     let record_static_str = format!(r#"{{
//         owner: {}.private,
//         liters: 97u64.public,
//         _nonce: 0group.public,
//         _version: 1u8.public
//     }}"#, caller);

//     // Construct the static and dynamic records.
//     let r0_static = Record::<CurrentNetwork, Plaintext<CurrentNetwork>>::from_str(&record_static_str).unwrap(); 
//     let r0_dynamic = DynamicRecord::<CurrentNetwork>::from_record(&r0_static).unwrap();

//     // TODO (Antonio) remove
//     println!("DYNAMIC RECORD OWNER 1: {:#?}", r0_dynamic.owner());

//     // Input and expected output
//     let r0_value = Value::<CurrentNetwork>::DynamicRecord(r0_dynamic);
//     let expected_output = Value::<CurrentNetwork>::from_str("97u64").unwrap();

//     // Initialize a new block store.
//     let block_store = BlockStore::<CurrentNetwork, BlockMemory<_>>::open(StorageMode::new_test(None)).unwrap();
//     // Initialize a new finalize store.
//     let finalize_store = FinalizeStore::<_, FinalizeMemory<_>>::open(StorageMode::new_test(None)).unwrap();

//     // TODO (Antonio) remove
//     println!("\n\n\n\n\n\n\nDeploying...\n\n\n\n\n\n\n");

//     // Add the program to the process.
//     let deployment = process.deploy::<CurrentAleo, _>(&program, rng).unwrap();
//     // Check that the deployment verifies.
//     process.verify_deployment::<CurrentAleo, _>(ConsensusVersion::V12, &deployment, rng).unwrap();

//     // TODO (Antonio) remove
//     println!("\n\n\n\n\n\n\nDeployed and verified\n\n\n\n\n\n\n");

//     // Compute the fee.
//     let fee = sample_fee::<_, CurrentAleo, _, _>(&process, &block_store, &finalize_store, rng);
//     // Finalize the deployment.
//     let (stack, _) = process.finalize_deployment(sample_finalize_state(1), &finalize_store, &deployment, &fee).unwrap();
//     // Synthesize the translation key.
//     stack.synthesize_translation_key::<CurrentAleo, _>(&Identifier::from_str("liquid_container").unwrap(), rng).unwrap();
//     // Add the stack *manually* to the process.
//     process.add_stack(stack);

//     // Authorize the function call.
//     let authorization = process
//         .authorize::<CurrentAleo, _>(&caller_private_key, program.id(), get_dynamic_liters_function_name, [r0_value].iter(), rng)
//         .unwrap();
//     assert_eq!(authorization.len(), 2);
//     println!("\nAuthorize\n{:#?}\n\n", authorization.to_vec_deque());

//     // Compute the output value.
//     let response = process.evaluate::<CurrentAleo>(authorization.replicate()).unwrap();
//     let candidate = response.outputs();
//     assert_eq!(1, candidate.len());
//     assert_eq!(expected_output, candidate[0]);

//     // Check again to make sure we didn't modify the authorization after calling `evaluate`.
//     assert_eq!(authorization.len(), 1);

//     // Execute the request.
//     let (response, mut trace) = process.execute::<CurrentAleo, _>(authorization, rng).unwrap();
//     let candidate = response.outputs();
//     assert_eq!(1, candidate.len());
//     assert_eq!(expected_output, candidate[0]);

//     // TODO (Antonio) reintroduce
//     // // Construct the expected transition order.
//     // let expected_order = [
//     //     (program.id(), consume_liquid_function_name),
//     // ];

//     // // Check the expected transition order.
//     // for (transition, (expected_program_id, expected_function_name)) in
//     //     trace.transitions().iter().zip_eq(expected_order.iter())
//     // {
//     //     assert_eq!(transition.program_id(), *expected_program_id);
//     //     assert_eq!(transition.function_name(), expected_function_name);
//     // }

//     // Prepare the trace.
//     trace.prepare(&Query::from(block_store)).unwrap();

//     // Prove the execution.
//     let execution = trace.prove_execution::<CurrentAleo, _>(program_name_str, VarunaVersion::V2, rng).unwrap();

//     // Verify the execution.
//     process.verify_execution(ConsensusVersion::V10, VarunaVersion::V2, InclusionVersion::V1, &execution).unwrap();
// }

// #[test]
// fn test_execute_with_translation_case_2() {

//     let program_id_as_field = Identifier::<CurrentNetwork>::from_str("liter_fetcher").unwrap().to_field().unwrap();
//     let network_as_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();
//     let function_as_field = Identifier::<CurrentNetwork>::from_str("consume_liquid").unwrap().to_field().unwrap();
    
//     let program_string = format!(r"
//     program liter_fetcher.aleo;
    
//     record liquid_container:
//       owner as address.private;
//       liters as u64.public;
    
//     record gas_container:
//       owner as address.private;
//       liters as u64.public;

//     function consume_liquid:
//       input r0 as liquid_container.record;
//       call.dynamic {program_id_as_field} {network_as_field} {function_as_field} with r0 (as liquid_container.record) into r1 (as boolean.public);
//       output r1 as boolean.public;

//     function consume_container:
//       input r0 as dynamic.record;
//       output true as boolean.public;

//     constructor:
//         assert.eq true true;
//     ");

//     // Initialize a new program.
//     let (remainder, program) = Program::<CurrentNetwork>::parse(&program_string).unwrap();
//     assert!(remainder.is_empty(), "Parser did not consume all of the string: '{remainder}'");

//     // Construct the process.
//     let mut process = crate::test_helpers::sample_process(&program);
// }

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
import credits.aleo;
        
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
        
function dyn_dub_transfer_public:
    input r0 as field.public;
    input r1 as field.public;
    input r2 as field.public;
    input r3 as address.public;
    input r4 as u64.public;
    call.dynamic r0 r1 r2 with r3 r4 (as address.public u64.public) into r5 (as dynamic.future);
    call.dynamic r0 r1 r2 with r3 r4 (as address.public u64.public) into r6 (as dynamic.future);
    async dyn_dub_transfer_public r5 r6 into r7;
    output r7 as test_dcall.aleo/dyn_dub_transfer_public.future;
finalize dyn_dub_transfer_public:
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
        ("test_dynamic_call_to_transfer_public.aleo", "dynamic"),
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
    let amm_program = Program::from_str(
        r"
import credits_a.aleo;
import credits_b.aleo;

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
  input r0 as credits_a.aleo/credits.record;
  // Token a amount
  input r1 as u64.public;
  // Token b amount
  input r2 as u64.public;
  cast r1 r2 into r3 as reserves;
  call credits_a.aleo/transfer_private_to_public r0 aleo1rrj2mgall8mw57lcpkkvkxwqkawpc5rjarqm57w8gux2ahnt9sxqf0md56 r1 into r4 r5;
  call credits_b.aleo/transfer_public_to_private self.signer r2 into r6 r7;
  async buy_token_b r1 r2 r5 r7 into r8;
  // token_a change record
  output r4 as credits_a.aleo/credits.record;
  // token_b receiver record
  output r6 as credits_b.aleo/credits.record;
  output r8 as amm.aleo/buy_token_b.future;

finalize buy_token_b:
  // token_a amount
  input r0 as u64.public;
  // token_b amount
  input r1 as u64.public;
  input r2 as credits_a.aleo/transfer_private_to_public.future;
  input r3 as credits_b.aleo/transfer_public_to_private.future;
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
    let execute_mint_a = vm.execute(&caller_private_key, ("credits_a.aleo", "mint_private"), vec![Value::from_str("100u64")].into_iter(), None, 0, None, rng).unwrap();
    // Execute credits_b.aleo/mint_private to mint a few credits_b records.
    let execute_mint_b = vm.execute(&caller_private_key, ("credits_b.aleo", "mint_private"), vec![Value::from_str("100u64")].into_iter(), None, 0, None, rng).unwrap();
    let block = sample_next_block(&vm, &caller_private_key, &[execute_mint_a, execute_mint_b], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 2);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();
    
    // Obtain the credits records.
    let records = block.records().map(|(_, record)| record.decrypt(&caller_view_key)).collect::<Result<Vec<_>>>().unwrap();
    // Split the records into credits_a and credits_b records.
    let (records_a, records_b) = records.split_at(2);

    // Create the AMM program address.
    let amm_address: Address<CurrentNetwork> = ProgramID::from_str("amm.aleo").unwrap().to_address().unwrap();
    let amm_address_value = Value::from_str(&amm_address.to_string()).unwrap();

    // Execute credits_a.aleo/transfer_private_to_public to give amm.aleo an initial balance of credits_a.
    let execute_transfer_a = vm.execute(&caller_private_key, ("credits_a.aleo", "transfer_private_to_public"), vec![Value::Record(records_a[0].clone()), amm_address_value.clone(), Value::from_str("100u64").unwrap()].into_iter(), None, 0, None, rng).unwrap();
    // Execute credits_b.aleo/transfer_private_to_public to give amm.aleo an initial balance of credits_b.
    let execute_transfer_b = vm.execute(&caller_private_key, ("credits_b.aleo", "transfer_private_to_public"), vec![Value::Record(records_b[0].clone()), amm_address_value.clone(), Value::from_str("100u64").unwrap()].into_iter(), None, 0, None, rng).unwrap();
    let block = sample_next_block(&vm, &caller_private_key, &[execute_transfer_a, execute_transfer_b], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 2);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();
    
    // Execute amm.aleo/buy_token_b to buy token_b.
    let execute_buy_token_b = vm.execute(&caller_private_key, ("amm.aleo", "buy_token_b"), vec![Value::Record(records_a[1].clone()), Value::from_str("100u64").unwrap(), Value::from_str("100u64").unwrap()].into_iter(), None, 0, None, rng).unwrap();
    let block = sample_next_block(&vm, &caller_private_key, &[execute_buy_token_b], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 1);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();
    
    // Obtain the credits_a change and credits_b receiver records.
    let (_change_record, _receiver_record) = block.records().map(|(_, record)| record.decrypt(&caller_view_key)).collect::<Result<Vec<_>>>().unwrap().split_at(1);
}

// TODO (dynamic_dispatch) translation test cases
// - input static -> dynamic
// - input dynamic -> static
// - output static -> dynamic
// - output dynamic -> static
// - input dynamic -> dynamic (no translation)
// - input static -> static (no translation)
// - input dynamic -> dynamic (no translation)
// Chained combinations (not exhaustive)
// - input static -> dynamic subsequently passed as input dynamic -> static
// - output static -> dynamic subsequently passed as output dynamic -> static
// Triple chains
// 
