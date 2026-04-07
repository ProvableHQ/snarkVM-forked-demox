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

// Tests for the record-existence check.
mod record_existence;

// Tests on the input/output behaviour of closures and related functionality.
mod closure_records;
// Tests on the use of `commit_*_raw` instruction variants.
mod commit_raw;

use super::*;

use crate::vm::test_helpers::{sample_vm_at_height, *};

use console::{
    account::ViewKey,
    network::ConsensusVersion,
    program::{Identifier, Value},
};

use snarkvm_synthesizer_program::Program;
use snarkvm_utilities::TestRng;

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
