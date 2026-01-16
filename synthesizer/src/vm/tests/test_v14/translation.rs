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

use snarkvm_ledger_block::{Input, Transition, TransitionCallerMetadata};

use super::*;

fn test_translation(
    caller_private_key: &PrivateKey<CurrentNetwork>,
    // Program and function to call
    root_program_name: &str,
    root_function_name: &str,
    // Inputs to the root call; if None gas_to_mint is used as explained below.
    input_values: Option<Vec<Value<CurrentNetwork>>>,
    // If Some, precedes the root call with a transaction that mints the given
    // gas_container record and uses the corresponding dynamic record as input
    // to the root call.
    gas_to_mint: Option<Record<CurrentNetwork, Plaintext<CurrentNetwork>>>,
    // The expected outputs.
    expected_public_outputs: Option<Vec<Plaintext<CurrentNetwork>>>,
    rng: &mut TestRng,
) {
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(caller_private_key).unwrap();
    let caller_address = Address::<CurrentNetwork>::try_from(caller_private_key).unwrap();

    // Various parameters for call.dynamic instructions.
    let program_a_name_str = "flow";
    let program_a_name_field = Identifier::<CurrentNetwork>::from_str(program_a_name_str).unwrap().to_field().unwrap();
    let program_b_name_str = "gas_manager";
    let program_b_name_field = Identifier::<CurrentNetwork>::from_str(program_b_name_str).unwrap().to_field().unwrap();
    let network_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();

    let get_liquid_liters_function_name = Identifier::<CurrentNetwork>::from_str("get_liquid_liters").unwrap();
    let get_gas_liters_function_name = Identifier::<CurrentNetwork>::from_str("get_gas_liters").unwrap();
    let nitrogen_pump_function_name = Identifier::<CurrentNetwork>::from_str("nitrogen_pump").unwrap();
    let get_external_liters_function_name = Identifier::<CurrentNetwork>::from_str("get_external_liters").unwrap();
    let gas_pipe_function_name = Identifier::<CurrentNetwork>::from_str("gas_pipe").unwrap();
    let static_gas_leak_function_name = Identifier::<CurrentNetwork>::from_str("static_gas_leak").unwrap();
    let get_dynamic_liters_from_gas_function_name =
        Identifier::<CurrentNetwork>::from_str("get_dynamic_liters_from_gas").unwrap();

    let get_liquid_liters_function_field = get_liquid_liters_function_name.to_field().unwrap();
    let get_gas_liters_function_field = get_gas_liters_function_name.to_field().unwrap();
    let nitrogen_pump_function_field = nitrogen_pump_function_name.to_field().unwrap();
    let get_external_liters_function_field = get_external_liters_function_name.to_field().unwrap();
    let gas_pipe_function_field = gas_pipe_function_name.to_field().unwrap();
    let static_gas_leak_function_field = static_gas_leak_function_name.to_field().unwrap();
    let get_dynamic_liters_from_gas_function_field = get_dynamic_liters_from_gas_function_name.to_field().unwrap();

    let program_a_str = format!(
        r"
    import {program_b_name_str}.aleo;

    program {program_a_name_str}.aleo;

    // Tries to consume a container passed as dynamic as a specifically liquid one
    function get_dynamic_liters_from_liquid:
        input r0 as record.dynamic;
        
        call.dynamic {program_b_name_field} {network_field} {get_liquid_liters_function_field}
            with r0 (as record.dynamic)
            into r1 (as u64.public);

        output r1 as u64.public;
    
    function {get_dynamic_liters_from_gas_function_name}:
        input r0 as record.dynamic;
        
        call.dynamic {program_b_name_field} {network_field} {get_gas_liters_function_field}
            with r0 (as record.dynamic)
            into r1 (as u64.public);

        output r1 as u64.public;

    function consume_dynamic_blob:
        input r0 as record.dynamic;
        output true as boolean.private;

    function dynamic_pump:
        call.dynamic {program_b_name_field} {network_field} {nitrogen_pump_function_field}
            with 1u64 (as u64.public)
            into r0 (as record.dynamic);
        
        output r0 as record.dynamic;

    // Get the liters in an external liquid record
    function {get_external_liters_function_name}:
        input r0 as {program_b_name_str}.aleo/gas_container.record;
        output r0.liters as u64.public;

    // Input and output the same gas record
    function {gas_pipe_function_name}:
        input r0 as {program_b_name_str}.aleo/gas_container.record;
        output r0 as {program_b_name_str}.aleo/gas_container.record;

    // Receive a dynamic blob of gas and pass it to another function for leaking,
    // then receive it and measure its liters with yet another function
    function dynamic_gas_leak:
        input r0 as record.dynamic;
        
        call.dynamic {program_b_name_field} {network_field} {static_gas_leak_function_field}
            with r0 (as record.dynamic)
            into r1 (as record.dynamic);
        call.dynamic {program_a_name_field} {network_field} {get_dynamic_liters_from_gas_function_field}
            with r1 (as record.dynamic)
            into r2 (as u64.public);

        output r2 as u64.public;

    constructor:
        assert.eq true true;
    "
    );

    // Preparing the record values for the hardcoded gas_record minter
    let (gas_owner, gas_liters, gas_flammable) = if let Some(gas_to_mint_record) = &gas_to_mint {
        let liters_entry =
            gas_to_mint_record.data().get(&Identifier::<CurrentNetwork>::from_str("liters").unwrap()).unwrap();
        let flammable_entry =
            gas_to_mint_record.data().get(&Identifier::<CurrentNetwork>::from_str("flammable").unwrap()).unwrap();
        let liters_value = match liters_entry {
            Entry::Public(plaintext) => plaintext.to_string(),
            _ => panic!("`liters` entry should be public"),
        };
        let flammable_value = match flammable_entry {
            Entry::Private(plaintext) => plaintext.to_string(),
            _ => panic!("`flammable` entry should be private"),
        };
        (caller_address.to_string(), liters_value, flammable_value)
    } else {
        (caller_address.to_string(), "100u64".to_string(), "false".to_string())
    };

    let program_b_str = format!(
        r"
    program {program_b_name_str}.aleo;

    record liquid_container:
        owner as address.private;
        liters as u64.public;

    record gas_container:
        owner as address.private;
        liters as u64.public;
        flammable as boolean.private;

    function {get_liquid_liters_function_name}:
        input r0 as liquid_container.record;
        
        output r0.liters as u64.public;

    function get_gas_liters_externally:
        input r0 as record.dynamic;
        
        call.dynamic {program_a_name_field} {network_field} {get_external_liters_function_field}
            with r0 (as record.dynamic)
            into r1 (as u64.public);
        
        output r1 as u64.public;

    function {get_gas_liters_function_name}:
        input r0 as gas_container.record;
        
        output r0.liters as u64.public;

    function {nitrogen_pump_function_name}:
        input r0 as u64.public;
        
        cast self.caller r0 false into r1 as gas_container.record;
        
        output r1 as gas_container.record;

    function hardcoded_gas_pump:
        cast {gas_owner} {gas_liters} {gas_flammable} into r0 as gas_container.record;
        
        output r0 as gas_container.record;

    function pump_and_send_through_pipe:
        input r0 as record.dynamic;
        
        call.dynamic {program_a_name_field} {network_field} {gas_pipe_function_field}
            with r0 (as record.dynamic)
            into r1 (as record.dynamic);
    
    // Consume a gas record and produce a new one containing 10 fewer liters
    function static_gas_leak:
        input r0 as gas_container.record;
        
        sub r0.liters 10u64 into r1;
        cast r0.owner r1 r0.flammable into r2 as gas_container.record;
        
        output r2 as gas_container.record;

    constructor:
        assert.eq true true;
    "
    );

    // Initialize a new program.
    let program_a = Program::<CurrentNetwork>::from_str(&program_a_str).unwrap();
    let program_b = Program::<CurrentNetwork>::from_str(&program_b_str).unwrap();

    // Initialize the VM.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    // Deploy the programs.
    println!("Deploying program {program_b_name_str}...");
    let transaction_b = vm.deploy(caller_private_key, &program_b, None, 0, None, rng).unwrap();
    add_and_test(&vm, caller_private_key, &[transaction_b], rng);

    println!("Deploying program {program_a_name_str}...");
    let transaction_a = vm.deploy(caller_private_key, &program_a, None, 0, None, rng).unwrap();
    add_and_test(&vm, caller_private_key, &[transaction_a], rng);

    assert!(
        input_values.is_none() || gas_to_mint.is_none(),
        "When gas_to_mint is provided, the resulting static input is converted to dynamic record is used instead of input_values, which should be None",
    );

    assert!(
        input_values.is_some() || gas_to_mint.is_some(),
        "Exactly one of input_values or gas_to_mint must be provided",
    );

    let computed_input_values = input_values.unwrap_or_else(|| {
        println!("Minting gas_container record...");
        let transaction_mint = vm
            .execute(
                caller_private_key,
                (format!("{program_b_name_str}.aleo"), "hardcoded_gas_pump"),
                Vec::<Value<CurrentNetwork>>::new().iter(),
                None,
                0,
                None,
                rng,
            )
            .unwrap();

        let mint_output = transaction_mint.transitions().next().unwrap().outputs().iter().next().unwrap();

        let output_gas_record = match mint_output {
            Output::Record(_, _, record_ciphertext, _) => {
                record_ciphertext.as_ref().unwrap().decrypt(&caller_view_key).unwrap()
            }
            _ => panic!("Minted record is not a record"),
        };

        let block_mint = sample_next_block(&vm, caller_private_key, &[transaction_mint], rng).unwrap();
        assert_eq!(block_mint.transactions().num_accepted(), 1);
        assert_eq!(block_mint.transactions().num_rejected(), 0);
        assert_eq!(block_mint.aborted_transaction_ids().len(), 0);
        vm.add_next_block(&block_mint).unwrap();

        let dynamic_record = DynamicRecord::from_record(&output_gas_record).unwrap();
        vec![Value::<CurrentNetwork>::DynamicRecord(dynamic_record)]
    });

    println!("Executing root function {root_program_name}/{root_function_name}...");

    // Execute the root function.
    let transaction = vm
        .execute(
            caller_private_key,
            (root_program_name, root_function_name),
            computed_input_values.into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    println!("Verifying transaction...");
    add_and_test(&vm, caller_private_key, &[transaction.clone()], rng);

    if let Some(expected_public_outputs) = expected_public_outputs {
        println!("Asserting output correctness on {} expected public outputs...", expected_public_outputs.len());

        // Note the last transition is the fee transition
        let num_transitions = transaction.transitions().count();
        let root_transition = transaction.transitions().nth(num_transitions - 2).unwrap();

        let public_outputs = root_transition
            .outputs()
            .iter()
            .filter_map(|output| match output {
                Output::Public(_, Some(plaintext)) => Some(plaintext),
                _ => None,
            })
            .collect_vec();

        assert_eq!(public_outputs.into_iter().cloned().collect_vec(), expected_public_outputs);
    }
}

// Tests translation of a dynamic record input to a non-external static record.
#[test]
fn test_translation_input_dynamic_non_external() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    let record_static_str = format!(
        r#"{{
        owner: {caller_address}.private,
        liters: 1888u64.public,
        flammable: false.private,
        _nonce: 0group.public,
        _version: 1u8.public
    }}"#
    );

    let r0_static = Record::<CurrentNetwork, Plaintext<CurrentNetwork>>::from_str(&record_static_str).unwrap();

    let expected_output = Plaintext::<CurrentNetwork>::from_str("1888u64").unwrap();

    test_translation(
        &caller_private_key,
        "flow.aleo",
        "get_dynamic_liters_from_gas",
        None,
        Some(r0_static),
        Some(vec![expected_output]),
        rng,
    );
}

