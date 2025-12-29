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

mod cast;

mod get_record_dynamic;

mod dynamic_mapping_operations;

mod mixed;

mod call_dynamic;

mod dynamic_futures;

mod recursion;

mod translation;

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
use snarkvm_synthesizer_process::execution_cost_for_authorization;
use snarkvm_synthesizer_program::Program;
use snarkvm_utilities::TestRng;

// TODO (dynamic_dispatch)
// - Test the case with the interface of a dynamic call doesn't match the mode
// - Conditional execution with finalize scopes

fn add_and_test(
    vm: &VM<CurrentNetwork, LedgerType>,
    caller_private_key: &PrivateKey<CurrentNetwork>,
    transactions: &[Transaction<CurrentNetwork>],
    rng: &mut TestRng,
) {
    for (index, transaction) in transactions.iter().enumerate() {
        vm.check_transaction(transaction, None, rng)
            .map_err(|e| anyhow!("Transaction {index} check failed: {e}"))
            .unwrap();
    }
    let block = sample_next_block(vm, caller_private_key, transactions, rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), transactions.len());
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();
}
