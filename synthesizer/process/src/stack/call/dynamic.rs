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

use console::{program::compute_function_id, types::U16};

use super::*;

impl<N: Network> CallTrait<N> for CallDynamic<N> {
    /// Evaluates the instruction.
    #[inline]
    fn evaluate<A: circuit::Aleo<Network = N>, R: CryptoRng + Rng>(
        &self,
        stack: &Stack<N>,
        registers: &mut Registers<N, A>,
        rng: &mut R,
    ) -> Result<()> {
        let timer = timer!("Call::evaluate");

        // Load the operands values.
        let inputs: Vec<_> = self.operands().iter().map(|operand| registers.load(stack, operand)).try_collect()?;

        // Get the program name.
        let Value::Plaintext(Plaintext::Literal(Literal::Field(program_name_as_field), _)) = &inputs[0] else {
            bail!("Expected the first operand of `call.dynamic` to be a 'Field' literal.")
        };
        let program_name = Identifier::from_field(&program_name_as_field)?;

        // Get the program network.
        let Value::Plaintext(Plaintext::Literal(Literal::Field(program_network_id), _)) = &inputs[1] else {
            bail!("Expected the second operand of `call.dynamic` to be a 'Field' literal.")
        };
        let program_network = Identifier::from_field(&program_network_id)?;

        // Construct the program ID.
        let program_id = ProgramID::try_from((program_name, program_network))?;

        // Get the function name.
        let Value::Plaintext(Plaintext::Literal(Literal::Field(function_name_as_field), _)) = &inputs[2] else {
            bail!("Expected the third operand of `call.dynamic` to be a 'Field' literal.")
        };
        let function_name = Identifier::from_field(&function_name_as_field)?;

        // Separate the remaining inputs as the function inputs.
        let inputs = &inputs[3..];

        // Retrieve the optional external stack and resource.
        let external_stack = match stack.program().id() == &program_id {
            // Retrieve the call stack and resource from the locator.
            false => {
                // Check the external call locator.
                let is_credits_program = &program_id.to_string() == "credits.aleo";
                let is_fee_private = function_name.to_string() == "fee_private";
                let is_fee_public = &function_name.to_string() == "fee_public";

                // Ensure the external call is not to 'credits.aleo/fee_private' or 'credits.aleo/fee_public'.
                if is_credits_program && (is_fee_private || is_fee_public) {
                    bail!("Cannot perform an external call to 'credits.aleo/fee_private' or 'credits.aleo/fee_public'.")
                } else {
                    Some(stack.get_external_stack(&program_id)?)
                }
            }
            true => {
                // TODO (howardwu): Revisit this decision to forbid calling internal functions. A record cannot be spent again.
                //  But there are legitimate uses for passing a record through to an internal function.
                //  We could invoke the internal function without a state transition, but need to match visibility.
                // TODO (@d0cd): Resolve recursion with records.
                if stack.program().contains_function(&function_name) {
                    bail!("Cannot dynamically evaluate a local '{function_name}' ")
                }
                None
            }
        };
        // Retrieve the substack.
        let substack = match &external_stack {
            Some(external_stack) => external_stack.as_ref(),
            None => stack,
        };
        lap!(timer, "Retrieved the substack");

        // If the operator is a closure, retrieve the closure and compute the output.
        let outputs = if substack.program().get_closure(&function_name).is_ok() {
            // A closure cannot be dynamically called.
            bail!("Cannot dynamically evaluate a closure: {function_name}")
        }
        // If the operator is a function, retrieve the function and compute the output.
        else if let Ok(function) = substack.program().get_function(&function_name) {
            // Ensure the number of inputs matches the number of input statements.
            if function.inputs().len() != inputs.len() {
                bail!("Expected {} inputs, found {}", function.inputs().len(), inputs.len())
            }

            // Get the 'root_tvk'.
            let root_tvk = Some(registers.root_tvk()?);

            // Get the call stack.
            let mut call_stack = registers.call_stack();

            // In Authorize mode, we need to compute the new request and push it onto the call stack.
            if let CallStack::Authorize(requests, private_key, authorization) = &mut call_stack {
                // Set 'is_root'.
                let is_root = false;
                // Ensure that we have a private key to sign the new request.
                let Some(private_key) = private_key else {
                    bail!("Cannot authorize a new function call without a private key.")
                };

                // Retrieve the program checksum, if the program has a constructor.
                let program_checksum = match substack.program().contains_constructor() {
                    true => Some(substack.program_checksum_as_field()?),
                    false => None,
                };
                // Compute the request.
                let request = Request::sign(
                    private_key,
                    *substack.program_id(),
                    *function.name(),
                    inputs.iter(),
                    self.operand_types(),
                    root_tvk,
                    is_root,
                    program_checksum,
                    Some(true),
                    rng,
                )?;
                // Add the request to the requests.
                requests.push(request.clone());
                // Add the request to the authorization.
                authorization.push(request.clone())?;
            };

            // Set the (console) caller.
            let console_caller = Some((*stack.program_id(), *registers.function_name().unwrap()));
            // Evaluate the function.
            let response = substack.evaluate_function::<A, R>(call_stack, console_caller, root_tvk, rng)?;
            // Load the outputs.
            let outputs = response.outputs().to_vec();

            // Return the outputs.
            outputs
        }
        // Else, throw an error.
        else {
            bail!("Dynamic call to '{program_id}/{function_name}' is invalid or unsupported.")
        };
        lap!(timer, "Computed outputs");

        // Assign the outputs to the destination registers.
        for (output, register) in outputs.into_iter().zip_eq(&self.destinations()) {
            // Assign the output to the register.
            registers.store(stack, register, output)?;
        }
        finish!(timer);

        Ok(())
    }

