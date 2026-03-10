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

// The checks in this file ensure that every Transition t in the given execution satisfies the following:
//
// Guarantee (G): Every non-static record (DynamicRecord or ExternalRecord) input to t or received by t from a
// *function* callee is "connected" to a static Record which exists on the ledger at the end of the full execution
// (whether consumed or not).
//
// Here "connected" refers to the transitive relation between two or more record-like (Record, DynamicRecord or
// ExternalRecord) registers that arises when one of the following happens:
// - two of them coincide at a function or closure input or output boundary
// - two of them are related by a cast-to-dynamic instruction
//
// The following is *required* of every function and closure in the execution:
//
// Contract (C): Assuming every non-static record received as an input or received from a (necessarily function)
// callee exists on the ledger at the end of the execution (whether consumed or not), every record-like register
// output or passed to a *function* callee (but not necessarily to a closure) exists on the ledger at the end of
// the full execution.
//
// For instance, it is okay to output a DynamicRecord received from a function callee or as an input.
//
// (G) and (C) can be ensured as follows:
//
//   1. Keeping track of each DynamicRecord and ExternalRecord register input to the root call as it travels across
//   function and closure input or output boundaries and cast-to-dynamic (in the external case) instructions, and
//   ensuring that one register connected to it is input to a function receiving it as a static Record (thus
//   consuming it). We refer to this as the *global check*.
//
//   Materialization of non-static recordds input to the root call cannot occur at an output boundary.
//
//   2. Local check for functions: ensuring that, in each transition t in the execution, if a static Record R_s is
//   minted locally and cast to a DynamicRecord R_d, if a record-like register connected to R_d is output by t or
//   passed to a child *transition* (i.e. function, not closure) of t, then R_s or a static-Record register connected
//   to it (necessarily via an external closure call) is also output by t. We refer to this as the *local check*.
//
//   3. Local check for closures (note that closures cannot output static Record s or call other functions or
//   closures): ensuring that, for each closure c in the execution, no DynamicRecords cast from static Records
//   minted in c are output (by c).
//
// ensure_records_exist explores the execution tree recursively starting at the root Transition and processing
// instructions (input/output boundaries, closure calls, function calls and Record mints and cast-to-dynamic
// instructions) in order of execution.
//
// - When a function call is encountered, the function process_transition is called. This initialises a
//   FunctionLocalCheck struct which keeps track of which DynamicRecords cast from locally minted static Records
//   are passed to callee functions or output.
//
//     If the function corresponds to the root transition, a GlobalCheck is also initialised with one singleton
//     family per non-static Record input. It is passed across all recursive calls.
//
//     process_transition processes the following situations:
//
//     - Case 1: RecordWithDynamicID and Record registers input to the function, whose families are marked as
//       existing if they are being tracked (global check)
//     - Case 2: connections between the caller registers where call inputs are passed and the callee's input
//       registers (global check).
//     - Case 3: cast instruction minting a static Record (local check)
//     - Case 4: casts of ExternalRecords to DynamicRecords (global check; note an external record cannot be
//       minted locally)
//     - Case 5: casts of locally minted static Records to DynamicRecords (local check)
//     - Call instructions (static or dynamic calls to functions, static calls to closures), in which case
//       process_function and process_closure are called accordingly. In particular
//         - Case 6a: DynamicRecords cast from locally minted static Records and passed to a function call (local
//           check)
//         - Case 6b: Static-Record registers connected to a locally minted static Record and passed to a function
//           call (local check)
//     - Case 7: DynamicRecords cast from locally minted static Records and output (local check)
//     - Case 8: Connections between the caller registers where call outputs are received and call and the callee's
//       output registers (global check).
//     - Checking that, for each static Record R_s which must be output because of the last two points, either R_s or a
//       connected static-Record register is output.
//
// - When a closure call is encountered, the function process_closure is called, passing it GlobalCheck and the
//   caller's FunctionLocalCheck (note that a closure can affect the local check of its caller - for instance, if
//   it receives a DynamicRecord and outputs it directly; or if it casts a static Record Rs received as an input
//   to a dynamic one Rd and outputs the latter).
//
//     process_closure acts directly on the GlobalCheck and the callers FunctionLocalCheck *in terms of the
//     caller's registers* instead of creating register connections that refer specifically to its own records. It
//     also instantiates a ClosureLocalCheck which keeps track of the simpler closure version of the local check.
//
//     process_closure processes the following situations:
//
//     - Case 1: cast instruction minting a static Record (closure's local check)
//     - Case 2: casts of locally minted static Records to DynamicRecords, the latter of which cannot be output
//       (closure's local check)
//     - Case 3: casts of static Records minted locally in the caller and received from it to DynamicRecords
//       returned to the caller (caller function's local check)
//     - Case 4: casts of ExternalRecords which were input to the caller itself, to DynamicRecords returned to
//       the caller (global check)
//     - Effective remappings of caller registers due to the input/output declaration
//         - Case 5: Adding the caller's output register to the family containing the call's input register, if any
//           (global check)
//         - Case 6: Adding a cast-static-to-dynamic conversion R_s -> R_d' if a DynamicRecord at register R_d
//           cast from a static Record at register R_s minted in the caller function was passed to the closure and
//           received at a different register R_d'. (caller function's local check)
//         - Case 7: Adding a connection between (caller-side) static-Record registers connected to a static Record
//           minted in the caller.

