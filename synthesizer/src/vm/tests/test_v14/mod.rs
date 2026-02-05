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

// Tests for casting static records to `dynamic.record`.
mod cast;

// Tests for the `get.record.dynamic` instruction.
mod get_record_dynamic;

// Tests for `contains.dynamic`, `get.dynamic`, and `get.or_use.dynamic` in finalize blocks.
mod dynamic_mapping_operations;

// Integration tests combining translation, casting, and dynamic record operations.
mod mixed;

// Tests for the `call.dynamic` instruction with various call patterns.
mod call_dynamic;

// Tests for `DynamicFuture` behavior including await ordering and conditional execution.
mod dynamic_futures;

// Tests for recursive dynamic function calls and double-spend detection.
mod recursion;

// Tests for record translation between static and dynamic representations.
mod translation;

// Tests comparing static vs dynamic calls to all credits.aleo functions.
mod compare_calls_to_credits;

// Tests for restricted keywords at V14.
mod restricted_keywords;

// Tests for V4 deployments (amendments).
mod amendments;

use super::*;

use crate::{
    circuit::{Eject, Inject, Mode},
    vm::test_helpers::{sample_vm_at_height, *},
};

use anyhow::Result;
use console::{
    account::{Address, ViewKey},
    network::ConsensusVersion,
    program::{DynamicRecord, Entry, Identifier, Value},
};
use snarkvm_synthesizer_process::{deployment_cost, execution_cost, execution_cost_for_authorization};
use snarkvm_synthesizer_program::Program;
use snarkvm_utilities::TestRng;

/************************* Dynamic-record test cases *************************/
//
// The following list contains some translation- and dynamic-record-related
// situations tested in this module. Note it is non-exhaustive in that it does
// not detail all tested aspects of the functionality. Each situation is
// followed by a test case (of several, in some instances) where it arises.
//
// Single-translation test cases
// - input dynamic -> static external
//   In: translation.rs::test_translation_input_dynamic_external
// - input dynamic -> static non-external
//   In: translation.rs::test_translation_input_dynamic_non_external
// - output static non-external -> dynamic
//   In: translation.rs::test_translation_output_non_external_dynamic
// - output static external -> dynamic
//   In: translation.rs::test_translation_output_external_dynamic
//
// Chained cases
// - Static record minted in previous transaction converted to dynamic one outside the ledger and VM, then:
//       passed as input dynamic -> static
//       modify it (= mint new one)
//       output static -> dynamic
//       input dynamic -> dynamic (no translation)
//       input dynamic -> static
//   In: translation.rs::test_translation_triple
// - Input (dynamic, dynamic, dynamic) -> (static, static, static), output as static -> dynamic
//   In: mixed.rs: test_execution_cost_for_authorization
//
// get.record.dynamic
// - Record entries with different visibility but coinciding identifiers can be read with the same get.record.dynamic instruction
//   In: mixed.rs::test_translation_get_dynamic_cast_to_dynamic
//       note product_id is private in toy.record and public in ladder.record and both are read in manager.aleo/verify_signature
// - Dynamic records coming from different static records can be read with the same get.record.dynamic instruction
//   In: mixed.rs::test_translation_get_dynamic_cast_to_dynamic (e. g. manager.aleo/verify_signature)
//
// Consumption/production
// - Casting a static record into a dynamic one and passing the latter to a function expecting a dynamic record does not consume it
//   In: mixed.rs::test_translation_get_dynamic_cast_to_dynamic Case 1
//       (the call to function_verify_signature_field does not cause a double spend, as expected)
// - Casting a static record into a dynamic one and passing the latter to a function expecting a static record (translation involved) consumes it
//   In: cast.rs::test_cast_simple Case 2 (fails due to double spend)
// - A record is only produced once even if it is subsequently output as a dynamic record by the caller
//   In: mixed.rs::test_execution_cost_for_authorization
// - A record is only consumed once even if it is subsequently passed as a dynamic record to a callee
//   In: mixed.rs::test_translation_get_dynamic_cast_to_dynamic
//
// Key-fetching
// - Translations for the same record across different transitions are proved/verified with the same key (in the same Varuna batch)
//   In: translation.rs::test_translation_triple
//       three translations for gas.record:
//        - input dynamic -> static non-external
//        - output static non-external -> dynamic
//        - input dynamic -> static external
//       Run with the snark-print feature and observe the batch with 3 instances at the end
// - output static {program_a/record_name_a, program_a/record_name_b, program_b/record_name_a, program_b/ record_name_b} -> dynamic: four differeny keys should be fetched
//   In: translation.rs::test_differing_keys
//       Run with the snark-print feature and observe the batch sizes [1, 1, 1, 1, 1, 1, 1, 1, 1] (translation key IDs are also displayed for convenience)
//
// Signature consistency
// - Translate an output record fom a call to a preexisting program to ensure signature-verification circuit has not changed
//   In: get_record_dynamic.rs::translate_transfer_public_to_private

