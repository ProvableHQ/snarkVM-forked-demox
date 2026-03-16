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

use super::add_and_test;

/// Advances the VM to V14 consensus height by adding empty blocks.
fn advance_to_v14(
    vm: &VM<CurrentNetwork, LedgerType>,
    caller_private_key: &PrivateKey<CurrentNetwork>,
    rng: &mut TestRng,
) -> Result<()> {
    let v14_height = CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?;
    for _ in vm.block_store().current_block_height()..v14_height {
        let block = sample_next_block(vm, caller_private_key, &[], rng)?;
        vm.add_next_block(&block)?;
    }
    Ok(())
}

/// Decrypts the first record output from the first transition in a transaction.
fn decrypt_first_record(
    tx: &Transaction<CurrentNetwork>,
    view_key: &ViewKey<CurrentNetwork>,
) -> Record<CurrentNetwork, Plaintext<CurrentNetwork>> {
    match &tx.transitions().next().unwrap().outputs()[0] {
        Output::Record(_, _, ct, _) => ct.as_ref().unwrap().decrypt(view_key).unwrap(),
        other => panic!("Expected record output, got: {other:?}"),
    }
}

// Tests that a closure can accept a record as input and read its fields.
#[test]
fn test_closure_record_input() -> Result<()> {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;

    // Define a program with a closure that reads a record field.
    let program = Program::from_str(
        r"
        program closure_rec_input.aleo;

        record token:
            owner as address.private;
            amount as u64.private;

        closure extract_amount:
            input r0 as token.record;
            add r0.amount 0u64 into r1;
            output r1 as u64;

        function mint_and_extract:
            input r0 as address.private;
            input r1 as u64.private;
            cast r0 r1 into r2 as token.record;
            call extract_amount r2 into r3;
            output r2 as token.record;
            output r3 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Initialize the VM at V14.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?, rng);

    // Deploy the program.
    let transaction = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    // Execute the function to mint a record and extract its amount via the closure.
    let transaction = vm.execute(
        &caller_private_key,
        ("closure_rec_input.aleo", "mint_and_extract"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("42u64")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    // Verify the public output matches the expected amount.
    let expected = Plaintext::from_str("42u64")?;
    match &transaction.transitions().next().unwrap().outputs()[1] {
        Output::Public(_, Some(plaintext)) => assert_eq!(*plaintext, expected),
        other => panic!("Expected public output, got: {other:?}"),
    }

    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    Ok(())
}

// Tests that a closure cannot output a record type (rejected at parse time).
#[test]
fn test_closure_record_output_rejected() {
    // Attempt to parse a program with a closure that outputs a record.
    let result = Program::<CurrentNetwork>::from_str(
        r"
        program closure_rec_out_bad.aleo;

        record token:
            owner as address.private;
            amount as u64.private;

        closure bad_output:
            input r0 as address;
            input r1 as u64;
            cast r0 r1 into r2 as token.record;
            output r2 as token.record;

        function unused:
            input r0 as u64.public;
            output r0 as u64.public;

        constructor:
            assert.eq true true;
        ",
    );

    // Parsing should fail because closures cannot output records.
    assert!(result.is_err(), "Program with record output from closure should fail to parse");
}

// Tests that a closure can accept an external record as input and read its fields.
#[test]
fn test_closure_external_record_input() -> Result<()> {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;
    let caller_view_key = ViewKey::try_from(&caller_private_key)?;

    // Parent program defines a record.
    let parent_program = Program::from_str(
        r"
        program closure_ext_parent.aleo;

        record item:
            owner as address.private;
            worth as u32.private;

        function mint_item:
            input r0 as address.private;
            input r1 as u32.private;
            cast r0 r1 into r2 as item.record;
            output r2 as item.record;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Child program imports parent and has a closure taking the external record.
    let child_program = Program::from_str(
        r"
        import closure_ext_parent.aleo;

        program closure_ext_child.aleo;

        closure read_external_item:
            input r0 as closure_ext_parent.aleo/item.record;
            add r0.worth 0u32 into r1;
            output r1 as u32;

        function extract_external_value:
            input r0 as closure_ext_parent.aleo/item.record;
            call read_external_item r0 into r1;
            output r1 as u32.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Initialize the VM at V14.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?, rng);

    // Deploy the parent program.
    let transaction = vm.deploy(&caller_private_key, &parent_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    // Deploy the child program.
    let transaction = vm.deploy(&caller_private_key, &child_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    // Mint an item via the parent program.
    let transaction = vm.execute(
        &caller_private_key,
        ("closure_ext_parent.aleo", "mint_item"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("99u32")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    // Decrypt the minted record.
    let item_record = decrypt_first_record(&transaction, &caller_view_key);

    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    // Execute the child function with the external record.
    let transaction = vm.execute(
        &caller_private_key,
        ("closure_ext_child.aleo", "extract_external_value"),
        [Value::<CurrentNetwork>::Record(item_record)].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    // Verify the public output matches the expected value.
    let expected = Plaintext::from_str("99u32")?;
    match &transaction.transitions().next().unwrap().outputs()[0] {
        Output::Public(_, Some(plaintext)) => assert_eq!(*plaintext, expected),
        other => panic!("Expected public output, got: {other:?}"),
    }

    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    Ok(())
}

// Tests that a closure outputting an ExternalRecord is rejected at V14+ deployment.
// The program parses successfully, but `verify_deployment` (called during block production)
// rejects it at V14+ because `ensure_records_exist` assumes closures cannot extend record families.
#[test]
fn test_closure_external_record_output_rejected_at_v14() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);

    let parent_program = Program::from_str(
        r"
        program ext_rec_out_parent.aleo;

        record item:
            owner as address.private;
            amount as u64.private;

        function noop:
            input r0 as u64.public;
            output r0 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    let child_program = Program::from_str(
        r"
        import ext_rec_out_parent.aleo;

        program ext_rec_out_child.aleo;

        closure passthrough_external:
            input r0 as ext_rec_out_parent.aleo/item.record;
            assert.eq true true;
            output r0 as ext_rec_out_parent.aleo/item.record;

        function noop:
            input r0 as u64.public;
            output r0 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?, rng);

    let tx = vm.deploy(&caller_private_key, &parent_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // The deployment transaction is created successfully, but rejected during block production.
    let tx = vm.deploy(&caller_private_key, &child_program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[tx], rng)?;
    assert_eq!(block.transactions().num_accepted(), 0, "ExternalRecord closure output should be rejected at V14+");
    assert_eq!(block.aborted_transaction_ids().len(), 1);

    Ok(())
}

// Tests that a closure can accept a dynamic record as input and read it with `get.record.dynamic`.
#[test]
fn test_closure_dynamic_record_input() -> Result<()> {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;
    let caller_view_key = ViewKey::try_from(&caller_private_key)?;

    // Program with a closure that reads a dynamic record.
    let program = Program::from_str(
        r"
        program closure_dyn_input.aleo;

        record asset:
            owner as address.private;
            quantity as u64.private;

        closure read_dynamic_quantity:
            input r0 as dynamic.record;
            get.record.dynamic r0.quantity into r1 as u64;
            output r1 as u64;

        function mint_asset:
            input r0 as address.private;
            input r1 as u64.private;
            cast r0 r1 into r2 as asset.record;
            output r2 as asset.record;

        function read_via_closure:
            input r0 as asset.record;
            cast r0 into r1 as dynamic.record;
            call read_dynamic_quantity r1 into r2;
            output r2 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Initialize the VM at V14.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?, rng);

    // Deploy the program.
    let transaction = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    // Mint an asset record.
    let transaction = vm.execute(
        &caller_private_key,
        ("closure_dyn_input.aleo", "mint_asset"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("500u64")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    // Decrypt the minted record.
    let asset_record = decrypt_first_record(&transaction, &caller_view_key);

    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    // Execute the function that reads the dynamic record via the closure.
    let transaction = vm.execute(
        &caller_private_key,
        ("closure_dyn_input.aleo", "read_via_closure"),
        [Value::<CurrentNetwork>::Record(asset_record)].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    // Verify the public output matches the expected quantity.
    let expected = Plaintext::from_str("500u64")?;
    match &transaction.transitions().next().unwrap().outputs()[0] {
        Output::Public(_, Some(plaintext)) => assert_eq!(*plaintext, expected),
        other => panic!("Expected public output, got: {other:?}"),
    }

    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    Ok(())
}

// Tests that closures cannot use `call` instructions (closures are leaf computations).
// This documents a language constraint: record inputs in closures are read-only —
// a closure cannot forward a record to another closure or function via `call`.
#[test]
fn test_closure_cannot_contain_call() {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);

    // A program whose closure body contains a `call` instruction. The program
    // parses successfully but must be rejected when deployed (stack initialization).
    let program = Program::<CurrentNetwork>::from_str(
        r"
        program closure_call_bad.aleo;

        record coin:
            owner as address.private;
            amount as u64.private;

        closure inner_read:
            input r0 as coin.record;
            add r0.amount 0u64 into r1;
            output r1 as u64;

        closure outer_read:
            input r0 as coin.record;
            call inner_read r0 into r1;
            output r1 as u64;

        function use_outer:
            input r0 as address.private;
            input r1 as u64.private;
            cast r0 r1 into r2 as coin.record;
            call outer_read r2 into r3;
            output r3 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )
    .expect("program should parse");

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);
    let result = vm.deploy(&caller_private_key, &program, None, 0, None, rng);

    assert!(result.is_err(), "A closure containing a `call` instruction should be rejected at deployment");
}

// Tests that the existence check is NOT bypassed when an ExternalRecord is cast to a
// DynamicRecord and then only used in a closure. The `retain()` logic auto-resolves
// families whose only member is an ExternalRecord root (the inclusion-proof case), but
// once a cast creates an additional DynamicRecord member the family must still be
// verified through a function call. This is the closure analog of Antonio's test 4.1.
#[test]
fn test_external_record_cast_to_dynamic_then_closure_fails_existence_check() -> Result<()> {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;
    let caller_view_key = ViewKey::try_from(&caller_private_key)?;

    // Parent program defines the record.
    let parent_program = Program::from_str(
        r"
        program cast_dyn_parent.aleo;

        record gem:
            owner as address.private;
            karat as u32.private;

        function mint_gem:
            input r0 as address.private;
            input r1 as u32.private;
            cast r0 r1 into r2 as gem.record;
            output r2 as gem.record;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Child program imports parent, casts the ExternalRecord to DynamicRecord, then
    // passes the DynamicRecord ONLY to a closure (no function call to verify it).
    let child_program = Program::from_str(
        r"
        import cast_dyn_parent.aleo;

        program cast_dyn_child.aleo;

        closure read_dynamic_karat:
            input r0 as dynamic.record;
            get.record.dynamic r0.karat into r1 as u32;
            output r1 as u32;

        // Receives an ExternalRecord, casts it to DynamicRecord, and passes it only
        // to a closure. The ExternalRecord->DynamicRecord cast creates a two-member
        // family that cannot be auto-resolved; the closure cannot resolve it either.
        function read_gem_via_cast_and_closure:
            input r0 as cast_dyn_parent.aleo/gem.record;
            cast r0 into r1 as dynamic.record;
            call read_dynamic_karat r1 into r2;
            output r2 as u32.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?, rng);

    let tx = vm.deploy(&caller_private_key, &parent_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    let tx = vm.deploy(&caller_private_key, &child_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Mint a gem.
    let tx = vm.execute(
        &caller_private_key,
        ("cast_dyn_parent.aleo", "mint_gem"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("24u32")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    let gem_record = decrypt_first_record(&tx, &caller_view_key);

    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Execute the child function. The ExternalRecord is cast to DynamicRecord and passed only
    // to a closure. The existence check must reject this because the DynamicRecord family
    // was extended by the cast (so `retain()` does not auto-resolve it) and the closure
    // cannot provide the function-call verification needed to resolve it.
    let result = vm.execute(
        &caller_private_key,
        ("cast_dyn_child.aleo", "read_gem_via_cast_and_closure"),
        [Value::<CurrentNetwork>::Record(gem_record)].into_iter(),
        None,
        0,
        None,
        rng,
    );

    assert!(
        result.is_err(),
        "Execution should fail: ExternalRecord cast to DynamicRecord and only passed to closure has no existence verification"
    );

    Ok(())
}

// Tests V14 backward compatibility: a program with a closure that accepts an ExternalRecord as
// input (but does NOT output it) can be deployed before V14 and executed correctly at V14+.
// This is the happy-path counterpart to `test_closure_external_record_output_rejected_at_v14`.
#[test]
fn test_pre_v14_closure_external_record_input_works_at_v14() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;
    let caller_view_key = ViewKey::try_from(&caller_private_key)?;

    // Start at V9 (height 12 in the test schedule), which uses Varuna V2 (consistent with V14's
    // proof system) and supports constructor syntax, but is before V14 (height 17). At this height
    // the deployment-time closure-output restriction does not yet apply.
    let vm = sample_vm_at_height(12, rng);

    // Parent defines the record.
    let parent_program = Program::from_str(
        r"
        program closure_legacy_parent.aleo;

        record gem:
            owner as address.private;
            amount as u64.private;

        function mint_gem:
            input r0 as address.private;
            input r1 as u64.private;
            cast r0 r1 into r2 as gem.record;
            output r2 as gem.record;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Child has a closure that accepts an ExternalRecord as input and extracts a field value,
    // producing a plain u64. The closure does not output a record.
    let child_program = Program::from_str(
        r"
        import closure_legacy_parent.aleo;

        program closure_legacy_child.aleo;

        closure extract_gem_amount:
            input r0 as closure_legacy_parent.aleo/gem.record;
            add r0.amount 0u64 into r1;
            output r1 as u64;

        function use_gem_closure:
            input r0 as closure_legacy_parent.aleo/gem.record;
            call extract_gem_amount r0 into r1;
            output r1 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Deploy both programs before V14. The deployments must be accepted.
    let tx = vm.deploy(&caller_private_key, &parent_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    let tx = vm.deploy(&caller_private_key, &child_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Mint a gem to use as input to the child function.
    let mint_tx = vm.execute(
        &caller_private_key,
        ("closure_legacy_parent.aleo", "mint_gem"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("77u64")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    let gem_record = decrypt_first_record(&mint_tx, &caller_view_key);
    add_and_test(&vm, &caller_private_key, &[mint_tx], rng);

    // Advance to V14 (height 17 in the test schedule) by adding empty blocks.
    advance_to_v14(&vm, &caller_private_key, rng)?;

    // At V14+, executing a function that calls a closure with an ExternalRecord *input* (but no
    // record output) must succeed. The closure only reads a field, so `ensure_records_exist` has
    // nothing to validate — no record family is extended.
    let transaction = vm.execute(
        &caller_private_key,
        ("closure_legacy_child.aleo", "use_gem_closure"),
        [Value::<CurrentNetwork>::Record(gem_record)].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    // Verify the public output equals the minted amount.
    let expected = Plaintext::from_str("77u64")?;
    match &transaction.transitions().next().unwrap().outputs()[0] {
        Output::Public(_, Some(plaintext)) => assert_eq!(*plaintext, expected),
        other => panic!("Expected public output, got: {other:?}"),
    }

    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    Ok(())
}

// Tests that executing a pre-V14 program whose closure outputs ExternalRecord is rejected at V14+.
// This covers the runtime `has_forbidden_output` check, which is the only code path exercising
// that check — deployment-time rejection prevents new programs from reaching execution.
#[test]
fn test_pre_v14_closure_external_record_output_rejected_at_v14_runtime() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;
    let caller_view_key = ViewKey::try_from(&caller_private_key)?;

    // Start at V9 (height 12), before the V14 closure-output restriction (height 17).
    let vm = sample_vm_at_height(12, rng);

    // Parent defines the record.
    let parent_program = Program::from_str(
        r"
        program pre_v14_ext_out_parent.aleo;

        record widget:
            owner as address.private;
            value as u64.private;

        function mint_widget:
            input r0 as address.private;
            input r1 as u64.private;
            cast r0 r1 into r2 as widget.record;
            output r2 as widget.record;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Child has a closure that passes an ExternalRecord through as an output.
    // This is allowed at pre-V14 but forbidden at V14+.
    let child_program = Program::from_str(
        r"
        import pre_v14_ext_out_parent.aleo;

        program pre_v14_ext_out_child.aleo;

        closure passthrough_widget:
            input r0 as pre_v14_ext_out_parent.aleo/widget.record;
            assert.eq true true;
            output r0 as pre_v14_ext_out_parent.aleo/widget.record;

        function use_closure:
            input r0 as pre_v14_ext_out_parent.aleo/widget.record;
            call passthrough_widget r0 into r1;
            output r1 as pre_v14_ext_out_parent.aleo/widget.record;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Deploy both programs before V14. Both deployments must be accepted.
    let tx = vm.deploy(&caller_private_key, &parent_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    let tx = vm.deploy(&caller_private_key, &child_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Mint a widget to use as input.
    let tx = vm.execute(
        &caller_private_key,
        ("pre_v14_ext_out_parent.aleo", "mint_widget"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("42u64")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    let widget_record = decrypt_first_record(&tx, &caller_view_key);
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Advance to V14 (height 17) by adding empty blocks.
    advance_to_v14(&vm, &caller_private_key, rng)?;

    // At V14+, executing `use_closure` must fail: the runtime `has_forbidden_output` check
    // detects that `passthrough_widget` outputs ExternalRecord and rejects the execution.
    let result = vm.execute(
        &caller_private_key,
        ("pre_v14_ext_out_child.aleo", "use_closure"),
        [Value::<CurrentNetwork>::Record(widget_record)].into_iter(),
        None,
        0,
        None,
        rng,
    );

    assert!(result.is_err(), "Execution must fail: pre-V14 closure outputting ExternalRecord is disallowed at V14+");

    Ok(())
}

// Tests that executing a pre-V14 program whose closure outputs DynamicRecord is rejected at V14+.
// Parallel to `test_pre_v14_closure_external_record_output_rejected_at_v14_runtime` but for the
// DynamicRecord variant of the `has_forbidden_output` check.
#[test]
fn test_pre_v14_closure_dynamic_record_output_rejected_at_v14_runtime() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;
    let caller_view_key = ViewKey::try_from(&caller_private_key)?;

    // Start at V9 (height 12), before the V14 closure-output restriction (height 17).
    let vm = sample_vm_at_height(12, rng);

    // Program with a closure that casts a record to DynamicRecord and outputs it.
    // This is allowed at pre-V14 but forbidden at V14+.
    let program = Program::from_str(
        r"
        program pre_v14_dyn_out.aleo;

        record coin:
            owner as address.private;
            amount as u64.private;

        closure cast_to_dynamic:
            input r0 as coin.record;
            cast r0 into r1 as dynamic.record;
            output r1 as dynamic.record;

        function use_closure:
            input r0 as coin.record;
            call cast_to_dynamic r0 into r1;
            output r1 as dynamic.record;

        function mint_coin:
            input r0 as address.private;
            input r1 as u64.private;
            cast r0 r1 into r2 as coin.record;
            output r2 as coin.record;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Deploy before V14. Deployment must be accepted.
    let tx = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Mint a coin to use as input.
    let tx = vm.execute(
        &caller_private_key,
        ("pre_v14_dyn_out.aleo", "mint_coin"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("10u64")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    let coin_record = decrypt_first_record(&tx, &caller_view_key);
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Advance to V14 (height 17) by adding empty blocks.
    advance_to_v14(&vm, &caller_private_key, rng)?;

    // At V14+, executing `use_closure` must fail: the runtime `has_forbidden_output` check
    // detects that `cast_to_dynamic` outputs DynamicRecord and rejects the execution.
    let result = vm.execute(
        &caller_private_key,
        ("pre_v14_dyn_out.aleo", "use_closure"),
        [Value::<CurrentNetwork>::Record(coin_record)].into_iter(),
        None,
        0,
        None,
        rng,
    );

    assert!(result.is_err(), "Execution must fail: pre-V14 closure outputting DynamicRecord is disallowed at V14+");

    Ok(())
}

// Tests that when a function contains multiple closures — one with a forbidden output and one
// without — execution is rejected. Verifies that the check is per-closure and not accidentally
// bypassed when benign closures are present.
#[test]
fn test_mixed_closures_forbidden_output_rejected_at_v14_runtime() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;
    let caller_view_key = ViewKey::try_from(&caller_private_key)?;

    // Start at V9 (height 12), before the V14 closure-output restriction (height 17).
    let vm = sample_vm_at_height(12, rng);

    let parent_program = Program::from_str(
        r"
        program mixed_closures_parent.aleo;

        record token:
            owner as address.private;
            amount as u64.private;

        function mint_token:
            input r0 as address.private;
            input r1 as u64.private;
            cast r0 r1 into r2 as token.record;
            output r2 as token.record;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Child has two closures: one benign (reads a field), one forbidden (outputs ExternalRecord).
    // The function calls the benign closure first, then the forbidden one.
    let child_program = Program::from_str(
        r"
        import mixed_closures_parent.aleo;

        program mixed_closures_child.aleo;

        closure extract_amount:
            input r0 as mixed_closures_parent.aleo/token.record;
            add r0.amount 0u64 into r1;
            output r1 as u64;

        closure passthrough:
            input r0 as mixed_closures_parent.aleo/token.record;
            assert.eq true true;
            output r0 as mixed_closures_parent.aleo/token.record;

        function use_both_closures:
            input r0 as mixed_closures_parent.aleo/token.record;
            call extract_amount r0 into r1;
            call passthrough r0 into r2;
            output r1 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Deploy both programs before V14. Both deployments must be accepted.
    let tx = vm.deploy(&caller_private_key, &parent_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    let tx = vm.deploy(&caller_private_key, &child_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Mint a token.
    let tx = vm.execute(
        &caller_private_key,
        ("mixed_closures_parent.aleo", "mint_token"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("50u64")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    let token_record = decrypt_first_record(&tx, &caller_view_key);
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Advance to V14.
    advance_to_v14(&vm, &caller_private_key, rng)?;

    // At V14+, the function calls both a safe closure and a forbidden one. Execution must be
    // rejected because `passthrough` outputs ExternalRecord.
    let result = vm.execute(
        &caller_private_key,
        ("mixed_closures_child.aleo", "use_both_closures"),
        [Value::<CurrentNetwork>::Record(token_record)].into_iter(),
        None,
        0,
        None,
        rng,
    );

    assert!(result.is_err(), "Execution must fail: function contains a closure with forbidden ExternalRecord output");

    Ok(())
}

// Tests that executing a pre-V14 cross-program closure call (`CallOperator::Locator`) whose
// closure outputs ExternalRecord is rejected at V14+ runtime. This covers the Locator branch of
// `has_forbidden_output`, where the closure lives in a different program and is referenced by
// `call other_program.aleo/closure_name` syntax.
#[test]
fn test_pre_v14_cross_program_closure_forbidden_output_rejected_at_v14_runtime() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;
    let caller_view_key = ViewKey::try_from(&caller_private_key)?;

    // Start at V9 (height 12), before the V14 closure-output restriction (height 17).
    let vm = sample_vm_at_height(12, rng);

    // Parent program defines the record.
    let parent_program = Program::from_str(
        r"
        program locator_parent.aleo;

        record widget:
            owner as address.private;
            amount as u64.private;

        function mint_widget:
            input r0 as address.private;
            input r1 as u64.private;
            cast r0 r1 into r2 as widget.record;
            output r2 as widget.record;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Lib program imports parent and defines a closure that passes an ExternalRecord through as
    // an output. This is allowed at pre-V14 but forbidden at V14+. A dummy function is required
    // because programs must have at least one function.
    let lib_program = Program::from_str(
        r"
        import locator_parent.aleo;

        program locator_lib.aleo;

        closure passthrough_widget:
            input r0 as locator_parent.aleo/widget.record;
            assert.eq true true;
            output r0 as locator_parent.aleo/widget.record;

        function noop:
            input r0 as u64.public;
            output r0 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Caller program imports both and calls the closure from locator_lib.aleo via a Locator
    // (`call locator_lib.aleo/passthrough_widget`), creating a `CallOperator::Locator`.
    let caller_program = Program::from_str(
        r"
        import locator_parent.aleo;
        import locator_lib.aleo;

        program locator_caller.aleo;

        function use_external_closure:
            input r0 as locator_parent.aleo/widget.record;
            call locator_lib.aleo/passthrough_widget r0 into r1;
            output r1 as locator_parent.aleo/widget.record;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Deploy all three programs before V14. All deployments must be accepted.
    let tx = vm.deploy(&caller_private_key, &parent_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    let tx = vm.deploy(&caller_private_key, &lib_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    let tx = vm.deploy(&caller_private_key, &caller_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Mint a widget to use as input.
    let tx = vm.execute(
        &caller_private_key,
        ("locator_parent.aleo", "mint_widget"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("42u64")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    let widget_record = decrypt_first_record(&tx, &caller_view_key);
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Advance to V14 (height 17) by adding empty blocks.
    advance_to_v14(&vm, &caller_private_key, rng)?;

    // At V14+, executing `use_external_closure` must fail: the runtime `has_forbidden_output`
    // check (Locator branch) detects that `locator_lib.aleo/passthrough_widget` outputs
    // ExternalRecord and rejects the execution.
    let result = vm.execute(
        &caller_private_key,
        ("locator_caller.aleo", "use_external_closure"),
        [Value::<CurrentNetwork>::Record(widget_record)].into_iter(),
        None,
        0,
        None,
        rng,
    );

    assert!(
        result.is_err(),
        "Execution must fail: pre-V14 cross-program closure (Locator) outputting ExternalRecord is disallowed at V14+"
    );

    Ok(())
}

// Tests that a closure outputting a DynamicRecord is rejected at V14+ deployment.
// The program parses successfully, but `verify_deployment` (called during block production)
// rejects it at V14+ because `ensure_records_exist` assumes closures cannot extend record families.
#[test]
fn test_closure_dynamic_record_output_rejected_at_v14() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);

    // A program whose closure casts a record to DynamicRecord and outputs it. This is
    // rejected at V14+ because `ensure_records_exist` assumes closures cannot extend
    // record families.
    let program = Program::from_str(
        r"
        program closure_dyn_output_bad.aleo;

        record asset:
            owner as address.private;
            quantity as u64.private;

        closure cast_to_dynamic:
            input r0 as asset.record;
            cast r0 into r1 as dynamic.record;
            output r1 as dynamic.record;

        function unused:
            input r0 as u64.public;
            output r0 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    // The deployment transaction is created successfully, but rejected during block production.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?, rng);
    let tx = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[tx], rng)?;
    assert_eq!(block.transactions().num_accepted(), 0, "DynamicRecord closure output should be rejected at V14+");
    assert_eq!(block.aborted_transaction_ids().len(), 1);

    Ok(())
}

// Tests that a pre-V14 program whose closure outputs ExternalRecord executes successfully
// *before* V14. This is the backward-compatibility counterpart to
// `test_pre_v14_closure_external_record_output_rejected_at_v14_runtime`: both deploy at pre-V14,
// but this test executes before V14 (where the restriction does not apply) and confirms the
// output is accepted.
#[test]
fn test_pre_v14_closure_external_record_output_works_before_v14() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;
    let caller_view_key = ViewKey::try_from(&caller_private_key)?;

    // Start at V9 (height 12), before the V14 closure-output restriction (height 17).
    let vm = sample_vm_at_height(12, rng);

    // Parent defines the record.
    let parent_program = Program::from_str(
        r"
        program pre_v14_ok_parent.aleo;

        record widget:
            owner as address.private;
            value as u64.private;

        function mint_widget:
            input r0 as address.private;
            input r1 as u64.private;
            cast r0 r1 into r2 as widget.record;
            output r2 as widget.record;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Child has a closure that passes an ExternalRecord through as an output.
    let child_program = Program::from_str(
        r"
        import pre_v14_ok_parent.aleo;

        program pre_v14_ok_child.aleo;

        closure passthrough_widget:
            input r0 as pre_v14_ok_parent.aleo/widget.record;
            assert.eq true true;
            output r0 as pre_v14_ok_parent.aleo/widget.record;

        function use_closure:
            input r0 as pre_v14_ok_parent.aleo/widget.record;
            call passthrough_widget r0 into r1;
            output r1 as pre_v14_ok_parent.aleo/widget.record;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Deploy both programs before V14.
    let tx = vm.deploy(&caller_private_key, &parent_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    let tx = vm.deploy(&caller_private_key, &child_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Mint a widget.
    let tx = vm.execute(
        &caller_private_key,
        ("pre_v14_ok_parent.aleo", "mint_widget"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("42u64")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    let widget_record = decrypt_first_record(&tx, &caller_view_key);
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Execute at pre-V14. The closure outputs ExternalRecord, which must be accepted.
    let transaction = vm.execute(
        &caller_private_key,
        ("pre_v14_ok_child.aleo", "use_closure"),
        [Value::<CurrentNetwork>::Record(widget_record)].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    Ok(())
}

// Tests that a pre-V14 program whose closure accepts DynamicRecord as input (but does NOT output
// it) executes successfully at V14+. This is the DynamicRecord-input counterpart to
// `test_pre_v14_closure_external_record_input_works_at_v14`.
#[test]
fn test_pre_v14_closure_dynamic_record_input_works_at_v14() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;
    let caller_view_key = ViewKey::try_from(&caller_private_key)?;

    // Start at V9 (height 12), before the V14 closure-output restriction (height 17).
    let vm = sample_vm_at_height(12, rng);

    // Program with a closure that reads a DynamicRecord field but outputs a plain scalar.
    let program = Program::from_str(
        r"
        program pre_v14_dyn_input_ok.aleo;

        record coin:
            owner as address.private;
            amount as u64.private;

        closure read_dynamic_amount:
            input r0 as dynamic.record;
            get.record.dynamic r0.amount into r1 as u64;
            output r1 as u64;

        function mint_coin:
            input r0 as address.private;
            input r1 as u64.private;
            cast r0 r1 into r2 as coin.record;
            output r2 as coin.record;

        function read_via_closure:
            input r0 as coin.record;
            cast r0 into r1 as dynamic.record;
            call read_dynamic_amount r1 into r2;
            output r2 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Deploy before V14.
    let tx = vm.deploy(&caller_private_key, &program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Mint a coin.
    let tx = vm.execute(
        &caller_private_key,
        ("pre_v14_dyn_input_ok.aleo", "mint_coin"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("123u64")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;
    let coin_record = decrypt_first_record(&tx, &caller_view_key);
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Advance to V14 (height 17) by adding empty blocks.
    advance_to_v14(&vm, &caller_private_key, rng)?;

    // At V14+, executing `read_via_closure` must succeed: the closure only takes DynamicRecord
    // as input and outputs a plain scalar. The `has_forbidden_output` check only looks at outputs.
    let transaction = vm.execute(
        &caller_private_key,
        ("pre_v14_dyn_input_ok.aleo", "read_via_closure"),
        [Value::<CurrentNetwork>::Record(coin_record)].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    // Verify the public output matches the minted amount.
    let expected = Plaintext::from_str("123u64")?;
    match &transaction.transitions().next().unwrap().outputs()[0] {
        Output::Public(_, Some(plaintext)) => assert_eq!(*plaintext, expected),
        other => panic!("Expected public output, got: {other:?}"),
    }

    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    Ok(())
}

// Tests that deploying a program at V14+ with mixed closures — one safe (outputs scalar) and one
// forbidden (outputs ExternalRecord) — is rejected. Ensures the deploy-time check iterates all
// closures and does not short-circuit after the first safe one.
#[test]
fn test_mixed_closures_deploy_rejected_at_v14() -> Result<()> {
    let rng = &mut TestRng::default();
    let caller_private_key = sample_genesis_private_key(rng);

    // Parent defines the record.
    let parent_program = Program::from_str(
        r"
        program mixed_deploy_parent.aleo;

        record token:
            owner as address.private;
            amount as u64.private;

        function noop:
            input r0 as u64.public;
            output r0 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    // Child has two closures: one safe (outputs u64), one forbidden (outputs ExternalRecord).
    let child_program = Program::from_str(
        r"
        import mixed_deploy_parent.aleo;

        program mixed_deploy_child.aleo;

        closure safe_extract:
            input r0 as mixed_deploy_parent.aleo/token.record;
            add r0.amount 0u64 into r1;
            output r1 as u64;

        closure bad_passthrough:
            input r0 as mixed_deploy_parent.aleo/token.record;
            assert.eq true true;
            output r0 as mixed_deploy_parent.aleo/token.record;

        function noop:
            input r0 as u64.public;
            output r0 as u64.public;

        constructor:
            assert.eq true true;
        ",
    )?;

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14)?, rng);

    // Deploy the parent program.
    let tx = vm.deploy(&caller_private_key, &parent_program, None, 0, None, rng)?;
    add_and_test(&vm, &caller_private_key, &[tx], rng);

    // Deploy the child program. The transaction is created, but rejected during block production
    // because `bad_passthrough` outputs ExternalRecord.
    let tx = vm.deploy(&caller_private_key, &child_program, None, 0, None, rng)?;
    let block = sample_next_block(&vm, &caller_private_key, &[tx], rng)?;
    assert_eq!(
        block.transactions().num_accepted(),
        0,
        "Mixed closures with one forbidden output should be rejected at V14+"
    );
    assert_eq!(block.aborted_transaction_ids().len(), 1);

    Ok(())
}