// Tests translation of a non-external static record output to a dynamic record.
#[test]
fn test_translation_output_non_external_dynamic() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);

    test_translation(&caller_private_key, "flow.aleo", "dynamic_pump", Some(vec![]), None, None, rng);
}

// Tests translation of a dynamic record input to an external static record.
#[test]
fn test_translation_input_dynamic_external() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    let record_static_str = format!(
        r#"{{
        owner: {caller_address}.private,
        liters: 292u64.public,
        flammable: true.private,
        _nonce: 0group.public,
        _version: 1u8.public
    }}"#
    );

    let r0_static = Record::<CurrentNetwork, Plaintext<CurrentNetwork>>::from_str(&record_static_str).unwrap();

    let expected_output = Plaintext::<CurrentNetwork>::from_str("292u64").unwrap();

    test_translation(
        &caller_private_key,
        "gas_manager.aleo",
        "get_gas_liters_externally",
        None,
        Some(r0_static),
        Some(vec![expected_output]),
        rng,
    );
}

// Tests translation of an external static record output to a dynamic record.
#[test]
fn test_translation_output_external_dynamic() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    let record_static_str = format!(
        r#"{{
        owner: {caller_address}.private,
        liters: 292u64.public,
        flammable: true.private,
        _nonce: 0group.public,
        _version: 1u8.public
    }}"#
    );

    // Construct the static and dynamic records.
    let r0_static = Record::<CurrentNetwork, Plaintext<CurrentNetwork>>::from_str(&record_static_str).unwrap();

    test_translation(
        &caller_private_key,
        "gas_manager.aleo",
        "pump_and_send_through_pipe",
        None,
        Some(r0_static),
        None,
        rng,
    );
}