/// Tests that V3 deployments without records have empty translation verifying keys.
#[test]
fn test_v3_deployment_without_records() {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    // Deploy a simple program without records (with constructor for V9+).
    let program = Program::from_str(
        r"
program v3_no_records_test.aleo;

function compute:
    input r0 as u64.private;
    add r0 1u64 into r1;
    output r1 as u64.private;

constructor:
    assert.eq true true;
",
    )
    .unwrap();

    // Create a deployment transaction.
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();

    // Verify the deployment has translation verifying keys (V3 format).
    // At V14, all deployments must have translation_verifying_keys = Some(...).
    match &deployment {
        Transaction::Deploy(_, _, _, deploy, _) => {
            // Programs without records should have Some(vec![]).
            assert!(
                deploy.translation_verifying_keys().is_some(),
                "V14 deployment should have translation_verifying_keys = Some(...)"
            );
            assert!(
                deploy.translation_verifying_keys().as_ref().unwrap().is_empty(),
                "Program without records should have empty translation verifying keys"
            );
        }
        _ => panic!("Expected deploy transaction"),
    }

    // The deployment should succeed since it's properly formatted for V14.
    add_and_test(&vm, &caller_private_key, &[deployment], rng);
}

/// Tests that V3 deployments with records have non-empty translation verifying keys.
#[test]
fn test_v3_deployment_with_records() {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    // Deploy a program with records (with constructor for V9+).
    let program = Program::from_str(
        r"
program v3_record_test.aleo;

record token:
    owner as address.private;
    amount as u64.private;

function mint:
    input r0 as address.private;
    input r1 as u64.private;
    cast r0 r1 into r2 as token.record;
    output r2 as token.record;

constructor:
    assert.eq true true;
",
    )
    .unwrap();

    // Create a deployment transaction.
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();

    // Verify the deployment has non-empty translation verifying keys.
    match &deployment {
        Transaction::Deploy(_, _, _, deploy, _) => {
            assert!(
                deploy.translation_verifying_keys().is_some(),
                "V14 deployment should have translation_verifying_keys = Some(...)"
            );
            assert!(
                !deploy.translation_verifying_keys().as_ref().unwrap().is_empty(),
                "Program with records should have non-empty translation verifying keys"
            );
        }
        _ => panic!("Expected deploy transaction"),
    }

    // The deployment should succeed.
    add_and_test(&vm, &caller_private_key, &[deployment], rng);
}

/// Tests that V2 deployments are allowed before V14.
#[test]
fn test_v2_deployment_allowed_before_v14() {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    // Use V13 height instead of V14.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V13).unwrap(), rng);

    // Deploy a simple program (with constructor for V9+).
    let program = Program::from_str(
        r"
program v2_before_v14_test.aleo;

function compute:
    input r0 as u64.private;
    add r0 1u64 into r1;
    output r1 as u64.private;

constructor:
    assert.eq true true;
",
    )
    .unwrap();

    // Create a deployment transaction.
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();

    // Verify the deployment does NOT have translation verifying keys (V2 format).
    match &deployment {
        Transaction::Deploy(_, _, _, deploy, _) => {
            assert!(
                deploy.translation_verifying_keys().is_none(),
                "Pre-V14 deployment should have translation_verifying_keys = None (V2 format)"
            );
        }
        _ => panic!("Expected deploy transaction"),
    }

    // The deployment should succeed.
    let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 1);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();
}

/// Tests that a V2 deployment (created with translation_verifying_keys = None)
/// is rejected when verified at V14.
#[test]
fn test_v2_deployment_transaction_rejected_at_v14() {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);

    // Create a VM at V13 to construct a V2 deployment.
    let vm_v13 = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V13).unwrap(), rng);

    // Deploy a simple program at V13 to get a V2 deployment transaction (with constructor for V9+).
    let program = Program::from_str(
        r"
program v2_rejected_at_v14_test.aleo;

function compute:
    input r0 as u64.private;
    add r0 1u64 into r1;
    output r1 as u64.private;

constructor:
    assert.eq true true;
",
    )
    .unwrap();

    // Create a V2 deployment transaction at V13.
    let v2_deployment = vm_v13.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();

    // Verify it's a V2 deployment.
    match &v2_deployment {
        Transaction::Deploy(_, _, _, deploy, _) => {
            assert!(
                deploy.translation_verifying_keys().is_none(),
                "V2 deployment should have translation_verifying_keys = None"
            );
        }
        _ => panic!("Expected deploy transaction"),
    }

    // Now create a VM at V14 and try to verify/include the V2 deployment.
    let vm_v14 = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    // The V2 deployment should be rejected at V14.
    let block = sample_next_block(&vm_v14, &caller_private_key, &[v2_deployment], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 0, "V2 deployment should be rejected at V14");
    assert_eq!(block.aborted_transaction_ids().len(), 1, "V2 deployment should be aborted at V14");
}