// Structure that keeps track of the global record-existence check of an execution, i.e. that each DynamicRecord and
// ExternalRecord input to the root transition corresponds to a static Record that exists on the ledger at the end of
// the execution.
struct GlobalCheck<N: Network> {
    // Each element of the vector corresponds to a family of registers that are connected to a DynamicRecord or
    // ExternalRecord input to the root transition. Once a family is known to exist on the ledger, it is removed from
    // the vector.
    families: Vec<IndexSet<(N::TransitionID, u64)>>,
}

impl<N: Network> GlobalCheck<N> {
    // Initialises the global check with an empty collection of register families. Families are created only at one
    // point of the existence check: when processing the inputs to the root transition.
    fn new() -> Self {
        Self { families: Vec::new() }
    }

    // Adds a singleton family containing a given register.
    fn add_family(&mut self, register: (N::TransitionID, u64)) {
        self.families.push(IndexSet::from_iter([register]));
    }

    // Adds a register to the family containing another register, if any, thus connecting their existence status.
    fn add_to_family(&mut self, old_register: (N::TransitionID, u64), new_register: (N::TransitionID, u64)) {
        // Sanity check: at most one family contains each of the given registers.
        for record in [old_register, new_register] {
            debug_assert!(
                self.families.iter().filter(|family| family.contains(&record)).count() <= 1,
                "Multiple families contain register {} for transition ID {}",
                record.1,
                record.0
            );
        }

        let family = self.families.iter_mut().find(|family| family.contains(&old_register));

        if let Some(found_family) = family {
            found_family.insert(new_register);
        }
    }

    // Marks the family containing a given record-like register as existing on the ledger by removing it from the
    // vector of families.
    fn mark_existing(&mut self, register: (N::TransitionID, u64)) {
        // Sanity check: at most one family contains the given register,
        debug_assert!(
            self.families.iter().filter(|family| family.contains(&register)).count() <= 1,
            "Multiple families contain register {} for transition ID {}",
            register.1,
            register.0
        );

        let family = self.families.iter().position(|family| family.contains(&register));

        if let Some(family_index) = family {
            self.families.swap_remove(family_index);
        }
    }

    // Returns the families of registers that are not known to correspond to static Records on the ledger. This is only
    // called once in the entire existence check, at the very end.
    fn non_existent_families(&self) -> &[IndexSet<(N::TransitionID, u64)>] {
        &self.families
    }
}

