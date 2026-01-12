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

use crate::vm::test_helpers::*;

use console::{
    account::{Address, PrivateKey},
    network::ConsensusVersion,
    program::{Identifier, ProgramOwner, Value},
    types::U8,
};
use snarkvm_ledger_block::Transaction;
use snarkvm_synthesizer_program::{Program, StackTrait};
use snarkvm_utilities::TestRng;

// This test verifies that:
// - V3 (amendment) deployments are rejected before ConsensusVersion::V14
// - V3 deployments are accepted at ConsensusVersion::V14
#[test]
fn test_v3_deployment_requires_v14() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = sample_genesis_private_key(rng);

    // Get the V9 and V14 heights.
    let v9_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V9)?;
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;

    // Initialize the VM at V9 height (where V2 deployments are allowed).
    let vm = sample_vm_at_height(v9_height, rng);

    // Define a program.
    let program = Program::from_str(
        r"
program amendment_test.aleo;

function compute:
    input r0 as u32.public;
    add r0 r0 into r1;
    output r1 as u32.public;

constructor:
    assert.eq true true;
",
    )?;

    // Deploy the program as V2.
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block)?;

    // Verify the program is deployed with edition 0.
    let stack = vm.process().read().get_stack("amendment_test.aleo")?;
    assert_eq!(*stack.program_edition(), 0);

    // Create a V3 amendment deployment.
    let deployed_program = stack.program().clone();
    let checksum = deployed_program.to_checksum();

    // Generate new VKs by deploying again.
    let mut v3_deployment = vm.process().read().deploy::<CurrentAleo, _>(&deployed_program, rng)?;
    v3_deployment.set_program_checksum_raw(Some(checksum));
    v3_deployment.set_program_owner_raw(None); // V3 has no owner in the deployment struct

    // Create a V3 deployment transaction.
    let deployment_id = v3_deployment.to_deployment_id()?;
    let owner = ProgramOwner::new(&caller_private_key, deployment_id, rng)?;
    let fee = snarkvm_ledger_test_helpers::sample_fee_public(deployment_id, rng);
    let v3_transaction = Transaction::from_deployment(owner, v3_deployment.clone(), fee)?;

    // Attempt to add the V3 deployment before V14 - it should be aborted.
    let block = sample_next_block(&vm, &caller_private_key, &[v3_transaction.clone()], rng)?;
    assert_eq!(block.transactions().num_accepted(), 0, "V3 should be rejected before V14");
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 1, "V3 should be aborted before V14");
    vm.add_next_block(&block)?;

    // Advance the VM to V14 height.
    let transactions: [Transaction<CurrentNetwork>; 0] = [];
    while vm.block_store().current_block_height() < v14_height {
        let block = sample_next_block(&vm, &caller_private_key, &transactions, rng)?;
        vm.add_next_block(&block)?;
    }

    // Verify we're at V14.
    assert!(vm.block_store().current_block_height() >= v14_height);

    // Create a new V3 deployment transaction (need fresh fee).
    let mut v3_deployment = vm.process().read().deploy::<CurrentAleo, _>(&deployed_program, rng)?;
    v3_deployment.set_program_checksum_raw(Some(checksum));
    v3_deployment.set_program_owner_raw(None);

    let deployment_id = v3_deployment.to_deployment_id()?;
    let owner = ProgramOwner::new(&caller_private_key, deployment_id, rng)?;
    let fee = snarkvm_ledger_test_helpers::sample_fee_public(deployment_id, rng);
    let v3_transaction = Transaction::from_deployment(owner, v3_deployment, fee)?;

    // Attempt to add the V3 deployment at V14 - it should succeed.
    let block = sample_next_block(&vm, &caller_private_key, &[v3_transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1, "V3 should be accepted at V14");
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block)?;

    // Verify the edition hasn't changed (V3 doesn't change edition).
    let stack = vm.process().read().get_stack("amendment_test.aleo")?;
    assert_eq!(*stack.program_edition(), 0, "Edition should remain 0 after V3 amendment");

    Ok(())
}

