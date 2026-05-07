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

//! End-to-end tests for the `query` function prototype.
//!
//! These tests build a real VM at V15 height, deploy programs containing query functions, run
//! transactions that mutate mappings via `finalize`, then call `vm.evaluate_query(...)` to
//! verify the typed return values reflect on-chain state.

use super::*;

use console::program::{Literal, Plaintext};

/// Convenience: extract a single `u64` from a single-output query result.
fn expect_u64(outputs: &[Value<CurrentNetwork>]) -> u64 {
    assert_eq!(outputs.len(), 1, "expected exactly one output, got {}", outputs.len());
    match &outputs[0] {
        Value::Plaintext(Plaintext::Literal(Literal::U64(v), _)) => **v,
        other => panic!("expected u64 plaintext, got: {other}"),
    }
}

/// Full lifecycle: deploy a program with a query function, run a transition that updates a
/// mapping via finalize, then evaluate the query and observe the new value.
#[test]
fn test_evaluate_query_reflects_finalize_state() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;

    // The program defines:
    //   - `balances` mapping
    //   - `increment(addr, amount)` transition + finalize that writes balances[addr] += amount
    //   - `total_balance(addr)` query that returns balances[addr] (default 0)
    let program = Program::from_str(
        r"
        program qy_lifecycle.aleo;

        mapping balances:
            key as address.public;
            value as u64.public;

        function increment:
            input r0 as address.public;
            input r1 as u64.public;
            async increment r0 r1 into r2;
            output r2 as qy_lifecycle.aleo/increment.future;

        finalize increment:
            input r0 as address.public;
            input r1 as u64.public;
            get.or_use balances[r0] 0u64 into r2;
            add r2 r1 into r3;
            set r3 into balances[r0];

        query total_balance:
            input r0 as address.public;
            get.or_use balances[r0] 0u64 into r1;
            output r1 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V15)?, rng);

    // Deploy.
    let tx = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    add_and_test_with_costs(&vm, &caller_private_key, &caller_address, None, &[tx], rng);

    // Read against an untouched mapping should return the default (0).
    let (height, outputs) =
        vm.evaluate_query("qy_lifecycle.aleo", "total_balance", vec![Value::from_str(&caller_address.to_string())?])?;
    assert_eq!(height, vm.block_store().current_block_height());
    assert_eq!(expect_u64(&outputs), 0);

    // Execute increment(addr, 10).
    let inputs = [Value::from_str(&caller_address.to_string())?, Value::from_str("10u64")?];
    let tx = vm.execute(&caller_private_key, ("qy_lifecycle.aleo", "increment"), inputs.iter(), None, 0, None, rng)?;
    add_and_test_with_costs(&vm, &caller_private_key, &caller_address, Some(&[&inputs]), &[tx], rng);

    // Read should now reflect the finalize-set value.
    let (height, outputs) =
        vm.evaluate_query("qy_lifecycle.aleo", "total_balance", vec![Value::from_str(&caller_address.to_string())?])?;
    assert_eq!(height, vm.block_store().current_block_height());
    assert_eq!(expect_u64(&outputs), 10);

    // Increment again by 32. New total: 42.
    let inputs = [Value::from_str(&caller_address.to_string())?, Value::from_str("32u64")?];
    let tx = vm.execute(&caller_private_key, ("qy_lifecycle.aleo", "increment"), inputs.iter(), None, 0, None, rng)?;
    add_and_test_with_costs(&vm, &caller_private_key, &caller_address, Some(&[&inputs]), &[tx], rng);

    let (height, outputs) =
        vm.evaluate_query("qy_lifecycle.aleo", "total_balance", vec![Value::from_str(&caller_address.to_string())?])?;
    assert_eq!(height, vm.block_store().current_block_height());
    assert_eq!(expect_u64(&outputs), 42);

    Ok(())
}