// Auxiliary structure for the local record-existence check of a function, i.e. the check that, for each DynamicRecord
// cast from a locally minted static Record and passed to a non-closure call or output, the static Record is output.
struct FunctionLocalCheck {
    // Each element of this vector is a set containing a locally minted static Record and the local static-Record
    // registers connected to it via an external closure call.
    connected_locally_minted_static: Vec<HashSet<u64>>,
    // For each DynamicRecord at r_j cast from a locally minted static Record at r_i, this map contains the entry
    // r_j -> r_i. This also takes into account static-to-dynamic casts occurring in child closures and effective
    // remappings of DynamicRecord registers caused by closure calls.
    locally_minted_dynamic: HashMap<u64, u64>,
    // Contains the registers of locally minted static Records which must be output because they are cast to dynamic
    // and passed to a non-closure call or output.
    must_be_output: HashSet<u64>,
}

impl FunctionLocalCheck {
    // Initialises a function's local check.
    fn new() -> Self {
        Self {
            connected_locally_minted_static: Vec::new(),
            locally_minted_dynamic: HashMap::new(),
            must_be_output: HashSet::new(),
        }
    }

    // Adds a locally minted static Record to the local check.
    fn mint_static(&mut self, register: u64) {
        self.connected_locally_minted_static.push(HashSet::from_iter([register]));
    }

    // Tracks the information about a static-to-dynamic cast if the operand corresponds to a locally minted static
    // Record.
    fn cast_to_dynamic(&mut self, operand_register: u64, destination_register: u64) {
        if self.is_locally_minted_static(operand_register) {
            self.locally_minted_dynamic.insert(destination_register, operand_register);
        }
    }

    // Returns whether a given register corresponds to a static Record minted in the function or to a register remapped
    // to it via an external closure call.
    fn is_locally_minted_static(&self, register: u64) -> bool {
        self.connected_locally_minted_static.iter().any(|set| set.contains(&register))
    }

    // If `old_register` corresponds to a locally minted static Record or is a static-Record register connected to one,
    // this function adds a connection between `new_register` and `old_register`.
    fn connect_locally_minted_static(&mut self, old_register: u64, new_register: u64) {
        for set in self.connected_locally_minted_static.iter_mut() {
            if set.contains(&old_register) {
                set.insert(new_register);
                // At most one set can contain `old_register` by construction.
                break;
            }
        }
    }

    // Returns the register of the static Record from which a given DynamicRecord is cast, if any.
    fn get_static_source_of_dynamic(&self, register: u64) -> Option<u64> {
        self.locally_minted_dynamic.get(&register).copied()
    }

    // Keeps track of the fact that a register has been passed to a function call: if the register is connected to a
    // locally minted static Record, this stores the fact that the latter must be output.
    fn passed_to_function_call(&mut self, register: u64) {
        if self.is_locally_minted_static(register) {
            // Case a)
            self.add_must_be_output(register);
        } else if let Some(static_record) = self.locally_minted_dynamic.get(&register) {
            // Case b)
            self.add_must_be_output(*static_record);
        }
    }

    // Adds a register to the set of registers that must be output because they are cast to a DynamicRecord and passed
    // to a non-closure call or output.
    fn add_must_be_output(&mut self, register: u64) {
        self.must_be_output.insert(register);
    }

    // Given a list of which registers are output by the function, checks that all registers that must be output
    // according to the local check are actually so. If one or more registers break that condition, returns Some(i),
    // where r_i is one of them.
    fn check_all_are_output(&self, actually_output: HashSet<u64>) -> Option<u64> {
        for register in self.must_be_output.iter() {
            // The unwrap is safe by construction because only registers containing locally minted static Records and
            // static-Record registers connected to those can appear in must_be_output.
            let connected_registers =
                self.connected_locally_minted_static.iter().find(|set| set.contains(register)).unwrap();

            // It is enough for any connected static-Record register to be output.
            if connected_registers.iter().all(|register| !actually_output.contains(register)) {
                return Some(*register);
            }
        }
        None
    }
}