/// Tests that a V2 deployment deployed before V14 can still be accessed from the VM after V14.
/// This verifies backwards compatibility for reading existing V2 deployments.
#[test]
fn test_v2_deployment_accessible_after_v14() {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);

    // Start at V13.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V13).unwrap(), rng);

    // Deploy a program at V13 (V2 format, with constructor for V9+).
    let program = Program::from_str(
        r"
program v2_accessible_after_v14_test.aleo;

function compute:
    input r0 as u64.private;
    add r0 1u64 into r1;
    output r1 as u64.private;

constructor:
    assert.eq true true;
",
    )
    .unwrap();

    // Create and finalize the deployment at V13.
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
    let transaction_id = deployment.id();

    // Verify it's a V2 deployment.
    match &deployment {
        Transaction::Deploy(_, _, _, deploy, _) => {
            assert!(
                deploy.translation_verifying_keys().is_none(),
                "V2 deployment should have translation_verifying_keys = None"
            );
        }
        _ => panic!("Expected deploy transaction"),
    }

    // Add the deployment to the ledger at V13.
    let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 1);
    vm.add_next_block(&block).unwrap();

    // Advance the VM to V14 by adding blocks.
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap();
    while vm.block_store().current_block_height() < v14_height {
        let block = sample_next_block(&vm, &caller_private_key, &[], rng).unwrap();
        vm.add_next_block(&block).unwrap();
    }

    // Verify we're at V14.
    assert!(vm.block_store().current_block_height() >= v14_height, "VM should be at or past V14 height");

    // The V2 deployment should still be accessible from the VM.
    let retrieved_tx = vm.transaction_store().get_transaction(&transaction_id).unwrap();
    assert!(retrieved_tx.is_some(), "V2 deployment should still be accessible after V14");

    // Verify the retrieved deployment still has V2 format (translation_verifying_keys = None).
    match retrieved_tx.unwrap() {
        Transaction::Deploy(_, _, _, deploy, _) => {
            assert!(
                deploy.translation_verifying_keys().is_none(),
                "Retrieved V2 deployment should still have translation_verifying_keys = None"
            );
        }
        _ => panic!("Expected deploy transaction"),
    }

    // Verify the program can still be used by executing it.
    let program_id = ProgramID::from_str("v2_accessible_after_v14_test.aleo").unwrap();
    let inputs = vec![Value::from_str("1u64").unwrap()];
    let execution = vm
        .execute(
            &caller_private_key,
            (program_id, Identifier::from_str("compute").unwrap()),
            inputs.iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();
    let exec_block = sample_next_block(&vm, &caller_private_key, &[execution], rng).unwrap();
    assert_eq!(exec_block.transactions().num_accepted(), 1, "Execution of V2 program should succeed after V14");
}

// Adds the given transactions to a new block and asserts all of them were
// accepted
fn add_and_test(
    vm: &VM<CurrentNetwork, LedgerType>,
    caller_private_key: &PrivateKey<CurrentNetwork>,
    transactions: &[Transaction<CurrentNetwork>],
    rng: &mut TestRng,
) {
    // Check the transactions.
    let transactions: Vec<_> = transactions
        .iter()
        .map(|tx_0| {
            // Serialize and deserialize the transaction to ensure consistency.
            let tx_bytes_0 = tx_0.to_bytes_le().unwrap();
            let tx_1 = Transaction::<CurrentNetwork>::from_bytes_le(&tx_bytes_0).unwrap();
            assert_eq!(tx_0, &tx_1);
            assert_eq!(tx_bytes_0, tx_1.to_bytes_le().unwrap());
            // Stringify and parse the transaction to ensure consistency.
            let tx_1_string = tx_1.to_string();
            let tx = Transaction::<CurrentNetwork>::from_str(&tx_1_string).unwrap();
            assert_eq!(tx_0, &tx);
            assert_eq!(tx_1_string, tx.to_string());
            // Check the transaction.
            vm.check_transaction(&tx, None, rng).map_err(|e| anyhow!("Transaction check failed: {e}")).unwrap();
            tx
        })
        .collect();
    // Sample the next block.
    let block = sample_next_block(vm, caller_private_key, &transactions, rng).unwrap();
    // Assert all transactions were accepted.
    assert_eq!(block.transactions().num_accepted(), transactions.len());
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    // Add the next block to the VM.
    vm.add_next_block(&block).unwrap();
}
