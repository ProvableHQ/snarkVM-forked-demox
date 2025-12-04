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

// This test checks that the execution graph computed from an execution
// involving dynamic calls is correct. The functions are invoked in the
// following order:
// "four::a"
//   --> "two::b"
//        --> "zero::c"
//        --> "one::d"
//   --> "three::e"
//        --> "two::b"
//             --> "zero::c"
//             --> "one::d"
//        --> "one::d"
//        --> "zero::c"
//
// Each of the call instructions can be static or dynamic depending on the
// boolean inputs to the test function.
//
// The linearized order is:
//  - [a, b, c, d, e, b, c, d, d, c]
// The transitions must be included in the `Execution` in the order they finish.
// The execution order is:
//  - [c, d, b, c, d, b, d, c, e, a]
fn test_complex_dynamic_graph_construction_internal(
    // In each of the arguments, call_X_Y_dynamic indicates whether the call to
    // function Y inside function X should be static or dynamic.
    call_a_b_dynamic: bool,
    call_a_e_dynamic: bool,
    call_b_c_dynamic: bool,
    call_b_d_dynamic: bool,
    call_e_b_dynamic: bool,
    call_e_d_dynamic: bool,
    call_e_c_dynamic: bool,
) {
    let network_field = Identifier::<CurrentNetwork>::from_str("aleo").unwrap().to_field().unwrap();

    let program_0_name_str = "zero";
    let program_1_name_str = "one";
    let program_2_name_str = "two";
    let program_3_name_str = "three";
    let program_4_name_str = "four";

    let program_0_name_field = Identifier::<CurrentNetwork>::from_str(program_0_name_str).unwrap().to_field().unwrap();
    let program_1_name_field = Identifier::<CurrentNetwork>::from_str(program_1_name_str).unwrap().to_field().unwrap();
    let program_2_name_field = Identifier::<CurrentNetwork>::from_str(program_2_name_str).unwrap().to_field().unwrap();
    let program_3_name_field = Identifier::<CurrentNetwork>::from_str(program_3_name_str).unwrap().to_field().unwrap();

    let function_b_name_id = Identifier::<CurrentNetwork>::from_str("b").unwrap();
    let function_c_name_id = Identifier::<CurrentNetwork>::from_str("c").unwrap();
    let function_d_name_id = Identifier::<CurrentNetwork>::from_str("d").unwrap();
    let function_e_name_id = Identifier::<CurrentNetwork>::from_str("e").unwrap();

    let function_b_name_field = function_b_name_id.to_field().unwrap();
    let function_c_name_field = function_c_name_id.to_field().unwrap();
    let function_d_name_field = function_d_name_id.to_field().unwrap();
    let function_e_name_field = function_e_name_id.to_field().unwrap();

    /******************************* program 0 *******************************/
    let (string, program0) = Program::<CurrentNetwork>::parse(
        r"
    program zero.aleo;

    function c:
        input r0 as u8.private;
        input r1 as u8.private;
        add r0 r1 into r2;
        output r2 as u8.private;
        
    constructor:
        assert.eq true true;",
    )
    .unwrap();
    assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");

    /******************************* program 1 *******************************/
    let (string, program1) = Program::<CurrentNetwork>::parse(
        r"
    program one.aleo;

    function d:
        input r0 as u8.private;
        input r1 as u8.private;
        add r0 r1 into r2;
        output r2 as u8.private;
        
    constructor:
        assert.eq true true;",
    )
    .unwrap();
    assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");

    /******************************* program 2 *******************************/
    let call_b_c_str = if call_b_c_dynamic {
        format!(
            "call.dynamic {program_0_name_field} {network_field} {function_c_name_field} with r0 r1 (as u8.private u8.private) into r2 (as u8.private);"
        )
    } else {
        "call zero.aleo/c r0 r1 into r2;".to_string()
    };

    let call_b_d_str = if call_b_d_dynamic {
        format!(
            "call.dynamic {program_1_name_field} {network_field} {function_d_name_field} with r1 r2 (as u8.private u8.private) into r3 (as u8.private);"
        )
    } else {
        "call one.aleo/d r1 r2 into r3;".to_string()
    };

    let program2_string = format!(
        r"
        import zero.aleo;
        import one.aleo;

        program two.aleo;

        function b:
            input r0 as u8.private;
            input r1 as u8.private;
            {call_b_c_str}
            {call_b_d_str}
            output r3 as u8.private;
            
        constructor:
            assert.eq true true;",
    );
    let (string, program2) = Program::<CurrentNetwork>::parse(program2_string.as_str()).unwrap();
    assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");

    /******************************* program 3 *******************************/

    let call_e_b_str = if call_e_b_dynamic {
        format!(
            "call.dynamic {program_2_name_field} {network_field} {function_b_name_field} with r0 r1 (as u8.private u8.private) into r2 (as u8.private);"
        )
    } else {
        "call two.aleo/b r0 r1 into r2;".to_string()
    };

    let call_e_d_str = if call_e_d_dynamic {
        format!(
            "call.dynamic {program_1_name_field} {network_field} {function_d_name_field} with r1 r2 (as u8.private u8.private) into r3 (as u8.private);"
        )
    } else {
        "call one.aleo/d r1 r2 into r3;".to_string()
    };

    let call_e_c_str = if call_e_c_dynamic {
        format!(
            "call.dynamic {program_0_name_field} {network_field} {function_c_name_field} with r1 r2 (as u8.private u8.private) into r4 (as u8.private);"
        )
    } else {
        "call zero.aleo/c r1 r2 into r4;".to_string()
    };

    let program3_string = format!(
        r"
        import zero.aleo;
        import one.aleo;
        import two.aleo;

        program three.aleo;

        function e:
            input r0 as u8.private;
            input r1 as u8.private;
            {call_e_b_str}
            {call_e_d_str}
            {call_e_c_str}
            output r4 as u8.private;
            
        constructor:
            assert.eq true true;",
    );

    let (string, program3) = Program::<CurrentNetwork>::parse(program3_string.as_str()).unwrap();
    assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");

    /******************************* program 4 *******************************/

    let call_a_b_str = if call_a_b_dynamic {
        format!(
            "call.dynamic {program_2_name_field} {network_field} {function_b_name_field} with r0 r1 (as u8.private u8.private) into r2 (as u8.private);"
        )
    } else {
        "call two.aleo/b r0 r1 into r2;".to_string()
    };

    let call_a_e_str = if call_a_e_dynamic {
        format!(
            "call.dynamic {program_3_name_field} {network_field} {function_e_name_field} with r1 r2 (as u8.private u8.private) into r3 (as u8.private);"
        )
    } else {
        "call three.aleo/e r1 r2 into r3;".to_string()
    };

    let program4_string = format!(
        r"
    import two.aleo;
    import three.aleo;

    program four.aleo;

    function a:
        input r0 as u8.private;
        input r1 as u8.private;
        {call_a_b_str}
        {call_a_e_str}
        output r3 as u8.private;
        
    constructor:
        assert.eq true true;",
    );

    let (string, program4) = Program::<CurrentNetwork>::parse(program4_string.as_str()).unwrap();
    assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");

    // Initialize the RNG.
    let rng = &mut TestRng::default();

    // Initialize caller.
    let caller_private_key = sample_genesis_private_key(rng);

    let vm = sample_vm_at_height(CurrentNetwork::CONSENSUS_HEIGHT(ConsensusVersion::V12).unwrap(), rng);

    for (program, program_name_str) in [
        (program0, program_0_name_str),
        (program1, program_1_name_str),
        (program2, program_2_name_str),
        (program3, program_3_name_str),
        (program4, program_4_name_str),
    ] {
        // Deploy the program.
        println!("Deploying program {program_name_str}...");
        let transaction = vm.deploy(&caller_private_key, &program, None, 0, None, rng).unwrap();
        add_and_test(&vm, &caller_private_key, &[transaction], rng);
    }

    println!("Executing program four::a...");

    // Declare the input value.
    let r0 = Value::<CurrentNetwork>::from_str("1u8").unwrap();
    let r1 = Value::<CurrentNetwork>::from_str("2u8").unwrap();

    // Execute the "dynamic" function.
    let transaction =
        vm.execute(&caller_private_key, ("four.aleo", "a"), [r0, r1].into_iter(), None, 0, None, rng).unwrap();

    println!("Reconstructing call graph...");

    let transitions = transaction.transitions().collect_vec();
    let tids = transitions.iter().map(|transition| transition.id()).collect_vec();

    // Call tree                    transition index
    // "four::a"                    (9)
    //   --> "two::b"               (2)
    //        --> "zero::c"         (0)
    //        --> "one::d"          (1)
    //   --> "three::e"             (8)
    //        --> "two::b"          (5)
    //             --> "zero::c"    (3)
    //             --> "one::d"     (4)
    //        --> "one::d"          (6)
    //        --> "zero::c"         (7)
    //
    // The expected call graph is:
    // 9 -> [2, 8]
    // 8 -> [5, 6, 7]
    // 5 -> [3, 4]
    // 2 -> [0, 1]
    // 0 -> []
    // 1 -> []
    // 3 -> []
    // 4 -> []
    // 6 -> []
    // 7 -> []

    let graph = vm.process().read().construct_call_graph(transitions.into_iter()).unwrap();
    assert_eq!(graph[tids[9]], &[*tids[2], *tids[8]]);
    assert_eq!(graph[tids[8]], &[*tids[5], *tids[6], *tids[7]]);
    assert_eq!(graph[tids[5]], &[*tids[3], *tids[4]]);
    assert_eq!(graph[tids[2]], &[*tids[0], *tids[1]]);
    assert_eq!(graph[tids[0]], &[]);
    assert_eq!(graph[tids[1]], &[]);
    assert_eq!(graph[tids[3]], &[]);
    assert_eq!(graph[tids[4]], &[]);
    assert_eq!(graph[tids[6]], &[]);
    assert_eq!(graph[tids[7]], &[]);

    add_and_test(&vm, &caller_private_key, &[transaction.clone()], rng);
}

#[test]
fn test_complex_dynamic_graph_construction() {
    let num_random_mixes = 3;

    // All static calls
    test_complex_dynamic_graph_construction_internal(false, false, false, false, false, false, false);
    // All dynamic calls
    test_complex_dynamic_graph_construction_internal(true, true, true, true, true, true, true);

    // Random static-/dynamic-call mixes
    let rng = &mut TestRng::default();
    for _ in 0..num_random_mixes {
        let mix: [bool; 7] = rng.r#gen();
        test_complex_dynamic_graph_construction_internal(mix[0], mix[1], mix[2], mix[3], mix[4], mix[5], mix[6]);
    }
}