// Auxiliary structure for the local record-existence check of a closure, i.e. the check that each DynamicRecord cast
// from a static Record minted in the closure is not output.
struct ClosureLocalCheck {
    // Registers of static Records minted in this closure.
    locally_minted_static: HashSet<u64>,
}

impl ClosureLocalCheck {
    // Initialises a closure's local check.
    fn new() -> Self {
        Self { locally_minted_static: HashSet::new() }
    }

    // Adds a locally minted static Record to the local check.
    fn mint_static(&mut self, register: u64) {
        self.locally_minted_static.insert(register);
    }

    // Returns whether a given register corresponds to a static Record minted in the closure.
    fn is_locally_minted_static(&self, register: u64) -> bool {
        self.locally_minted_static.contains(&register)
    }
}

impl<N: Network> Process<N> {
    /// Checks that, for each non-closure function in the execution, each ExternalRecord and DynamicRecord received as
    /// an input or from a callee corresponds to a static Record that exists on the ledger at the end of the execution
    /// (whether spent or not). A function given this guarantee should itself ensure that all records it outputs or
    /// passes to non-closure callees exist on the ledger at the end of the execution.
    ///
    /// Input `transitions`: Iterator over the `Transition`s in the execution. The root transition must be last.
    ///
    /// Input `call_graph`: A copy of the call graph. It is assumed to contain all transitions in `transitions`. All
    /// children of a given Transition ID must appear in the same order as the corresponding calls happen in the
    /// function.
    pub fn ensure_records_exist<'a>(
        &self,
        transitions: impl ExactSizeIterator<Item = &'a Transition<N>> + DoubleEndedIterator + Clone,
        call_graph: &HashMap<N::TransitionID, Vec<N::TransitionID>>,
    ) -> Result<()> {
        let root_transition = transitions.clone().last().ok_or_else(|| anyhow!("Empty transition list"))?;

        let tid_to_transition: HashMap<N::TransitionID, &Transition<N>> =
            transitions.map(|transition| (*transition.id(), transition)).collect();

        let mut global_check = GlobalCheck::new();

        // Recursively explore the execution, keeping track of record connections across the relevant casts and calls.
        process_transition(self, &mut global_check, root_transition.id(), None, &tid_to_transition, call_graph)?;

        // Ensure the global check passes.
        if global_check.non_existent_families().is_empty() {
            Ok(())
        } else {
            Err(anyhow!(
                "Non-static record input at r{} of the root function {}/{} is not known to correspond to a record on the ledger",
                // The unwrap is safe since all families have at least one element by construction.
                global_check.non_existent_families()[0].iter().next().unwrap().1,
                root_transition.program_id(),
                root_transition.function_name(),
            ))
        }
    }
}

