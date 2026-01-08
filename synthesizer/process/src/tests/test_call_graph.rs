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

use crate::Process;
use console::{
    network::{MainnetV0, prelude::*},
    program::{Identifier, Locator, ProgramID},
    types::Field,
};

use indexmap::IndexMap;
use std::collections::HashMap;

type CurrentNetwork = MainnetV0;

// Helper function to construct a transition ID.
fn tid(n: u64) -> <CurrentNetwork as Network>::TransitionID {
    <CurrentNetwork as Network>::TransitionID::from(Field::<CurrentNetwork>::from_u64(n))
}

// Helper function to construct a program ID.
fn pid(s: &str) -> ProgramID<CurrentNetwork> {
    ProgramID::<CurrentNetwork>::from_str(s).unwrap()
}

// Helper function to construct an identifier.
fn ident(s: &str) -> Identifier<CurrentNetwork> {
    Identifier::<CurrentNetwork>::from_str(s).unwrap()
}

// Helper function to construct a locator.
fn locator(program_id: &str, function_name: &str) -> Locator<CurrentNetwork> {
    Locator::<CurrentNetwork>::new(pid(program_id), ident(function_name))
}

#[test]
fn test_ensure_acyclic_call_graph_bails_on_cycle() {
    // A -> B -> B
    // This represents a recursive call cycle by function identity (even though transition IDs differ).
    let a = tid(1);
    let b_0 = tid(2);
    let b_1 = tid(3);

    let call_graph: HashMap<_, Vec<_>> = HashMap::from([(a, vec![b_0]), (b_0, vec![b_1]), (b_1, vec![])]);
    // Post-order: b_1, b_0, a
    let tid_to_locator: IndexMap<_, _> = IndexMap::from([
        (b_1, locator("foo.aleo", "b")),
        (b_0, locator("foo.aleo", "b")),
        (a, locator("foo.aleo", "a")),
    ]);

    let err =
        Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap_err().to_string();
    assert!(err.contains("Cycle detected"), "Unexpected error: {err}");
}

#[test]
fn test_ensure_acyclic_call_graph_accepts_single_node() {
    let call_graph: HashMap<_, Vec<_>> = HashMap::from([(tid(1), vec![])]);
    let tid_to_locator = IndexMap::from([(tid(1), locator("foo.aleo", "f"))]);
    Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap();
}

#[test]
fn test_ensure_acyclic_call_graph_accepts_tree() {
    // A -> {B, C}; B -> D; C -> D
    let a = tid(1);
    let b = tid(2);
    let c = tid(3);
    let d_0 = tid(4);
    let d_1 = tid(5);

    let call_graph: HashMap<_, Vec<_>> =
        HashMap::from([(a, vec![b, c]), (b, vec![d_0]), (c, vec![d_1]), (d_0, vec![]), (d_1, vec![])]);
    // Post-order: d_0, b, d_1, c, a
    let tid_to_locator: IndexMap<_, _> = IndexMap::from([
        (d_0, locator("foo.aleo", "d")),
        (b, locator("foo.aleo", "b")),
        (d_1, locator("foo.aleo", "d")),
        (c, locator("foo.aleo", "c")),
        (a, locator("foo.aleo", "a")),
    ]);

    Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap();
}

#[test]
fn test_ensure_acyclic_call_graph_bails_on_self_recursion_by_locator() {
    // A -> A
    let a_0 = tid(1);
    let a_1 = tid(2);

    let call_graph: HashMap<_, Vec<_>> = HashMap::from([(a_0, vec![a_1]), (a_1, vec![])]);
    let tid_to_locator: IndexMap<_, _> =
        IndexMap::from([(a_1, locator("foo.aleo", "a")), (a_0, locator("foo.aleo", "a"))]);

    let err =
        Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap_err().to_string();
    assert!(err.contains("Cycle detected"), "Unexpected error: {err}");
}

#[test]
fn test_ensure_acyclic_call_graph_bails_on_missing_locator_mapping() {
    let a = tid(1);
    let b = tid(2);

    let call_graph: HashMap<_, Vec<_>> = HashMap::from([(a, vec![b]), (b, vec![])]);
    // Missing locator for `b`.
    let tid_to_locator: IndexMap<_, _> = IndexMap::from([(a, locator("foo.aleo", "a"))]);

    let err =
        Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap_err().to_string();
    assert!(err.contains("Missing locator"), "Unexpected error: {err}");
}

#[test]
fn test_ensure_acyclic_call_graph_bails_on_transition_id_cycle() {
    // Even though this shouldn't happen for valid executions, the validator should reject it.
    let a = tid(1);
    let b = tid(2);

    let call_graph: HashMap<_, Vec<_>> = HashMap::from([(a, vec![b]), (b, vec![a])]);
    let tid_to_locator: IndexMap<_, _> = IndexMap::from([(a, locator("foo.aleo", "f")), (b, locator("foo.aleo", "g"))]);

    let err =
        Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap_err().to_string();
    assert!(err.contains("Cycle detected"), "Unexpected error: {err}");
}

