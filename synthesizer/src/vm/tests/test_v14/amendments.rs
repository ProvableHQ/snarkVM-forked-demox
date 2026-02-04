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

use console::{
    program::{Identifier, ProgramOwner},
    types::U8,
};
use snarkvm_ledger_block::Transaction;
use snarkvm_synthesizer_program::StackTrait;

/// Helper to create a V4 deployment transaction (amendment) with a properly connected fee.
/// V4 deployments have checksum but no owner. They retain translation VKs if the program has records.
fn create_v4_deployment_transaction<C: ConsensusStorage<CurrentNetwork>, R: Rng + CryptoRng>(
    vm: &VM<CurrentNetwork, C>,
    private_key: &PrivateKey<CurrentNetwork>,
    program: &Program<CurrentNetwork>,
    edition: u16,
    rng: &mut R,
) -> Result<Transaction<CurrentNetwork>> {
    // Create a deployment for the program.
    let mut v4_deployment = vm.process().read().deploy::<CurrentAleo, _>(program, rng)?;

    // Set the V4 deployment fields (amendment: checksum but no owner).
    // Translation VKs are retained if the program has records.
    v4_deployment.set_edition_raw(edition);
    v4_deployment.set_program_checksum_raw(Some(program.to_checksum()));
    v4_deployment.set_program_owner_raw(None);

    // Compute the deployment ID.
    let deployment_id = v4_deployment.to_deployment_id()?;

    // Create the owner signature.
    let owner = ProgramOwner::new(private_key, deployment_id, rng)?;

    // Compute the deployment cost.
    let consensus_version = CurrentNetwork::CONSENSUS_VERSION(vm.block_store().current_block_height())?;
    let (minimum_cost, _) = deployment_cost(&vm.process().read(), &v4_deployment, consensus_version)?;

    // Authorize and execute the fee.
    let fee_authorization = vm.authorize_fee_public(private_key, minimum_cost, 0, deployment_id, rng)?;
    let fee = vm.execute_fee_authorization(fee_authorization, None, rng)?;

    // Return the V4 deployment transaction.
    Transaction::from_deployment(owner, v4_deployment, fee)
}

// This test verifies that:
// - V4 deployments (amendments) are rejected before ConsensusVersion::V14
// - V4 deployments (amendments) are accepted at ConsensusVersion::V14
//
// Test flow:
// 1. V9: Deploy a program with records as V2 (checksum + owner, NO translation VKs)
// 2. V14: Create an amendment (checksum, no owner, WITH translation VKs for records)
// 3. Validation passes because translation VKs were added (didn't exist in original V2)
#[test]
fn test_v4_deployment_requires_v14() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = sample_genesis_private_key(rng);

    // Get the V9 and V14 heights.
    let v9_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V9)?;
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;

    // Initialize the VM at V9 height (where V2 deployments are allowed).
    let vm = sample_vm_at_height(v9_height, rng);

    // Define a program with records (so translation VKs will be generated at V14+).
    // At V9, this program is deployed as V2 (no translation VKs).
    // At V14, the amendment adds translation VKs, satisfying the "at least one VK changed" check.
    let program = Program::from_str(
        r"
program amendment_test.aleo;

record token:
    owner as address.private;
    amount as u64.private;

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

    // Create a V4 deployment (amendment).
    let deployed_program = stack.program().clone();

    // Create a V4 deployment transaction with proper fee.
    let v4_transaction = create_v4_deployment_transaction(&vm, &caller_private_key, &deployed_program, 0, rng)?;

    // Attempt to add the V4 deployment before V14 - it should be aborted.
    let block = sample_next_block(&vm, &caller_private_key, &[v4_transaction.clone()], rng)?;
    assert_eq!(block.transactions().num_accepted(), 0, "V4 should be rejected before V14");
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 1, "V4 should be aborted before V14");
    vm.add_next_block(&block)?;

    // Advance the VM to V14 height.
    let transactions: [Transaction<CurrentNetwork>; 0] = [];
    while vm.block_store().current_block_height() < v14_height {
        let block = sample_next_block(&vm, &caller_private_key, &transactions, rng)?;
        vm.add_next_block(&block)?;
    }

    // Verify we're at V14.
    assert!(vm.block_store().current_block_height() >= v14_height);

    // Create a new V4 deployment transaction (need fresh fee).
    let v4_transaction = create_v4_deployment_transaction(&vm, &caller_private_key, &deployed_program, 0, rng)?;

    // Attempt to add the V4 deployment at V14 - it should succeed.
    let block = sample_next_block(&vm, &caller_private_key, &[v4_transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1, "V4 should be accepted at V14");
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block)?;

    // Verify the edition hasn't changed (V4 doesn't change edition).
    let stack = vm.process().read().get_stack("amendment_test.aleo")?;
    assert_eq!(*stack.program_edition(), 0, "Edition should remain 0 after an amendment");

    Ok(())
}

