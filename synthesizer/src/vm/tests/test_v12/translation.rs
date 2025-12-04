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

    let program_a_string = format!(
        r"
    import {program_b_name_str}.aleo;

    program {program_a_name_str}.aleo;

    // Tries to consume a container passed as dynamic as a specifically liquid one
    function get_dynamic_liters_from_liquid:
        input r0 as dynamic.record;
        
        call.dynamic {program_b_name_field} {network_field} {get_liquid_liters_function_field}
            with r0 (as dynamic.record)
            into r1 (as u64.public);

        output r1 as u64.public;
    
    function {get_dynamic_liters_from_gas_function_name}:
        input r0 as dynamic.record;
        
        call.dynamic {program_b_name_field} {network_field} {get_gas_liters_function_field}
            with r0 (as dynamic.record)
            into r1 (as u64.public);

        output r1 as u64.public;

    function consume_dynamic_blob:
        input r0 as dynamic.record;
        output true as boolean.private;

    function dynamic_pump:
        call.dynamic {program_b_name_field} {network_field} {nitrogen_pump_function_field}
            with 1u64 (as u64.public)
            into r0 (as dynamic.record);
        
        output r0 as dynamic.record;

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
        input r0 as dynamic.record;
        
        call.dynamic {program_b_name_field} {network_field} {static_gas_leak_function_field}
            with r0 (as dynamic.record)
            into r1 (as dynamic.record);
        call.dynamic {program_a_name_field} {network_field} {get_dynamic_liters_from_gas_function_field}
            with r1 (as dynamic.record)
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

    let program_b_string = format!(
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
        input r0 as dynamic.record;
        
        call.dynamic {program_a_name_field} {network_field} {get_external_liters_function_field}
            with r0 (as dynamic.record)
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
        input r0 as dynamic.record;
        
        call.dynamic {program_a_name_field} {network_field} {gas_pipe_function_field}
            with r0 (as dynamic.record)
            into r1 (as dynamic.record);
    
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
    let program_a = Program::<CurrentNetwork>::from_str(&program_a_string).unwrap();
    let program_b = Program::<CurrentNetwork>::from_str(&program_b_string).unwrap();

    // Initialize the VM.
    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V12).unwrap(), rng);

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

/************************** Translation test cases ***************************/

// TODO (dynamic_dispatch) remove the legend once working
//
// Single-translation test cases (O: coded, P: passing)
// P input dynamic -> static external
// P input dynamic -> static non-external
// P output static non-external -> dynamic
// P output static external -> dynamic
// Double-translation test cases (non-exhaustive)
// - input static-external -> dynamic subsequently passed as input dynamic -> static
// - output static -> dynamic subsequently passed as output dynamic -> static
// Polimorphy
// - input static-type-1 -> dynamic, then static-type-2 -> dynamic (e. g. controlled by a boolean private input)
// - input static-type-1 + static-type2 -> dynamic, dynamic
// Other chained cases (non-exhaustive)
// - input static -> dynamic passed as static -> dynamic, output as dynamic -> static
// - input static -> dynamic passed as static -> dynamic, output as dynamic (check dynamic-record OutputID changes as expected)
// Key-fetching
// - input static -> dynamic, input dynamic -> static, output static -> dynamic, output dynamic -> static all witht he same static definition: only one translation proving key should be fetched
// - static {program_1 - record_name_1, program_1 - record_name_1, program_1 - record_name_2, program_2 - record_name_1, program_2 - record_name_2}: 4 translation proving keys should be fetched
// Signature consistency
// - test involve translation of the output of a call from a preexisting program to ensure signature-verification circuit hasn't changed
// More

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

#[test]
fn test_translation_output_non_external_dynamic() {
    let rng = &mut TestRng::default();

    let caller_private_key = sample_genesis_private_key(rng);

    test_translation(&caller_private_key, "flow.aleo", "dynamic_pump", Some(vec![]), None, None, rng);
}

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

#[test]
fn test_translation_triple() {
    // Before root call: pump a gas_container.record
    // Root call:
    //    - Receive as input a dynamic record corresponding to the above static one
    //    - Pass it to static_gas_leak, which consumes a static gas_container.record (first translation) and produces a new one with 10 fewer liters.
    //    - Receive as output the new static record as dynamic (second translation)
    //    - Call get_dynamic_liters_from_gas, which receives a dynamic record (no translation) and calls get_gas_liters, which in turn expects an static record (third translation)
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

    test_translation(&caller_private_key, "flow.aleo", "dynamic_gas_leak", None, Some(r0_static), Some(vec![expected_output]), rng);
}

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
        format!("{}", quote.into_iter().map(|c| format!("{c}u8")).join(" "))
    }

    let processed_quixote_quote = process_quote(quixote_quote);
    let processed_hamlet_quote = process_quote(hamlet_quote);
    let processed_lotr_quote = process_quote(lotr_quote);

    let program_string = format!(
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
        input r0 as dynamic.record; // type A
        input r1 as dynamic.record; // type B
        input r2 as dynamic.record; // type B
        
        // underlying input types: A B
        // underlying output types: B
        call.dynamic {program_name_field} {network_field} {leaf_two_one_function_field}
            with r0 r1 (as dynamic.record dynamic.record)
            into r3 (as dynamic.record);

        // underlying input types: B
        // underlying output types: B C
        call.dynamic {program_name_field} {network_field} {leaf_one_two_function_field}
            with r3 (as dynamic.record)
            into r4 r5 (as dynamic.record dynamic.record);

        // underlying input types: C B B (the last two are not translated)
        call.dynamic {program_name_field} {network_field} {double_caller_one_zero_function_field}
            with r5 r4 r2 (as dynamic.record dynamic.record dynamic.record);

        // underlying output types: B
        call.dynamic {program_name_field} {network_field} {leaf_zero_one_function_field}
            into r6 (as dynamic.record);

    function double_caller_one_zero:
        input r0 as c.record;
        input r1 as dynamic.record; // type B
        input r2 as dynamic.record; // type B

        // underlying input types: B
        // underlying output types: A
        call.dynamic {program_name_field} {network_field} {leaf_one_one_function_field}
            with r1 (as dynamic.record)
            into r3 (as dynamic.record);

        // underlying input types: A B
        // underlying output types: B
        call.dynamic {program_name_field} {network_field} {leaf_two_one_function_field}
            with r3 r2 (as dynamic.record dynamic.record)
            into r4 (as dynamic.record);

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

    let program = Program::<CurrentNetwork>::from_str(&program_string).unwrap();

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V12).unwrap(), rng);

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