// This test verifies that:
// - After a V3 amendment, executions use the new VKs.
// - Multiple amendments can be applied to the same program.
#[test]
fn test_v3_amendment_updates_vks() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = sample_genesis_private_key(rng);

    // Initialize the VM at V14 height.
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;
    let vm = sample_vm_at_height(v14_height, rng);

    // Define a program.
    let program = Program::from_str(
        r"
program vk_test.aleo;

function add_numbers:
    input r0 as u32.public;
    input r1 as u32.public;
    add r0 r1 into r2;
    output r2 as u32.public;

constructor:
    assert.eq true true;
",
    )?;

    // Deploy the program.
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    vm.add_next_block(&block)?;

    // Execute the program to verify it works.
    let execution = vm.execute(
        &caller_private_key,
        ("vk_test.aleo", "add_numbers"),
        vec![Value::from_str("5u32")?, Value::from_str("3u32")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    assert!(vm.check_transaction(&execution, None, rng).is_ok());
    let block = sample_next_block(&vm, &caller_private_key, &[execution.clone()], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    vm.add_next_block(&block)?;

    // Get the original VK.
    let stack = vm.process().read().get_stack("vk_test.aleo")?;
    let _original_vk = stack.get_verifying_key(&Identifier::from_str("add_numbers")?)?;
    let deployed_program = stack.program().clone();
    let checksum = deployed_program.to_checksum();

    // Create and apply a V3 amendment.
    let mut v3_deployment = vm.process().read().deploy::<CurrentAleo, _>(&deployed_program, rng)?;
    v3_deployment.set_program_checksum_raw(Some(checksum));
    v3_deployment.set_program_owner_raw(None);

    // Store the new VK from the amendment.
    let function_name = Identifier::from_str("add_numbers")?;
    let amended_vk = v3_deployment.verifying_keys().iter().find(|(id, _)| *id == function_name).unwrap().1.0.clone();

    let deployment_id = v3_deployment.to_deployment_id()?;
    let owner = ProgramOwner::new(&caller_private_key, deployment_id, rng)?;
    let fee = snarkvm_ledger_test_helpers::sample_fee_public(deployment_id, rng);
    let v3_transaction = Transaction::from_deployment(owner, v3_deployment, fee)?;

    let block = sample_next_block(&vm, &caller_private_key, &[v3_transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    vm.add_next_block(&block)?;

    // Verify the VK was updated.
    let stack = vm.process().read().get_stack("vk_test.aleo")?;
    let current_vk = stack.get_verifying_key(&Identifier::from_str("add_numbers")?)?;
    assert_eq!(*current_vk, *amended_vk, "VK should be updated after V3 amendment");

    // Verify the edition is still 0.
    assert_eq!(*stack.program_edition(), 0);

    // Execute the program again with the new VKs - should still work.
    let execution = vm.execute(
        &caller_private_key,
        ("vk_test.aleo", "add_numbers"),
        vec![Value::from_str("10u32")?, Value::from_str("20u32")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    assert!(vm.check_transaction(&execution, None, rng).is_ok());
    let block = sample_next_block(&vm, &caller_private_key, &[execution], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    vm.add_next_block(&block)?;

    Ok(())
}

// This test verifies that:
// - V3 amendments must match the existing program exactly.
// - V3 amendments must have the correct checksum.
// - V3 amendments must target an existing program.
#[test]
fn test_v3_amendment_validation() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = sample_genesis_private_key(rng);

    // Initialize the VM at V14 height.
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;
    let vm = sample_vm_at_height(v14_height, rng);

    // Define a program.
    let program = Program::from_str(
        r"
program validation_test.aleo;

function dummy:
    input r0 as u32.public;
    output r0 as u32.public;

constructor:
    assert.eq true true;
",
    )?;

    // Deploy the program.
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    vm.add_next_block(&block)?;

    // Get the deployed program info.
    let stack = vm.process().read().get_stack("validation_test.aleo")?;
    let deployed_program = stack.program().clone();
    let correct_checksum = deployed_program.to_checksum();

    // Test 1: V3 amendment with wrong checksum should fail.
    let mut wrong_checksum_deployment = vm.process().read().deploy::<CurrentAleo, _>(&deployed_program, rng)?;
    wrong_checksum_deployment.set_program_checksum_raw(Some([0u8; 32].map(U8::new))); // Wrong checksum
    wrong_checksum_deployment.set_program_owner_raw(None);

    let deployment_id = wrong_checksum_deployment.to_deployment_id()?;
    let owner = ProgramOwner::new(&caller_private_key, deployment_id, rng)?;
    let fee = snarkvm_ledger_test_helpers::sample_fee_public(deployment_id, rng);
    let wrong_checksum_tx = Transaction::from_deployment(owner, wrong_checksum_deployment, fee)?;

    let block = sample_next_block(&vm, &caller_private_key, &[wrong_checksum_tx], rng)?;
    assert_eq!(block.transactions().num_accepted(), 0, "V3 with wrong checksum should be rejected");
    assert_eq!(block.aborted_transaction_ids().len(), 1);
    vm.add_next_block(&block)?;

    // Test 2: V3 amendment for non-existent program should fail.
    let nonexistent_program = Program::from_str(
        r"
program nonexistent.aleo;

function dummy:

constructor:
    assert.eq true true;
",
    )?;

    let mut nonexistent_deployment = vm.process().read().deploy::<CurrentAleo, _>(&nonexistent_program, rng)?;
    nonexistent_deployment.set_program_checksum_raw(Some(nonexistent_program.to_checksum()));
    nonexistent_deployment.set_program_owner_raw(None);

    let deployment_id = nonexistent_deployment.to_deployment_id()?;
    let owner = ProgramOwner::new(&caller_private_key, deployment_id, rng)?;
    let fee = snarkvm_ledger_test_helpers::sample_fee_public(deployment_id, rng);
    let nonexistent_tx = Transaction::from_deployment(owner, nonexistent_deployment, fee)?;

    let block = sample_next_block(&vm, &caller_private_key, &[nonexistent_tx], rng)?;
    assert_eq!(block.transactions().num_accepted(), 0, "V3 for non-existent program should be rejected");
    assert_eq!(block.aborted_transaction_ids().len(), 1);
    vm.add_next_block(&block)?;

    // Test 3: Valid V3 amendment should succeed.
    let mut valid_deployment = vm.process().read().deploy::<CurrentAleo, _>(&deployed_program, rng)?;
    valid_deployment.set_program_checksum_raw(Some(correct_checksum));
    valid_deployment.set_program_owner_raw(None);

    let deployment_id = valid_deployment.to_deployment_id()?;
    let owner = ProgramOwner::new(&caller_private_key, deployment_id, rng)?;
    let fee = snarkvm_ledger_test_helpers::sample_fee_public(deployment_id, rng);
    let valid_tx = Transaction::from_deployment(owner, valid_deployment, fee)?;

    let block = sample_next_block(&vm, &caller_private_key, &[valid_tx], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1, "Valid V3 should be accepted");
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block)?;

    Ok(())
}

// This test verifies that:
// - Anyone can submit a V3 amendment (not just the original owner).
// - The amendment submitter is recorded but doesn't affect the program owner.
#[test]
fn test_v3_amendment_permissionless() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize the original deployer.
    let original_owner = sample_genesis_private_key(rng);

    // Initialize a different user.
    let other_user = PrivateKey::new(rng)?;
    let other_address = Address::try_from(&other_user)?;

    // Initialize the VM at V14 height.
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;
    let vm = sample_vm_at_height(v14_height, rng);

    // Fund the other user.
    let transfer = vm.execute(
        &original_owner,
        ("credits.aleo", "transfer_public"),
        vec![Value::from_str(&format!("{other_address}"))?, Value::from_str("1_000_000_000_000u64")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    let block = sample_next_block(&vm, &original_owner, &[transfer], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    vm.add_next_block(&block)?;

    // Deploy a program as the original owner.
    let program = Program::from_str(
        r"
program permissionless_test.aleo;

function dummy:
    input r0 as u32.public;
    output r0 as u32.public;

constructor:
    assert.eq true true;
",
    )?;

    let deployment = vm.deploy(&original_owner, &program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &original_owner, &[deployment], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    vm.add_next_block(&block)?;

    // Get the deployed program info.
    let stack = vm.process().read().get_stack("permissionless_test.aleo")?;
    let deployed_program = stack.program().clone();
    let checksum = deployed_program.to_checksum();
    let original_program_owner = *stack.program_owner();

    // Submit a V3 amendment as the OTHER user (not the original owner).
    let mut v3_deployment = vm.process().read().deploy::<CurrentAleo, _>(&deployed_program, rng)?;
    v3_deployment.set_program_checksum_raw(Some(checksum));
    v3_deployment.set_program_owner_raw(None);

    let deployment_id = v3_deployment.to_deployment_id()?;
    // The OTHER user signs the deployment transaction.
    let owner = ProgramOwner::new(&other_user, deployment_id, rng)?;
    let fee = snarkvm_ledger_test_helpers::sample_fee_public(deployment_id, rng);
    let v3_transaction = Transaction::from_deployment(owner, v3_deployment, fee)?;

    // The V3 amendment should be accepted even though submitted by a different user.
    let block = sample_next_block(&vm, &original_owner, &[v3_transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1, "V3 from different user should be accepted");
    vm.add_next_block(&block)?;

    // Verify the program owner hasn't changed.
    let stack = vm.process().read().get_stack("permissionless_test.aleo")?;
    assert_eq!(
        *stack.program_owner(),
        original_program_owner,
        "Program owner should remain unchanged after V3 amendment"
    );

    Ok(())
}

// This test verifies that:
// - credits.aleo cannot be amended with V3.
#[test]
fn test_credits_cannot_be_amended() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = sample_genesis_private_key(rng);

    // Initialize the VM at V14 height.
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;
    let vm = sample_vm_at_height(v14_height, rng);

    // Get the credits program.
    let credits_program = Program::credits()?;
    let checksum = credits_program.to_checksum();

    // Attempt to create a V3 amendment for credits.aleo.
    let mut v3_deployment = vm.process().read().deploy::<CurrentAleo, _>(&credits_program, rng)?;
    v3_deployment.set_program_checksum_raw(Some(checksum));
    v3_deployment.set_program_owner_raw(None);

    let deployment_id = v3_deployment.to_deployment_id()?;
    let owner = ProgramOwner::new(&caller_private_key, deployment_id, rng)?;
    let fee = snarkvm_ledger_test_helpers::sample_fee_public(deployment_id, rng);
    let v3_transaction = Transaction::from_deployment(owner, v3_deployment, fee)?;

    // The V3 amendment for credits.aleo should be rejected.
    let block = sample_next_block(&vm, &caller_private_key, &[v3_transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 0, "V3 for credits.aleo should be rejected");
    assert_eq!(block.aborted_transaction_ids().len(), 1);
    vm.add_next_block(&block)?;

    Ok(())
}

// This test verifies that:
// - Multiple sequential V3 amendments can be applied to the same program.
// - Each amendment updates the VKs correctly.
#[test]
fn test_multiple_v3_amendments() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = sample_genesis_private_key(rng);

    // Initialize the VM at V14 height.
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;
    let vm = sample_vm_at_height(v14_height, rng);

    // Define a program.
    let program = Program::from_str(
        r"
program multi_amend.aleo;

function increment:
    input r0 as u32.public;
    add r0 1u32 into r1;
    output r1 as u32.public;

constructor:
    assert.eq true true;
",
    )?;

    // Deploy the program.
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    vm.add_next_block(&block)?;

    let stack = vm.process().read().get_stack("multi_amend.aleo")?;
    let deployed_program = stack.program().clone();
    let checksum = deployed_program.to_checksum();

    // Apply multiple V3 amendments.
    for i in 1..=3 {
        let mut v3_deployment = vm.process().read().deploy::<CurrentAleo, _>(&deployed_program, rng)?;
        v3_deployment.set_program_checksum_raw(Some(checksum));
        v3_deployment.set_program_owner_raw(None);

        let function_name = Identifier::from_str("increment")?;
        let new_vk = v3_deployment.verifying_keys().iter().find(|(id, _)| *id == function_name).unwrap().1.0.clone();

        let deployment_id = v3_deployment.to_deployment_id()?;
        let owner = ProgramOwner::new(&caller_private_key, deployment_id, rng)?;
        let fee = snarkvm_ledger_test_helpers::sample_fee_public(deployment_id, rng);
        let v3_transaction = Transaction::from_deployment(owner, v3_deployment, fee)?;

        let block = sample_next_block(&vm, &caller_private_key, &[v3_transaction], rng)?;
        assert_eq!(block.transactions().num_accepted(), 1, "Amendment {i} should be accepted");
        vm.add_next_block(&block)?;

        // Verify the VK was updated.
        let stack = vm.process().read().get_stack("multi_amend.aleo")?;
        let current_vk = stack.get_verifying_key(&Identifier::from_str("increment")?)?;
        assert_eq!(*current_vk, *new_vk, "VK should be updated after amendment {i}");

        // Verify edition is still 0.
        assert_eq!(*stack.program_edition(), 0, "Edition should remain 0 after amendment {i}");
    }

    // Execute the program to verify it still works after multiple amendments.
    let execution = vm.execute(
        &caller_private_key,
        ("multi_amend.aleo", "increment"),
        vec![Value::from_str("42u32")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    assert!(vm.check_transaction(&execution, None, rng).is_ok());

    Ok(())
}