/// Read with multiple outputs returns each in declaration order.
#[test]
fn test_evaluate_query_multi_output() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;

    let program = Program::from_str(
        r"
        program qy_multi_output.aleo;

        mapping balances:
            key as address.public;
            value as u64.public;

        function noop:
            input r0 as u64.private;
            output r0 as u64.private;

        constructor:
            assert.eq true true;

        query summary:
            input r0 as address.public;
            get.or_use balances[r0] 0u64 into r1;
            add r1 1u64 into r2;
            mul r1 2u64 into r3;
            output r1 as u64.public;
            output r2 as u64.public;
            output r3 as u64.public;
        ",
    )?;

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V15)?, rng);
    let tx = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    add_and_test_with_costs(&vm, &caller_private_key, &caller_address, None, &[tx], rng);

    let (_, outputs) =
        vm.evaluate_query("qy_multi_output.aleo", "summary", vec![Value::from_str(&caller_address.to_string())?])?;

    // Default 0u64 for the unknown key, then +1 and *2.
    assert_eq!(outputs.len(), 3);
    assert_eq!(expect_u64(&outputs[0..1]), 0);
    assert_eq!(expect_u64(&outputs[1..2]), 1);
    assert_eq!(expect_u64(&outputs[2..3]), 0);
    Ok(())
}

/// Read with multiple inputs computes a typed return.
#[test]
fn test_evaluate_query_multi_input() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;

    // Pure-arithmetic query, no mappings needed.
    let program = Program::from_str(
        r"
        program qy_multi_in.aleo;

        function noop:
            input r0 as u64.private;
            output r0 as u64.private;

        constructor:
            assert.eq true true;

        query add3:
            input r0 as u64.public;
            input r1 as u64.public;
            input r2 as u64.public;
            add r0 r1 into r3;
            add r3 r2 into r4;
            output r4 as u64.public;
        ",
    )?;

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V15)?, rng);
    let tx = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    add_and_test_with_costs(&vm, &caller_private_key, &caller_address, None, &[tx], rng);

    let (_, outputs) = vm.evaluate_query("qy_multi_in.aleo", "add3", vec![
        Value::from_str("10u64")?,
        Value::from_str("20u64")?,
        Value::from_str("12u64")?,
    ])?;
    assert_eq!(expect_u64(&outputs), 42);
    Ok(())
}

/// Read that uses `branch.eq` to take a forward branch over a noop. The output is
/// invariant under the branch (computed before it), but the branch path itself is
/// exercised: when `r0 == 0`, the doubling step is skipped at runtime; when `r0 != 0`,
/// the doubling step runs and writes `r2`. Either way the query's declared output
/// references `r1`, which is always written. This proves the query evaluator handles
/// `branch.eq` without crashing under both taken and not-taken paths.
#[test]
fn test_evaluate_query_with_branch() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;

    let program = Program::from_str(
        r"
        program qy_branch.aleo;

        function noop:
            input r0 as u64.private;
            output r0 as u64.private;

        constructor:
            assert.eq true true;

        query maybe_extra:
            input r0 as u64.public;
            add r0 10u64 into r1;
            branch.eq r0 0u64 to skip;
            add r1 r1 into r2;
            position skip;
            output r1 as u64.public;
        ",
    )?;

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V15)?, rng);
    let tx = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    add_and_test_with_costs(&vm, &caller_private_key, &caller_address, None, &[tx], rng);

    // r0 = 0: branch is taken, doubling is skipped.
    let (_, outputs) = vm.evaluate_query("qy_branch.aleo", "maybe_extra", vec![Value::from_str("0u64")?])?;
    assert_eq!(expect_u64(&outputs), 10);

    // r0 = 5: branch is NOT taken, the unused r2 destination is written but we still output r1.
    let (_, outputs) = vm.evaluate_query("qy_branch.aleo", "maybe_extra", vec![Value::from_str("5u64")?])?;
    assert_eq!(expect_u64(&outputs), 15);
    Ok(())
}

