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
            // Calculate the minimum number of calls for the root transition.
            let minimum_number_of_calls = stack.get_minimum_number_of_calls(transition.function_name())?;
            // If the root transition contains a dynamic call,
            // - ensure that the number of calls is less than or equal to the number of transitions.
            // - otherwise, ensure that the number of calls matches the number of transitions.
            if stack.contains_dynamic_call(transition.function_name())? {
                ensure!(
                    minimum_number_of_calls <= execution.len(),
                    "The number of transitions in the execution is incorrect. Expected at least {minimum_number_of_calls}, but found {}",
                    execution.len()
                );
            } else {
                ensure!(
                    minimum_number_of_calls == execution.len(),
                    "The number of transitions in the execution is incorrect. Expected {minimum_number_of_calls}, but found {}",
                    execution.len()
                );
            }

            // Output the locator of the main function.
            Locator::new(*transition.program_id(), *transition.function_name()).to_string()
        };
        lap!(timer, "Verify the number of transitions");

        // Construct the call graph of the execution.
        let call_graph = self.construct_call_graph(execution.transitions())?;

        // From ConsensusVersion::V14 onwards, ensure non-static records exist on the ledger.
        if consensus_version >= ConsensusVersion::V14 {
            self.ensure_records_exist(execution.transitions(), call_graph.clone())?;
        }

        // Construct the reverse call graph of the execution.
        // Note: This is a mapping of the child transition ID to the parent transition ID.
        let reverse_call_graph = Self::reverse_call_graph(&call_graph);

        // Initialize a map of verifying keys to public inputs.
        let mut verifier_inputs = HashMap::with_capacity(execution.transitions().len());

        // Initialize a map of transition IDs to references of the transition.
        let mut transition_map = HashMap::with_capacity(execution.transitions().len());

        // Verify each transition.
        for transition in execution.transitions() {
            dev_println!("Verifying transition for {}/{}...", transition.program_id(), transition.function_name());
            // Debug-mode only, as the `Transition` constructor recomputes the transition ID at initialization.
            let expected_id = N::hash_bhp512(&(transition.to_root()?, *transition.tcm()).to_bits_le())?;
            debug_assert_eq!(**transition.id(), expected_id, "The transition ID is incorrect");

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
            for (function_input, transition_input) in function.input_types().iter().zip_eq(transition.inputs().iter()) {
                ensure!(
                    transition_input.is_type(function_input),
                    "Input variant mismatch: expected '{function_input}', found '{transition_input}'",
                );
            }
            ensure!(
                function.output_types().len() == transition.outputs().len(),
                "The number of transition outputs is incorrect"
            );
            for (output_index, (function_output, transition_output)) in
                function.output_types().iter().zip_eq(transition.outputs().iter()).enumerate()
            {
                ensure!(
                    transition_output.is_type(function_output),
                    "Output variant mismatch at index {output_index} in '{}/{}': expected '{function_output}', found '{transition_output}'",
                    transition.program_id(),
                    transition.function_name(),
                );
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
            &|(program_id, record_name)| {
                self.get_stack(program_id).and_then(|stack| stack.get_verifying_key(record_name))
            },
        )?;
        // Ensure `Authorization::translation_batches` matches the number of translations.
        // We only compare the totals because `Authorization::translation_batches` does not preserve order.
        // Note that in general the prover and verifier agree on order through the use of `translation_index`.
        let expected_n_translations =
            Authorization::translation_batches(self, execution.transitions())?.into_iter().sum::<usize>();
        let actual_n_translations = batch_translation_inputs.iter().map(|(_, inputs)| inputs.len()).sum::<usize>();
        ensure!(
            actual_n_translations == expected_n_translations,
            "Unexpected number of translation inputs: {actual_n_translations} instead of {expected_n_translations}",
        );

        for (verifying_key, batch_translation_inputs_for_record) in batch_translation_inputs.into_iter() {
            // Retrieve the number of public and private variables.
            // Note: This number does *NOT* include the number of constants. This is safe because
            // this program is never deployed, as it is a first-class citizen of the protocol.
            // TODO (@vicsn) should this be used?
            let _num_variables = verifying_key.circuit_info.num_public_and_private_variables as u64;
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
        // Retrieve the network ID.
        let network_id = U16::new(N::ID);

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
        let child_transition_ids = call_graph.get(transition.id()).ok_or_else(|| {
            anyhow!(
                "Child transition IDs not found for transition {} (function: {})",
                transition.id(),
                transition.function_name()
            )
        })?;

        // Retrieve the parent function from the transition map.
        let parent_function = transition_map.get(transition.id()).map(|(_, function)| function.clone());

        // Collect the function call instructions from the parent function.
        // Each entry is (is_dynamic, instruction), where `is_dynamic` indicates a `call.dynamic`.
        let parent_function_calls = match parent_function {
            Some(function) => {
                let mut calls = Vec::new();
                for instruction in function.instructions() {
                    match instruction {
                        // Dynamic calls (`call.dynamic`) are always function calls and contribute to the call graph.
                        Instruction::CallDynamic(..) => calls.push((true, instruction.clone())),
                        // Static calls (`call`) are included only if they invoke a function (not a closure).
                        // Closures are inlined and do not produce separate transitions.
                        Instruction::Call(call) => {
                            // Retrieve the stack for the current program.
                            let stack = self.get_stack(transition.program_id())?;
                            // Check if this call invokes a function (as opposed to a closure).
                            match call.is_function_call(stack.as_ref()) {
                                Ok(true) => calls.push((false, instruction.clone())),
                                Ok(false) => { /* Closure call - skip */ }
                                Err(e) => bail!("Failed to determine if call is a function call: {e}"),
                            }
                        }
                        // All other instruction types (arithmetic, hashing, casting, etc.) are not function calls.
                        _ => {}
                    }
                }
                calls
            }
            // This should never occur, since `call_graph` and `transition_map` are populated together.
            None => bail!("Function not found for transition {} ({})", transition.id(), transition.function_name()),
        };

        ensure!(
            parent_function_calls.len() == child_transition_ids.len(),
            "The number of parent function calls ({}) and child transition IDs ({}) do not match",
            parent_function_calls.len(),
            child_transition_ids.len()
        );

        for (child_transition_id, (is_dynamic_call, call_instruction)) in
            child_transition_ids.iter().zip(parent_function_calls)
        {
            // Note: This unwrap is safe, as we are processing transitions in post-order,
            // which implies that all child transition IDs have been added to `transition_map`.
            let (child_transition, _) = transition_map.get(child_transition_id).unwrap();

            // Compute the function ID for the child transition.
            let child_function_id =
                compute_function_id(&network_id, child_transition.program_id(), child_transition.function_name())?;

            // Extract the `CallDynamic` instruction data, if this is a dynamic call.
            let call_dynamic = match call_instruction {
                Instruction::CallDynamic(cd) => Some(cd),
                Instruction::Call(..) => None,
                // This should never occur, since `parent_function_calls` only contains `Call` and `CallDynamic`.
                _ => bail!("Unexpected instruction type in parent function calls"),
            };

            // [Inputs] Extend the verifier inputs with the program ID and function name if the child transition is dynamic.
            if is_dynamic_call {
                verifier_inputs.extend(child_transition.program_id().to_fields()?.into_iter().map(|field| *field));
                verifier_inputs.extend([*child_transition.function_name().to_field()?]);
                verifier_inputs.extend([*child_function_id]);
            }
            // [Inputs] Extend the verifier inputs with the transition commitment of the external call.
            verifier_inputs.extend([**child_transition.tcm()]);

            // [Inputs] Extend the verifier inputs with the input IDs of the external call.
            let num_inputs = child_transition.inputs().len();
            if is_dynamic_call {
                // For dynamic calls, use the dynamic ID when present, otherwise use normal verifier inputs.
                // Note: This unwrap is safe because `is_dynamic_call` is true only for `CallDynamic` instructions.
                let call_dynamic_ref = call_dynamic.as_ref().unwrap();
                let operand_types = call_dynamic_ref.operand_types();
                ensure!(
                    child_transition.inputs().len() == operand_types.len(),
                    "The number of inputs ({}) and dynamic call operand types ({}) do not match",
                    child_transition.inputs().len(),
                    operand_types.len(),
                );
                for (i, (input, input_type)) in
                    child_transition.inputs().iter().zip_eq(operand_types.iter()).enumerate()
                {
                    // Ensure the input is not a plain Record or ExternalRecord.
                    // Dynamic calls must use the `*WithDynamicID` variants for record inputs.
                    ensure!(
                        !matches!(input, Input::Record(..) | Input::ExternalRecord(..)),
                        "Input {i} in dynamic call to {} must not be a plain Record or ExternalRecord, found: {}",
                        child_transition.function_name(),
                        input,
                    );
                    // Ensure the input type matches the caller's expectation.
                    // Use the caller's view of the input (e.g., RecordWithDynamicID -> DynamicRecord).
                    ensure!(
                        input.to_caller_input().is_type(input_type),
                        "Input {i} in dynamic call to {} should be of type {}, found: {}",
                        child_transition.function_name(),
                        input_type,
                        input,
                    );
                    // Extend the verifier inputs based on the input variant.
                    match input {
                        // Inputs with a dynamic ID contribute only the dynamic ID.
                        Input::DynamicRecord(dynamic_id)
                        | Input::RecordWithDynamicID(_, _, dynamic_id)
                        | Input::ExternalRecordWithDynamicID(_, dynamic_id) => {
                            verifier_inputs.push(**dynamic_id);
                        }
                        // All other inputs contribute their standard verifier inputs.
                        Input::Constant(..) | Input::Public(..) | Input::Private(..) => {
                            verifier_inputs.extend(input.verifier_inputs());
                        }
                        // Record and ExternalRecord are excluded above.
                        Input::Record(..) | Input::ExternalRecord(..) => {
                            unreachable!("Record and ExternalRecord are excluded above")
                        }
                    }
                }
            } else {
                verifier_inputs.extend(child_transition.inputs().iter().flat_map(|input| input.verifier_inputs()));
            }

            // [Outputs] Extend the verifier inputs with the output IDs of the external call.
            if is_dynamic_call {
                // Note: This unwrap is safe because `is_dynamic_call` is true only for `CallDynamic` instructions.
                let call_dynamic_ref = call_dynamic.as_ref().unwrap();
                let destination_types = call_dynamic_ref.destination_types();
                ensure!(
                    child_transition.outputs().len() == destination_types.len(),
                    "The number of outputs ({}) and dynamic call destination types ({}) do not match",
                    child_transition.outputs().len(),
                    destination_types.len(),
                );
                for (index, (output, destination_type)) in
                    child_transition.outputs().iter().zip_eq(destination_types.iter()).enumerate()
                {
                    match (output, destination_type) {
                        // A `DynamicFuture` output: the verifier computes the hash of the dynamic future directly.
                        (Output::Future(_id, future), ValueType::DynamicFuture) => {
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
                            verifier_inputs.push(*dynamic_future_id);
                        }
                        // Outputs with a dynamic ID contribute only the dynamic ID.
                        (Output::DynamicRecord(dynamic_id), _)
                        | (Output::RecordWithDynamicID(_, _, _, _, dynamic_id), _)
                        | (Output::ExternalRecordWithDynamicID(_, dynamic_id), _) => {
                            ensure!(
                                output.to_caller_output().is_type(destination_type),
                                "Output {index} in dynamic call to {} should be of type {}, found: {}",
                                child_transition.function_name(),
                                destination_type,
                                output,
                            );
                            verifier_inputs.push(**dynamic_id);
                        }
                        // All other outputs contribute their standard output ID.
                        (Output::Constant(..), _)
                        | (Output::Public(..), _)
                        | (Output::Private(..), _)
                        | (Output::Record(..), _)
                        | (Output::ExternalRecord(..), _)
                        | (Output::Future(..), _) => {
                            ensure!(
                                output.to_caller_output().is_type(destination_type),
                                "Output {index} in dynamic call to {} should be of type {}, found: {}",
                                child_transition.function_name(),
                                destination_type,
                                output,
                            );
                            verifier_inputs.push(**output.id());
                        }
                    }
                }
            } else {
                verifier_inputs.extend(child_transition.output_ids().map(|id| **id));
            }
        }

        // [Outputs] Extend the verifier inputs with the output IDs.
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
    pub(crate) fn reverse_call_graph(
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

    /// Checks that, for each function in the execution (whether transition or
    /// closure), each `ExternalRecord` and `DynamicRecord` received as an input
    /// or from a callee corresponds to a static `Record` that exists on the
    /// ledger at the end of the execution (whether spent or not).
    ///
    /// Input `transitions`: Iterator over the transitions in the execution. The
    /// root transition must be last.
    ///
    /// Input `call_graph`: A copy of the call graph (which will be modified in
    /// place). It is assumed to contain all transitions in `transitions`. All
    /// children of a given Transition ID must appear in the same order as the
    /// corresponding calls happen in the function.
    pub fn ensure_records_exist<'a>(
        &self,
        transitions: impl ExactSizeIterator<Item = &'a Transition<N>> + DoubleEndedIterator + Clone,
        mut call_graph: HashMap<N::TransitionID, Vec<N::TransitionID>>,
    ) -> Result<()> {
        let mut register_families: Vec<IndexSet<(N::TransitionID, u64)>> = Vec::new();

        let root_transition = transitions.clone().last().ok_or_else(|| anyhow!("Empty transition list"))?;

        let tid_to_transition = transitions
            .clone()
            .map(|transition| (*transition.id(), transition))
            .collect::<HashMap<N::TransitionID, &Transition<N>>>();

        // Recursively explore the execution, keeping track of record relations across the relevant casts and calls.
        self.process_transition(
            &mut register_families,
            root_transition.id(),
            None,
            &tid_to_transition,
            &mut call_graph,
        )?;

        // Sanity check: exploration should have consumed all calls in all functions.
        for (parent, children) in call_graph {
            if !children.is_empty() {
                let caller_transition = tid_to_transition
                    .get(&parent)
                    .ok_or_else(|| anyhow!("Missing caller transition with ID {parent}"))?;
                bail!(
                    "Entry for Transition ID {parent} ({}/{}) in the call graph has unprocessed children",
                    caller_transition.program_id(),
                    caller_transition.function_name(),
                );
            }
        }

        if register_families.is_empty() {
            Ok(())
        } else {
            let non_existing_register = register_families[0][0];
            let root_program = root_transition.program_id();
            let root_function = root_transition.function_name();

            Err(anyhow!(
                "Non-static record input at register r{} of function {}/{} is not known to correspond to a record on the ledger",
                non_existing_register.1,
                root_program,
                root_function
            ))
        }
    }

    // Auxiliary function for `ensure_records_exist` that connects the relevant
    // record families of the given transition, tracking linked and connected
    // records. Furthermore, it also checks that locally minted records which
    // should materialise are output.
    fn process_transition(
        &self,
        register_families: &mut Vec<IndexSet<(N::TransitionID, u64)>>,
        transition_id: &N::TransitionID,
        // For non-root transitions, Some containing:
        //  - transition ID of the caller
        //  - indices of the caller input registers, with None for inputs that are not registers
        //  - indices of the caller output registers
        caller_info: Option<(N::TransitionID, Vec<Option<u64>>, Vec<u64>)>,
        tid_to_transition: &HashMap<N::TransitionID, &Transition<N>>,
        call_graph: &mut HashMap<N::TransitionID, Vec<N::TransitionID>>,
    ) -> Result<()> {
        let transition = tid_to_transition
            .get(transition_id)
            .ok_or_else(|| anyhow!("Missing transition with ID {transition_id}"))?;
        let stack = self.get_stack(transition.program_id())?;
        let function = stack.get_function(transition.function_name())?;
        let locator = Locator::new(*transition.program_id(), *transition.function_name());

        let inputs = transition.inputs();
        let input_registers = function.inputs().iter().map(|input| input.register().locator()).collect::<Vec<u64>>();

        // Contains the registers of static Records minted locally. Used for the local check.
        let mut locally_minted_static = HashSet::new();

        // For each instruction which casts a static Record at register r_i into a DynamicRecord at register r_j,
        // if r_i was minted locally, this map contains an entry r_j -> r_i. Used for the local check.
        let mut locally_minted_dynamic = HashMap::new();

        // Contains the locally minted static records which must be output because they are cast to dynamic
        // and passed to a call or output. Used for the local check.
        let mut must_be_output = HashSet::new();

        // Processing the inputs
        if let Some((caller_tid, caller_input_registers, _)) = &caller_info {
            // Non-root transition case

            ensure!(
                inputs.len() == input_registers.len() && inputs.len() == caller_input_registers.len(),
                "Mismatch in the number of callee/caller inputs and registers in call to {} (transition ID {})",
                transition.function_name(),
                transition_id,
            );

            for (caller_input_register_opt, callee_input_register, callee_input) in
                izip!(caller_input_registers, input_registers, inputs)
            {
                if let Some(caller_input_register) = caller_input_register_opt {
                    match callee_input {
                        Input::RecordWithDynamicID(..) => {
                            Self::mark_existing(register_families, (*caller_tid, *caller_input_register));
                        }
                        Input::ExternalRecord(..)
                        | Input::ExternalRecordWithDynamicID(..)
                        | Input::DynamicRecord(..) => {
                            let old_register = (*caller_tid, *caller_input_register);
                            let new_register = (*transition_id, callee_input_register);
                            Self::add_to_family(register_families, old_register, new_register);
                        }
                        _ => {}
                    }
                }
            }
        } else {
            // Root transition case

            ensure!(
                input_registers.len() == transition.inputs().len(),
                "Mismatch in the number of inputs and registers in the root call"
            );

            for (input, register) in transition.inputs().iter().zip(input_registers.iter()) {
                if matches!(input, Input::DynamicRecord(..) | Input::ExternalRecord(..)) {
                    register_families.push(IndexSet::from_iter([(*transition_id, *register)]));
                }
            }

            // Early return if the root transition does not receive non-static records
            if register_families.is_empty() {
                return Ok(());
            }
        }

        for instruction in function.instructions() {
            match instruction {
                Instruction::Cast(cast) => {
                    match cast.cast_type() {
                        CastType::DynamicRecord => {
                            let operand_register = match cast.operands().first() {
                                Some(Operand::Register(register)) => register.locator(),
                                _ => bail!("Failed to retrieve operand register for cast to DynamicRecord instruction"),
                            };

                            let destination_register = cast.destinations()[0].locator();

                            let old_register = (*transition_id, operand_register);
                            let new_register = (*transition_id, destination_register);

                            // Since static records never exist in any family and add_to_family only adds the new record if the
                            // old record exists in some family, this call only handles the external-to-dynamic case, as desired.
                            Self::add_to_family(register_families, old_register, new_register);

                            // If the operand is a locally minted static record, keep track of this cast for the local check.
                            if locally_minted_static.contains(&operand_register) {
                                locally_minted_dynamic.insert(destination_register, operand_register);
                            }
                        }
                        CastType::Record(_) => {
                            locally_minted_static.insert(cast.destinations()[0].locator());
                        }
                        _ => {}
                    }
                }
                Instruction::Call(..) | Instruction::CallDynamic(..) => {
                    let caller_input_operands = if matches!(instruction, Instruction::Call(..)) {
                        instruction.operands()
                    } else {
                        &instruction.operands()[3..]
                    };

                    let caller_input_registers: Vec<Option<u64>> =
                        caller_input_operands
                            .iter()
                            .map(|operand| {
                                if let Operand::Register(register) = operand { Some(register.locator()) } else { None }
                            })
                            .collect();

                    let caller_output_registers =
                        instruction.destinations().iter().map(|destination| destination.locator()).collect();

                    if let Instruction::Call(call) = instruction
                        && !call.is_function_call(stack.as_ref())?
                    {
                        // Closure case
                        let closure = {
                            let operator = call.operator();
                            match operator {
                                CallOperator::Resource(closure_identifier) => {
                                    // Local closure call
                                    self.get_stack(transition.program_id())?.get_function(closure_identifier)?
                                }
                                CallOperator::Locator(external_locator) => {
                                    // External closure call
                                    self.get_stack(external_locator.program_id())?
                                        .get_function(external_locator.resource())?
                                }
                            }
                        };

                        self.process_closure(
                            transition_id,
                            &closure,
                            register_families,
                            &locally_minted_static,
                            &mut locally_minted_dynamic,
                            &caller_input_registers,
                            &caller_output_registers,
                        )?;
                    } else {
                        // Function case
                        let remaining_children = call_graph.get_mut(transition_id).unwrap();

                        ensure!(
                            !remaining_children.is_empty(),
                            "Entry with Transition ID {transition_id} ({locator}) in the call graph has fewer elements than the number of calls in the corresponding function",
                        );

                        let tid_callee = remaining_children.remove(0);

                        for input_register in caller_input_operands.iter() {
                            if let Operand::Register(register) = input_register {
                                let register_index = register.locator();
                                // Any dynamic records which are passed to a (non-closure) call and come from locally minted
                                // static records must be output. This is part of the local check.
                                if let Some(static_record) = locally_minted_dynamic.get(&register_index) {
                                    must_be_output.insert(*static_record);
                                }
                                // Furthermore, any static records which are passed to (non-closure) calls (necessarily as
                                // external records) must be output.
                                if locally_minted_static.contains(&register_index) {
                                    must_be_output.insert(register_index);
                                }
                            }
                        }

                        self.process_transition(
                            register_families,
                            &tid_callee,
                            Some((*transition_id, caller_input_registers, caller_output_registers)),
                            tid_to_transition,
                            call_graph,
                        )?;
                    }
                }
                _ => {}
            }
        }

        // Processing the outputs

        // Track which dynamic records coming from locally minted static records are output.
        function.outputs().iter().for_each(|output| {
            if let Operand::Register(output_register) = output.operand() {
                if let Some(static_record) = locally_minted_dynamic.get(&output_register.locator()) {
                    must_be_output.insert(*static_record);
                }
            }
        });

        // In a second pass, ensure all static records which must be output (according to the local check) are so
        function.outputs().iter().for_each(|output| {
            if let Operand::Register(output_register) = output.operand() {
                let _ = must_be_output.remove(&output_register.locator());
            }
        });

        ensure!(
            must_be_output.is_empty(),
            "In function {}, Some dynamic records which are passed to a call or are output refer to locally minted static records which are not output. Static-record registers: {:?}",
            function.name(),
            must_be_output
        );

        // For non-root calls, keep track of record families
        if let Some((caller_tid, _, caller_output_registers)) = &caller_info {
            let outputs = transition.outputs();

            ensure!(
                outputs.len() == caller_output_registers.len() && outputs.len() == function.outputs().len(),
                "Mismatch in the number of callee/caller outputs and registers in call to {} (transition ID {})",
                transition.function_name(),
                transition_id,
            );

            for (caller_output_register, callee_output_operand, callee_output) in
                izip!(caller_output_registers, function.outputs(), outputs)
            {
                if let Operand::Register(callee_output_register) = callee_output_operand.operand()
                    && matches!(callee_output, Output::ExternalRecord(..) | Output::DynamicRecord(..))
                {
                    let old_register = (*transition_id, callee_output_register.locator());
                    let new_register = (*caller_tid, *caller_output_register);
                    Self::add_to_family(register_families, old_register, new_register);
                }
            }
        }

        Ok(())
    }

    // Auxiliary function for `ensure_records_exist` which processes a closure.
    // The caller function's `record_families` (global-check tracking) and
    // `locally_minted_dynamic` (local-check tracking) are updated taking into
    // account the cast instructions in the closure as well as its input-output
    // relations. Furthermore, this function ensures the closure does not output
    // any DynamicRecords cast from locally minted static Records.
    fn process_closure(
        &self,
        // TransitionID of the caller function
        caller_tid: &N::TransitionID,
        // Closure being processed
        closure: &FunctionCore<N>,
        // Families of registers being tracked of as part of the caller's global existence check
        caller_register_families: &mut [IndexSet<(N::TransitionID, u64)>],
        // (Caller) registers of static Records minted in the caller function
        caller_locally_minted_static: &HashSet<u64>,
        // Map from DynamicRecord registers to the locally minted static Record registers they were cast from (in the caller's view)
        caller_locally_minted_dynamic: &mut HashMap<u64, u64>,
        // Caller registers of inputs to the closure call (`None` for inputs that are not registers)
        caller_input_registers: &Vec<Option<u64>>,
        // Caller registers of outputs of the closure call
        caller_output_registers: &Vec<u64>,
    ) -> Result<()> {
        // Sets of registers of static Records minted locally in the closure
        let mut callee_locally_minted_static = HashSet::new();

        ensure!(
            caller_input_registers.len() == closure.inputs().len()
                && caller_input_registers.len() == closure.input_types().len(),
            "Mismatch in the number of caller/callee inputs types and registers in call to closure {}",
            closure.name()
        );
        ensure!(
            caller_output_registers.len() == closure.outputs().len()
                && caller_output_registers.len() == closure.output_types().len(),
            "Mismatch in the number of caller/callee output types  and registers in call to closure {}",
            closure.name()
        );

        // Construct a map { callee register -> caller register } for the closure's inputs (Record, DynamicRecord or ExternalRecord)
        let input_map = izip!(caller_input_registers, closure.inputs(), closure.input_types())
            .filter_map(|(caller_input_register_opt, closure_input, closure_input_type)| {
                if matches!(
                    closure_input_type,
                    ValueType::Record(..) | ValueType::DynamicRecord | ValueType::ExternalRecord(..)
                ) {
                    if let Some(caller_input_register) = caller_input_register_opt {
                        Some(Ok((closure_input.register().locator(), *caller_input_register)))
                    } else {
                        Some(Err(anyhow!(
                            "Missing register information for the caller input to closure {}",
                            closure.name()
                        )))
                    }
                } else {
                    None
                }
            })
            .collect::<Result<HashMap<u64, u64>>>()?;

        // Construct a map { callee register -> caller register } for the closure's outputs (DynamicRecord or ExternalRecord - closures cannot output Records)
        let output_map = izip!(caller_output_registers, closure.outputs(), closure.output_types())
            .filter_map(|(caller_output_register, closure_output, closure_output_type)| {
                if matches!(closure_output_type, ValueType::DynamicRecord | ValueType::ExternalRecord(..)) {
                    if let Operand::Register(register) = closure_output.operand() {
                        Some(Ok((register.locator(), *caller_output_register)))
                    } else {
                        Some(Err(anyhow!("Missing output register information in closure {}", closure.name())))
                    }
                } else {
                    None
                }
            })
            .collect::<Result<HashMap<u64, u64>>>()?;

        for instruction in closure.instructions() {
            // Only cast instructions need to be processed at this stage.
            if let Instruction::Cast(cast) = instruction {
                match cast.cast_type() {
                    CastType::Record(..) => {
                        // Case 1: minting a static Record locally. We keep track to ensure DynamicRecords cast from it are not output.
                        let destination_register = instruction.destinations()[0].locator();
                        callee_locally_minted_static.insert(destination_register);
                    }
                    CastType::DynamicRecord => {
                        let operand_register = match cast.operands().first() {
                            Some(Operand::Register(register)) => register.locator(),
                            _ => bail!(
                                "Failed to retrieve operand register for cast to DynamicRecord instruction in closure {}",
                                closure.name()
                            ),
                        };

                        let destination_register = match cast.destinations().first() {
                            Some(destination) => destination.locator(),
                            _ => bail!(
                                "Failed to retrieve destination register for cast to DynamicRecord instruction in closure {}",
                                closure.name()
                            ),
                        };

                        if callee_locally_minted_static.contains(&operand_register) {
                            // Case 2: Casting a locally minted static Record to a DynamicRecord. We ensure the latter is not output.
                            if output_map.contains_key(&destination_register) {
                                bail!(
                                    "Closure {} attempts to output dynamic record at {destination_register} cast from locally minted Record at {operand_register}",
                                    closure.name()
                                );
                            }
                        } else {
                            // In this case, the input to the Cast instruction is necessarily an input to the closure itself. We retrieve its caller register.
                            let caller_input_register = input_map.get(&operand_register).ok_or_else(|| anyhow!("Missing caller input register for Cast instruction from register {operand_register} in closure {}", closure.name()))?;

                            // We only need to process this cast instruction if the destination register is output by the closure.
                            if let Some(caller_output_register) = output_map.get(&destination_register) {
                                if caller_locally_minted_static.contains(caller_input_register) {
                                    // Case 3: Effectively casting performing a static-to-dynamic cast on the caller. We update the caller's local-check tracking accordingly. Note the input operand in the closure could still be an ExternalRecord (if the call to the closure is external)
                                    caller_locally_minted_dynamic
                                        .insert(*caller_output_register, *caller_input_register);
                                } else {
                                    // Case 4: Casting a value already received as a Record or ExternalRecord input by the caller itself. In the Record case, nothing was being kept track of. In the ExternalRecord case, we inform the caller's global check of the relation between the two registers.
                                    let old_register = (*caller_tid, *caller_input_register);
                                    let new_register = (*caller_tid, destination_register);
                                    Self::add_to_family(caller_register_families, old_register, new_register);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Detecting effective remappings of caller registers resulting from closure input-output relations not involving casts
        for (callee_input_register, caller_input_register) in input_map {
            if let Some(caller_output_register) = output_map.get(&callee_input_register) {
                // Caller global-check update (only adds the new register if the old one is in some family)
                let old_register = (*caller_tid, caller_input_register);
                let new_register = (*caller_tid, *caller_output_register);
                Self::add_to_family(caller_register_families, old_register, new_register);

                // Caller local-check update
                if let Some(original_static) = caller_locally_minted_dynamic.get(&caller_input_register) {
                    caller_locally_minted_dynamic.insert(*caller_output_register, *original_static);
                }
            }
        }

        Ok(())
    }

    // Auxiliary function for ensure_records_exist that adds a record to the set
    // containing another record, of any, thus connecting their linking status.
    // A debug_assert ensures that at most one family contains each of the given
    // records.
    fn add_to_family(
        register_families: &mut [IndexSet<(N::TransitionID, u64)>],
        old_register: (N::TransitionID, u64),
        new_register: (N::TransitionID, u64),
    ) {
        for record in [old_register, new_register] {
            debug_assert!(
                register_families.iter().filter(|family| family.contains(&record)).count() <= 1,
                "Multiple families contain register {} for transition ID {}",
                record.1,
                record.0
            );
        }

        let family = register_families.iter_mut().find(|family| family.contains(&old_register));

        if let Some(found_family) = family {
            found_family.insert(new_register);
        }
    }

    // Auxiliary function for ensure_records_exist that removes the set
    // containing a given record, if any, from register_families, thus
    // implicitly marking it as existing. A debug_assert ensures that at most
    // one family contains the given record.
    fn mark_existing(register_families: &mut Vec<IndexSet<(N::TransitionID, u64)>>, record: (N::TransitionID, u64)) {
        debug_assert!(
            register_families.iter().filter(|family| family.contains(&record)).count() <= 1,
            "Multiple families contain register {} for transition ID {}",
            record.1,
            record.0
        );

        let family = register_families.iter().position(|family| family.contains(&record));

        if let Some(family_index) = family {
            register_families.remove(family_index);
        }
    }
}
