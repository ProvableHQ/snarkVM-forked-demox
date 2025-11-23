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

impl<N: Network> Process<N> {
    /// Verifies the given execution is valid.
    /// Note: This does *not* check that the global state root exists in the ledger.
    #[inline]
    pub fn verify_execution(
        &self,
        consensus_version: ConsensusVersion,
        varuna_version: VarunaVersion,
        inclusion_version: InclusionVersion,
        execution: &Execution<N>,
    ) -> Result<()> {
        let timer = timer!("Process::verify_execution");

        // Ensure the execution contains transitions.
        ensure!(!execution.is_empty(), "There are no transitions in the execution");
        // Ensure that the execution does not exceed the maximum number of transitions.
        ensure!(
            execution.len() < Transaction::<N>::MAX_TRANSITIONS,
            "The number of transitions in an execution must be less than '{}'",
            Transaction::<N>::MAX_TRANSITIONS
        );

        // Determine the function locator and ensure the number of transitions matches the number of calls.
        let locator = {
            // Retrieve the transition (without popping it).
            let transition = execution.peek()?;
            // Retrieve the stack.
            let stack = self.get_stack(transition.program_id())?;
            // Ensure the number of calls matches the number of transitions.
            let number_of_calls = stack.get_number_of_calls(transition.function_name())?;

            // TODO (dynamic_dispatch) re-introduce or redesign, fails to account for dynamic calls
            // ensure!(
            //     number_of_calls == execution.len(),
            //     "The number of transitions in the execution is incorrect. Expected {number_of_calls}, but found {}",
            //     execution.len()
            // );

            // Output the locator of the main function.
            Locator::new(*transition.program_id(), *transition.function_name()).to_string()
        };
        lap!(timer, "Verify the number of transitions");

        // Construct the call graph of the execution.
        let call_graph = self.construct_call_graph(execution.transitions())?;
        // Construct the reverse call graph of the execution.
        // Note: This is a mapping of the child transition ID to the parent transition ID.
        let reverse_call_graph = Self::reverse_call_graph(&call_graph);

        // Initialize a map of verifying keys to public inputs.
        let mut verifier_inputs = HashMap::new();

        // Initialize a map of transition IDs to references of the transition.
        let mut transition_map = HashMap::new();

        // Initialize a map of (program ID, record identifier) to translation verifying keys.
        let mut translation_verifying_keys: HashMap<(ProgramID<N>, Identifier<N>), VerifyingKey<N>> = HashMap::new();

        // Verify each transition.
        for transition in execution.transitions() {
            dev_println!("Verifying transition for {}/{}...", transition.program_id(), transition.function_name());
            // Debug-mode only, as the `Transition` constructor recomputes the transition ID at initialization.
            debug_assert_eq!(
                **transition.id(),
                N::hash_bhp512(&(transition.to_root()?, *transition.tcm()).to_bits_le())?,
                "The transition ID is incorrect"
            );

            // Ensure the transition is not a fee transition.
            let is_fee_transition = transition.is_fee_private() || transition.is_fee_public();
            ensure!(!is_fee_transition, "Fee transitions are not allowed in executions");
            // Ensure the number of inputs is within the allowed range.
            ensure!(transition.inputs().len() <= N::MAX_INPUTS, "Transition exceeded maximum number of inputs");
            // Ensure the number of outputs is within the allowed range.
            ensure!(transition.outputs().len() <= N::MAX_OUTPUTS, "Transition exceeded maximum number of outputs");

            // Retrieve the network ID.
            let network_id = U16::new(N::ID);
            // Compute the function ID.
            let function_id = compute_function_id(&network_id, transition.program_id(), transition.function_name())?;

            // Ensure each input is valid.
            if transition
                .inputs()
                .iter()
                .enumerate()
                .any(|(index, input)| !input.verify(function_id, transition.tcm(), index))
            {
                bail!("Failed to verify a transition input")
            }
            lap!(timer, "Verify the inputs");

            // Ensure each output is valid.
            let num_inputs = transition.inputs().len();
            let num_outputs = transition.outputs().len();
            for (index, output) in transition.outputs().iter().enumerate() {
                // If the consensus version are before `ConsensusVersion::V8`, ensure the output record is on Version 0.
                // if the consensus version is on or after `ConsensusVersion::V8`, ensure the output record is on Version 1.
                if let Some((_, record)) = output.record() {
                    if (ConsensusVersion::V1..=ConsensusVersion::V7).contains(&consensus_version) {
                        #[cfg(not(any(test, feature = "test")))]
                        ensure!(record.version().is_zero(), "Output record must be Version 0 before Consensus V8");
                        #[cfg(any(test, feature = "test"))]
                        ensure!(
                            record.version().is_one(),
                            "Output record must be Version 1 before Consensus V8 in tests."
                        );
                    } else {
                        ensure!(record.version().is_one(), "Output record must be Version 1 on or after Consensus V8");
                    }
                }
                // Ensure the output is valid.
                if !output.verify(function_id, transition.tcm(), num_inputs + index) {
                    bail!("Failed to verify a transition output")
                }
            }
            lap!(timer, "Verify the outputs");

            // Retrieve the stack.
            let stack = self.get_stack(transition.program_id())?;
            // Retrieve the function from the stack.
            let function = stack.get_function(transition.function_name())?;
            // Retrieve the program checksum, if the program has a constructor.
            let program_checksum = match stack.program().contains_constructor() {
                true => Some(stack.program_checksum_as_field()?),
                false => None,
            };
            // Retrieve the translation verifying keys for the transition's program.
            for record_name in stack.program().records().keys() {
                let key = (*transition.program_id(), *record_name);

                // TODO (dynamic_dispatch) do better (e.g with .entry)
                if !translation_verifying_keys.contains_key(&key) {
                    if key
                        == (
                            ProgramID::<N>::from_str("credits.aleo").unwrap(),
                            Identifier::<N>::from_str("credits").unwrap(),
                        )
                    {
                        let verifying_key = N::translation_credits_verifying_key().clone();
                        // Retrieve the number of public and private variables.
                        // Note: This number does *NOT* include the number of constants. This is safe because
                        // this program is never deployed, as it is a first-class citizen of the protocol.
                        let num_variables = verifying_key.circuit_info.num_public_and_private_variables as u64;
                        // Insert the translation verifying key.
                        translation_verifying_keys.insert(key, VerifyingKey::<N>::new(verifying_key, num_variables));
                    } else {
                        let translation_verifying_key = stack
                            .get_translation_verifying_key(record_name)
                            .map_err(|_| anyhow!("Translation verifying key not found for {}/{}", key.0, key.1))?;
                        translation_verifying_keys.insert(key, translation_verifying_key);
                    }
                }
            }

            // Ensure the number of inputs and outputs match the expected number in the function.
            ensure!(function.inputs().len() == num_inputs, "The number of transition inputs is incorrect");
            ensure!(function.outputs().len() == num_outputs, "The number of transition outputs is incorrect");

            // Ensure the input and output types are equivalent to the ones defined in the function.
            // We only need to check that the variant type matches because we already check the hashes in
            // the `Input::verify` and `Output::verify` functions.
            ensure!(
                function.input_types().len() == transition.inputs().len(),
                "The number of transition inputs is incorrect"
            );
            for (function_input, transition_input) in function.input_types().iter().zip(transition.inputs().iter()) {
                match (function_input, transition_input) {
                    (ValueType::Constant(..), Input::Constant(..))
                    | (ValueType::Public(..), Input::Public(..))
                    | (ValueType::Private(..), Input::Private(..))
                    | (ValueType::Record(..), Input::Record(..))
                    | (ValueType::ExternalRecord(..), Input::ExternalRecord(..))
                    | (ValueType::DynamicRecord, Input::DynamicRecord(..)) => {}
                    _ => bail!("The input variants do not match"),
                }
            }
            ensure!(
                function.input_types().len() == transition.inputs().len(),
                "[verify Execution] Expected {} inputs, but {} were provided.",
                function.input_types().len(),
                transition.inputs().len()
            );
            for (function_output, transition_output) in function.output_types().iter().zip(transition.outputs().iter())
            {
                match (function_output, transition_output) {
                    (ValueType::Constant(..), Output::Constant(..))
                    | (ValueType::Public(..), Output::Public(..))
                    | (ValueType::Private(..), Output::Private(..))
                    | (ValueType::Record(..), Output::Record(..))
                    | (ValueType::ExternalRecord(..), Output::ExternalRecord(..))
                    | (ValueType::Future(..), Output::Future(..))
                    | (ValueType::DynamicRecord, Output::DynamicRecord(..)) => {}
                    _ => bail!("The output variants do not match"),
                }
            }

            // Retrieve the parent program ID.
            // Note: The last transition in the execution does not have a parent, by definition.
            let parent = reverse_call_graph.get(transition.id()).and_then(|tid| execution.get_program_id(tid));

            // Add the transition to the transition map.
            transition_map.insert(*transition.id(), (transition, function.clone()));

            // Construct the verifier inputs for the transition.
            let inputs = self.to_transition_verifier_inputs(
                transition,
                parent,
                &call_graph,
                program_checksum.map(|checksum| *checksum),
                &mut transition_map,
            )?;
            lap!(timer, "Constructed the verifier inputs for a transition of {}", function.name());

            // Save the verifying key and its inputs.
            verifier_inputs
                .entry(Locator::new(*stack.program_id(), *function.name()))
                // Retrieve the verifying key, if it does not already exist.
                .or_insert((stack.get_verifying_key(function.name())?, vec![]))
                .1
                .push(inputs);
            lap!(timer, "Stored the verifier inputs for a transition of {}", function.name());
        }

        // Count the number of verifier instances.
        let num_instances = verifier_inputs.values().map(|(_, inputs)| inputs.len()).sum::<usize>();
        // Ensure the number of instances matches the number of transitions.
        ensure!(num_instances == execution.transitions().len(), "The number of verifier instances is incorrect");
        // Ensure the same signer is used for all transitions.
        execution.transitions().try_fold(None, |signer, transition| {
            Ok(match signer {
                None => Some(transition.scm()),
                Some(signer) => {
                    ensure!(signer == transition.scm(), "The transitions did not use the same signer");
                    Some(signer)
                }
            })
        })?;

        // Construct the list of verifier inputs.
        let mut verifier_inputs: Vec<_> = verifier_inputs.values().cloned().collect();

        // Construct the batch of translation verifier inputs.
        let batch_translation_inputs = Translation::prepare_verifier_inputs(
            execution.transitions(),
            &transition_map,
            &translation_verifying_keys,
        )?;

        println!("[verify_execution.rs] Prepared {} translation verifier inputs.", batch_translation_inputs.len());

        // TODO(dynamic_dispatch): bring appropriate new measurement functions from execution_cost_for_authorization to here.

        for (verifying_key, batch_translation_inputs_for_record) in batch_translation_inputs.into_iter() {
            // Retrieve the number of public and private variables.
            // Note: This number does *NOT* include the number of constants. This is safe because
            // this program is never deployed, as it is a first-class citizen of the protocol.
            // TODO (dynamic_dispatch) should this be used?
            let num_variables = verifying_key.circuit_info.num_public_and_private_variables as u64;
            // Insert the inclusion verifier inputs.
            verifier_inputs.push((verifying_key.clone(), batch_translation_inputs_for_record));
        }

        // Verify the execution proof.
        Trace::verify_execution_proof(&locator, varuna_version, inclusion_version, verifier_inputs, execution)?;

        lap!(timer, "Verify the proof");

        finish!(timer);
        Ok(())
    }
}

