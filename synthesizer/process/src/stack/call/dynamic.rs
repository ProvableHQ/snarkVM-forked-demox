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

impl<N: Network> CallTrait<N> for DynamicCall<N> {
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
        let program_name = Identifier::from_field(program_name_as_field)?;

        // Get the program network.
        let Value::Plaintext(Plaintext::Literal(Literal::Field(program_network_id), _)) = &inputs[1] else {
            bail!("Expected the second operand of `call.dynamic` to be a 'Field' literal.")
        };
        let program_network = Identifier::from_field(program_network_id)?;

        // Construct the program ID.
        let program_id = ProgramID::try_from((program_name, program_network))?;

        // Get the function name.
        let Value::Plaintext(Plaintext::Literal(Literal::Field(function_name_as_field), _)) = &inputs[2] else {
            bail!("Expected the third operand of `call.dynamic` to be a 'Field' literal.")
        };
        let function_name = Identifier::from_field(function_name_as_field)?;

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
                    &function.input_types(),
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
            let console_caller = Some(*stack.program_id());
            // Evaluate the function.
            let response = substack.evaluate_function::<A, R>(call_stack, console_caller, root_tvk, rng)?;
            // Load the outputs.
            response.outputs().to_vec()
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
        let console_program_name = Identifier::from_field(&program_name_as_field.eject_value())?;

        // Get the program network.
        let circuit::Value::Plaintext(circuit::Plaintext::Literal(circuit::Literal::Field(program_network_id), _)) =
            &inputs[1]
        else {
            bail!("Expected the second operand of `call.dynamic` to be a 'Field' literal.")
        };
        let console_program_network = Identifier::from_field(&program_network_id.eject_value())?;

        // Construct the program ID.
        let console_program_id = ProgramID::try_from((console_program_name, console_program_network))?;

        // Get the function name.
        let circuit::Value::Plaintext(circuit::Plaintext::Literal(circuit::Literal::Field(function_name_as_field), _)) =
            &inputs[2]
        else {
            bail!("Expected the third operand of `call.dynamic` to be a 'Field' literal.")
        };
        let console_function_name = Identifier::from_field(&function_name_as_field.eject_value())?;

        // Separate the remaining inputs as the function inputs.
        let inputs = &inputs[3..];

        // Retrieve the optional external stack and resource.
        let (external_stack, resource) = match stack.program().id() == &console_program_id {
            // Retrieve the call stack and resource from the locator.
            false => {
                // Check the external call locator.
                let is_credits_program = &console_program_id.to_string() == "credits.aleo";
                let is_fee_private = &console_function_name.to_string() == "fee_private";
                let is_fee_public = &console_function_name.to_string() == "fee_public";

                // Ensure the external call is not to 'credits.aleo/fee_private' or 'credits.aleo/fee_public'.
                if is_credits_program && (is_fee_private || is_fee_public) {
                    bail!("Cannot perform an external call to 'credits.aleo/fee_private' or 'credits.aleo/fee_public'.")
                } else {
                    (Some(stack.get_external_stack(&console_program_id)?), console_function_name)
                }
            }
            true => {
                // TODO (howardwu): Revisit this decision to forbid calling internal functions. A record cannot be spent again.
                //  But there are legitimate uses for passing a record through to an internal function.
                //  We could invoke the internal function without a state transition, but need to match visibility.
                // TODO (@d0cd): Resolve recursion with records.
                if stack.program().contains_function(&console_function_name) {
                    bail!("Cannot dynamically execute a local '{console_function_name}' ")
                }
                (None, console_function_name)
            }
        };
        // Retrieve the substack.
        let substack = match &external_stack {
            Some(external_stack) => external_stack.as_ref(),
            None => stack,
        };
        lap!(timer, "Retrieved the substack");

        // If we are not handling the root request, retrieve the root request's tvk
        let root_tvk = registers.root_tvk().ok();