// This test verifies that:
// - After an amendment adds translation VKs, the program can still be executed.
// - The amendment correctly adds the translation VKs to the stack.
//
// Test flow:
// 1. Deploy a program with records at V9 (V2: checksum + owner, NO translation VKs)
// 2. Execute the program to verify it works
// 3. Advance to V14 and create an amendment (adds translation VKs)
// 4. Execute the program again to verify it still works with the new VKs
#[test]
fn test_amendment_updates_vks() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = sample_genesis_private_key(rng);

    // Get the V9 and V14 heights.
    let v9_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V9)?;
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;

    // Initialize the VM at V9 height.
    let vm = sample_vm_at_height(v9_height, rng);

    // Define a program with records.
    let program = Program::from_str(
        r"
program vk_test.aleo;

record token:
    owner as address.private;
    amount as u64.private;

function add_numbers:
    input r0 as u32.public;
    input r1 as u32.public;
    add r0 r1 into r2;
    output r2 as u32.public;

constructor:
    assert.eq true true;
",
    )?;

    // Deploy the program at V9 (V2 deployment: checksum + owner, NO translation VKs).
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    vm.add_next_block(&block)?;

    // Verify the program was deployed without translation VKs.
    let stack = vm.process().read().get_stack("vk_test.aleo")?;
    assert!(
        stack.get_translation_verifying_key(&Identifier::from_str("token")?).is_err(),
        "V2 deployment at V9 should NOT have translation VKs"
    );

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

    // Advance the VM to V14 height.
    let transactions: [Transaction<CurrentNetwork>; 0] = [];
    while vm.block_store().current_block_height() < v14_height {
        let block = sample_next_block(&vm, &caller_private_key, &transactions, rng)?;
        vm.add_next_block(&block)?;
    }

    // Get the deployed program.
    let stack = vm.process().read().get_stack("vk_test.aleo")?;
    let deployed_program = stack.program().clone();

    // Create and apply a V4 deployment (amendment) that adds translation VKs.
    let v4_transaction = create_v4_deployment_transaction(&vm, &caller_private_key, &deployed_program, 0, rng)?;

    let block = sample_next_block(&vm, &caller_private_key, &[v4_transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1, "Amendment should be accepted");
    vm.add_next_block(&block)?;

    // Verify the translation VK was added by the amendment.
    let stack = vm.process().read().get_stack("vk_test.aleo")?;
    assert!(
        stack.get_translation_verifying_key(&Identifier::from_str("token")?).is_ok(),
        "Amendment should have added translation VKs"
    );

    // Verify the edition is still 0.
    assert_eq!(*stack.program_edition(), 0);

    // Execute the program again - should still work with the new translation VKs.
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
// - Amendments must match the existing program exactly.
// - Amendments must have the correct checksum.
// - Amendments must target an existing program.
//
// Test flow:
// 1. Deploy a program with records at V9 (no translation VKs)
// 2. Advance to V14
// 3. Test various invalid amendments (wrong checksum, non-existent program)
// 4. Test valid amendment that adds translation VKs
#[test]
fn test_amendment_validation() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = sample_genesis_private_key(rng);

    // Get the V9 and V14 heights.
    let v9_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V9)?;
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;

    // Initialize the VM at V9 height.
    let vm = sample_vm_at_height(v9_height, rng);

    // Define a program with records (so translation VKs can be added by the amendment).
    let program = Program::from_str(
        r"
program validation_test.aleo;

record token:
    owner as address.private;
    amount as u64.private;

function dummy:
    input r0 as u32.public;
    output r0 as u32.public;

constructor:
    assert.eq true true;
",
    )?;

    // Deploy the program at V9 (no translation VKs).
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    vm.add_next_block(&block)?;

    // Get the deployed program info.
    let stack = vm.process().read().get_stack("validation_test.aleo")?;
    let deployed_program = stack.program().clone();

    // Advance the VM to V14 height.
    let transactions: [Transaction<CurrentNetwork>; 0] = [];
    while vm.block_store().current_block_height() < v14_height {
        let block = sample_next_block(&vm, &caller_private_key, &transactions, rng)?;
        vm.add_next_block(&block)?;
    }

    // Test 1: An amendment with wrong checksum should be rejected by the VM.
    // Note: Raw setters bypass construction validation, so the transaction is created,
    // but the VM will reject it during check_transaction because the checksum doesn't match.
    let mut wrong_checksum_deployment = vm.process().read().deploy::<CurrentAleo, _>(&deployed_program, rng)?;
    wrong_checksum_deployment.set_edition_raw(0);
    wrong_checksum_deployment.set_program_checksum_raw(Some([0u8; 32].map(U8::new))); // Wrong checksum
    wrong_checksum_deployment.set_program_owner_raw(None);

    let deployment_id = wrong_checksum_deployment.to_deployment_id()?;
    let owner = ProgramOwner::new(&caller_private_key, deployment_id, rng)?;
    let consensus_version = CurrentNetwork::CONSENSUS_VERSION(vm.block_store().current_block_height())?;
    let (minimum_cost, _) = deployment_cost(&vm.process().read(), &wrong_checksum_deployment, consensus_version)?;
    let fee_authorization = vm.authorize_fee_public(&caller_private_key, minimum_cost, 0, deployment_id, rng)?;
    let fee = vm.execute_fee_authorization(fee_authorization, None, rng)?;
    let wrong_checksum_tx = Transaction::from_deployment(owner, wrong_checksum_deployment, fee)?;

    // The transaction is created, but should be aborted due to checksum mismatch.
    let block = sample_next_block(&vm, &caller_private_key, &[wrong_checksum_tx], rng)?;
    assert_eq!(block.transactions().num_accepted(), 0, "V4 with wrong checksum should be rejected");
    assert_eq!(block.aborted_transaction_ids().len(), 1);
    vm.add_next_block(&block)?;

    // Test 2: An amendment for non-existent program should fail.
    let nonexistent_program = Program::from_str(
        r"
program nonexistent.aleo;

record token:
    owner as address.private;
    amount as u64.private;

function dummy:

constructor:
    assert.eq true true;
",
    )?;

    let mut nonexistent_deployment = vm.process().read().deploy::<CurrentAleo, _>(&nonexistent_program, rng)?;
    nonexistent_deployment.set_edition_raw(0);
    nonexistent_deployment.set_program_checksum_raw(Some(nonexistent_program.to_checksum()));
    nonexistent_deployment.set_program_owner_raw(None);

    let deployment_id = nonexistent_deployment.to_deployment_id()?;
    let owner = ProgramOwner::new(&caller_private_key, deployment_id, rng)?;
    let (minimum_cost, _) = deployment_cost(&vm.process().read(), &nonexistent_deployment, consensus_version)?;
    let fee_authorization = vm.authorize_fee_public(&caller_private_key, minimum_cost, 0, deployment_id, rng)?;
    let fee = vm.execute_fee_authorization(fee_authorization, None, rng)?;
    let nonexistent_tx = Transaction::from_deployment(owner, nonexistent_deployment, fee)?;

    let block = sample_next_block(&vm, &caller_private_key, &[nonexistent_tx], rng)?;
    assert_eq!(block.transactions().num_accepted(), 0, "V4 for non-existent program should be rejected");
    assert_eq!(block.aborted_transaction_ids().len(), 1);
    vm.add_next_block(&block)?;

    // Test 3: Valid amendment should succeed (adds translation VKs).
    let v4_transaction = create_v4_deployment_transaction(&vm, &caller_private_key, &deployed_program, 0, rng)?;

    let block = sample_next_block(&vm, &caller_private_key, &[v4_transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1, "Valid V4 should be accepted");
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block)?;

    Ok(())
}

