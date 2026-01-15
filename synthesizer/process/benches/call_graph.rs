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

#[macro_use]
extern crate criterion;

use snarkvm_console::{
    network::{MainnetV0, Network},
    program::{Identifier, Locator, ProgramID},
    types::Field,
};
use snarkvm_synthesizer_process::Process;

use criterion::Criterion;
use indexmap::IndexMap;
use std::{collections::HashMap, str::FromStr};

type CurrentNetwork = MainnetV0;

/// Type alias for call graph return type.
type CallGraphData = (
    HashMap<<CurrentNetwork as Network>::TransitionID, Vec<<CurrentNetwork as Network>::TransitionID>>,
    IndexMap<<CurrentNetwork as Network>::TransitionID, Locator<CurrentNetwork>>,
);

/// Constructs a transition ID from a u64.
fn tid(n: u64) -> <CurrentNetwork as Network>::TransitionID {
    <CurrentNetwork as Network>::TransitionID::from(Field::<CurrentNetwork>::from_u64(n))
}

/// Constructs a locator from program and function names.
fn locator(program: &str, function: &str) -> Locator<CurrentNetwork> {
    Locator::<CurrentNetwork>::new(
        ProgramID::<CurrentNetwork>::from_str(program).unwrap(),
        Identifier::<CurrentNetwork>::from_str(function).unwrap(),
    )
}

/// Builds a linear chain call graph: A → B → C → D → ...
/// Returns (call_graph, tid_to_locator) where tid_to_locator is in post-order.
fn build_linear_chain(size: usize) -> CallGraphData {
    let mut call_graph = HashMap::new();
    let mut tid_to_locator = IndexMap::new();

    for i in 0..size {
        let current = tid(i as u64);
        let children = if i + 1 < size { vec![tid((i + 1) as u64)] } else { vec![] };
        call_graph.insert(current, children);
    }

    // Post-order: leaf first, root last.
    for i in (0..size).rev() {
        tid_to_locator.insert(tid(i as u64), locator(&format!("p{i}.aleo"), &format!("f{i}")));
    }

    (call_graph, tid_to_locator)
}

/// Builds a wide tree call graph: root → {child_0, child_1, ..., child_n}.
/// Returns (call_graph, tid_to_locator) where tid_to_locator is in post-order.
fn build_wide_tree(num_children: usize) -> CallGraphData {
    let mut call_graph = HashMap::new();
    let mut tid_to_locator = IndexMap::new();

    let root = tid(0);
    let children: Vec<_> = (1..=num_children).map(|i| tid(i as u64)).collect();

    call_graph.insert(root, children.clone());
    for &child in &children {
        call_graph.insert(child, vec![]);
    }

    // Post-order: children first, then root.
    for i in 1..=num_children {
        tid_to_locator.insert(tid(i as u64), locator(&format!("p{i}.aleo"), &format!("f{i}")));
    }
    tid_to_locator.insert(root, locator("root.aleo", "root"));

    (call_graph, tid_to_locator)
}

/// Builds a binary tree call graph of given depth.
/// Depth 1 = 1 node, depth 2 = 3 nodes, depth 3 = 7 nodes, etc.
/// Returns (call_graph, tid_to_locator) where tid_to_locator is in post-order.
fn build_binary_tree(depth: usize) -> CallGraphData {
    let mut call_graph = HashMap::new();
    let mut tid_to_locator = IndexMap::new();
    let mut counter = 0u64;

    // Build tree using level-order, then reverse for post-order insertion.
    fn build_subtree(
        depth: usize,
        counter: &mut u64,
        call_graph: &mut HashMap<
            <CurrentNetwork as Network>::TransitionID,
            Vec<<CurrentNetwork as Network>::TransitionID>,
        >,
        post_order: &mut Vec<(<CurrentNetwork as Network>::TransitionID, Locator<CurrentNetwork>)>,
    ) -> <CurrentNetwork as Network>::TransitionID {
        let current_id = *counter;
        *counter += 1;
        let current_tid = tid(current_id);

        if depth == 1 {
            call_graph.insert(current_tid, vec![]);
            post_order.push((current_tid, locator(&format!("p{current_id}.aleo"), &format!("f{current_id}"))));
        } else {
            let left = build_subtree(depth - 1, counter, call_graph, post_order);
            let right = build_subtree(depth - 1, counter, call_graph, post_order);
            call_graph.insert(current_tid, vec![left, right]);
            post_order.push((current_tid, locator(&format!("p{current_id}.aleo"), &format!("f{current_id}"))));
        }

        current_tid
    }

    let mut post_order = Vec::new();
    build_subtree(depth, &mut counter, &mut call_graph, &mut post_order);

    for (t, loc) in post_order {
        tid_to_locator.insert(t, loc);
    }

    (call_graph, tid_to_locator)
}

fn bench_linear_chain(c: &mut Criterion) {
    for size in [1, 4, 8, 16, 31] {
        let (call_graph, tid_to_locator) = build_linear_chain(size);
        c.bench_function(&format!("ensure_acyclic | linear | {size}"), |b| {
            b.iter(|| Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap())
        });
    }
}

fn bench_wide_tree(c: &mut Criterion) {
    for num_children in [4, 8, 16, 30] {
        let (call_graph, tid_to_locator) = build_wide_tree(num_children);
        let total = num_children + 1;
        c.bench_function(&format!("ensure_acyclic | wide | {total}"), |b| {
            b.iter(|| Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap())
        });
    }
}

fn bench_binary_tree(c: &mut Criterion) {
    // depth 2 = 3 nodes, depth 3 = 7, depth 4 = 15, depth 5 = 31
    for depth in [2, 3, 4, 5] {
        let (call_graph, tid_to_locator) = build_binary_tree(depth);
        let total = (1 << depth) - 1; // 2^depth - 1
        c.bench_function(&format!("ensure_acyclic | binary | {total}"), |b| {
            b.iter(|| Process::<CurrentNetwork>::ensure_acyclic_call_graph(&call_graph, &tid_to_locator).unwrap())
        });
    }
}

criterion_group! {
    name = call_graph;
    config = Criterion::default().sample_size(100);
    targets = bench_linear_chain, bench_wide_tree, bench_binary_tree
}

criterion_main!(call_graph);