        // Retrieve the program checksum, if the program has a constructor.
        let program_checksum = match substack.program().contains_constructor() {
            true => Some(substack.program_checksum_as_field()?),
            false => None,
        };

        // If the operator is a closure, retrieve the closure and compute the output.
        let outputs = if let Ok(closure) = substack.program().get_closure(&console_function_name) {
            bail!("Cannot dynamically execute a closure.")
        }
        // If the operator is a function, retrieve the function and compute the output.
        else if let Ok(function) = substack.program().get_function(&console_function_name) {
            lap!(timer, "Execute the function");
            // Retrieve the number of inputs.
            let num_inputs = function.inputs().len();
            // Ensure the number of inputs matches the number of input statements.
            if num_inputs != inputs.len() {
                bail!("Expected {} inputs, found {}", num_inputs, inputs.len())
            }

            // Retrieve the number of public variables in the circuit.
            let num_public = A::num_public();

            // Indicate that external calls are never a root request.
            let is_root = false;

            // Eject the existing circuit.
            let r1cs = A::eject_r1cs_and_reset();
            let (request, response) = {
                // Eject the circuit inputs.
                let inputs = inputs.eject_value();

                // Set the (console) caller.
                let console_caller = Some(*stack.program_id());
                // Check if the substack has a proving key or not.
                let pk_missing = !substack.contains_proving_key(function.name());

                match registers.call_stack_ref() {
                    // If the circuit is in authorize mode, then add any external calls to the stack.
                    CallStack::Authorize(_, private_key, authorization) => {
                        // Ensure that we have a private key to sign the new request.
                        let Some(private_key) = private_key else {
                            bail!("Cannot authorize a new function call without a private key.")
                        };
                        // Compute the request.
                        let request = Request::sign(
                            private_key,
                            *substack.program_id(),
                            *function.name(),
                            inputs.iter(),
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
                        let response = substack.execute_function::<A, R>(call_stack, console_caller, root_tvk, rng)?;

                        // Return the request and response.
                        (request, response)
                    }
                    // If the proving key is missing, build real sub-circuit.
                    CallStack::Synthesize(_, private_key, ..) if pk_missing => {
                        // Compute the request.
                        let request = Request::sign(
                            private_key,
                            *substack.program_id(),
                            *function.name(),
                            inputs.iter(),
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

                        // Execute the request.
                        let response = substack.execute_function::<A, R>(call_stack, console_caller, root_tvk, rng)?;

                        // Return the request and response.
                        (request, response)
                    }
                    // In Synthesize mode (with an existing proving key) or CheckDeployment mode, we generate dummy outputs to avoid building a full sub-circuit.
                    CallStack::Synthesize(_, private_key, ..) | CallStack::CheckDeployment(_, private_key, ..) => {
                        // Compute the request.
                        let request = Request::sign(
                            private_key,
                            *substack.program_id(),
                            *function.name(),
                            inputs.iter(),
                            &function.input_types(),
                            root_tvk,
                            is_root,
                            program_checksum,
                            Some(true),
                            rng,
                        )?;

                        // Compute the address.
                        let address = Address::try_from(private_key)?;

                        // For each output, if it's a record, compute the randomizer and nonce.
                        let outputs = function
                            .outputs()
                            .iter()
                            .map(|output| match output.value_type() {
                                ValueType::Record(record_name) => {
                                    let index = match output.operand() {
                                        Operand::Register(Register::Locator(index)) => Field::from_u64(*index),
                                        _ => bail!("Expected a `Register::Locator` operand for a record output."),
                                    };
                                    // Sample the record.
                                    Ok(Value::Record(substack.sample_record_using_tvk(
                                        &address,
                                        record_name,
                                        *request.tvk(),
                                        index,
                                        rng,
                                    )?))
                                }
                                // For non-record outputs, call sample_value.
                                _ => substack.sample_value(&address, &output.value_type().into(), rng),
                            })
                            .collect::<Result<Vec<_>>>()?;

                        // Construct the dummy response from these outputs.
                        let output_registers = function
                            .outputs()
                            .iter()
                            .map(|output| match output.operand() {
                                Operand::Register(register) => Some(register.clone()),
                                _ => None,
                            })
                            .collect::<Vec<_>>();

                        // Execute the request.
                        let response = crate::Response::new(
                            request.signer(),
                            request.network_id(),
                            substack.program().id(),
                            function.name(),
                            request.inputs().len(),
                            request.tvk(),
                            request.tcm(),
                            outputs,
                            &function.output_types(),
                            &output_registers,
                        )?;

                        // Return the request and response.
                        (request, response)
                    }
                    // In PackageRun mode, we sign and execute the request once.
                    CallStack::PackageRun(_, private_key, ..) => {
                        // Compute the request.
                        let request = Request::sign(
                            private_key,
                            *substack.program_id(),
                            *function.name(),
                            inputs.iter(),
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

                        // Evaluate the request.
                        let response = substack.execute_function::<A, _>(call_stack, console_caller, root_tvk, rng)?;

                        // Return the request and response.
                        (request, response)
                    }
                    // If the circuit is in evaluate mode, then throw an error.
                    CallStack::Evaluate(..) => {
                        bail!("Cannot 'execute' a function in 'evaluate' mode.")
                    }
                    // If the circuit is in execute mode, then evaluate and execute the instructions.
                    CallStack::Execute(authorization, ..) => {
                        // Retrieve the next request (without popping it).
                        let request = authorization.peek_next()?;
                        // Ensure the inputs match the original inputs.
                        request.inputs().iter().zip_eq(&inputs).try_for_each(|(request_input, input)| {
                            ensure!(request_input == input, "Inputs do not match in a 'call' instruction.");
                            Ok(())
                        })?;

                        // Evaluate the function, and load the outputs.
                        let console_response = substack.evaluate_function::<A, R>(
                            registers.call_stack(),
                            console_caller,
                            root_tvk,
                            rng,
                        )?;
                        // Execute the request.
                        let response =
                            substack.execute_function::<A, R>(registers.call_stack(), console_caller, root_tvk, rng)?;
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

            // Inject the existing circuit.
            A::inject_r1cs(r1cs);

            use circuit::Inject;

            // Inject the network ID as `Mode::Constant`.
            let network_id = circuit::U16::constant(*request.network_id());
            // Inject the program ID name as `Mode::Public`.
            let program_id = circuit::ProgramID::new_public(console_program_id);
            // Inject the function name as `Mode::Public`.
            let function_name = circuit::Identifier::new_public(console_function_name);

            // Ensure the number of public variables remains the same.
            ensure!(A::num_public() == num_public, "Forbidden: 'call' injected excess public variables");

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
            let input_ids = request
                .input_ids()
                .iter()
                .map(|input_id| circuit::InputID::new(circuit::Mode::Public, *input_id))
                .collect::<Vec<_>>();

            // Ensure the candidate input IDs match their computed inputs.
            let (check_input_ids, _) = circuit::Request::check_input_ids::<false>(
                &network_id,
                &program_id,
                &function_name,
                &input_ids,
                &inputs,
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

            // Retrieve the output registers.
            let output_registers = function
                .outputs()
                .iter()
                .map(|output| match output.operand() {
                    Operand::Register(register) => Some(register.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();

            // Inject the outputs as `Mode::Private` (with the 'tcm' and output IDs as `Mode::Public`).
            let outputs = circuit::Response::process_outputs_from_callback(
                &network_id,
                &program_id,
                &function_name,
                num_inputs,
                &tvk,
                &tcm,
                response.outputs().to_vec(),
                &function.output_types(),
                &output_registers,
            );
            lap!(timer, "Checked the outputs");
            // Return the circuit outputs.
            outputs
        }
        // Else, throw an error.
        else {
            bail!("Call operator '{}' is invalid or unsupported.", self.operator())
        };

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