// Auxiliary function for `ensure_records_exist` that connects registers in the transition to the relevant record
// families, tracking connections and marking families as existing if they are found to correspond to a record on the
// ledger (global check). Furthermore, it also checks that, if a DynamicRecord cast from a locally minted static Record
// is output or passed to a callee, the static one is output (local check).
fn process_transition<N: Network>(
    // Process being checked, from which function and closure definitions are retrieved
    process: &Process<N>,
    // Global check keeping track of families of registers connected to DynamicRecords or ExternalRecords input to the
    // root transition.
    global_check: &mut GlobalCheck<N>,
    // TransitionID of the transition being processed.
    transition_id: &N::TransitionID,
    // `None` for the root transition. For non-root transitions, `Some` containing:
    //  - TransitionID of the caller.
    //  - indices of the caller registers where it passes inputs to this transition's call (with `None` for inputs
    //    that are not registers)
    //  - indices of the caller registers where it receives outputs from this transition's call
    caller_info: Option<(N::TransitionID, &[Option<u64>], &[u64])>,
    // Map from TransitionID to the corresponding Transition
    tid_to_transition: &HashMap<N::TransitionID, &Transition<N>>,
    // Call graph of the execution
    call_graph: &HashMap<N::TransitionID, Vec<N::TransitionID>>,
) -> Result<()> {
    let transition =
        tid_to_transition.get(transition_id).ok_or_else(|| anyhow!("Missing transition with ID {transition_id}"))?;
    let stack = process.get_stack(transition.program_id())?;
    let function = stack.get_function_ref(transition.function_name())?;
    let locator = Locator::new(*transition.program_id(), *transition.function_name());

    let inputs = transition.inputs();
    let input_registers = function.inputs().iter().map(|input| input.register().locator()).collect::<Vec<u64>>();

    ensure!(
        input_registers.len() == transition.inputs().len(),
        "Mismatch in the number of inputs and registers in the call to {locator}"
    );

    // Initialise the machinery which keeps track of the local check.
    let mut local_check = FunctionLocalCheck::new();

    // Number of function calls encountered in the current transition so far.
    let mut processed_function_calls = 0;

    // Processing the inputs.
    if let Some((caller_tid, caller_input_registers, _)) = &caller_info {
        // Non-root transition case.

        ensure!(
            inputs.len() == input_registers.len() && inputs.len() == caller_input_registers.len(),
            "Mismatch in the number of callee/caller inputs and registers in call to {locator} (TransitionID {transition_id})",
        );

        for ((caller_input_register_opt, callee_input_register), callee_input) in
            caller_input_registers.iter().zip_eq(input_registers.iter()).zip_eq(inputs.iter())
        {
            if let Some(caller_input_register) = caller_input_register_opt {
                match callee_input {
                    Input::RecordWithDynamicID(..) | Input::Record(..) => {
                        // Case 1: consuming a Record translating from a DynamicRecord or passed as an ExternalRecord
                        // by the caller: the family is now known to exist on the ledger.
                        global_check.mark_existing((*caller_tid, *caller_input_register));
                    }
                    Input::ExternalRecord(..) | Input::ExternalRecordWithDynamicID(..) | Input::DynamicRecord(..) => {
                        // Case 2: connection at the input boundary
                        let old_register = (*caller_tid, *caller_input_register);
                        let new_register = (*transition_id, *callee_input_register);
                        // This call only adds the new register if the old register is being tracked, i.e. if it belongs to a family
                        global_check.add_to_family(old_register, new_register);
                    }
                    _ => {}
                }
            }
        }
    } else {
        // Root transition case.

        // Initialise the record families with one (singleton) family per DynamicRecord and ExternalRecord input.
        // Length check performed above.
        for (input, register) in transition.inputs().iter().zip_eq(input_registers.iter()) {
            if matches!(input, Input::DynamicRecord(..) | Input::ExternalRecord(..)) {
                global_check.add_family((*transition_id, *register));
            }
        }

        // Note: even if there are no record families to keep track of in the global check, we need to process
        // the transitions to enforce the local check.
    }

    for instruction in function.instructions() {
        match instruction {
            Instruction::Cast(cast) => {
                match cast.cast_type() {
                    CastType::Record(_) => {
                        // Case 3: minting a static Record locally.
                        local_check.mint_static(cast.destinations()[0].locator());
                    }
                    CastType::DynamicRecord => {
                        let operand_register = match cast.operands().first() {
                            Some(Operand::Register(register)) => register.locator(),
                            _ => bail!(
                                "Failed to retrieve operand register for cast to DynamicRecord instruction in {locator}"
                            ),
                        };

                        let destination_register = cast.destinations()[0].locator();

                        // Case 4: Global-check update. Since static Records never exist in any family and add_to_family
                        // only adds the new register if the old register exists in some family, this call only handles
                        // external-to-dynamic casts.
                        let old_register = (*transition_id, operand_register);
                        let new_register = (*transition_id, destination_register);
                        global_check.add_to_family(old_register, new_register);

                        // Case 5: Local-check update. This keeps track of the cast if the operand is a locally minted
                        // static Record.
                        local_check.cast_to_dynamic(operand_register, destination_register);
                    }
                    _ => {}
                }
            }
            Instruction::Call(..) | Instruction::CallDynamic(..) => {
                let caller_input_operands = match instruction {
                    Instruction::Call(..) => instruction.operands(),
                    // The first three operands of a call.dynamic instruction are reserved for the target and always
                    // present.
                    Instruction::CallDynamic(..) => &instruction.operands()[3..],
                    _ => bail!("Unreachable"),
                };

                let caller_input_registers: Vec<Option<u64>> = caller_input_operands
                    .iter()
                    .map(
                        |operand| {
                            if let Operand::Register(register) = operand { Some(register.locator()) } else { None }
                        },
                    )
                    .collect();

                let caller_output_registers: Vec<u64> =
                    instruction.destinations().iter().map(|destination| destination.locator()).collect();

                if let Instruction::Call(call) = instruction
                    && !call.is_function_call(stack.as_ref())?
                {
                    // Closure case
                    let (program_stack, identifier) = match call.operator() {
                        CallOperator::Resource(identifier) => (stack.clone(), identifier),
                        CallOperator::Locator(external_locator) => {
                            (stack.get_external_stack(external_locator.program_id())?, external_locator.resource())
                        }
                    };

                    let closure = program_stack.program().get_closure_ref(identifier)?;

                    process_closure(
                        transition_id,
                        closure,
                        global_check,
                        &mut local_check,
                        &caller_input_registers,
                        &caller_output_registers,
                    )?;
                } else {
                    // Function case

                    let tid_callee = if let Some(tid) =
                        // The unwrap is safe (assuming a correct call graph) as we are processing a Transition which
                        // is part of the execution.
                        call_graph.get(transition_id).unwrap().get(processed_function_calls)
                    {
                        processed_function_calls += 1;
                        tid
                    } else {
                        bail!(
                            "Entry with Transition ID {transition_id} ({locator}) in the call graph has fewer elements than the number of calls in the corresponding function"
                        );
                    };

                    for input_register in caller_input_operands.iter() {
                        if let Operand::Register(register) = input_register {
                            // Case 6: Any DynamicRecord which is passed to a non-closure call and was cast from a
                            // locally minted static Record must be output.
                            local_check.passed_to_function_call(register.locator());

                            // Note: the input register cannot correspond to a locally minted static Record in a
                            // successful execution:
                            // - if it were received as a static Record by the callee this would amount to spending
                            //   a still unminted Record
                            // - if it were received as an external ExternalRecord, this would constitute a dependency
                            //   cycle involving records - which is disallowed during deployment.
                        }
                    }

                    // Recursively updating the global check and performing the local check in the callee.
                    process_transition(
                        process,
                        global_check,
                        tid_callee,
                        Some((*transition_id, &caller_input_registers, &caller_output_registers)),
                        tid_to_transition,
                        call_graph,
                    )?;
                }
            }
            _ => {}
        }
    }

    // Output processing

    // Track which DynamicRecords coming from locally minted static Records are directly output (so the latter must
    // be output as well) and collect the output registers corresponding to locally minted static Records.
    let mut locally_minted_output = HashSet::new();

    function.outputs().iter().for_each(|output| {
        if let Operand::Register(output_register) = output.operand() {
            let output_locator = output_register.locator();

            if let Some(static_record_register) = local_check.get_static_source_of_dynamic(output_locator) {
                // Case 7: DynamicRecord cast from a locally minted static Record is output.
                local_check.add_must_be_output(static_record_register);
            } else if local_check.is_locally_minted_static(output_locator) {
                locally_minted_output.insert(output_locator);
            }
        }
    });

    if let Some(not_output) = local_check.check_all_are_output(locally_minted_output) {
        bail!(
            "{locator} does not pass the local record-existence check: locally minted static Record at r{not_output} \
            is cast to a DynamicRecord and passed to a callee or output, yet is not itself output"
        );
    }

    // For non-root calls, update the global check's record families with the connections at the output boundary.
    if let Some((caller_tid, _, caller_output_registers)) = &caller_info {
        let outputs = function.outputs();

        ensure!(
            outputs.len() == caller_output_registers.len(),
            "Mismatch in the number of callee/caller outputs in call to {locator} (transition ID {transition_id})",
        );

        for (caller_output_register, callee_output) in caller_output_registers.iter().zip_eq(outputs.iter()) {
            if let Operand::Register(callee_output_register) = callee_output.operand()
                && matches!(callee_output.value_type(), ValueType::DynamicRecord | ValueType::ExternalRecord(..))
            {
                // Case 8: add the caller's output register to the family containing the callee's. Note that
                // output registers with type ValueType::Record are never tracked as part of the global check.
                let old_register = (*transition_id, callee_output_register.locator());
                let new_register = (*caller_tid, *caller_output_register);
                global_check.add_to_family(old_register, new_register);
            }
        }
    }

    // Sanity check: exploration should have processed all calls in the graph.
    ensure!(
        // The unwrap is safe (assuming a correct call graph) as we are processing a Transition which is part of the
        // execution.
        processed_function_calls == call_graph.get(transition_id).unwrap().len(),
        "In the record-existence check, entry for Transition ID {transition_id} ({locator}) in the call graph has unprocessed children",
    );

    Ok(())
}

