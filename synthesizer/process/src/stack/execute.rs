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

use console::program::{InputID, compute_function_id};
use snarkvm_synthesizer_program::RecordTranslationData;

use super::*;

impl<N: Network> Stack<N> {
    /// Executes a program closure on the given inputs.
    ///
    /// # Errors
    /// This method will halt if the given inputs are not the same length as the input statements.
    pub fn execute_closure<A: circuit::Aleo<Network = N>>(
        &self,
        closure: &Closure<N>,
        inputs: &[circuit::Value<A>],
        call_stack: CallStack<N>,
        signer: circuit::Address<A>,
        caller: circuit::Address<A>,
        tvk: circuit::Field<A>,
    ) -> Result<Vec<circuit::Value<A>>> {
        let timer = timer!("Stack::execute_closure");

        // Ensure the call stack is not `Evaluate`.
        ensure!(!matches!(call_stack, CallStack::Evaluate(..)), "Illegal operation: cannot evaluate in execute mode");

        // Ensure the number of inputs matches the number of input statements.
        if closure.inputs().len() != inputs.len() {
            bail!("Expected {} inputs, found {}", closure.inputs().len(), inputs.len())
        }
        lap!(timer, "Check the number of inputs");

        // Retrieve the number of public variables in the circuit.
        let num_public = A::num_public();

        // Initialize the registers.
        let mut registers = Registers::new(call_stack, self.get_register_types(closure.name())?.clone());
        // Set the transition signer, as a circuit.
        registers.set_signer_circuit(signer);
        // Set the transition caller, as a circuit.
        registers.set_caller_circuit(caller);
        // Set the transition view key, as a circuit.
        registers.set_tvk_circuit(tvk);
        lap!(timer, "Initialize the registers");

        // Store the inputs.
        closure.inputs().iter().map(|i| i.register()).zip_eq(inputs).try_for_each(|(register, input)| {
            // If the circuit is in execute mode, then store the console input.
            if let CallStack::Execute(..) = registers.call_stack_ref() {
                use circuit::Eject;
                // Assign the console input to the register.
                registers.store(self, register, input.eject_value())?;
            }
            // Assign the circuit input to the register.
            registers.store_circuit(self, register, input.clone())
        })?;
        lap!(timer, "Store the inputs");

        // Execute the instructions.
        for instruction in closure.instructions() {
            // If the circuit is in execute mode, then evaluate the instructions.
            if let CallStack::Execute(..) = registers.call_stack_ref() {
                // If the evaluation fails, bail and return the error.
                if let Err(error) = instruction.evaluate(self, &mut registers) {
                    bail!("Failed to evaluate instruction ({instruction}): {error}");
                }
            }
            // Execute the instruction.
            instruction.execute(self, &mut registers)?;
        }
        lap!(timer, "Execute the instructions");

        // Ensure the number of public variables remains the same.
        ensure!(A::num_public() == num_public, "Illegal closure operation: instructions injected public variables");

        use circuit::Inject;

        // Load the outputs.
        let outputs = closure
            .outputs()
            .iter()
            .map(|output| {
                match output.operand() {
                    // If the operand is a literal, use the literal directly.
                    Operand::Literal(literal) => Ok(circuit::Value::Plaintext(circuit::Plaintext::from(
                        circuit::Literal::new(circuit::Mode::Constant, literal.clone()),
                    ))),
                    // If the operand is a register, retrieve the stack value from the register.
                    Operand::Register(register) => registers.load_circuit(self, &Operand::Register(register.clone())),
                    // If the operand is the program ID, convert the program ID into an address.
                    Operand::ProgramID(program_id) => {
                        Ok(circuit::Value::Plaintext(circuit::Plaintext::from(circuit::Literal::Address(
                            circuit::Address::new(circuit::Mode::Constant, program_id.to_address()?),
                        ))))
                    }
                    // If the operand is the signer, retrieve the signer from the registers.
                    Operand::Signer => Ok(circuit::Value::Plaintext(circuit::Plaintext::from(
                        circuit::Literal::Address(registers.signer_circuit()?),
                    ))),
                    // If the operand is the caller, retrieve the caller from the registers.
                    Operand::Caller => Ok(circuit::Value::Plaintext(circuit::Plaintext::from(
                        circuit::Literal::Address(registers.caller_circuit()?),
                    ))),
                    // If the operand is the block height, throw an error.
                    Operand::BlockHeight => {
                        bail!("Illegal operation: cannot retrieve the block height in a closure scope")
                    }
                    // If the operand is the network id, throw an error.
                    Operand::NetworkID => {
                        bail!("Illegal operation: cannot retrieve the network id in a closure scope")
                    }
                    // If the operand is the checksum, throw an error.
                    Operand::Checksum(_) => bail!("Illegal operation: cannot retrieve the checksum in a closure scope"),
                    // If the operand is the edition, throw an error.
                    Operand::Edition(_) => bail!("Illegal operation: cannot retrieve the edition in a closure scope"),
                    // If the operand is the program owner, throw an error.
                    Operand::ProgramOwner(_) => {
                        bail!("Illegal operation: cannot retrieve the program owner in a closure scope")
                    }
                }
            })
            .collect();
        lap!(timer, "Load the outputs");

        finish!(timer);
        outputs
    }