// Tests three consecutive translations in a single execution path with consistent prover/verifier traversal order.
#[test]
fn test_translation_triple() {
    // Before root call: pump a gas_container.record
    // Root call:
    //    - Receive as input a dynamic record corresponding to the above static one
    //    - Pass it to static_gas_leak, which consumes a static gas_container.record (first translation) and produces a new one with 10 fewer liters.
    //    - Receive as output the new static record as dynamic (second translation)
    //    - Call get_dynamic_liters_from_gas, which receives a dynamic record (no translation) and calls get_gas_liters, which in turn expects a static record (third translation)
    //
    // This also checks consistency in the traversal order of translation tasks for the prover and verifier.

    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    let record_static_str = format!(
        r#"{{
        owner: {caller_address}.private,
        liters: 333u64.public,
        flammable: true.private,
        _nonce: 0group.public,
        _version: 1u8.public
    }}"#
    );

    // Construct the static and dynamic records.
    let r0_static = Record::<CurrentNetwork, Plaintext<CurrentNetwork>>::from_str(&record_static_str).unwrap();

    // Expected output (10 liters have leaked)
    let expected_output = Plaintext::<CurrentNetwork>::from_str("323u64").unwrap();

    test_translation(
        &caller_private_key,
        "flow.aleo",
        "dynamic_gas_leak",
        None,
        Some(r0_static),
        Some(vec![expected_output]),
        rng,
    );
}