#[test]
fn test_ensure_acyclic_call_graph_accepts_single_root() {
    // Root A -> B -> C
    let a = tid(1);
    let b = tid(2);
    let c = tid(3);

    let call_graph: HashMap<_, Vec<_>> = HashMap::from([(a, vec![b]), (b, vec![c]), (c, vec![])]);
    let tid_to_locator: IndexMap<_, _> =
        IndexMap::from([(c, locator("foo.aleo", "c")), (b, locator("foo.aleo", "b")), (a, locator("foo.aleo", "a"))]);

    Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap();
}

#[test]
fn test_ensure_acyclic_call_graph_bails_on_cycle_with_reused_locator() {
    // A -> {B, C}; B -> D; C -> D -> D
    let a = tid(1);
    let b = tid(2);
    let c = tid(3);
    let d_0 = tid(4);
    let d_1 = tid(5);
    let d_2 = tid(6);

    let call_graph: HashMap<_, Vec<_>> = HashMap::from([
        (a, vec![b, c]),
        (b, vec![d_0]),
        (c, vec![d_1]),
        (d_1, vec![d_2]),
        (d_0, vec![]),
        (d_2, vec![]),
    ]);
    // Post-order: d_0, b, d_2, d_1, c, a
    let tid_to_locator: IndexMap<_, _> = IndexMap::from([
        (d_0, locator("foo.aleo", "d")),
        (b, locator("foo.aleo", "b")),
        (d_2, locator("foo.aleo", "d")),
        (d_1, locator("foo.aleo", "d")),
        (c, locator("foo.aleo", "c")),
        (a, locator("foo.aleo", "a")),
    ]);

    let err =
        Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap_err().to_string();
    assert!(err.contains("Cycle detected"), "Unexpected error: {err}");
}

#[test]
fn test_ensure_acyclic_call_graph_bails_on_no_root_found() {
    // Empty call graph - no root to start traversal from.
    let call_graph: HashMap<_, Vec<_>> = HashMap::new();
    let tid_to_locator: IndexMap<_, _> = IndexMap::new();

    let err =
        Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap_err().to_string();
    assert!(err.contains("No root found"), "Unexpected error: {err}");
}

#[test]
fn test_ensure_acyclic_call_graph_distinguishes_paths() {
    // A -> {B, C}; B -> C; C -> B
    // This should pass as cycle-free because B and C are called in different contexts.
    let a = tid(1);
    let b_0 = tid(2);
    let c_0 = tid(3);
    let b_1 = tid(4);
    let c_1 = tid(5);

    let call_graph: HashMap<_, Vec<_>> =
        HashMap::from([(a, vec![b_0, c_0]), (b_0, vec![c_1]), (c_0, vec![b_1]), (b_1, vec![]), (c_1, vec![])]);

    let tid_to_locator: IndexMap<_, _> = IndexMap::from([
        (c_1, locator("foo.aleo", "c")),
        (b_0, locator("foo.aleo", "b")),
        (b_1, locator("foo.aleo", "b")),
        (c_0, locator("foo.aleo", "c")),
        (a, locator("foo.aleo", "a")),
    ]);

    Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap();
}

#[test]
fn test_ensure_acyclic_call_graph_detects_longer_cycles() {
    // A0 -> {B0, C0}; B0 -> {C1, D0}; D0 -> A1; C0 -> B1
    // This creates a cycle: A0 -> B0 -> D0 -> A1 (where A1 has the same locator as A0)
    let a_0 = tid(1);
    let a_1 = tid(2);
    let b_0 = tid(3);
    let b_1 = tid(4);
    let c_0 = tid(5);
    let c_1 = tid(6);
    let d_0 = tid(7);

    let call_graph: HashMap<_, Vec<_>> = HashMap::from([
        (a_0, vec![b_0, c_0]),
        (b_0, vec![c_1, d_0]),
        (b_1, vec![]),
        (c_0, vec![b_1]),
        (c_1, vec![]),
        (d_0, vec![a_1]),
        (a_1, vec![]),
    ]);

    let tid_to_locator: IndexMap<_, _> = IndexMap::from([
        (c_1, locator("foo.aleo", "c")),
        (a_1, locator("foo.aleo", "a")),
        (d_0, locator("foo.aleo", "d")),
        (b_0, locator("foo.aleo", "b")),
        (b_1, locator("foo.aleo", "b")),
        (c_0, locator("foo.aleo", "c")),
        (a_0, locator("foo.aleo", "a")),
    ]);

    let err =
        Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap_err().to_string();
    assert!(err.contains("Cycle detected"), "Unexpected error: {err}");
}