    /// Executes the instruction.
    #[inline]
    fn execute<A: circuit::Aleo<Network = N>, R: CryptoRng + Rng>(
        &self,
        stack: &Stack<N>,
        registers: &mut Registers<N, A>,
        rng: &mut R,
    ) -> Result<()> {
        use circuit::Eject;

        let timer = timer!("Call::execute");

        // Load the operands values.
        let inputs: Vec<_> =
            self.operands().iter().map(|operand| registers.load_circuit(stack, operand)).try_collect()?;

        // Get the program name.
        let circuit::Value::Plaintext(circuit::Plaintext::Literal(circuit::Literal::Field(program_name_as_field), _)) =
            &inputs[0]
        else {
            bail!("Expected the first operand of `call.dynamic` to be a 'Field' literal.")
        };

        // Get the program network.
        let circuit::Value::Plaintext(circuit::Plaintext::Literal(
            circuit::Literal::Field(program_network_as_field),
            _,
        )) = &inputs[1]
        else {
            bail!("Expected the second operand of `call.dynamic` to be a 'Field' literal.")
        };

        // Get the function name.
        let circuit::Value::Plaintext(circuit::Plaintext::Literal(circuit::Literal::Field(function_name_as_field), _)) =
            &inputs[2]
        else {
            bail!("Expected the third operand of `call.dynamic` to be a 'Field' literal.")
        };

        // Separate the remaining inputs as the function inputs.
        let inputs = &inputs[3..];

        // If we are not handling the root request, retrieve the root request's tvk
        let root_tvk = registers.root_tvk().ok();

        // Retrieve the program checksum, if the program has a constructor.
        // TODO (@d0cd): Every dynamic request should take in a checksum.
        // let program_checksum = None;
        // TODO (dynamic_dispatch) added, perhaps incorrect
        let program_checksum = match stack.program().contains_constructor() {
            true => Some(stack.program_checksum_as_field()?),
            false => None,
        };

        // Resolve the program and function.
        let function = {
            let Some(target) = resolve_dynamic_target(
                registers.call_stack_ref(),
                stack,
                &program_name_as_field.eject_value(),
                &program_network_as_field.eject_value(),
                &function_name_as_field.eject_value(),
            )? else {
                bail!("Failed to resolve the target of the dynamic call in 'Authorize' mode.")
            };
            let substack = target.substack();
            substack.program().get_function_ref(target.function_name())?.clone()
        };

        let target = resolve_dynamic_target(
            registers.call_stack_ref(),
            stack,
            &program_name_as_field.eject_value(),
            &program_network_as_field.eject_value(),
            &function_name_as_field.eject_value(),
        )?;
        
        // Execute the function.
        let outputs = {
            lap!(timer, "Execute the function");

            // Retrieve the number of public variables in the circuit.
            let num_public = A::num_public();

            // Indicate that external calls are never a root request.
            let is_root = false;

            // Eject the existing circuit.
            let r1cs = A::eject_r1cs_and_reset();
            let (request, response) = {
                // Eject the circuit inputs.
                let inputs = inputs.eject_value();

                // TODO (@d0cd): Process the inputs, converting them to the appropriate record type.

                // Set the (console) caller.
                let console_caller = Some((*stack.program_id(), *registers.function_name().unwrap()));

                match registers.call_stack_ref() {
                    // If the circuit is in authorize mode, then add any external calls to the stack.
                    CallStack::Authorize(_, private_key, authorization) => {
                        // Get the target.
                        let Some(target) = target else {
                            bail!("Failed to resolve the target of the dynamic call in 'Authorize' mode.")
                        };
                        // Get the function.
                        let function = target.substack().program().get_function_ref(target.function_name())?;
                        // Retrieve the number of inputs.
                        let num_inputs = function.inputs().len();
                        // Ensure the number of inputs matches the number of input statements.
                        if num_inputs != inputs.len() {
                            bail!("Expected {} inputs, found {}", num_inputs, inputs.len())
                        }
                        // Ensure that we have a private key to sign the new request.
                        let Some(private_key) = private_key else {
                            bail!("Cannot authorize a new function call without a private key.")
                        };

                        // TODO (Antonio) this provably overlaps; just patching things up for the test
                        // TODO (dynamic_dispatch)
                        // TODO (d0cd)
                        println!("INPUTS BEFORE: {:#?}", inputs);
                        use console::program::DynamicRecord;
                        let mut inputs = inputs;
                        let child_expected_input_types = function.input_types();
                        for (child_input_value, child_expected_input_type) in inputs.iter_mut().zip_eq(child_expected_input_types) {
                            match (&child_input_value, child_expected_input_type) {
                                (Value::Record(record), ValueType::DynamicRecord) => {
                                    // TODO (dynamic_dispatch) Remove
                                    println!("Dynamic dispatch patch (Execute;in function {}): converting input static record from parent to expected dynamic record", function.name());
                                    *child_input_value = Value::DynamicRecord(DynamicRecord::from_record(record)?);
                                }
                                (Value::DynamicRecord(dynamic_record), ValueType::Record(name)) => {
                                    // TODO (dynamic_dispatch) get the right value of owner_is_private
                                    println!("Dynamic dispatch patch (Execute; in function {}): converting input dynamic from parent record to expected static record {name}", function.name());
                                    println!("    - Dynamic record data is some? {:#?}", dynamic_record.data());
                                    *child_input_value = Value::Record(dynamic_record.to_record(true)?);
                                }
                                _ => {}
                            }
                        }
                        println!("INPUTS AFTER: {:#?}", inputs);

                        // TODO (Antonio) remove
                        println!("@@@@@@@@@@@@@@@@\n\nSigning request for function {}\n\n", function.name());
                        println!("Input types:");
                        for input_type in function.input_types() {
                            println!("    OUTSIDE: input type: {:#?}", input_type);
                        }

                        // Compute the request.
                        let request = Request::sign(
                            private_key,
                            *target.substack().program_id(),
                            *function.name(),
                            inputs.iter(),
                            // TODO (dynamic_dispatch) updated from self.operand_types() to child_expected_input_types
                            &function.input_types(),
                            root_tvk,
                            is_root,
                            program_checksum,
                            Some(true),
                            rng,
                        )?;

                        // Retrieve the call stack.
                        let mut call_stack = registers.call_stack();
                        // Push the request onto the call stack.
                        call_stack.push(request.clone())?;

                        // Add the request to the authorization.
                        authorization.push(request.clone())?;

                        // Execute the request.
                        let response =
                            target.substack().execute_function::<A, R>(call_stack, console_caller, root_tvk, rng)?;

                        // Return the request and response.
                        (request, response)
                    }
                    // TODO (@d0cd): Synthesize based on the declared outputs of the instruction.
                    // In Synthesize mode (with an existing proving key) or CheckDeployment mode, we generate dummy outputs to avoid building a full sub-circuit.
                    CallStack::Synthesize(_, private_key, ..) | CallStack::CheckDeployment(_, private_key, ..) => {
                        // Sample a random program ID.
                        // Note. It does not matter what program ID we use here, since we are only synthesizing dummy outputs.
                        let program_id = if let Some(some_target) = target {
                            *some_target.substack().program_id()
                        } else {
                            ProgramID::from_str("a.aleo")?
                        };
                        // Sample a random function name.
                        // Note. It does not matter what function name we use here, since we are only synthesizing dummy outputs.
                        let function_name = *function.name();

                        // TODO (Antonio) this provably overlaps; just patching things up for the test
                        // TODO (dynamic_dispatch)
                        // TODO (d0cd)
                        println!("INPUTS BEFORE: {:#?}", inputs);
                        use console::program::DynamicRecord;
                        use indexmap::IndexMap;
                        let mut inputs = inputs;
                        let child_expected_input_types = function.input_types();
                        for (child_input_value, child_expected_input_type) in inputs.iter_mut().zip_eq(child_expected_input_types) {
                            match (&child_input_value, child_expected_input_type) {
                                (Value::Record(record), ValueType::DynamicRecord) => {
                                    // TODO (dynamic_dispatch) Remove
                                    println!("Dynamic dispatch patch (Synthesize; in function {}): converting input static record from parent to expected dynamic record", function.name());
                                    *child_input_value = Value::DynamicRecord(DynamicRecord::from_record(record)?);
                                }
                                (Value::DynamicRecord(dynamic_record), ValueType::Record(name)) => {
                                    // TODO (dynamic_dispatch) get the right value of owner_is_private
                                    // TODO (dynamic_dispatch) perhaps sampling a random record is okay
                                    let dynamic_record_with_data = DynamicRecord::new_unchecked(
                                        // TODO (dynamic_dispatch) why was the owner not set as the signer before? Is a dynamic record sampled without signer information?
                                        //                         Getting the following error during synthesis: Input record for (...) must belong to the signer
                                        Address::try_from(private_key)?,
                                        *dynamic_record.root(),
                                        *dynamic_record.nonce(),
                                        *dynamic_record.version(),
                                        dynamic_record.tree().clone(),
                                        Some(IndexMap::new())
                                    );
                                    println!("Dynamic dispatch patch (Synthesize; in function {}): converting input dynamic from parent record to expected static record {name}", function.name());
                                    println!("    - Dynamic record data is some? {:#?}", dynamic_record_with_data.data().is_some());
                                    println!("      OWNER: {:#?}", dynamic_record_with_data.owner());
                                    println!("      SIGNER: {:#?}", Address::try_from(private_key)?);

                                    *child_input_value = Value::Record(dynamic_record_with_data.to_record(true)?);
                                }
                                _ => {}
                            }
                        }
                        println!("INPUTS AFTER: {:#?}", inputs);

                        // TODO (Antonio) remove
                        println!("@@@@@@@@@@@@@@@@\n\nSigning request for function {}\n\n", function.name());
                        println!("Input types:");
                        for input_type in function.input_types() {
                            println!("    input type: {:#?}", input_type);
                        }

                        // Compute the request.
                        let request = Request::sign(
                            private_key,
                            program_id,
                            function_name,
                            inputs.iter(),
                            // TODO (dynamic_dispatch) updated from self.operand_types() to child_expected_input_types
                            &function.input_types(),
                            root_tvk,
                            is_root,
                            program_checksum,
                            Some(true),
                            rng,
                        )?;

                        // Compute the address.
                        let address = Address::try_from(private_key)?;

                        // Sample the outputs.
                        let outputs = self
                            .destination_types()
                            .iter()
                            .map(|output_type| match output_type {
                                ValueType::Record(_) => bail!("A dynamic call cannot return a record."),
                                ValueType::ExternalRecord(_) => {
                                    bail!("A dynamic call cannot return an external record.")
                                }
                                ValueType::Future(_) => bail!("A dynamic call cannot return a future."),
                                // Sample the value.
                                _ => stack.sample_value(&address, &output_type.into(), rng),
                            })
                            .collect::<Result<Vec<_>>>()?;

                        // Use `None` for the output registers. This is safe since an output of a dynamic call cannot be a record.
                        let output_registers = vec![None; outputs.len()];
                        // Execute the request.
                        let response = crate::Response::new(
                            request.signer(),
                            request.network_id(),
                            &program_id,
                            &function_name,
                            request.inputs().len(),
                            request.tvk(),
                            request.tcm(),
                            outputs,
                            &self.destination_types(),
                            &output_registers,
                            true,
                        )?;

                        // Return the request and response.
                        (request, response)
                    }
                    // In PackageRun mode, we sign and execute the request once.
                    CallStack::PackageRun(_, private_key, ..) => {
                        // Get the target.
                        let Some(target) = target else {
                            bail!("Failed to resolve the target of the dynamic call in 'Authorize' mode.")
                        };
                        // Get the function.
                        let function = target.substack().program().get_function_ref(target.function_name())?;
                        // Retrieve the number of inputs.
                        let num_inputs = function.inputs().len();
                        // Ensure the number of inputs matches the number of input statements.
                        if num_inputs != inputs.len() {
                            bail!("Expected {} inputs, found {}", num_inputs, inputs.len())
                        }
                        // Compute the request.
                        let request = Request::sign(
                            private_key,
                            *target.substack().program_id(),
                            *function.name(),
                            inputs.iter(),
                            &self.operand_types(),
                            root_tvk,
                            is_root,
                            program_checksum,
                            Some(true),
                            rng,
                        )?;

                        // Retrieve the call stack.
                        let mut call_stack = registers.call_stack();
                        // Push the request onto the call stack.
                        call_stack.push(request.clone())?;

                        // Evaluate the request.
                        let response =
                            target.substack().execute_function::<A, _>(call_stack, console_caller, root_tvk, rng)?;

                        // Return the request and response.
                        (request, response)
                    }
                    // If the circuit is in evaluate mode, then throw an error.
                    CallStack::Evaluate(..) => {
                        bail!("Cannot 'execute' a function in 'evaluate' mode.")
                    }
                    // If the circuit is in execute mode, then evaluate and execute the instructions.
                    CallStack::Execute(authorization, ..) => {
                        // Get the target.
                        let Some(target) = target else {
                            bail!("Failed to resolve the target of the dynamic call in 'Authorize' mode.")
                        };
                        // Get the function.
                        let function = target.substack().program().get_function_ref(target.function_name())?;
                        // Retrieve the number of inputs.
                        let num_inputs = function.inputs().len();
                        // Ensure the number of inputs matches the number of input statements.
                        if num_inputs != inputs.len() {
                            bail!("Expected {} inputs, found {}", num_inputs, inputs.len())
                        }

                        // Retrieve the next request (without popping it).
                        let request = authorization.peek_next()?;
                        // Ensure the inputs match the original inputs.
                        request.inputs().iter().zip_eq(&inputs).try_for_each(|(request_input, input)| {
                            ensure!(request_input == input, "Inputs do not match in a 'call' instruction.");
                            Ok(())
                        })?;

                        // Evaluate the function, and load the outputs.
                        let console_response = target.substack().evaluate_function::<A, R>(
                            registers.call_stack(),
                            console_caller,
                            root_tvk,
                            rng,
                        )?;
                        // Execute the request.
                        let response = target.substack().execute_function::<A, R>(
                            registers.call_stack(),
                            console_caller,
                            root_tvk,
                            rng,
                        )?;
                        // Ensure the values are equal.
                        if console_response.outputs() != response.outputs() {
                            dev_eprintln!("\n{:#?} != {:#?}\n", console_response.outputs(), response.outputs());
                            bail!("Function '{}' outputs do not match in a 'call' instruction.", function.name())
                        }
                        // Return the request and response.
                        (request, response)
                    }
                }
            };
            lap!(timer, "Computed the request and response");

            // Inject the existing circuit.∫
            A::inject_r1cs(r1cs);

            use circuit::Inject;

            // Inject the network ID as `Mode::Constant`.
            let network_id = circuit::U16::constant(*request.network_id());
            // Inject the program ID name as `Mode::Public`.
            let program_id = circuit::ProgramID::public(*request.program_id());
            // Inject the function name as `Mode::Public`.
            let function_name = circuit::Identifier::public(*request.function_name());
            // TODO (@d0cd) Constraint the program and function names to match the witnessed values.

            // Ensure that the program and function names in the registers match the witnessed values.
            A::assert_eq(program_id.name(), program_name_as_field);
            A::assert_eq(program_id.network(), program_network_as_field);
            A::assert_eq(&function_name, function_name_as_field);

            // Ensure the number of public variables remains the same.
            ensure!(A::num_public() == num_public + 3, "Forbidden: 'call.dynamic' injected excess public variables");

            // Inject the `signer` (from the request) as `Mode::Private`.
            let signer = circuit::Address::new(circuit::Mode::Private, *request.signer());
            // Inject the `sk_tag` (from the request) as `Mode::Private`.
            let sk_tag = circuit::Field::new(circuit::Mode::Private, *request.sk_tag());
            // Inject the `tvk` (from the request) as `Mode::Private`.
            let tvk = circuit::Field::new(circuit::Mode::Private, *request.tvk());
            // Inject the `tcm` (from the request) as `Mode::Public`.
            let tcm = circuit::Field::new(circuit::Mode::Public, *request.tcm());
            // Compute the transition commitment as `Hash(tvk)`.
            let candidate_tcm = A::hash_psd2(&[tvk.clone()]);
            // Ensure the transition commitment matches the computed transition commitment.
            A::assert_eq(&tcm, candidate_tcm);
            // Inject the input IDs (from the request) as `Mode::Public`.
            //  TODO(dynamic_dispatch) we need to inject caller input types not callee. Same for the outputs. In the case of translation.
            let input_ids = request
                .input_ids()
                .iter()
                .map(|input_id| circuit::InputID::new(circuit::Mode::Public, *input_id))
                .collect::<Vec<_>>();

            // TODO (Antonio) remove
            println!("BEFORE CHECK INPUT IDS 2 in function {}, {}", function.name(), function_name);

            // Ensure the candidate input IDs match their computed inputs.
            let (check_input_ids, _) = circuit::Request::check_input_ids::<false>(
                &network_id,
                &program_id,
                &function_name,
                &input_ids,
                inputs,
                // TODO (dynamic_dispatch) updated from self.operand_types() to child_expected_input_types. Correct?
                &function.input_types(),
                &signer,
                &sk_tag,
                &tvk,
                &tcm,
                None,
                true,
            );
            A::assert(check_input_ids);
            lap!(timer, "Checked the input ids");

            // TODO (Antonio) remove
            println!("BEFORE CHECK INPUT IDS 2");

            // Get the outputs from the response, checking that none are records.
            let outputs = response.outputs().to_vec();
            for output in &outputs {
                match output {
                    Value::Record(_) => bail!("A dynamic call cannot return a record."),
                    Value::Future(_) => bail!("A dynamic call cannot return a future."),
                    Value::Plaintext(_) | Value::DynamicRecord(_) | Value::DynamicFuture(_) => {} // Do nothing.
                }
            }

            // Use `None` for the output registers. This is safe since an output of a dynamic call cannot be a record.
            let output_registers = vec![None; outputs.len()];

            // Inject the outputs as `Mode::Private` (with the 'tcm' and output IDs as `Mode::Public`).
            let outputs = circuit::Response::process_outputs_from_callback(
                &network_id,
                &program_id,
                &function_name,
                inputs.len(),
                &tvk,
                &tcm,
                response.outputs().to_vec(),
                &self.destination_types(),
                &output_registers,
                true,
            );
            lap!(timer, "Checked the outputs");

            // Return the circuit outputs.
            outputs
        };

        // Collect record inputs to translate.
        // TODO(dynamic_dispatch): it is inconsistent to compare InputID and ValueType.
        // Consider taking InputID from the new Request, or getting the ValueType from the caller function.
        ensure!(inputs.len() == function.inputs().len(), "Expected {} inputs, found {}", function.inputs().len(), inputs.len());
        for (index, (parent_input_id, child_input)) in inputs.iter().zip(function.inputs()).enumerate() {
            match (parent_input_id.eject_value(), child_input.value_type()) {
                (Value::Record(record), ValueType::DynamicRecord) => {
                    // TODO (dynamic_dispatch) 
                    registers.insert_record_translation_argument(*id)
                    bail!("Translation case input static -> dynamic not implemented")
                },
                // TODO (dynamic_dispatch) ExternalRecord handling deferred
                // (InputID::DynamicRecord(id), ValueType::ExternalRecord(_)) => registers.insert_record_translation_argument(*id),
                (Value::DynamicRecord(dynamic_record), ValueType::Record(_)) => {
                    let function_id = compute_function_id(
                        &U16::<N>::new(N::ID),
                        stack.program_id(),
                        &function.name(),
                        // TODO (dynamic_dispatch) Is this correct?
                        true,
                    );

                    let dynamic_record_id = dynamic_record.to_id(
                        function_id?,
                        registers.tvk()?,
                        U16::<N>::new(index as u16),
                    );
                    registers.insert_record_translation_argument(dynamic_record_id?, index as u16)
                },
                _ => { } // No translation to perform.
            }
        }

        // Assign the outputs to the destination registers.
        for (output, register) in outputs.into_iter().zip_eq(&self.destinations()) {
            // Assign the output to the register.
            registers.store_circuit(stack, register, output)?;
        }
        lap!(timer, "Assigned the outputs to registers");

        finish!(timer);

        Ok(())
    }
}