// Tests that prover and verifier traverse translation tasks in the same order using a complex execution graph.
#[test]
fn test_translation_traversal_consistency() {
    // This tests checks the prover and verifier order all translation tasks associated to a
    // transaction the same way by asserting correct verification of the following execution
    // graph (capital leters denote record types):
    // quadruple_caller (receives two dynamic records of types A B)
    //   -> leaf_two_one (two input translations A B, one output translation B)
    //   -> leaf_one_two (one input translation B, two output translations, B C)
    //   -> double_caller_one_zero (one input translation C)
    //        -> leaf_one_one (one input translation B, one output translation A)
    //        -> leaf_two_one (two input translations A B, one output translation B)
    //   -> leaf_zero_one (one output translation B)

    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(caller_private_key).unwrap();
    let caller_address = Address::<CurrentNetwork>::try_from(caller_private_key).unwrap();

    // Various parameters for call.dynamic instructions.
    let program_name_str = "quotes";
    let program_name_field = Identifier::<CurrentNetwork>::from_str(program_name_str).unwrap().to_field().unwrap();
    let network_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();

    let double_caller_one_zero_function_name =
        Identifier::<CurrentNetwork>::from_str("double_caller_one_zero").unwrap();
    let leaf_two_one_function_name = Identifier::<CurrentNetwork>::from_str("leaf_two_one").unwrap();
    let leaf_one_two_function_name = Identifier::<CurrentNetwork>::from_str("leaf_one_two").unwrap();
    let leaf_one_one_function_name = Identifier::<CurrentNetwork>::from_str("leaf_one_one").unwrap();
    let leaf_zero_one_function_name = Identifier::<CurrentNetwork>::from_str("leaf_zero_one").unwrap();

    let double_caller_one_zero_function_field = double_caller_one_zero_function_name.to_field().unwrap();
    let leaf_two_one_function_field = leaf_two_one_function_name.to_field().unwrap();
    let leaf_one_two_function_field = leaf_one_two_function_name.to_field().unwrap();
    let leaf_one_one_function_field = leaf_one_one_function_name.to_field().unwrap();
    let leaf_zero_one_function_field = leaf_zero_one_function_name.to_field().unwrap();

    let quixote_quote = vec![
        69u8, 110, 32, 117, 110, 32, 108, 117, 103, 97, 114, 32, 100, 101, 32, 108, 97, 32, 77, 97, 110, 99, 104, 97,
    ];
    let hamlet_quote = vec![84u8, 111, 32, 98, 101, 32, 111, 114, 32, 110, 111, 116, 32, 116, 111, 32, 98, 101];
    let lotr_quote = vec![
        73u8, 116, 39, 115, 32, 97, 32, 100, 97, 110, 103, 101, 114, 111, 117, 115, 32, 98, 117, 115, 105, 110, 101,
        115, 115, 44, 32, 70, 114, 111, 100, 111,
    ];

    fn process_quote(mut quote: Vec<u8>) -> String {
        quote.resize(50, 0);
        quote.into_iter().map(|c| format!("{c}u8")).join(" ").to_string()
    }

    let processed_quixote_quote = process_quote(quixote_quote);
    let processed_hamlet_quote = process_quote(hamlet_quote);
    let processed_lotr_quote = process_quote(lotr_quote);

    let program_str = format!(
        r"
    program {program_name_str}.aleo;

    record a:
        owner as address.private;
        quixote_tweet as [u8; 50u32].public;

    record b:
        owner as address.private;
        hamlet_tweet as [u8; 50u32].public;
        understandable as boolean.public;

    record c:
        owner as address.private;
        lotr_tweet as [u8; 50u32].public;
        book_number as u8.public;
        canon as boolean.private;

    function quadruple_caller:
        input r0 as record.dynamic; // type A
        input r1 as record.dynamic; // type B
        input r2 as record.dynamic; // type B
        
        // underlying input types: A B
        // underlying output types: B
        call.dynamic {program_name_field} {network_field} {leaf_two_one_function_field}
            with r0 r1 (as record.dynamic record.dynamic)
            into r3 (as record.dynamic);

        // underlying input types: B
        // underlying output types: B C
        call.dynamic {program_name_field} {network_field} {leaf_one_two_function_field}
            with r3 (as record.dynamic)
            into r4 r5 (as record.dynamic record.dynamic);

        // underlying input types: C B B (the last two are not translated)
        call.dynamic {program_name_field} {network_field} {double_caller_one_zero_function_field}
            with r5 r4 r2 (as record.dynamic record.dynamic record.dynamic);

        // underlying output types: B
        call.dynamic {program_name_field} {network_field} {leaf_zero_one_function_field}
            into r6 (as record.dynamic);

    function double_caller_one_zero:
        input r0 as c.record;
        input r1 as record.dynamic; // type B
        input r2 as record.dynamic; // type B

        // underlying input types: B
        // underlying output types: A
        call.dynamic {program_name_field} {network_field} {leaf_one_one_function_field}
            with r1 (as record.dynamic)
            into r3 (as record.dynamic);

        // underlying input types: A B
        // underlying output types: B
        call.dynamic {program_name_field} {network_field} {leaf_two_one_function_field}
            with r3 r2 (as record.dynamic record.dynamic)
            into r4 (as record.dynamic);

    function leaf_two_one:
        input r0 as a.record;
        input r1 as b.record;

        cast {processed_hamlet_quote} into r2 as [u8; 50u32];
        cast {caller_address} r2 false into r3 as b.record;

        output r3 as b.record;

    function leaf_one_two:
        input r0 as b.record;

        cast {processed_hamlet_quote} into r1 as [u8; 50u32];
        cast {caller_address} r1 false into r2 as b.record;

        cast {processed_lotr_quote} into r3 as [u8; 50u32];
        cast {caller_address} r3 1u8 false into r4 as c.record;

        output r2 as b.record;
        output r4 as c.record;

    function leaf_zero_one:
        cast {processed_hamlet_quote} into r0 as [u8; 50u32];
        cast {caller_address} r0 false into r1 as b.record;

        output r1 as b.record;

    function leaf_one_one:
        input r0 as b.record;

        cast {processed_quixote_quote} into r1 as [u8; 50u32];
        cast {caller_address} r1 into r2 as a.record;

        output r2 as a.record;

    function mint_a:
        cast {processed_quixote_quote} into r0 as [u8; 50u32];
        cast {caller_address} r0 into r1 as a.record;
        output r1 as a.record;
    
    function mint_b:
        cast {processed_hamlet_quote} into r0 as [u8; 50u32];
        cast {caller_address} r0 false into r1 as b.record;
        output r1 as b.record;

    function mint_c:
        cast {processed_lotr_quote} into r0 as [u8; 50u32];
        cast {caller_address} r0 1u8 false into r1 as c.record;
        output r1 as c.record;

    constructor:
        assert.eq true true;
    "
    );

    let program = Program::<CurrentNetwork>::from_str(&program_str).unwrap();

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    // Deploy the program.
    let transaction = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction], rng);

    let mut mint_record = |function_name: &str| {
        println!("Executing {function_name}...");

        let transaction_mint = vm
            .execute(
                &caller_private_key,
                ("quotes.aleo", function_name),
                Vec::<Value<CurrentNetwork>>::new().into_iter(),
                None,
                0,
                None,
                rng,
            )
            .unwrap();

        let mint_output = transaction_mint.transitions().next().unwrap().outputs().iter().next().unwrap();

        let output_record = match mint_output {
            Output::Record(_, _, record_ciphertext, _) => {
                record_ciphertext.as_ref().unwrap().decrypt(&caller_view_key).unwrap()
            }
            _ => panic!("Minted record is not a record"),
        };

        let dynamic_record = DynamicRecord::from_record(&output_record).unwrap();

        (transaction_mint, dynamic_record)
    };

    let (transaction_mint_a, dynamic_record_a) = mint_record("mint_a");
    let (transaction_mint_b_1, dynamic_record_b_1) = mint_record("mint_b");
    let (transaction_mint_b_2, dynamic_record_b_2) = mint_record("mint_b");

    add_and_test(&vm, &caller_private_key, &[transaction_mint_a, transaction_mint_b_1, transaction_mint_b_2], rng);

    let transaction = vm
        .execute(
            &caller_private_key,
            ("quotes.aleo", "quadruple_caller"),
            [
                Value::DynamicRecord(dynamic_record_a),
                Value::DynamicRecord(dynamic_record_b_1),
                Value::DynamicRecord(dynamic_record_b_2),
            ]
            .into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    // This indeed results of three batches for translation proving/verification:
    // one of size 3 for a.record, one of size 8 for b.record, and one of size 2 for c.record.
    add_and_test(&vm, &caller_private_key, &[transaction], rng);
}