    /// Executes a program function on the given inputs.
    ///
    /// Note: To execute a transition, do **not** call this method. Instead, call `Process::execute`.
    ///
    /// # Errors
    /// This method will halt if the given inputs are not the same length as the input statements.
    pub fn execute_function<A: circuit::Aleo<Network = N>, R: CryptoRng + Rng>(
        &self,
        mut call_stack: CallStack<N>,
        console_caller: Option<(ProgramID<N>, Identifier<N>)>,
        console_root_tvk: Option<Field<N>>,
        rng: &mut R,
    ) -> Result<Response<N>> {
        let timer = timer!("Stack::execute_function");

        // Ensure the global constants for the Aleo environment are initialized.
        A::initialize_global_constants();
        // Ensure the circuit environment is clean.
        A::reset();

        // If in 'CheckDeployment' mode, set the constraint limit and variable limit.
        // We do not have to reset it after function calls because `CheckDeployment` mode does not execute those.
        if let CallStack::CheckDeployment(_, _, _, constraint_limit, variable_limit) = &call_stack {
            A::set_constraint_limit(*constraint_limit);
            A::set_variable_limit(*variable_limit);
        }

        // Retrieve the next request.
        let console_request = call_stack.pop()?;

        // Ensure the network ID matches.
        ensure!(
            **console_request.network_id() == N::ID,
            "Network ID mismatch. Expected {}, but found {}",
            N::ID,
            console_request.network_id()
        );

        // We can only have a root_tvk if this request was called by another request
        ensure!(console_caller.is_some() == console_root_tvk.is_some());
        // Determine if this is the top-level caller.
        let console_is_root = console_caller.is_none();

        // Determine the parent.
        //  - If this execution is the top-level caller, then the parent is the program ID.
        //  - If this execution is a child caller, then the parent is the caller.
        let (console_parent_address, console_parent_id, console_parent_function_name) = match console_caller {
            // If this execution is the top-level caller, then the parent is the program ID.
            None => (console_request.program_id().to_address()?, *console_request.program_id(), None),
            // If this execution is a child caller, then the parent is the caller.
            Some((console_caller, function_name)) => (console_caller.to_address()?, console_caller, Some(function_name)),
        };

        let console_parent_function_id = compute_function_id(
            &U16::<N>::new(N::ID as u16),
            console_request.program_id(),
            console_request.function_name(),
            false,
        )?;

        // Retrieve the function from the program.
        let function = self.get_function(console_request.function_name())?;
        
        // Retrieve the number of inputs.
        let num_inputs = function.inputs().len();
        // Ensure the number of inputs matches the number of input statements.
        if num_inputs != console_request.inputs().len() {
            bail!("Expected {num_inputs} inputs, found {}", console_request.inputs().len())
        }
        // Retrieve the input types.
        let input_types = function.input_types();
        // Retrieve the output types.
        let output_types = function.output_types();
        lap!(timer, "Retrieve the input and output types");

        // Ensure the inputs match their expected types.
        console_request.inputs().iter().zip_eq(&input_types).try_for_each(|(input, input_type)| {
            // Ensure the input matches the input type in the function.
            self.matches_value_type(input, input_type)
        })?;
        lap!(timer, "Verify the input types");

        // Retrieve the program checksum, if the program has a constructor.
        let program_checksum = match self.program().contains_constructor() {
            true => Some(self.program_checksum_as_field()?),
            false => None,
        };

        // Ensure the request is well-formed.
        ensure!(
            console_request.verify(&input_types, console_is_root, program_checksum),
            "[Execute] Request is invalid"
        );
        lap!(timer, "Verify the console request");

        // Initialize the registers.
        let mut registers = Registers::new(call_stack, self.get_register_types(function.name())?.clone());

        // Set the root tvk, from a parent request or the current request.
        let console_root_tvk = console_root_tvk.unwrap_or(*console_request.tvk());
        // Inject the `root_tvk` as `Mode::Private`.
        let root_tvk = circuit::Field::<A>::new(circuit::Mode::Private, console_root_tvk);
        // Set the root tvk.
        registers.set_root_tvk(console_root_tvk);
        // Set the root tvk, as a circuit.
        registers.set_root_tvk_circuit(root_tvk.clone());

        // If a program checksum was passed in, Inject it as `Mode::Public`.
        let program_checksum = program_checksum.map(|c| circuit::Field::<A>::new(circuit::Mode::Public, c));

        use circuit::{Eject, Inject};

        // Inject the transition public key `tpk` as `Mode::Public`.
        let tpk = circuit::Group::<A>::new(circuit::Mode::Public, console_request.to_tpk());
        // Inject the request as `Mode::Private`.
        let request = circuit::Request::new(circuit::Mode::Private, console_request.clone());

        // Inject `is_root` as `Mode::Public`.
        let is_root = circuit::Boolean::new(circuit::Mode::Public, console_is_root);
        // Inject the parent as `Mode::Public`.
        let parent = circuit::Address::new(circuit::Mode::Public, console_parent_address);
        // Determine the caller.
        let caller = Ternary::ternary(&is_root, request.signer(), &parent);

        // Ensure the request has a valid signature, inputs, and transition view key.
        A::assert(request.verify(&input_types, &tpk, Some(root_tvk), is_root, program_checksum));
        lap!(timer, "Verify the circuit request");

        // Set the transition signer.
        registers.set_signer(*console_request.signer());
        // Set the transition signer, as a circuit.
        registers.set_signer_circuit(request.signer().clone());

        // Set the transition caller.
        registers.set_caller(caller.eject_value());
        // Set the transition caller, as a circuit.
        registers.set_caller_circuit(caller);

        // Set the transition view key.
        registers.set_tvk(*console_request.tvk());
        // Set the transition view key, as a circuit.
        registers.set_tvk_circuit(request.tvk().clone());

        lap!(timer, "Initialize the registers");

        Self::log_circuit::<A>("Request");

        // Retrieve the number of constraints for verifying the request in the circuit.
        let num_request_constraints = A::num_constraints();

        // Retrieve the number of public variables in the circuit.
        let mut num_public = A::num_public();

        // Store the inputs.
        function.inputs().iter().map(|i| i.register()).zip_eq(request.inputs()).try_for_each(|(register, input)| {
            // If the circuit is in execute mode, then store the console input.
            if let CallStack::Execute(..) = registers.call_stack_ref() {
                // Assign the console input to the register.
                registers.store(self, register, input.eject_value())?;
            }
            // Assign the circuit input to the register.
            registers.store_circuit(self, register, input.clone())
        })?;
        lap!(timer, "Store the inputs");

        // TODO (dynamic_dispatch) Is this correct? Is the dynamic record id unique enough to serve as an identifier here?
        for (index, ((input_value, input_id), input_type)) in console_request.inputs().iter().zip_eq(console_request.input_ids()).zip_eq(function.input_types()).enumerate() {
            match (input_value, input_type, input_id) {
                // TODO (dynamic_dispatch) move or detect whether translation is happening
                (Value::Record(record_static), ValueType::Record(record_name), InputID::Record(record_id, gamma, record_view_key, _, _)) => {                    
                    registers.insert_record_translation_data(RecordTranslationData {
                        record_static: record_static.clone(),
                        program_id: *console_request.program_id(),
                        function_id: console_parent_function_id,
                        record_name,
                        to_static_record: true,
                        tvk: registers.tvk()?,
                        record_view_key: *record_view_key,
                        gamma: Some(gamma.clone()),
                        static_record_id: *record_id,
                        register_index: index as u16,
                    });
                }
                _ => {}
            }
        }

        let actual_inputs = console_request.inputs();
        let expected_input_types = function.input_types();
        actual_inputs.iter().zip_eq(expected_input_types).for_each(|(input, input_type)| {
            match (input, input_type) {
                (Value::Record(record), ValueType::DynamicRecord) => {
                    println!("Translation static -> dynamic");
                }
                (Value::DynamicRecord(dynamic_record), ValueType::Record(_)) => {
                    println!("Translation dynamic -> static");
                }
                _ => {}
            }
        });

        // Initialize a tracker to determine if there are any function calls.
        let mut contains_function_call = false;

        // Execute the instructions.
        for instruction in function.instructions() {
            // If the circuit is in execute mode, then evaluate the instructions.
            if let CallStack::Execute(..) = registers.call_stack_ref() {
                // Evaluate the instruction.
                let result = match instruction {
                    // If the instruction is a `call` instruction, we need to handle it separately.
                    Instruction::Call(call) => CallTrait::evaluate(call, self, &mut registers, rng),
                    // If the instruction is a `call.dynamic` instruction, we need to handle it separately.
                    Instruction::CallDynamic(call_dynamic) => {
                        // Evaluate the dynamic call.
                        CallTrait::evaluate(call_dynamic, self, &mut registers, rng)
                    }
                    // Otherwise, evaluate the instruction normally.
                    _ => instruction.evaluate(self, &mut registers),
                };
                // If the evaluation fails, bail and return the error.
                if let Err(error) = result {
                    bail!("Failed to evaluate instruction ({instruction}): {error}");
                }
            }

            // Execute the instruction.
            let result = match instruction {
                // If the instruction is a `call` instruction, we need to handle it separately.
                Instruction::Call(call) => CallTrait::execute(call, self, &mut registers, rng),
                // If the instruction is a `call.dynamic` instruction, we need to handle it separately.
                Instruction::CallDynamic(call_dynamic) => {
                    // Increment the number of public variables.
                    // TODO (@d0cd): Explain this count.
                    num_public += 7;
                    // Execute the dynamic call.
                    CallTrait::execute(call_dynamic, self, &mut registers, rng)
                }
                // Otherwise, execute the instruction normally.
                _ => instruction.execute(self, &mut registers),
            };
            // If the execution fails, bail and return the error.
            if let Err(error) = result {
                bail!("Failed to execute instruction ({instruction}): {error}");
            }

            // If the instruction was a function call, then set the tracker to `true`.
            match instruction {
                Instruction::Call(call) => {
                    if call.is_function_call(self)? {
                        contains_function_call = true;
                    }
                }
                // A dynamic call is always a function call.
                Instruction::CallDynamic(_) => {
                    contains_function_call = true;
                }
                _ => {}
            }

            // TODO (dynamic_dispatch) remove
            // // Preparing record-translation data to be fetched by the called functions
            // match instruction {
            //     Instruction::CallDynamic(call_dynamic) => {

            //         let program_id = match registers.load(self, &call_dynamic.operands()[0])? {
            //             Value::Plaintext(Plaintext::Literal(Literal::Field(program_id), _)) => program_id,
            //             _ => bail!("Expected the first operand of `call.dynamic` to be a ProgramID."),
            //         };
            //         let function_id = match registers.load(self, &call_dynamic.operands()[2])? {
            //             Value::Plaintext(Plaintext::Literal(Literal::Field(function_id), _)) => function_id,
            //             _ => bail!("Expected the third operand of `call.dynamic` to be a FunctionID."),
            //         };

            //         // TODO (dynamic_dispatch) we need the Identifier, not its Field representation
            //         let callee_function = self.get_function(callee_function_id)?;
            //         let callee_function_input_types = callee_function.input_types();
                    
            //         for (index, ((input_supplied, input_supplied_type), input_received_type)) in call_dynamic.operands().iter().skip(3)
            //             .zip_eq(call_dynamic.operand_types().iter().skip(3))
            //             .zip_eq(callee_function_input_types).enumerate() {
                            
            //             let input_supplied_value = registers.load(self, input_supplied)?;

            //             match (input_supplied_value, input_supplied_type, input_received_type) {
            //                 // Case 1: input dynamic -> static
            //                 // TODO (dynamic_dispatch) make sure the Record is not an ExternalRecord
            //                 (Value::DynamicRecord(dynamic_record), _, ValueType::Record(record_name)) => {

            //                     // TODO (dynamic_dispatch) decide whether this is the best solution or this can be read e. g. from the record definition
            //                     let owner_is_private = {
            //                         if let circuit::Value::DynamicRecord(circuit_dynamic_record) = registers.load_circuit(self, input_supplied)? {
            //                             circuit_dynamic_record.owner().is_private()
            //                         } else {
            //                             bail!("Register contains a console DynamicRecord, but its circuit object is not a circuit DynamicRecord")
            //                         }
            //                     };

            //                     let record_static = dynamic_record.to_record(owner_is_private)?;

            //                     let record_view_key = (record_static.nonce() * *view_key).to_x_coordinate();
            //                     let gamma = {
            //                         let h = N::hash_to_group_psd2(&[N::serial_number_domain(), record_static.to_commitment(&callee_program_id, record_identifier, &record_view_key)?.clone()]);

            //                         // TODO (dynamic_dispatch) I don't think we can get the signing key here
            //                         h * signing_key.sk_sig();
            //                     };

            //                     let record_translation_data = RecordTranslationData {
            //                         record_static,
            //                         // TODO (dynamic_dispatch) we need the Identifier, not its Field representation
            //                         // The definition of the static record lives in the called function
            //                         program_id: *callee_program_id,
            //                         // TODO (dynamic_dispatch) make sure this should always be the parent function ID
            //                         function_id,
            //                         // TODO (dynamic_dispatch) where to get?
            //                         record_name,
            //                         to_static_record: true,
            //                         tvk: registers.tvk()?,
            //                         register_index: index as u16,
            //                         record_view_key,
            //                         // TODO (dynamic_dispatch) where to get?
            //                         gamma: Some(Group::<N>::zero()),
            //                     };

            //                     registers.insert_record_translation_data(record_translation_data);
            //                 },
            //                 // Case 2: input static -> dynamic
            //                 (Value::Record(record_static), ValueType::Record(record_name), ValueType::DynamicRecord) => {

            //                     // TODO (dynamic_dispatch) console_request only contains the owner address, i. e. owner_view_key * G
            //                     //                         record_static.nonce() = randomizer * G, so one could do randomizer * owner_address, but I can't locate the randomizer
            //                     let record_view_key = (record_static.nonce() * owner_view_key).to_x_coordinate();
            
            //                     console_request.input_ids()
            //                     let gamma = {
            //                         let h = N::hash_to_group_psd2(&[N::serial_number_domain(), record_static.to_commitment(console_request.program_id(), record_name, &record_view_key)?.clone()]);

            //                         // TODO (dynamic_dispatch) I don't think we can get the signing key here
            //                         h * signing_key.sk_sig();
            //                     };

            //                     let record_translation_data = RecordTranslationData {
            //                         record_static,
            //                         // The definition of the static record lives in the called function
            //                         program_id: *console_request.program_id(),
            //                         function_id,
            //                         // TODO (dynamic_dispatch) where to get?
            //                         record_name,
            //                         to_static_record: false,
            //                         tvk: registers.tvk()?,
            //                         register_index: index as u16,
            //                         record_view_key,
            //                         // TODO (dynamic_dispatch) where to get?
            //                         gamma: None,
            //                     };

            //                     registers.insert_record_translation_data(record_translation_data);
            //                 },
            //                 // No translation
            //                 _ => {}
            //             }

            //         /* for (index, (input_supplied, input_received)) in call.operands().iter().zip_eq(callee_function_input_types).enumerate() {
            //             let input_supplied_value = registers.load(self, input_supplied)?;
            //             if let (Value::Record(static_record), ValueType::DynamicRecord) = (input_supplied_value, input_received) {
            //                 let record_translation_data = RecordTranslationData {
            //                     record_static: static_record.clone(),
            //                     // The definition of the static record lives in the called function
            //                     program_id: *callee_program_id,
            //                     // TODO (dynamic_dispatch) make sure this should always be the parent function ID
            //                     // TODO (dynamic_dispatch) provide
            //                     function_id: *function.name(),
            //                     // TODO (dynamic_dispatch) where to get?
            //                     record_name: input_received.name(),
            //                     to_static_record: true,
            //                     tvk: registers.tvk()?,
            //                     register_index: index as u16,
            //                     // TODO (dynamic_dispatch) where to get?
            //                     record_view_key: static_record.view_key(),
            //                     // TODO (dynamic_dispatch) where to get?
            //                     gamma: static_record.gamma(),
            //                 };
            //             } */
            //         }
            //     }
            //     // TODO (dynamic_dispatch) handle call_dynamic
            //     Instruction::CallDynamic(call_dynamic) => {},
            //     _ => {}
            // }
        }
        lap!(timer, "Execute the instructions");

        // Load the outputs.
        let output_operands = &function.outputs().iter().map(|output| output.operand()).collect::<Vec<_>>();
        let outputs = output_operands
            .iter()
            .map(|operand| {
                match operand {
                    // If the operand is a literal, use the literal directly.
                    Operand::Literal(literal) => Ok(circuit::Value::Plaintext(circuit::Plaintext::from(
                        circuit::Literal::new(circuit::Mode::Constant, literal.clone()),
                    ))),
                    // If the operand is a register, retrieve the stack value from the register.
                    Operand::Register(register) => registers.load_circuit(self, &Operand::Register(register.clone())),
                    // If the operand is the program ID, convert the program ID into an address.
                    Operand::ProgramID(program_id) => {
                        Ok(circuit::Value::Plaintext(circuit::Plaintext::from(circuit::Literal::Address(
                            circuit::Address::new(circuit::Mode::Constant, program_id.to_address()?),
                        ))))
                    }
                    // If the operand is the signer, retrieve the signer from the registers.
                    Operand::Signer => Ok(circuit::Value::Plaintext(circuit::Plaintext::from(
                        circuit::Literal::Address(registers.signer_circuit()?),
                    ))),
                    // If the operand is the caller, retrieve the caller from the registers.
                    Operand::Caller => Ok(circuit::Value::Plaintext(circuit::Plaintext::from(
                        circuit::Literal::Address(registers.caller_circuit()?),
                    ))),
                    // If the operand is the block height, throw an error.
                    Operand::BlockHeight => {
                        bail!("Illegal operation: cannot retrieve the block height in a function scope")
                    }
                    // If the operand is the network id, throw an error.
                    Operand::NetworkID => {
                        bail!("Illegal operation: cannot retrieve the network id in a function scope")
                    }
                    // If the operand is the checksum, throw an error.
                    Operand::Checksum(_) => {
                        bail!("Illegal operation: cannot retrieve the checksum in a function scope")
                    }
                    // If the operand is the edition, throw an error.
                    Operand::Edition(_) => {
                        bail!("Illegal operation: cannot retrieve the edition in a function scope")
                    }
                    // If the operand is the program owner, throw an error.
                    Operand::ProgramOwner(_) => {
                        bail!("Illegal operation: cannot retrieve the program owner in a function scope")
                    }
                }
            })
            .collect::<Result<Vec<_>>>()?;
        lap!(timer, "Load the outputs");

        // Map the output operands into registers.
        let output_registers = output_operands
            .iter()
            .map(|operand| match operand {
                Operand::Register(register) => Some(register.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();

        Self::log_circuit::<A>(format!("Function '{}()'", function.name()));

        // Retrieve the number of constraints for executing the function in the circuit.
        let num_function_constraints = A::num_constraints().saturating_sub(num_request_constraints);

        // If the function does not contain function calls, ensure no new public variables were injected.
        if !contains_function_call {
            // Ensure the number of public variables remains the same.
            ensure!(
                A::num_public() == num_public,
                "Instructions in function injected an unexpected number of public variables"
            );
        }

        // Construct the response.
        let response = circuit::Response::from_outputs(
            request.signer(),
            request.network_id(),
            request.program_id(),
            request.function_name(),
            num_inputs,
            request.tvk(),
            request.tcm(),
            outputs,
            &output_types,
            &output_registers,
            request.is_dynamic(),
        );
        lap!(timer, "Construct the response");

        Self::log_circuit::<A>("Response");

        // Retrieve the number of constraints for verifying the response in the circuit.
        let num_response_constraints =
            A::num_constraints().saturating_sub(num_request_constraints).saturating_sub(num_function_constraints);

        Self::log_circuit::<A>("Complete");

        // Eject the response.
        let response = response.eject_value();

        // Ensure the outputs matches the expected value types.
        response.outputs().iter().zip_eq(&output_types).try_for_each(|(output, output_type)| {
            // Ensure the output matches its expected type.
            self.matches_value_type(output, output_type)
        })?;

        // If the circuit is in `Execute` or `PackageRun` mode, then ensure the circuit is satisfied.
        if matches!(registers.call_stack_ref(), CallStack::Execute(..) | CallStack::PackageRun(..)) {
            // If the circuit is empty or not satisfied, then throw an error.
            ensure!(
                A::num_constraints() > 0 && A::is_satisfied(),
                "'{}/{}' is not satisfied on the given inputs ({} constraints).",
                self.program.id(),
                function.name(),
                A::num_constraints()
            );
        }

        // Eject the circuit assignment and reset the circuit.
        let assignment = A::eject_assignment_and_reset();

        // If the circuit is in `Synthesize` or `Execute` mode, synthesize the circuit key, if it does not exist.
        if matches!(registers.call_stack_ref(), CallStack::Synthesize(..) | CallStack::Execute(..)) {
            // If the proving key does not exist, then synthesize it.
            if !self.contains_proving_key(function.name()) {
                // Add the circuit key to the mapping.
                self.synthesize_from_assignment(function.name(), &assignment)?;
                lap!(timer, "Synthesize the {} circuit key", function.name());
            }
        }

        // If the circuit is in `Authorize` mode, then save the transition.
        if let CallStack::Authorize(_, _, authorization) = registers.call_stack_ref() {
            // Get the record translation arguments.
            let record_translation_arguments = registers.record_translation_arguments().cloned();
            // Construct the transition.
            let transition = Transition::from(&console_request, &response, &output_types, &output_registers, Some(record_translation_arguments.unwrap_or_default().iter().map(|(id, _)| *id).collect_vec()))?;
            // Add the transition to the authorization.
            authorization.insert_transition(transition)?;
            lap!(timer, "Save the transition");
        }
        // If the circuit is in `CheckDeployment` mode, then save the assignment.
        else if let CallStack::CheckDeployment(_, _, assignments, _, _) = registers.call_stack_ref() {
            // Construct the call metrics.
            let metrics = CallMetrics {
                program_id: *self.program_id(),
                function_name: *function.name(),
                num_instructions: function.instructions().len(),
                num_request_constraints,
                num_function_constraints,
                num_response_constraints,
            };
            // Add the assignment to the assignments.
            assignments.write().push((assignment, metrics));
            lap!(timer, "Save the circuit assignment");
        }
        // If the circuit is in `Execute` mode, then execute the circuit into a transition.
        else if let CallStack::Execute(_, trace) = registers.call_stack_ref() {
            registers.ensure_console_and_circuit_registers_match()?;

            // Get the record translation arguments.
            let record_translation_arguments = registers.record_translation_arguments().cloned();

            // Get the record translation data.
            let record_translation_data = registers.record_translation_data().cloned();

            // Construct the transition.
            let transition = Transition::from(&console_request, &response, &output_types, &output_registers, Some(record_translation_arguments.clone().unwrap_or_default().iter().map(|(id, _)| *id).collect_vec()))?;

            // Retrieve the proving key.
            let proving_key = self.get_proving_key(function.name())?;
            // Construct the call metrics.
            let metrics = CallMetrics {
                program_id: *self.program_id(),
                function_name: *function.name(),
                num_instructions: function.instructions().len(),
                num_request_constraints,
                num_function_constraints,
                num_response_constraints,
            };

            // Add the transition to the trace.
            trace.write().insert_transition(
                console_request.input_ids(),
                console_request.inputs(),
                &transition,
                (proving_key, assignment),
                metrics,
                // TODO (dynamic_dispatch) redesign, dedup, map...
                record_translation_arguments,
                record_translation_data,
            )?;
        }
        // If the circuit is in `PackageRun` mode, then save the assignment.
        else if let CallStack::PackageRun(_, _, assignments) = registers.call_stack_ref() {
            // Construct the call metrics.
            let metrics = CallMetrics {
                program_id: *self.program_id(),
                function_name: *function.name(),
                num_instructions: function.instructions().len(),
                num_request_constraints,
                num_function_constraints,
                num_response_constraints,
            };
            // Add the assignment to the assignments.
            assignments.write().push((assignment, metrics));
            lap!(timer, "Save the circuit assignment");
        }

        finish!(timer);

        // Return the response.
        Ok(response)
    }
}

impl<N: Network> Stack<N> {
    /// Prints the current state of the circuit.
    #[allow(unused_variables)]
    pub(crate) fn log_circuit<A: circuit::Aleo<Network = N>>(scope: impl std::fmt::Display) {
        #[cfg(debug_assertions)]
        {
            use snarkvm_utilities::dev_println;

            use colored::Colorize as _;

            // Determine if the circuit is satisfied.
            let is_satisfied = if A::is_satisfied() { "✅" } else { "❌" };
            // Determine the count.
            let (num_constant, num_public, num_private, num_constraints, num_nonzeros) = A::count();

            let scope = scope.to_string().bold();

            // Print the log.
            dev_println!(
                "{is_satisfied} {scope:width$} (Constant: {num_constant}, Public: {num_public}, Private: {num_private}, Constraints: {num_constraints}, NonZeros: {num_nonzeros:?})",
                width = 20
            );
        }
    }
}