// This test verifies that:
// - Anyone can submit an amendment.
// - The amendment submitter is recorded but doesn't affect the program owner.
//
// Test flow:
// 1. Deploy a program with records at V9 as the original owner
// 2. Advance to V14
// 3. Another user submits a amendment (adds translation VKs)
// 4. Verify the program owner hasn't changed
#[test]
fn test_amendment_permissionless() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize the original deployer.
    let original_owner = sample_genesis_private_key(rng);

    // Initialize a different user.
    let other_user = PrivateKey::new(rng)?;
    let other_address = Address::try_from(&other_user)?;

    // Get the V9 and V14 heights.
    let v9_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V9)?;
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;

    // Initialize the VM at V9 height.
    let vm = sample_vm_at_height(v9_height, rng);

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

    // Deploy a program with records as the original owner at V9 (no translation VKs).
    let program = Program::from_str(
        r"
program permissionless_test.aleo;

record token:
    owner as address.private;
    amount as u64.private;

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
    let original_program_owner = *stack.program_owner();

    // Advance the VM to V14 height.
    let transactions: [Transaction<CurrentNetwork>; 0] = [];
    while vm.block_store().current_block_height() < v14_height {
        let block = sample_next_block(&vm, &original_owner, &transactions, rng)?;
        vm.add_next_block(&block)?;
    }

    // Submit an amendment as the OTHER user (not the original owner).
    // This will add translation VKs since the original deployment didn't have them.
    let v4_transaction = create_v4_deployment_transaction(&vm, &other_user, &deployed_program, 0, rng)?;

    // The amendment should be accepted even though submitted by a different user.
    let block = sample_next_block(&vm, &original_owner, &[v4_transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1, "V4 from different user should be accepted");
    vm.add_next_block(&block)?;

    // Verify the program owner hasn't changed.
    let stack = vm.process().read().get_stack("permissionless_test.aleo")?;
    assert_eq!(
        *stack.program_owner(),
        original_program_owner,
        "Program owner should remain unchanged after an amendment"
    );

    Ok(())
}

// This test verifies that:
// - credits.aleo cannot be amended with V4.
// Note: credits.aleo is protected at multiple levels:
//   1. Process::deploy blocks re-initialization of credits.aleo
//   2. The verification logic also checks for credits.aleo amendments
// This test verifies level 1 - the deployment creation itself is blocked.
#[test]
fn test_credits_cannot_be_amended() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize the VM at V14 height.
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;
    let vm = sample_vm_at_height(v14_height, rng);

    // Get the credits program.
    let credits_program = Program::credits()?;

    // Attempt to create a V4 deployment for credits.aleo - this should fail.
    let result = vm.process().read().deploy::<CurrentAleo, _>(&credits_program, rng);

    // Verify that creating a deployment for credits.aleo fails.
    assert!(result.is_err(), "Creating a deployment for credits.aleo should fail");
    let error = result.unwrap_err().to_string();
    assert!(error.contains("credits.aleo"), "Error should mention credits.aleo: {error}");

    Ok(())
}