// Tests that malicious tampering with `caller_inputs` and `caller_outputs` (stripping, replacing dynamic with static) is detected.
#[test]
fn test_malicious_caller_inputs_outputs() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(caller_private_key).unwrap();
    let caller_address = Address::try_from(&caller_private_key).unwrap();

    let glass_pane_program_str = Identifier::<CurrentNetwork>::from_str("glass_panes").unwrap();
    let network_str = Identifier::<CurrentNetwork>::from_str("aleo").unwrap();
    let decolor_glass_statically_function_str =
        Identifier::<CurrentNetwork>::from_str("decolor_glass_statically").unwrap();

    let glass_pane_program_field = glass_pane_program_str.to_field().unwrap();
    let network_field = network_str.to_field().unwrap();
    let decolor_glass_statically_function_field = decolor_glass_statically_function_str.to_field().unwrap();

    let program_str = format!(
        r"
        program {glass_pane_program_str}.aleo;

        record stained_glass:
            owner as address.private;
            color as u8.public;

        function produce:
            input r0 as address.private;
            input r1 as u8.public;

            cast r0 r1 into r2 as stained_glass.record;

            output r2 as stained_glass.record;

        function decolor_glass_dynamically:
            input r0 as record.dynamic;

            call.dynamic {glass_pane_program_field} {network_field} {decolor_glass_statically_function_field}
                with r0 (as record.dynamic)
                into r1 (as record.dynamic);

            output r1 as record.dynamic;

        function decolor_glass_statically:
            input r0 as stained_glass.record;

            // Cannot decolor a glass that is not stained
            assert.neq r0.color 0u8;
            
            cast r0.owner 0u8 into r1 as stained_glass.record;

            output r1 as stained_glass.record;

        constructor:
            assert.eq true true;
    "
    );

    let program = Program::<CurrentNetwork>::from_str(&program_str).unwrap();

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    // Deploy the program.
    let transaction_deploy = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction_deploy], rng);

    // Mint a record and decrypt it
    let transaction_mint = vm
        .execute(
            &caller_private_key,
            ("glass_panes.aleo", "produce"),
            [Value::from_str(&caller_address.to_string()).unwrap(), Value::from_str("1u8").unwrap()].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    let output_record = transaction_mint.transitions().next().unwrap().outputs().iter().next().unwrap();

    let output_record = match output_record {
        Output::Record(_, _, record_ciphertext, _) => {
            record_ciphertext.as_ref().unwrap().decrypt(&caller_view_key).unwrap()
        }
        _ => panic!("Minted record is not a record"),
    };

    add_and_test(&vm, &caller_private_key, &[transaction_mint], rng);

    // Convert the static record into a dynamic one and pass it to decolor_glass_dynamically
    let dynamic_record = DynamicRecord::from_record(&output_record).unwrap();

    let transaction_consume = vm
        .execute(
            &caller_private_key,
            ("glass_panes.aleo", "decolor_glass_dynamically"),
            [Value::DynamicRecord(dynamic_record)].into_iter(),
            None,
            1000u64,
            None,
            rng,
        )
        .unwrap();

    // The transaction contains three transitions:
    //    [child (= decolor_glass_statically), parent (= decolor_glass_dynamically), fee].
    // Since the child transition corresponds to a dynamic call, its
    // caller_inputs and caller_outputs are set to Some.
    let child_transition = transaction_consume.transitions().next().unwrap();

    assert!(transaction_consume.transitions().count() == 3);
    assert_eq!(*child_transition.function_name(), decolor_glass_statically_function_str);
    assert!(child_transition.caller_inputs().is_some());
    assert!(child_transition.caller_outputs().is_some());

    // ************************* Case 1: Stipping caller inputs *************************

    // Here a malicious prover removes caller_inputs from a dynamic-call transition
    // and the verifier detects it.

    // We tamper with the transition by removing the caller metadata:
    let tampered_child_transition = Transition::new(
        *child_transition.program_id(),
        *child_transition.function_name(),
        child_transition.inputs().to_vec(),
        child_transition.outputs().to_vec(),
        *child_transition.tpk(),
        *child_transition.tcm(),
        *child_transition.scm(),
        None,
    )
    .unwrap();

    let mut tampered_transitions = transaction_consume.transitions().cloned().collect_vec();

    // We remove the fee transition, which added separately when creating the transaction below.
    tampered_transitions.pop().unwrap();

    tampered_transitions[0] = tampered_child_transition;

    let tampered_execution = Execution::from(
        tampered_transitions.into_iter(),
        transaction_consume.execution().unwrap().global_state_root(),
        transaction_consume.execution().unwrap().proof().cloned(),
    )
    .unwrap();

    let tampered_transaction =
        Transaction::from_execution(tampered_execution, transaction_consume.fee_transition()).unwrap();

    assert_eq!(tampered_transaction.id(), transaction_consume.id());

    // Make sure translation verification fails already at
    // verifier-input-construction time, and not later at proof-verification
    // time.
    assert!(
        vm.check_transaction(&tampered_transaction, None, rng)
            .unwrap_err()
            .to_string()
            .contains("does not contain dynamic-call data")
    );

    // ********* Case 2: Tampering caller_inputs to avoid translation triggering *********

    // In this subtler attack, a malicious prover leaves caller_inputs as Some,
    // but replaces a dynamic record therein by a static one (as present in the
    // callee's view of the inputs) to try and prevent the verifier from
    // detecting the need to check a translation-circuit instance.

    // We first modify the child transition
    let honest_caller_inputs = child_transition.caller_inputs().unwrap().to_vec();
    assert!(matches!(honest_caller_inputs[0], Input::DynamicRecord(..)));
    let mut dishonest_caller_inputs = honest_caller_inputs;

    let static_record_input = child_transition.inputs()[0].clone();
    assert!(matches!(static_record_input, Input::Record(..)));

    dishonest_caller_inputs[0] = static_record_input;

    let input_tampered_child_transition = Transition::new(
        *child_transition.program_id(),
        *child_transition.function_name(),
        child_transition.inputs().to_vec(),
        child_transition.outputs().to_vec(),
        *child_transition.tpk(),
        *child_transition.tcm(),
        *child_transition.scm(),
        Some(
            TransitionCallerMetadata::new_dynamic(
                dishonest_caller_inputs,
                child_transition.caller_outputs().unwrap().to_vec(),
            )
            .unwrap(),
        ),
    )
    .unwrap();

    let mut input_tampered_transitions = transaction_consume.transitions().cloned().collect_vec();

    // We remove the fee transition, which added separately when creating the transaction below.
    input_tampered_transitions.pop().unwrap();

    input_tampered_transitions[0] = input_tampered_child_transition;

    let input_tampered_execution = Execution::from(
        input_tampered_transitions.into_iter(),
        transaction_consume.execution().unwrap().global_state_root(),
        transaction_consume.execution().unwrap().proof().cloned(),
    )
    .unwrap();

    // Extra funds need to be added to the fee to account for the
    // increased size of the modified execution: using the previously constructed
    // transaction_consume.fee_transition() causes an earlier error than the one
    // we want to test due to the fee being insufficient.
    let fee_authorization = vm
        .process()
        .read()
        .authorize_fee_public::<CurrentAleo, _>(
            &caller_private_key,
            10000,
            0,
            input_tampered_execution.to_execution_id().unwrap(),
            rng,
        )
        .unwrap();

    let fee = vm.execute_fee_authorization(fee_authorization, None, rng).unwrap();

    // The full transaction can now be reconstructed using the modified child
    // and fee transitions.
    let transaction = Transaction::from_execution(input_tampered_execution, Some(fee)).unwrap();

    println!("Attempting to execute transaction with malicious caller_inputs...");

    assert!(
        vm.check_transaction(&transaction, None, rng)
            .unwrap_err()
            .to_string()
            .contains("Caller input 0 in dynamic call to decolor_glass_statically should be of type")
    );

    // ********* Case 3: Tampering caller_outputs to avoid translation triggering ********

    // This is analogous to case 2 but with the dishonest prover tampering with
    // caller_outputs instead of caller_inputs

    let honest_caller_outputs = child_transition.caller_outputs().unwrap().to_vec();
    assert!(matches!(honest_caller_outputs[0], Output::DynamicRecord(..)));
    let mut dishonest_caller_outputs = honest_caller_outputs;

    let static_record_output = child_transition.outputs()[0].clone();
    assert!(matches!(static_record_output, Output::Record(..)));

    dishonest_caller_outputs[0] = static_record_output;

    let output_tampered_child_transition = Transition::new(
        *child_transition.program_id(),
        *child_transition.function_name(),
        child_transition.inputs().to_vec(),
        child_transition.outputs().to_vec(),
        *child_transition.tpk(),
        *child_transition.tcm(),
        *child_transition.scm(),
        Some(
            TransitionCallerMetadata::new_dynamic(
                child_transition.caller_inputs().unwrap().to_vec(),
                dishonest_caller_outputs,
            )
            .unwrap(),
        ),
    )
    .unwrap();

    let mut output_tampered_transitions = transaction_consume.transitions().cloned().collect_vec();

    // We remove the fee transition, which added separately when creating the transaction below.
    output_tampered_transitions.pop().unwrap();

    output_tampered_transitions[0] = output_tampered_child_transition;

    let output_tampered_execution = Execution::from(
        output_tampered_transitions.into_iter(),
        transaction_consume.execution().unwrap().global_state_root(),
        transaction_consume.execution().unwrap().proof().cloned(),
    )
    .unwrap();

    // Extra funds need to be added to the fee to account for the
    // increased size of the modified execution: using the previously constructed
    // transaction_consume.fee_transition() causes an earlier error than the one
    // we want to test due to the fee being insufficient.
    let fee_authorization = vm
        .process()
        .read()
        .authorize_fee_public::<CurrentAleo, _>(
            &caller_private_key,
            10000,
            0,
            output_tampered_execution.to_execution_id().unwrap(),
            rng,
        )
        .unwrap();

    let fee = vm.execute_fee_authorization(fee_authorization, None, rng).unwrap();

    // The full transaction can now be reconstructed using the modified child
    // and fee transitions.
    let transaction = Transaction::from_execution(output_tampered_execution, Some(fee)).unwrap();

    println!("Attempting to execute transaction with malicious caller_outputs...");

    assert!(
        vm.check_transaction(&transaction, None, rng)
            .unwrap_err()
            .to_string()
            .contains("Caller output 0 in dynamic call to decolor_glass_statically should be of type")
    );
}