// A reference to a stack, either local or external.
enum StackRef<'a, N: Network> {
    Local(&'a Stack<N>),
    External(Arc<Stack<N>>),
}

impl<'a, N: Network> Deref for StackRef<'a, N> {
    type Target = Stack<N>;

    fn deref(&self) -> &Self::Target {
        match self {
            StackRef::Local(stack) => stack,
            StackRef::External(stack) => stack.as_ref(),
        }
    }
}

// A resolved target of a dynamic call.
struct ResolvedTarget<'a, N: Network> {
    // The program ID.
    program_id: ProgramID<N>,
    // The function name.
    function_name: Identifier<N>,
    // The stack.
    substack: StackRef<'a, N>,
}

impl<'a, N: Network> ResolvedTarget<'a, N> {
    /// Returns the program ID.
    #[inline]
    pub fn program_id(&self) -> &ProgramID<N> {
        &self.program_id
    }

    /// Returns the function name.
    #[inline]
    pub fn function_name(&self) -> &Identifier<N> {
        &self.function_name
    }

    /// Returns the stack.
    #[inline]
    pub fn substack(&self) -> &Stack<N> {
        &*self.substack
    }
}

// A helper function that attempts to resolve the target of a dynamic call.
// This function returns:
// - Some(ResolvedTarget) if the target is successfully resolved.
// - Ok(None) in `Synthesize` or `CheckDeployment` mode when the target cannot be resolved.
// - Err(_) in other modes when the target cannot be resolved.
fn resolve_dynamic_target<'a, N: Network>(
    call_stack: &'a CallStack<N>,
    stack: &'a Stack<N>,
    program_name_as_field: &Field<N>,
    program_network_as_field: &Field<N>,
    function_name_as_field: &Field<N>,
) -> Result<Option<ResolvedTarget<'a, N>>> {
    // Determine whether we are in "dummy" (`Synthesize` or `CheckDeployment`) mode.
    let in_dummy_mode = match call_stack {
        CallStack::Synthesize(..) | CallStack::CheckDeployment(..) => true,
        _ => false,
    };

    // Decode the program name, exiting gracefully in dummy mode if it fails.
    let program_name = match Identifier::from_field(program_name_as_field) {
        Ok(id) => id,
        Err(_) if in_dummy_mode => {
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
    };

    // Decode the program network, exiting gracefully in dummy mode if it fails.
    let program_network = match Identifier::from_field(program_network_as_field) {
        Ok(id) => id,
        Err(_) if in_dummy_mode => {
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
    };

    // Decode the function name, exiting gracefully in dummy mode if it fails.
    let function_name = match Identifier::from_field(function_name_as_field) {
        Ok(id) => id,
        Err(_) if in_dummy_mode => {
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
    };

    // Construct the program ID.
    let program_id = match ProgramID::try_from((program_name, program_network)) {
        Ok(id) => id,
        Err(_) if in_dummy_mode => {
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
    };

    // Verify that the call is not to 'credits.aleo/fee_private' or 'credits.aleo/fee_public'.
    let is_credits_program = &program_id.to_string() == "credits.aleo";
    let is_fee_private = function_name.to_string() == "fee_private";
    let is_fee_public = &function_name.to_string() == "fee_public";
    if is_credits_program && (is_fee_private || is_fee_public) {
        bail!("Cannot perform an external call to 'credits.aleo/fee_private' or 'credits.aleo/fee_public'.")
    }

    // Retrieve the optional external stack.
    let external_stack = match stack.program().id() == &program_id {
        false => match stack.get_external_stack(&program_id) {
            Ok(ext_stack) => Some(ext_stack),
            Err(_) if in_dummy_mode => {
                return Ok(None);
            }
            Err(e) => return Err(e),
        },
        true => None,
    };

    // Retrieve the substack.
    let substack = match &external_stack {
        Some(external_stack) => StackRef::External(external_stack.clone()),
        None => StackRef::Local(stack),
    };

    // Verify that the function is not a closure.
    if substack.program().get_closure(&function_name).is_ok() {
        bail!("Cannot dynamically evaluate a closure: {function_name}")
    } else if substack.program().contains_function(&function_name) {
        Ok(Some(ResolvedTarget { program_id, function_name, substack }))
    } else if in_dummy_mode {
        Ok(None)
    } else {
        bail!("Dynamic call to '{program_id}/{function_name}' is invalid or unsupported.")
    }
}