// This test verifies that:
// - An amendment that adds translation VKs succeeds.
// - A subsequent amendment with no VK changes is rejected.
//
// Test flow:
// 1. Deploy a program with records at V9 (no translation VKs)
// 2. Advance to V14
// 3. First amendment succeeds (adds translation VKs)
// 4. Second amendment fails (no VK changes since circuits are deterministic)
#[test]
fn test_multiple_amendments() -> Result<()> {
    let rng = &mut TestRng::default();

    // Initialize a new caller.
    let caller_private_key = sample_genesis_private_key(rng);

    // Get the V9 and V14 heights.
    let v9_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V9)?;
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;

    // Initialize the VM at V9 height.
    let vm = sample_vm_at_height(v9_height, rng);

    // Define a program with records.
    let program = Program::from_str(
        r"
program multi_amend.aleo;

record token:
    owner as address.private;
    amount as u64.private;

function increment:
    input r0 as u32.public;
    add r0 1u32 into r1;
    output r1 as u32.public;

constructor:
    assert.eq true true;
",
    )?;

    // Deploy the program at V9 (no translation VKs).
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1);
    vm.add_next_block(&block)?;

    // Verify deployed without translation VKs.
    let stack = vm.process().read().get_stack("multi_amend.aleo")?;
    assert!(
        stack.get_translation_verifying_key(&Identifier::from_str("token")?).is_err(),
        "V2 deployment should not have translation VKs"
    );
    let deployed_program = stack.program().clone();

    // Advance the VM to V14 height.
    let transactions: [Transaction<CurrentNetwork>; 0] = [];
    while vm.block_store().current_block_height() < v14_height {
        let block = sample_next_block(&vm, &caller_private_key, &transactions, rng)?;
        vm.add_next_block(&block)?;
    }

    // First amendment: Adds translation VKs - should succeed.
    let v4_transaction = create_v4_deployment_transaction(&vm, &caller_private_key, &deployed_program, 0, rng)?;

    let block = sample_next_block(&vm, &caller_private_key, &[v4_transaction], rng)?;
    assert_eq!(block.transactions().num_accepted(), 1, "First amendment should be accepted (adds translation VKs)");
    vm.add_next_block(&block)?;

    // Verify the translation VK was added.
    let stack = vm.process().read().get_stack("multi_amend.aleo")?;
    assert!(
        stack.get_translation_verifying_key(&Identifier::from_str("token")?).is_ok(),
        "First amendment should have added translation VKs"
    );

    // Verify edition is still 0.
    assert_eq!(*stack.program_edition(), 0, "Edition should remain 0 after amendment");

    // Second amendment: No VK changes (deterministic circuits) - should fail.
    let v4_transaction_2 = create_v4_deployment_transaction(&vm, &caller_private_key, &deployed_program, 0, rng)?;

    let block = sample_next_block(&vm, &caller_private_key, &[v4_transaction_2], rng)?;
    assert_eq!(block.transactions().num_accepted(), 0, "Second amendment should be rejected (no VK changes)");
    assert_eq!(block.aborted_transaction_ids().len(), 1);
    vm.add_next_block(&block)?;

    // Execute the program to verify it still works after the first amendment.
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