// Checks that translation keys for the same record name but different programs are different and vice versa.
// Run with `snark-print` feature to explore Varuna batch sizes and locate translation keys in verification batches.
#[test]
fn test_differing_keys() {
    // Parameters for dynamic calls
    let program_a_name = Identifier::<CurrentNetwork>::from_str("program_a").unwrap();
    let program_b_name = Identifier::<CurrentNetwork>::from_str("program_b").unwrap();
    let network_name = Identifier::<CurrentNetwork>::from_str("aleo").unwrap();
    let mint_record_a_function_name = Identifier::<CurrentNetwork>::from_str("mint_record_a").unwrap();
    let mint_record_b_function_name = Identifier::<CurrentNetwork>::from_str("mint_record_b").unwrap();
    let program_a_field = program_a_name.to_field().unwrap();
    let program_b_field = program_b_name.to_field().unwrap();
    let network_field = network_name.to_field().unwrap();
    let mint_record_a_function_field = mint_record_a_function_name.to_field().unwrap();
    let mint_record_b_function_field = mint_record_b_function_name.to_field().unwrap();

    let program_a_str = r"
        program program_a.aleo;

        record record_a:
            owner as address.private;

        record record_b:
            owner as address.private;
        
        function mint_record_a:
            cast self.signer into r0 as record_a.record;
            output r0 as record_a.record;

        function mint_record_b:
            cast self.signer into r0 as record_b.record;
            output r0 as record_b.record;

        constructor:
            assert.eq true true;";

    let program_b_str = format!(
        r"
        import program_a.aleo;

        program program_b.aleo;

        record record_a:
            owner as address.private;

        record record_b:
            owner as address.private;
        
        function mint_record_a:
            cast self.signer into r0 as record_a.record;
            output r0 as record_a.record;

        function mint_record_b:
            cast self.signer into r0 as record_b.record;
            output r0 as record_b.record;

        function mint_all:
            call.dynamic {program_a_field} {network_field} {mint_record_a_function_field}
                into r0 (as record.dynamic);

            call.dynamic {program_a_field} {network_field} {mint_record_b_function_field}
                into r1 (as record.dynamic);

            call.dynamic {program_b_field} {network_field} {mint_record_a_function_field}
                into r2 (as record.dynamic);

            call.dynamic {program_b_field} {network_field} {mint_record_b_function_field}
                into r3 (as record.dynamic);

        constructor:
            assert.eq true true;
    "
    );

    let program_a = Program::<CurrentNetwork>::from_str(program_a_str).unwrap();
    let program_b = Program::<CurrentNetwork>::from_str(&program_b_str).unwrap();

    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    // Deploy the programs.
    let transaction_a = vm.deploy(&caller_private_key, &program_a, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction_a], rng);

    let transaction_b = vm.deploy(&caller_private_key, &program_b, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction_b], rng);

    // Displaying the keys associated to each translation circuit for easier
    // identification with snark-print
    for program_name in ["program_a.aleo", "program_b.aleo"] {
        let stack = vm.process().read().get_stack(program_name).unwrap();
        println!("{program_name} translation keys:");
        for record_name in ["record_a", "record_b"] {
            let record_identifier = Identifier::<CurrentNetwork>::from_str(record_name).unwrap();
            let verifying_key = stack.get_translation_verifying_key(&record_identifier).unwrap();
            println!(" - {record_name}: {}", verifying_key.id);
        }
    }

    // Executing one translation task for each of the four record definitions
    println!("Minting and translating all records...");

    let transaction_mint_all = vm
        .execute(
            &caller_private_key,
            ("program_b.aleo", "mint_all"),
            Vec::<Value<CurrentNetwork>>::new().into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();
    add_and_test(&vm, &caller_private_key, &[transaction_mint_all], rng);
}