impl<N: Network> Process<N> {
    /// Returns the public inputs to verify the proof for the given transition.
    fn to_transition_verifier_inputs(
        &self,
        transition: &Transition<N>,
        parent: Option<&ProgramID<N>>,
        call_graph: &HashMap<N::TransitionID, Vec<N::TransitionID>>,
        program_checksum: Option<N::Field>,
        transition_map: &mut HashMap<N::TransitionID, (&Transition<N>, Function<N>)>,
    ) -> Result<Vec<N::Field>> {
        // Compute the x- and y-coordinate of `tpk`.
        let (tpk_x, tpk_y) = transition.tpk().to_xy_coordinates();

        // Determine the value of `is_root` and `parent`.
        let (is_root, parent) = match parent {
            // If there is a parent, then `is_root` is `0` and `parent` is the parent program ID.
            Some(program_id) => (Field::<N>::zero(), *program_id),
            // If there is no parent, then `is_root` is `1` and `parent` is the root program ID.
            None => (Field::one(), *transition.program_id()),
        };

        // Retrieve the address belonging to the parent.
        let stack = self.get_stack(parent)?;
        let parent_address = stack.program_address();

        // Compute the x- and y-coordinate of `parent`.
        let (parent_x, parent_y) = parent_address.to_xy_coordinates();

        // [Inputs] Construct the verifier inputs to verify the proof.
        let mut verifier_inputs = vec![N::Field::one()];
        // [Inputs] Extend the verifier inputs with the program checksum if it was provided.
        if let Some(program_checksum) = program_checksum {
            verifier_inputs.push(program_checksum);
        }
        // [Inputs] Extend the verifier inputs with the tpk, transition and signer commitments.
        verifier_inputs.extend([*tpk_x, *tpk_y, **transition.tcm(), **transition.scm()]);
        // [Inputs] Extend the verifier inputs with the input IDs.
        verifier_inputs.extend(transition.inputs().iter().flat_map(|input| input.verifier_inputs()));
        // [Inputs] Extend the verifier inputs with the public inputs for 'self.caller'.
        verifier_inputs.extend([*is_root, *parent_x, *parent_y]);

        // If there are function calls, append their inputs and outputs.
        println!("Parent function: {} in program.id {}", transition.function_name(), transition.program_id());
        let child_transition_ids = call_graph.get(transition.id()).unwrap();
        let parent_function = transition_map.get(&transition.id()).map(|(_, function)| function.clone());
        use snarkvm_synthesizer_program::{Call, CallOperator};
        let parent_function_calls = match parent_function {
            Some(function) => function
                .instructions()
                .iter()
                .filter(|instruction| matches!(instruction, Instruction::CallDynamic(_) | Instruction::Call(_)))
                .map(|instruction| Some(instruction.clone()))
                .collect::<Vec<_>>(),
            None => vec![None; child_transition_ids.len()],
        };
        ensure!(
            parent_function_calls.len() == child_transition_ids.len(),
            "The number of parent function calls and child transition IDs do not match"
        );
        // TODO(@vicsn): in case we stick to encoding and using *all* of the caller_{inputs, outputs} instead of just the dynamic ones,
        // we'll have to assert they equal the child's inputs/outputs.
        for (child_transition_id, parent_function_call) in child_transition_ids.iter().zip(parent_function_calls) {
            // Note: This unwrap is safe, as we are processing transitions in post-order,
            // which implies that all child transition IDs have been added to `transition_map`.
            let (child_transition, _) = transition_map.get(child_transition_id).unwrap();
            let child_function_id =
                compute_function_id(&U16::new(N::ID), child_transition.program_id(), child_transition.function_name())?;
            println!(
                "Child function: {} in program.id {}",
                child_transition.function_name(),
                child_transition.program_id()
            );
            // [Inputs] Extend the verifier inputs with the program ID and function name if the child transition is dynamic.
            if child_transition.is_dynamic() {
                verifier_inputs.extend(child_transition.program_id().to_fields()?.into_iter().map(|field| *field));
                verifier_inputs.extend([*child_transition.function_name().to_field()?]);
                verifier_inputs.extend([*compute_function_id(
                    &U16::new(N::ID),
                    child_transition.program_id(),
                    child_transition.function_name(),
                )?]);
            }
            println!("child_transition.tcm(): {:?}", **child_transition.tcm());
            // [Inputs] Extend the verifier inputs with the transition commitment of the external call.
            verifier_inputs.extend([**child_transition.tcm()]);
            // [Inputs] Extend the verifier inputs with the input IDs of the external call.
            let child_inputs = match (child_transition.is_dynamic(), child_transition.caller_inputs()) {
                (true, Some(caller_inputs)) => caller_inputs,
                (true, None) => bail!("Dynamic transition has no caller inputs"),
                (false, _) => child_transition.inputs(),
            };
            let num_inputs = child_transition.inputs().len();
            println!(
                "child_inputs: {:?}",
                child_inputs.iter().flat_map(|input| input.verifier_inputs()).collect::<Vec<_>>()
            );
            verifier_inputs.extend(child_inputs.iter().flat_map(|input| input.verifier_inputs()));
            // [Inputs] Extend the verifier inputs with the output IDs of the external call.
            let output_ids = match (child_transition.is_dynamic(), child_transition.caller_outputs()) {
                (false, _) => child_transition.output_ids().map(|id| **id).collect::<Vec<_>>(),
                (true, None) => bail!("Dynamic transition has no caller outputs"),
                (true, Some(caller_outputs)) => {
                    let Some(Instruction::CallDynamic(dynamic_call)) = parent_function_call else {
                        bail!("Parent function call is not a dynamic call: {:?}", parent_function_call);
                    };
                    let mut caller_output_ids = vec![];
                    ensure!(
                        caller_outputs.len() == dynamic_call.destination_types().len(),
                        "The number of caller outputs and dynamic call outputs do not match"
                    );
                    for (index, (caller_output, caller_destination_type)) in
                        caller_outputs.iter().zip(dynamic_call.destination_types().iter()).enumerate()
                    {
                        match (caller_output, caller_destination_type) {
                            // In the case of a DynamicFuture, the verifier computes the hash of the dynamic future directly.
                            (Output::Future(id, future), ValueType::DynamicFuture) => {
                                let Some(future) = future else {
                                    bail!("Future is not present for child transition {}", child_transition.id());
                                };
                                let dynamic_future = DynamicFuture::from_future(future)?;
                                let dynamic_future_id = {
                                    let index = Field::from_u16(
                                        u16::try_from(num_inputs + index).or_halt_with::<N>("Output index exceeds u16"),
                                    );
                                    // Construct the preimage as `(function ID || output || tcm || index)`.
                                    let mut preimage = Vec::new();
                                    preimage.push(child_function_id);
                                    preimage.extend(dynamic_future.to_fields()?);
                                    preimage.push(*child_transition.tcm());
                                    preimage.push(index);
                                    // Hash the output to a field element.
                                    N::hash_psd8(&preimage)?
                                };
                                caller_output_ids.push(*dynamic_future_id);
                            }
                            _ => caller_output_ids.push(**caller_output.id()),
                        }
                    }
                    caller_output_ids
                }
            };
            println!("child outputs: {:?}", output_ids);
            verifier_inputs.extend(output_ids);
        }

        // [Inputs] Extend the verifier inputs with the output IDs.
        verifier_inputs.extend(transition.outputs().iter().flat_map(|output| output.verifier_inputs()));

        dev_println!("Transition public inputs ({} elements): {:#?}", verifier_inputs.len(), verifier_inputs);
        Ok(verifier_inputs)
    }
}

