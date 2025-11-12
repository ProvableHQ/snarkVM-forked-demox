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
    program::{Identifier, Value},
};
use snarkvm_synthesizer_program::Program;
use snarkvm_utilities::TestRng;

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
        
program test_dcall_to_transfer_public.aleo;

//function static:
//    input r0 as address.public;
//    input r1 as u64.public;
//    dcall credits transfer_public with r0 r1 (as address.public u64.public) into r2 (as dynamic.future);
//    async static r2 into r3;
//    output r3 as test_dcall_to_transfer_public.aleo/static.future;
//finalize static:
//    input r0 as dynamic.future;
//    await r0; 
        
function dynamic:
    input r0 as field.public;
    input r1 as field.public;
    input r2 as field.public;
    input r3 as address.public;
    input r4 as u64.public;
    call.dynamic r0 r1 r2 with r3 r4 (as address.public u64.public) into r5 (as dynamic.future);
    call.dynamic r0 r1 r2 with r3 r4 (as address.public u64.public) into r6 (as dynamic.future);
    async dynamic r5 r6 into r7;
    output r7 as test_dcall_to_transfer_public.aleo/dynamic.future;
finalize dynamic:
    input r0 as dynamic.future;
    input r1 as dynamic.future;
    await r1;
    await r0;
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