// Tests translation with a record containing exactly 32 entries (MAX_DATA_ENTRIES boundary).
// This verifies that the Merkle tree of depth 5 can handle the maximum number of entries.
#[test]
fn test_translation_max_entries_record() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);
    let caller_view_key = ViewKey::<CurrentNetwork>::try_from(caller_private_key).unwrap();

    let program_name_field = Identifier::<CurrentNetwork>::from_str("max_entries").unwrap().to_field().unwrap();
    let network_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();
    let consume_max_field = Identifier::<CurrentNetwork>::from_str("consume_max_record").unwrap().to_field().unwrap();

    // Generate 32 field names (f0 through f31) for the record entries
    let field_names: Vec<String> = (0..32).map(|i| format!("f{i}")).collect();
    let field_declarations: String =
        field_names.iter().map(|name| format!("        {name} as u64.public;\n")).collect();

    // Generate cast arguments for minting
    let cast_args: String = (0..32).map(|i| format!("{i}u64 ")).collect::<String>().trim().to_string();

    // Generate sum computation for consuming (sum all 32 fields)
    let mut sum_computation = String::new();
    sum_computation.push_str("        add r0.f0 r0.f1 into r1;\n");
    for i in 2..32 {
        let prev = if i == 2 { 1 } else { i - 1 };
        sum_computation.push_str(&format!("        add r{prev} r0.f{i} into r{i};\n"));
    }

    let program_str = format!(
        r"
    program max_entries.aleo;

    // Record with exactly 32 entries (MAX_DATA_ENTRIES)
    record max_record:
        owner as address.private;
{field_declarations}
    // Mint a max_record with all fields set to their index value
    function mint_max_record:
        cast self.signer {cast_args} into r0 as max_record.record;
        output r0 as max_record.record;

    // Consume a max_record and return the sum of all fields
    function consume_max_record:
        input r0 as max_record.record;
{sum_computation}        output r31 as u64.public;

    // Dynamic caller that passes max_record through translation
    function dynamic_consume_max:
        input r0 as record.dynamic;
        call.dynamic {program_name_field} {network_field} {consume_max_field}
            with r0 (as record.dynamic)
            into r1 (as u64.public);
        output r1 as u64.public;

    constructor:
        assert.eq true true;
    "
    );

    let program = Program::<CurrentNetwork>::from_str(&program_str).unwrap();

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V14).unwrap(), rng);

    // Deploy the program
    println!("Deploying max_entries.aleo with 32-entry record...");
    let deploy_tx = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
    add_and_test(&vm, &caller_private_key, &[deploy_tx], rng);

    // Mint a max_record
    println!("Minting max_record with 32 entries...");
    let mint_tx = vm
        .execute(
            &caller_private_key,
            ("max_entries.aleo", "mint_max_record"),
            Vec::<Value<CurrentNetwork>>::new().into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    let mint_output = mint_tx.transitions().next().unwrap().outputs().iter().next().unwrap();
    let max_record = match mint_output {
        Output::Record(_, _, record_ciphertext, _) => {
            record_ciphertext.as_ref().unwrap().decrypt(&caller_view_key).unwrap()
        }
        _ => panic!("Expected a record output"),
    };

    add_and_test(&vm, &caller_private_key, &[mint_tx], rng);

    // Convert to dynamic record and test translation
    println!("Testing translation of 32-entry record...");
    let dynamic_record = DynamicRecord::from_record(&max_record).unwrap();

    let consume_tx = vm
        .execute(
            &caller_private_key,
            ("max_entries.aleo", "dynamic_consume_max"),
            vec![Value::<CurrentNetwork>::DynamicRecord(dynamic_record)].into_iter(),
            None,
            0,
            None,
            rng,
        )
        .unwrap();

    // Verify the transaction succeeds (sum of 0+1+2+...+31 = 496)
    add_and_test(&vm, &caller_private_key, &[consume_tx], rng);
    println!("Successfully translated 32-entry record (MAX_DATA_ENTRIES boundary)");
}