impl<N: Network> Process<N> {
    // A helper function to construct a call graph from an execution.
    //
    // The call graph represents a mapping of parent transition IDs to child transition IDs,
    // in the order that they were called.
    //
    // Suppose we have the following call structure.
    // The functions are invoked in the following order:
    // "three.aleo/a"
    //   --> "two.aleo/b"
    //        --> "zero.aleo/c"
    //   --> "zero.aleo/c"
    //   --> "one.aleo/d"
    //        --> "zero.aleo/c"
    // The order of the transitions in the `Execution` is:
    //  - [c, b, c, c, d, a]
    // However, the `Execution` only provides `Transition`s and not the call graph.
    // In other words, we do not know which transitions were invoked by which transitions.
    // Note that transition names are insufficient to reconstruct the call graph, since the same function can be invoked multiple times, in different ways.
    //
    // In order to reconstruct the call graph, we:
    // - Iterate over the call structure in reverse post-order. The ordering is maintained by the `traversal_stack`.
    // - Process each transition in the `Execution` in reverse, assigning its transition ID to the corresponding function call.
    pub fn construct_call_graph<'a>(
        &self,
        transitions: impl ExactSizeIterator<Item = &'a Transition<N>> + DoubleEndedIterator,
    ) -> Result<HashMap<N::TransitionID, Vec<N::TransitionID>>> {
        // Metadata for each transition the execution.
        struct TransitionMetadata<N: Network> {
            uid: usize,
            // pid and fname of the transition. For static calls, this is set at
            // metadata-creation time to be later matched against the data from
            // the actual transition found in the execution (defense in depth).
            // For dynamic calls, it is set to None and subsequently taken from
            // the data in the actual transition (no in-depth defense).
            locator: Option<(ProgramID<N>, Identifier<N>)>,
            tid: Option<N::TransitionID>,
            children: Option<Vec<usize>>,
        }

        impl<N: Network> TransitionMetadata<N> {
            fn new(
                counter: &mut usize,
                locator: Option<(ProgramID<N>, Identifier<N>)>,
                tid: Option<N::TransitionID>,
            ) -> Self {
                let uid = *counter;
                *counter += 1;
                Self { uid, locator, tid, children: None }
            }

            /// Returns 'true' if the subgraph starting from this transition has been fully-indexed.
            fn is_complete(&self) -> bool {
                self.tid.is_some() && self.children.is_some()
            }
        }

        // A helper function to update the call graph, given transition metadata.
        let update_call_graph = |metadata: TransitionMetadata<N>,
                                 call_graph: &mut HashMap<N::TransitionID, Vec<N::TransitionID>>,
                                 uid_to_tid: &mut HashMap<usize, N::TransitionID>|
         -> Result<()> {
            // Check that the transition metadata is complete.
            ensure!(metadata.is_complete(), "Invalid traversal - transition metadata is incomplete");
            // Update the call graph.
            call_graph.insert(
                metadata.tid.unwrap(),
                metadata
                    .children // Safe to unwrap, since the metadata is complete.
                    .unwrap()
                    .into_iter()
                    .map(|uid| match uid_to_tid.get(&uid) {
                        Some(tid) => Ok(*tid),
                        None => bail!("Invalid traversal - missing 'tid' for uid '{uid}'"),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
            // Update the UID to TID mapping.
            uid_to_tid.insert(metadata.uid, metadata.tid.unwrap());
            Ok(())
        };

        // Initialize a call graph, which is a map of transition IDs to the transition IDs it calls.
        let mut call_graph = HashMap::new();
        // Initialize a mapping from UIDs to transition IDs.
        let mut uid_to_tid = HashMap::new();

        // Initialize a stack to track transition metadata, while traversing the call graph.
        let mut traversal_stack: Vec<TransitionMetadata<N>> = Vec::new();
        // Initialize a counter to provide unique IDs for each transition.
        let mut counter = 0;

        let num_transitions = transitions.len();

        // Iterate over each transition in reverse post-order, and populate the call graph.
        for transition in transitions.rev() {
            // Now process the current `transition`.
            // At this point, the algorithm must maintain the following invariant:
            // - The stack is either empty, or the top entry is incomplete.
            match traversal_stack.last_mut() {
                // If the stack is empty, then push the `transition` to the top of the stack.
                None => {
                    traversal_stack.push(TransitionMetadata::new(
                        &mut counter,
                        Some((*transition.program_id(), *transition.function_name())),
                        Some(*transition.id()),
                    ));
                }
                // If the stack is not empty, then add the current transition ID to the entry.
                Some(head) => {
                    match head.locator {
                        Some((expected_pid, expected_fname)) => {
                            // Checking the pid and fname expected (from the static call instruction) against the actual transition.
                            ensure!(
                                expected_pid == *transition.program_id()
                                    && expected_fname == *transition.function_name(),
                                "Invalid traversal - unexpected transition in the execution"
                            );
                        }
                        None => {
                            // Setting the pid and fname from the actual transition
                            head.locator = Some((*transition.program_id(), *transition.function_name()));
                        }
                    }

                    head.tid = Some(*transition.id());
                }
            }

            // Process the entry at the top of the stack. By the previous step, this entry has a transition ID.
            // Note this unwrap is safe, since we either pushed an entry to the stack or modified the one at the top of the stack.
            let top = traversal_stack.last().unwrap();
            // If the entry is complete, then add it to the call graph.
            if top.is_complete() {
                // Note this unwrap is safe, for the same reason as above.
                update_call_graph(traversal_stack.pop().unwrap(), &mut call_graph, &mut uid_to_tid)?;
            } else {
                // This unwrap is safe as the locator field is set after all possible paths of the match
                let (caller_pid, caller_fname) = top.locator.as_ref().unwrap();

                // Retrieve the stack.
                let stack = self.get_stack(caller_pid)?;
                // Retrieve the function from the stack.
                let caller_fname = stack.get_function(caller_fname)?;
                // Collect the children of the current transition.
                let mut children = Vec::new();
                for instruction in caller_fname.instructions() {
                    if let Instruction::Call(call) = instruction {
                        let (pid, fname) = match call.operator() {
                            snarkvm_synthesizer_program::CallOperator::Locator(locator) => {
                                (locator.program_id(), locator.resource())
                            }
                            snarkvm_synthesizer_program::CallOperator::Resource(fname) => (caller_pid, fname),
                        };
                        // Add the child to the traversal stack, only if it is a call to a transition.
                        if self.get_stack(pid)?.get_function(fname).is_ok() {
                            children.push(TransitionMetadata::new(&mut counter, Some((*pid, *fname)), None));
                        }
                    }
                    if let Instruction::CallDynamic(_) = instruction {
                        // Add the child to the traversal stack.
                        // NOTE: for dynamic calls, the verifier doesn't have
                        // access to a locator or resource. However, the
                        // verifier can determine the program and function name
                        // directly from the DFS ordering of transitions in the
                        // Execution.
                        children.push(TransitionMetadata::new(&mut counter, None, None));
                    }
                }

                // Add the children UIDs to the metadata.
                // Note this unwrap is safe, for the same reason as above.
                let top = traversal_stack.last_mut().unwrap();
                let child_uids = children.iter().map(|child| child.uid).collect::<Vec<_>>();
                match top.children {
                    None => top.children = Some(child_uids),
                    Some(_) => bail!("Invalid traversal - children have already been processed"),
                }
                // Push the children to the top of the stack.
                traversal_stack.extend(children);
            }
            // If the stack has complete metadata entries, then remove and add them to the call graph.
            while let Some(metadata) = traversal_stack.last() {
                if metadata.is_complete() {
                    update_call_graph(traversal_stack.pop().unwrap(), &mut call_graph, &mut uid_to_tid)?;
                } else {
                    break;
                }
            }
        }
        // Check that the the traversal completed correctly.
        ensure!(traversal_stack.is_empty(), "Invalid traversal - traversal stack is not empty");

        ensure!(
            counter == num_transitions,
            "Invalid traversal - counter does not match the number of transitions in the execution"
        );

        Ok(call_graph)
    }

    /// A helper function to reverse the call graph.
    ///
    /// The call graph is a mapping of parent transition IDs to child transition IDs,
    /// in the order that they were called.
    ///
    /// The reverse call graph is a mapping of child transition IDs to parent transition IDs.
    /// Note: Each child transition only has one parent transition, by definition.
    fn reverse_call_graph(
        call_graph: &HashMap<N::TransitionID, Vec<N::TransitionID>>,
    ) -> HashMap<N::TransitionID, N::TransitionID> {
        // Initialize a map for the reverse call graph.
        let mut reverse_call_graph = HashMap::new();
        // Iterate over the (forward) call graph.
        for (parent, children) in call_graph {
            for child in children {
                let result = reverse_call_graph.insert(*child, *parent);
                debug_assert!(result.is_none(), "Found a child with multiple parents");
            }
        }
        reverse_call_graph
    }
}
