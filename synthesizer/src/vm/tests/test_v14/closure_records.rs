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

        function consume_item:
            input r0 as item.record;

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

            // Needed to pass the record-existence check (r0 must materialize)
            call closure_ext_parent.aleo/consume_item r0;

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
    let item_record = match &transaction.transitions().next().unwrap().outputs()[0] {
        Output::Record(_, _, record_ciphertext, _) => record_ciphertext.as_ref().unwrap().decrypt(&caller_view_key)?,
        _ => panic!("Expected record output"),
    };

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
    match &transaction.transitions().nth(1).unwrap().outputs()[0] {
        Output::Public(_, Some(plaintext)) => assert_eq!(*plaintext, expected),
        other => panic!("Expected public output, got: {other:?}"),
    }

    add_and_test(&vm, &caller_private_key, &[transaction], rng);

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
    let asset_record = match &transaction.transitions().next().unwrap().outputs()[0] {
        Output::Record(_, _, record_ciphertext, _) => record_ciphertext.as_ref().unwrap().decrypt(&caller_view_key)?,
        _ => panic!("Expected record output"),
    };

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

// Tests that a closure can output a dynamic record by casting from a static record.
#[test]
fn test_closure_dynamic_record_output() -> Result<()> {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key)?;
    let caller_view_key = ViewKey::try_from(&caller_private_key)?;

    // Program with a closure that casts a record to dynamic and outputs it.
    let program = Program::from_str(
        r"
        program closure_dyn_output.aleo;

        record asset:
            owner as address.private;
            quantity as u64.private;

        closure cast_to_dynamic:
            input r0 as asset.record;
            cast r0 into r1 as dynamic.record;
            output r1 as dynamic.record;

        function mint_asset:
            input r0 as address.private;
            input r1 as u64.private;
            cast r0 r1 into r2 as asset.record;
            output r2 as asset.record;

        function cast_via_closure:
            input r0 as asset.record;
            call cast_to_dynamic r0 into r1;
            get.record.dynamic r1.quantity into r2 as u64;
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
        ("closure_dyn_output.aleo", "mint_asset"),
        [Value::from_str(&caller_address.to_string())?, Value::from_str("250u64")?].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    // Decrypt the minted record.
    let asset_record = match &transaction.transitions().next().unwrap().outputs()[0] {
        Output::Record(_, _, record_ciphertext, _) => record_ciphertext.as_ref().unwrap().decrypt(&caller_view_key)?,
        _ => panic!("Expected record output"),
    };

    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    // Execute the function that casts via the closure and reads the dynamic record.
    let transaction = vm.execute(
        &caller_private_key,
        ("closure_dyn_output.aleo", "cast_via_closure"),
        [Value::<CurrentNetwork>::Record(asset_record)].into_iter(),
        None,
        0,
        None,
        rng,
    )?;

    // Verify the public output matches the expected quantity.
    let expected = Plaintext::from_str("250u64")?;
    match &transaction.transitions().next().unwrap().outputs()[0] {
        Output::Public(_, Some(plaintext)) => assert_eq!(*plaintext, expected),
        other => panic!("Expected public output, got: {other:?}"),
    }

    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    Ok(())
}