/// Read with no inputs (queries a fixed-key mapping or just returns a constant).
#[test]
fn test_evaluate_query_zero_inputs() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;

    let program = Program::from_str(
        r"
        program qy_zeroin.aleo;

        function noop:
            input r0 as u64.private;
            output r0 as u64.private;

        constructor:
            assert.eq true true;

        query fixed_value:
            add 0u64 1234u64 into r0;
            output r0 as u64.public;
        ",
    )?;

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V15)?, rng);
    let tx = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    add_and_test_with_costs(&vm, &caller_private_key, &caller_address, None, &[tx], rng);

    let (_, outputs) = vm.evaluate_query("qy_zeroin.aleo", "fixed_value", vec![])?;
    assert_eq!(expect_u64(&outputs), 1234);
    Ok(())
}

/// Calling evaluate_query with the wrong input arity returns an error.
#[test]
fn test_evaluate_query_arity_mismatch() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;

    let program = Program::from_str(
        r"
        program qy_arity.aleo;

        function noop:
            input r0 as u64.private;
            output r0 as u64.private;

        constructor:
            assert.eq true true;

        query takes_two:
            input r0 as u64.public;
            input r1 as u64.public;
            add r0 r1 into r2;
            output r2 as u64.public;
        ",
    )?;

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V15)?, rng);
    let tx = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    add_and_test_with_costs(&vm, &caller_private_key, &caller_address, None, &[tx], rng);

    // Too few inputs.
    let result = vm.evaluate_query("qy_arity.aleo", "takes_two", vec![Value::from_str("1u64")?]);
    assert!(result.is_err(), "expected error for too few inputs");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("expects 2"), "error should mention input count: {err}");

    // Too many inputs.
    let result = vm.evaluate_query("qy_arity.aleo", "takes_two", vec![
        Value::from_str("1u64")?,
        Value::from_str("2u64")?,
        Value::from_str("3u64")?,
    ]);
    assert!(result.is_err(), "expected error for too many inputs");

    Ok(())
}

/// Calling a query that does not exist on a deployed program returns a "not defined" error.
#[test]
fn test_evaluate_query_unknown_query() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;

    let program = Program::from_str(
        r"
        program qy_unknown.aleo;

        function noop:
            input r0 as u64.private;
            output r0 as u64.private;

        constructor:
            assert.eq true true;

        query existing:
            add 0u64 1u64 into r0;
            output r0 as u64.public;
        ",
    )?;

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V15)?, rng);
    let tx = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    add_and_test_with_costs(&vm, &caller_private_key, &caller_address, None, &[tx], rng);

    let result = vm.evaluate_query("qy_unknown.aleo", "missing", vec![]);
    assert!(result.is_err(), "expected error for unknown query");
    Ok(())
}

/// Calling evaluate_query against a program that was never deployed returns a "no such program" error.
#[test]
fn test_evaluate_query_unknown_program() {
    let rng = &mut TestRng::default();
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V15).unwrap(), rng);

    let result = vm.evaluate_query("never_deployed.aleo", "anything", vec![]);
    assert!(result.is_err(), "expected error for unknown program");
}

/// Construction-time rejection: declared output type must match the operand's actual type.
#[test]
fn test_query_output_type_mismatch_rejected() {
    // `r1` is a `u64`, but the query declares its output as `u32`.
    let result = Program::<CurrentNetwork>::from_str(
        r"
        program qy_typecheck.aleo;

        function noop:
            input r0 as u64.private;
            output r0 as u64.private;

        constructor:
            assert.eq true true;

        query bad_output:
            input r0 as u64.public;
            add r0 1u64 into r1;
            output r1 as u32.public;
        ",
    );
    // Parsing succeeds (the query is structurally valid); the mismatch is caught when the
    // stack is computed from the program at deploy time. We assert here that *either* parse
    // or the subsequent type-check rejects this — whichever fails first is acceptable.
    if let Ok(program) = result {
        let rng = &mut TestRng::default();
        let caller_private_key = sample_genesis_private_key(rng);
        let caller_address = Address::try_from(&caller_private_key).unwrap();
        let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V15).unwrap(), rng);
        let deploy = vm.deploy(&caller_private_key, &program, None, 0, None, rng);
        assert!(deploy.is_err(), "deploy should reject type-mismatched query output");
        let _ = caller_address;
    }
}