// Auxiliary function for ensure_records_exist which processes a closure. The global check and the caller function's
// local check are updated taking into account the cast instructions in the closure as well as its input-output
// relations. Furthermore, this function ensures the closure does not output any DynamicRecords cast from locally
// minted static Records.
fn process_closure<N: Network>(
    // TransitionID of the caller function
    caller_tid: &N::TransitionID,
    // Closure being processed
    closure: &ClosureCore<N>,
    // Global check keeping track of families of registers connected to DynamicRecords or ExternalRecords input to the
    // root transition.
    global_check: &mut GlobalCheck<N>,
    // Caller function's local check.
    caller_local_check: &mut FunctionLocalCheck,
    // Caller registers of inputs to the closure call (`None` for inputs that are not registers).
    caller_input_registers: &[Option<u64>],
    // Caller registers of outputs of the closure call
    caller_output_registers: &[u64],
) -> Result<()> {
    // Initialise the machinery which keeps track of the closure's local check.
    let mut local_check = ClosureLocalCheck::new();

    let closure_name = closure.name();

    ensure!(
        caller_input_registers.len() == closure.inputs().len(),
        "Mismatch between the number of input registers in the call instruction and the callee's input types in call to closure {closure_name}"
    );
    ensure!(
        caller_output_registers.len() == closure.outputs().len()
            && caller_output_registers.len() == closure.output_types().len(),
        "Mismatch between the number of output registers in the call instruction and the callee's output types in call to closure {closure_name}"
    );

    // Construct a map { callee register -> caller register } for the closure's inputs of type Record, DynamicRecord or
    // ExternalRecord.
    let input_map = caller_input_registers
        .iter()
        .zip_eq(closure.inputs().iter())
        .filter_map(|(caller_input_register_opt, closure_input)| {
            if matches!(
                closure_input.register_type(),
                RegisterType::Record(..) | RegisterType::DynamicRecord | RegisterType::ExternalRecord(..)
            ) {
                if let Some(caller_input_register) = caller_input_register_opt {
                    Some(Ok((closure_input.register().locator(), *caller_input_register)))
                } else {
                    Some(Err(anyhow!("Missing register information for the caller input to closure {closure_name}")))
                }
            } else {
                None
            }
        })
        .collect::<Result<HashMap<u64, u64>>>()?;

    // Construct a map { callee register -> caller register } for the closure's outputs of type DynamicRecord or
    // ExternalRecord (closures cannot output static Records).
    let output_map = caller_output_registers
        .iter()
        .zip_eq(closure.outputs().iter())
        .zip_eq(closure.output_types().iter())
        .filter_map(|((caller_output_register, closure_output), closure_output_type)| {
            if matches!(closure_output_type, RegisterType::DynamicRecord | RegisterType::ExternalRecord(..)) {
                if let Operand::Register(register) = closure_output.operand() {
                    Some(Ok((register.locator(), *caller_output_register)))
                } else {
                    Some(Err(anyhow!("Missing output register information in closure {closure_name}")))
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
                    // Case 1: minting a static Record locally. We keep track to ensure DynamicRecords cast from it are
                    // not output.
                    local_check.mint_static(instruction.destinations()[0].locator());
                }
                CastType::DynamicRecord => {
                    let operand_register = match cast.operands().first() {
                        Some(Operand::Register(register)) => register.locator(),
                        _ => bail!(
                            "Failed to retrieve operand register for cast to DynamicRecord instruction in closure {closure_name}"
                        ),
                    };

                    let destination_register = match cast.destinations().first() {
                        Some(destination) => destination.locator(),
                        _ => bail!(
                            "Failed to retrieve destination register for cast to DynamicRecord instruction in closure {closure_name}"
                        ),
                    };

                    if local_check.is_locally_minted_static(operand_register) {
                        // Case 2: Casting a locally minted static Record to a DynamicRecord. We ensure the latter is
                        // not output.
                        if output_map.contains_key(&destination_register) {
                            bail!(
                                "Closure {closure_name} attempts to output DynamicRecord at r{destination_register} cast from locally minted static Record at r{operand_register}",
                            );
                        }
                    } else {
                        // In this case, the input to the cast instruction is necessarily an input to the closure
                        // itself. We retrieve its caller register.
                        let caller_operand_register = input_map.get(&operand_register).ok_or_else(
                            || anyhow!("Missing caller input register for Cast instruction from register {operand_register} in closure {}", closure.name())
                        )?;

                        // We only need to process this cast instruction if the destination register is output by the
                        // closure.
                        if let Some(caller_destination_register) = output_map.get(&destination_register) {
                            if caller_local_check.is_locally_minted_static(*caller_operand_register) {
                                // Case 3: Effectively performing a static-to-dynamic cast in the caller. We update the
                                // caller's local check accordingly.
                                caller_local_check
                                    .cast_to_dynamic(*caller_operand_register, *caller_destination_register);
                            } else {
                                // Case 4: Casting a value already received as a Record or ExternalRecord input by the
                                // caller itself. In the Record case, nothing was being kept track of. In the
                                // ExternalRecord case, we inform the caller's global check of the connection between
                                // the two registers.
                                let old_register = (*caller_tid, *caller_operand_register);
                                let new_register = (*caller_tid, *caller_destination_register);
                                global_check.add_to_family(old_register, new_register);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Detecting connections of caller registers resulting from closure input-output relations not involving casts.
    for (callee_input_register, caller_input_register) in input_map {
        if let Some(caller_output_register) = output_map.get(&callee_input_register) {
            // Case 5. Caller global-check update (only adds the new register if the old one belongs to some family).
            let old_register = (*caller_tid, caller_input_register);
            let new_register = (*caller_tid, *caller_output_register);
            global_check.add_to_family(old_register, new_register);

            // Case 6. First caller local-check update: remapping of DynamicRecord registers.
            if let Some(original_static) = caller_local_check.get_static_source_of_dynamic(caller_input_register) {
                caller_local_check.cast_to_dynamic(original_static, *caller_output_register);
            }

            // Case 7. Second caller local-check update: remapping of static-Records registers (connected to a locally
            // minted one).
            caller_local_check.connect_locally_minted_static(caller_input_register, *caller_output_register);
        }
    }

    Ok(())
}