/// Construction-time rejection: write-style commands inside a query are rejected at parse.
#[test]
fn test_query_rejects_write_commands_at_parse() {
    let cases = [
        // `set` into a mapping
        r"
        program qy_bad_set.aleo;
        mapping m:
            key as u64.public;
            value as u64.public;
        function noop:
            input r0 as u64.private;
            output r0 as u64.private;

        constructor:
            assert.eq true true;
        query bad:
            input r0 as u64.public;
            set r0 into m[r0];
            output r0 as u64.public;
        ",
        // `remove` from a mapping
        r"
        program qy_bad_rm.aleo;
        mapping m:
            key as u64.public;
            value as u64.public;
        function noop:
            input r0 as u64.private;
            output r0 as u64.private;

        constructor:
            assert.eq true true;
        query bad:
            input r0 as u64.public;
            remove m[r0];
            output r0 as u64.public;
        ",
        // `rand.chacha`
        r"
        program qy_bad_rand.aleo;
        function noop:
            input r0 as u64.private;
            output r0 as u64.private;

        constructor:
            assert.eq true true;
        query bad:
            rand.chacha into r0 as u64;
            output r0 as u64.public;
        ",
    ];

    for source in cases {
        let result = Program::<CurrentNetwork>::from_str(source);
        assert!(result.is_err(), "expected parse error for query with forbidden command:\n{source}");
    }
}

/// Tests that a program containing a `query` block is rejected at V14 (since `query` is V15
/// syntax) and accepted at V15. Without this gate, deploying such a program pre-V15 would
/// fork: new nodes accept the bytes, old nodes reject the unknown component variant.
#[test]
fn test_deploy_query_before_and_at_v15() {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);

    // Start one block before V15 so that after the (rejected) deployment block we are
    // exactly at V15 and the same program is accepted.
    let v15_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V15).unwrap();
    let vm = sample_vm_at_height(v15_height - 1, rng);

    let program = Program::from_str(
        r"
program qy_v15_gate.aleo;

function noop:
    input r0 as u64.private;
    output r0 as u64.private;

constructor:
    assert.eq true true;

query fixed_value:
    add 0u64 1234u64 into r0;
    output r0 as u64.public;
",
    )
    .unwrap();

    // Deployment before V15 should be aborted.
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
    let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 0, "Deployment with query before V15 should not be accepted");
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 1, "Deployment with query before V15 should be aborted");
    vm.add_next_block(&block).unwrap();

    // We should now be at V15.
    assert_eq!(vm.block_store().current_block_height(), v15_height);

    // Deployment at V15 should succeed.
    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
    let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 1, "Deployment with query at V15 should be accepted");
    assert_eq!(block.transactions().num_rejected(), 0);
    assert_eq!(block.aborted_transaction_ids().len(), 0);
    vm.add_next_block(&block).unwrap();
}

/// Tests that a query containing a `string` type is rejected at deploy via the strengthened
/// `Program::contains_string_type` (which now walks `self.queries`). Without that fix, strings
/// could sneak in through query inputs/outputs even though they're banned post-V12.
#[test]
fn test_deploy_query_with_string_type_rejected() {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V15).unwrap(), rng);

    let program = Program::from_str(
        r"
program qy_string_input.aleo;

function noop:
    input r0 as u64.private;
    output r0 as u64.private;

constructor:
    assert.eq true true;

query echo:
    input r0 as string.public;
    add 0u64 0u64 into r1;
    output r0 as string.public;
",
    )
    .unwrap();

    let deployment = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
    let block = sample_next_block(&vm, &caller_private_key, &[deployment], rng).unwrap();
    assert_eq!(block.transactions().num_accepted(), 0, "Deployment with string-typed query input should be rejected");
    assert_eq!(block.aborted_transaction_ids().len(), 1);
}
