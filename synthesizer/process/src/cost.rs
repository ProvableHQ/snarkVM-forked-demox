// Copyright 2024 Aleo Network Foundation
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

use crate::{Process, Stack, StackProgramTypes};

use console::{
    prelude::*,
    program::{FinalizeType, Identifier, LiteralType, PlaintextType},
};
use ledger_block::{Deployment, Execution};
use synthesizer_program::{CastType, Command, Finalize, Instruction, Operand, StackProgram};

/// Returns the *minimum* cost in microcredits to publish the given deployment (total cost, (storage cost, synthesis cost, namespace cost)).
pub fn deployment_cost<N: Network>(deployment: &Deployment<N>) -> Result<(u64, (u64, u64, u64))> {
    // Determine the number of bytes in the deployment.
    let size_in_bytes = deployment.size_in_bytes()?;
    // Retrieve the program ID.
    let program_id = deployment.program_id();
    // Determine the number of characters in the program ID.
    let num_characters = u32::try_from(program_id.name().to_string().len())?;
    // Compute the number of combined variables in the program.
    let num_combined_variables = deployment.num_combined_variables()?;
    // Compute the number of combined constraints in the program.
    let num_combined_constraints = deployment.num_combined_constraints()?;

    // Compute the storage cost in microcredits.
    let storage_cost = size_in_bytes
        .checked_mul(N::DEPLOYMENT_FEE_MULTIPLIER)
        .ok_or(anyhow!("The storage cost computation overflowed for a deployment"))?;

    // Compute the synthesis cost in microcredits.
    let synthesis_cost = num_combined_variables.saturating_add(num_combined_constraints) * N::SYNTHESIS_FEE_MULTIPLIER;

    // Compute the namespace cost in credits: 10^(10 - num_characters).
    let namespace_cost = 10u64
        .checked_pow(10u32.saturating_sub(num_characters))
        .ok_or(anyhow!("The namespace cost computation overflowed for a deployment"))?
        .saturating_mul(1_000_000); // 1 microcredit = 1e-6 credits.

    // Compute the total cost in microcredits.
    let total_cost = storage_cost
        .checked_add(synthesis_cost)
        .and_then(|x| x.checked_add(namespace_cost))
        .ok_or(anyhow!("The total cost computation overflowed for a deployment"))?;

    Ok((total_cost, (storage_cost, synthesis_cost, namespace_cost)))
}

/// Returns the *minimum* cost in microcredits to publish the given execution (total cost, (storage cost, finalize cost)).
pub fn execution_cost<N: Network>(process: &Process<N>, execution: &Execution<N>) -> Result<(u64, (u64, u64))> {
    // Compute the storage cost in microcredits.
    let storage_cost = execution_storage_cost::<N>(execution.size_in_bytes()?);

    // Get the root transition.
    let transition = execution.peek()?;

    // Get the finalize cost for the root transition.
    let finalize_cost = process.get_stack(transition.program_id())?.get_finalize_cost(transition.function_name())?;

    // Compute the total cost in microcredits.
    let total_cost = storage_cost
        .checked_add(finalize_cost)
        .ok_or(anyhow!("The total cost computation overflowed for an execution"))?;

    Ok((total_cost, (storage_cost, finalize_cost)))
}

/// Returns the storage cost in microcredits for a program execution.
fn execution_storage_cost<N: Network>(size_in_bytes: u64) -> u64 {
    if size_in_bytes > N::EXECUTION_STORAGE_PENALTY_THRESHOLD {
        size_in_bytes.saturating_mul(size_in_bytes).saturating_div(N::EXECUTION_STORAGE_FEE_SCALING_FACTOR)
    } else {
        size_in_bytes
    }
}

/// Finalize costs for compute heavy operations, derived as:
/// `BASE_COST + (PER_BYTE_COST * SIZE_IN_BYTES)`.

const CAST_BASE_COST: u64 = 500;
const CAST_PER_BYTE_COST: u64 = 30;

const HASH_BASE_COST: u64 = 10_000;
const HASH_PER_BYTE_COST: u64 = 30;

const HASH_BHP_BASE_COST: u64 = 50_000;
const HASH_BHP_PER_BYTE_COST: u64 = 300;

const HASH_PSD_BASE_COST: u64 = 40_000;
const HASH_PSD_PER_BYTE_COST: u64 = 75;

const MAPPING_BASE_COST: u64 = 500; // 500;
const MAPPING_PER_BYTE_COST: u64 = 10;

const SET_BASE_COST: u64 = 10_000;
const SET_PER_BYTE_COST: u64 = 100;

/// A helper function to determine the plaintext type in bytes.
fn plaintext_size_in_bytes<N: Network>(stack: &Stack<N>, plaintext_type: &PlaintextType<N>) -> Result<u64> {
    match plaintext_type {
        PlaintextType::Literal(literal_type) => Ok(literal_type.size_in_bytes::<N>() as u64),
        PlaintextType::Struct(struct_name) => {
            // Retrieve the struct from the stack.
            let struct_ = stack.program().get_struct(struct_name)?;
            // Retrieve the size of the struct name.
            let size_of_name = struct_.name().to_bytes_le()?.len() as u64;
            // Retrieve the size of all the members of the struct.
            let size_of_members = struct_.members().iter().try_fold(0u64, |acc, (_, member_type)| {
                acc.checked_add(plaintext_size_in_bytes(stack, member_type)?).ok_or(anyhow!(
                    "Overflowed while computing the size of the struct '{}/{struct_name}' - {member_type}",
                    stack.program_id()
                ))
            })?;
            // Return the size of the struct.
            Ok(size_of_name.saturating_add(size_of_members))
        }
        PlaintextType::Array(array_type) => {
            // Retrieve the number of elements in the array.
            let num_elements = **array_type.length() as u64;
            // Compute the size of an array element.
            let size_of_element = plaintext_size_in_bytes(stack, array_type.next_element_type())?;
            // Return the size of the array.
            Ok(num_elements.saturating_mul(size_of_element))
        }
    }
}

/// A helper function to compute the following: base_cost + (byte_multiplier * size_of_operands).
fn cost_in_size<'a, N: Network>(
    stack: &Stack<N>,
    finalize: &Finalize<N>,
    operands: impl IntoIterator<Item = &'a Operand<N>>,
    byte_multiplier: u64,
    base_cost: u64,
) -> Result<u64> {
    // Retrieve the finalize types.
    let finalize_types = stack.get_finalize_types(finalize.name())?;
    // Compute the size of the operands.
    let size_of_operands = operands.into_iter().try_fold(0u64, |acc, operand| {
        // Determine the size of the operand.
        let operand_size = match finalize_types.get_type_from_operand(stack, operand)? {
            FinalizeType::Plaintext(plaintext_type) => plaintext_size_in_bytes(stack, &plaintext_type)?,
            FinalizeType::Future(future) => {
                bail!("Future '{future}' is not a valid operand in the finalize scope");
            }
        };
        // Safely add the size to the accumulator.
        acc.checked_add(operand_size).ok_or(anyhow!(
            "Overflowed while computing the size of the operand '{operand}' in '{}/{}' (finalize)",
            stack.program_id(),
            finalize.name()
        ))
    })?;
    // Return the cost.
    Ok(base_cost.saturating_add(byte_multiplier.saturating_mul(size_of_operands)))
}

/// Returns the the cost of a command in a finalize scope.
pub fn cost_per_command<N: Network>(stack: &Stack<N>, finalize: &Finalize<N>, command: &Command<N>) -> Result<u64> {
    match command {
        Command::Instruction(Instruction::Abs(_)) => Ok(500),
        Command::Instruction(Instruction::AbsWrapped(_)) => Ok(500),
        Command::Instruction(Instruction::Add(_)) => Ok(500),
        Command::Instruction(Instruction::AddWrapped(_)) => Ok(500),
        Command::Instruction(Instruction::And(_)) => Ok(500),
        Command::Instruction(Instruction::AssertEq(_)) => Ok(500),
        Command::Instruction(Instruction::AssertNeq(_)) => Ok(500),
        Command::Instruction(Instruction::Async(_)) => bail!("'async' is not supported in finalize"),
        Command::Instruction(Instruction::Call(_)) => bail!("'call' is not supported in finalize"),
        Command::Instruction(Instruction::Cast(cast)) => match cast.cast_type() {
            CastType::Plaintext(PlaintextType::Literal(_)) => Ok(500),
            CastType::Plaintext(plaintext_type) => Ok(plaintext_size_in_bytes(stack, plaintext_type)?
                .saturating_mul(CAST_PER_BYTE_COST)
                .saturating_add(CAST_BASE_COST)),
            CastType::GroupXCoordinate
            | CastType::GroupYCoordinate
            | CastType::Record(_)
            | CastType::ExternalRecord(_) => Ok(500),
        },
        Command::Instruction(Instruction::CastLossy(cast_lossy)) => match cast_lossy.cast_type() {
            CastType::Plaintext(PlaintextType::Literal(_)) => Ok(500),
            CastType::Plaintext(plaintext_type) => Ok(plaintext_size_in_bytes(stack, plaintext_type)?
                .saturating_mul(CAST_PER_BYTE_COST)
                .saturating_add(CAST_BASE_COST)),
            CastType::GroupXCoordinate
            | CastType::GroupYCoordinate
            | CastType::Record(_)
            | CastType::ExternalRecord(_) => Ok(500),
        },
        Command::Instruction(Instruction::CommitBHP256(commit)) => {
            cost_in_size(stack, finalize, commit.operands(), HASH_BHP_PER_BYTE_COST, HASH_BHP_BASE_COST)
        }
        Command::Instruction(Instruction::CommitBHP512(commit)) => {
            cost_in_size(stack, finalize, commit.operands(), HASH_BHP_PER_BYTE_COST, HASH_BHP_BASE_COST)
        }
        Command::Instruction(Instruction::CommitBHP768(commit)) => {
            cost_in_size(stack, finalize, commit.operands(), HASH_BHP_PER_BYTE_COST, HASH_BHP_BASE_COST)
        }
        Command::Instruction(Instruction::CommitBHP1024(commit)) => {
            cost_in_size(stack, finalize, commit.operands(), HASH_BHP_PER_BYTE_COST, HASH_BHP_BASE_COST)
        }
        Command::Instruction(Instruction::CommitPED64(commit)) => {
            cost_in_size(stack, finalize, commit.operands(), HASH_PER_BYTE_COST, HASH_BASE_COST)
        }
        Command::Instruction(Instruction::CommitPED128(commit)) => {
            cost_in_size(stack, finalize, commit.operands(), HASH_PER_BYTE_COST, HASH_BASE_COST)
        }
        Command::Instruction(Instruction::Div(div)) => {
            // Ensure `div` has exactly two operands.
            ensure!(div.operands().len() == 2, "'div' must contain exactly 2 operands");
            // Retrieve the finalize types.
            let finalize_types = stack.get_finalize_types(finalize.name())?;
            // Retrieve the price by the operand type.
            match finalize_types.get_type_from_operand(stack, &div.operands()[0])? {
                FinalizeType::Plaintext(PlaintextType::Literal(LiteralType::Field)) => Ok(1_500),
                FinalizeType::Plaintext(PlaintextType::Literal(_)) => Ok(500),
                FinalizeType::Plaintext(PlaintextType::Array(_)) => bail!("'div' does not support arrays"),
                FinalizeType::Plaintext(PlaintextType::Struct(_)) => bail!("'div' does not support structs"),
                FinalizeType::Future(_) => bail!("'div' does not support futures"),
            }
        }
        Command::Instruction(Instruction::DivWrapped(_)) => Ok(500),
        Command::Instruction(Instruction::Double(_)) => Ok(500),
        Command::Instruction(Instruction::GreaterThan(_)) => Ok(500),
        Command::Instruction(Instruction::GreaterThanOrEqual(_)) => Ok(500),
        Command::Instruction(Instruction::HashBHP256(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_BHP_PER_BYTE_COST, HASH_BHP_BASE_COST)
        }
        Command::Instruction(Instruction::HashBHP512(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_BHP_PER_BYTE_COST, HASH_BHP_BASE_COST)
        }
        Command::Instruction(Instruction::HashBHP768(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_BHP_PER_BYTE_COST, HASH_BHP_BASE_COST)
        }
        Command::Instruction(Instruction::HashBHP1024(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_BHP_PER_BYTE_COST, HASH_BHP_BASE_COST)
        }
        Command::Instruction(Instruction::HashKeccak256(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_PER_BYTE_COST, HASH_BASE_COST)
        }
        Command::Instruction(Instruction::HashKeccak384(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_PER_BYTE_COST, HASH_BASE_COST)
        }
        Command::Instruction(Instruction::HashKeccak512(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_PER_BYTE_COST, HASH_BASE_COST)
        }
        Command::Instruction(Instruction::HashPED64(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_PER_BYTE_COST, HASH_BASE_COST)
        }
        Command::Instruction(Instruction::HashPED128(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_PER_BYTE_COST, HASH_BASE_COST)
        }
        Command::Instruction(Instruction::HashPSD2(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_PSD_PER_BYTE_COST, HASH_PSD_BASE_COST)
        }
        Command::Instruction(Instruction::HashPSD4(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_PSD_PER_BYTE_COST, HASH_PSD_BASE_COST)
        }
        Command::Instruction(Instruction::HashPSD8(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_PSD_PER_BYTE_COST, HASH_PSD_BASE_COST)
        }
        Command::Instruction(Instruction::HashSha3_256(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_PER_BYTE_COST, HASH_BASE_COST)
        }
        Command::Instruction(Instruction::HashSha3_384(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_PER_BYTE_COST, HASH_BASE_COST)
        }
        Command::Instruction(Instruction::HashSha3_512(hash)) => {
            cost_in_size(stack, finalize, hash.operands(), HASH_PER_BYTE_COST, HASH_BASE_COST)
        }
        Command::Instruction(Instruction::HashManyPSD2(_)) => {
            bail!("`hash_many.psd2` is not supported in finalize")
        }
        Command::Instruction(Instruction::HashManyPSD4(_)) => {
            bail!("`hash_many.psd4` is not supported in finalize")
        }
        Command::Instruction(Instruction::HashManyPSD8(_)) => {
            bail!("`hash_many.psd8` is not supported in finalize")
        }
        Command::Instruction(Instruction::Inv(_)) => Ok(2_500),
        Command::Instruction(Instruction::IsEq(_)) => Ok(500),
        Command::Instruction(Instruction::IsNeq(_)) => Ok(500),
        Command::Instruction(Instruction::LessThan(_)) => Ok(500),
        Command::Instruction(Instruction::LessThanOrEqual(_)) => Ok(500),
        Command::Instruction(Instruction::Modulo(_)) => Ok(500),
        Command::Instruction(Instruction::Mul(mul)) => {
            // Ensure `mul` has exactly two operands.
            ensure!(mul.operands().len() == 2, "'mul' must contain exactly 2 operands");
            // Retrieve the finalize types.
            let finalize_types = stack.get_finalize_types(finalize.name())?;
            // Retrieve the price by operand type.
            match finalize_types.get_type_from_operand(stack, &mul.operands()[0])? {
                FinalizeType::Plaintext(PlaintextType::Literal(LiteralType::Group)) => Ok(10_000),
                FinalizeType::Plaintext(PlaintextType::Literal(LiteralType::Scalar)) => Ok(10_000),
                FinalizeType::Plaintext(PlaintextType::Literal(_)) => Ok(500),
                FinalizeType::Plaintext(PlaintextType::Array(_)) => bail!("'mul' does not support arrays"),
                FinalizeType::Plaintext(PlaintextType::Struct(_)) => bail!("'mul' does not support structs"),
                FinalizeType::Future(_) => bail!("'mul' does not support futures"),
            }
        }
        Command::Instruction(Instruction::MulWrapped(_)) => Ok(500),
        Command::Instruction(Instruction::Nand(_)) => Ok(500),
        Command::Instruction(Instruction::Neg(_)) => Ok(500),
        Command::Instruction(Instruction::Nor(_)) => Ok(500),
        Command::Instruction(Instruction::Not(_)) => Ok(500),
        Command::Instruction(Instruction::Or(_)) => Ok(500),
        Command::Instruction(Instruction::Pow(pow)) => {
            // Ensure `pow` has at least one operand.
            ensure!(!pow.operands().is_empty(), "'pow' must contain at least 1 operand");
            // Retrieve the finalize types.
            let finalize_types = stack.get_finalize_types(finalize.name())?;
            // Retrieve the price by operand type.
            match finalize_types.get_type_from_operand(stack, &pow.operands()[0])? {
                FinalizeType::Plaintext(PlaintextType::Literal(LiteralType::Field)) => Ok(1_500),
                FinalizeType::Plaintext(PlaintextType::Literal(_)) => Ok(500),
                FinalizeType::Plaintext(PlaintextType::Array(_)) => bail!("'pow' does not support arrays"),
                FinalizeType::Plaintext(PlaintextType::Struct(_)) => bail!("'pow' does not support structs"),
                FinalizeType::Future(_) => bail!("'pow' does not support futures"),
            }
        }
        Command::Instruction(Instruction::PowWrapped(_)) => Ok(500),
        Command::Instruction(Instruction::Rem(_)) => Ok(500),
        Command::Instruction(Instruction::RemWrapped(_)) => Ok(500),
        Command::Instruction(Instruction::SignVerify(sign)) => {
            cost_in_size(stack, finalize, sign.operands(), HASH_PSD_PER_BYTE_COST, HASH_PSD_BASE_COST)
        }
        Command::Instruction(Instruction::Shl(_)) => Ok(500),
        Command::Instruction(Instruction::ShlWrapped(_)) => Ok(500),
        Command::Instruction(Instruction::Shr(_)) => Ok(500),
        Command::Instruction(Instruction::ShrWrapped(_)) => Ok(500),
        Command::Instruction(Instruction::Square(_)) => Ok(500),
        Command::Instruction(Instruction::SquareRoot(_)) => Ok(2_500),
        Command::Instruction(Instruction::Sub(_)) => Ok(500),
        Command::Instruction(Instruction::SubWrapped(_)) => Ok(500),
        Command::Instruction(Instruction::Ternary(_)) => Ok(500),
        Command::Instruction(Instruction::Xor(_)) => Ok(500),
        Command::Await(_) => Ok(500),
        Command::Contains(command) => {
            cost_in_size(stack, finalize, [command.key()], MAPPING_PER_BYTE_COST, MAPPING_BASE_COST)
        }
        Command::Get(command) => {
            cost_in_size(stack, finalize, [command.key()], MAPPING_PER_BYTE_COST, MAPPING_BASE_COST)
        }
        Command::GetOrUse(command) => {
            cost_in_size(stack, finalize, [command.key()], MAPPING_PER_BYTE_COST, MAPPING_BASE_COST)
        }
        Command::RandChaCha(_) => Ok(25_000),
        Command::Remove(_) => Ok(MAPPING_BASE_COST),
        Command::Set(command) => {
            cost_in_size(stack, finalize, [command.key(), command.value()], SET_PER_BYTE_COST, SET_BASE_COST)
        }
        Command::BranchEq(_) | Command::BranchNeq(_) => Ok(500),
        Command::Position(_) => Ok(100),
    }
}

/// Returns the minimum number of microcredits required to run the finalize.
pub fn cost_in_microcredits<N: Network>(stack: &Stack<N>, function_name: &Identifier<N>) -> Result<u64> {
    // Retrieve the finalize logic.
    let Some(finalize) = stack.get_function_ref(function_name)?.finalize_logic() else {
        // Return a finalize cost of 0, if the function does not have a finalize scope.
        return Ok(0);
    };
    // Get the cost of finalizing all futures.
    let mut future_cost = 0u64;
    for input in finalize.inputs() {
        if let FinalizeType::Future(future) = input.finalize_type() {
            // Get the external stack for the future.
            let stack = stack.get_external_stack(future.program_id())?;
            // Accumulate the finalize cost of the future.
            future_cost = future_cost
                .checked_add(stack.get_finalize_cost(future.resource())?)
                .ok_or(anyhow!("Finalize cost overflowed"))?;
        }
    }
    // Aggregate the cost of all commands in the program.
    finalize
        .commands()
        .iter()
        .map(|command| cost_per_command(stack, finalize, command))
        .try_fold(future_cost, |acc, res| {
            res.and_then(|x| acc.checked_add(x).ok_or(anyhow!("Finalize cost overflowed")))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::get_execution;

    use console::network::{CanaryV0, MainnetV0, TestnetV0};
    use ledger_block::Transaction;
    use synthesizer_program::Program;

    // Test program with two functions just below and above the size threshold.
    const SIZE_BOUNDARY_PROGRAM: &str = r#"
program size_boundary.aleo;

function under_five_thousand:
    input r0 as group.public;
    cast r0 r0 r0 r0 r0 r0 r0 r0 r0 into r1 as [group; 9u32];
    cast r1 r1 r1 r1 r1 r1 r1 r1 r1 r1 into r2 as [[group; 9u32]; 10u32];
    cast r0 r0 r0 r0 r0 r0 r0 into r3 as [group; 7u32];
    output r2 as [[group; 9u32]; 10u32].public;
    output r3 as [group; 7u32].public;

function over_five_thousand:
    input r0 as group.public;
    cast r0 r0 r0 r0 r0 r0 r0 r0 r0 into r1 as [group; 9u32];
    cast r1 r1 r1 r1 r1 r1 r1 r1 r1 r1 into r2 as [[group; 9u32]; 10u32];
    cast r0 r0 r0 r0 r0 r0 r0 into r3 as [group; 7u32];
    output r2 as [[group; 9u32]; 10u32].public;
    output r3 as [group; 7u32].public;
    output 5u64 as u64.public;
    "#;
    // Cost for a program +1 byte above the threshold.
    const STORAGE_COST_ABOVE_THRESHOLD: u64 = 5002;
    // Storage cost for an execution transaction at the maximum transaction size.
    const STORAGE_COST_MAX: u64 = 3_276_800;

    fn test_storage_cost_bounds<N: Network>() {
        // Calculate the bounds directly above and below the size threshold.
        let threshold = N::EXECUTION_STORAGE_PENALTY_THRESHOLD;
        let threshold_lower_offset = threshold.saturating_sub(1);
        let threshold_upper_offset = threshold.saturating_add(1);

        // Test the storage cost bounds.
        assert_eq!(execution_storage_cost::<N>(0), 0);
        assert_eq!(execution_storage_cost::<N>(1), 1);
        assert_eq!(execution_storage_cost::<N>(threshold_lower_offset), threshold_lower_offset);
        assert_eq!(execution_storage_cost::<N>(threshold), threshold);
        assert_eq!(execution_storage_cost::<N>(threshold_upper_offset), STORAGE_COST_ABOVE_THRESHOLD);
        assert_eq!(execution_storage_cost::<N>(N::MAX_TRANSACTION_SIZE as u64), STORAGE_COST_MAX);
    }

    #[test]
    fn test_storage_cost_bounds_for_all_networks() {
        test_storage_cost_bounds::<CanaryV0>();
        test_storage_cost_bounds::<MainnetV0>();
        test_storage_cost_bounds::<TestnetV0>();
    }

    #[test]
    fn test_token_registry_cost() {
      // Initialize the process.
      let mut process = Process::<MainnetV0>::load().unwrap();

      // Fetch the large program to deploy.
      let program = Program::<MainnetV0>::from_str(include_str!("./resources/token_registry.aleo")).unwrap();

      // Add the program to the process
      process.add_program(&program).unwrap();

      let transaction = Transaction::<MainnetV0>::from_str(include_str!("./resources/transfer_public.txt")).unwrap();
      let execution = transaction.execution().unwrap();

      let (total_cost, (storage_cost, execution_cost)) = execution_cost(&process, &execution).unwrap();

      // Print all the costs
      println!("Total cost: {}", total_cost);
      println!("Storage cost: {}", storage_cost);
      println!("Execution cost: {}", execution_cost);
    }

    #[test]
    fn test_vlink_cost() {
      // Initialize the process.
      let mut process = Process::<MainnetV0>::load().unwrap();

      // Fetch the large program to deploy.
      let program = Program::<MainnetV0>::from_str(include_str!("./resources/token_registry.aleo")).unwrap();
      let program2 = Program::<MainnetV0>::from_str(include_str!("./resources/vlink_claim.aleo")).unwrap();

      // Add the program to the process
      process.add_program(&program).unwrap();
      process.add_program(&program2).unwrap();

      let transaction = Transaction::<MainnetV0>::from_str(include_str!("./resources/vlink_claim_tx.txt")).unwrap();
      let execution = transaction.execution().unwrap();

      let (total_cost, (storage_cost, execution_cost)) = execution_cost(&process, &execution).unwrap();

      // Print all the costs
      println!("Total cost: {}", total_cost);
      println!("Storage cost: {}", storage_cost);
      println!("Execution cost: {}", execution_cost);
    }

    #[test]
    fn test_pondo_cost() {
      // Initialize the process.
      let mut process = Process::<MainnetV0>::load().unwrap();

      // Fetch the large program to deploy.
      let program = Program::<MainnetV0>::from_str(include_str!("./resources/token_registry.aleo")).unwrap();
      let program2 = Program::<MainnetV0>::from_str(include_str!("./resources/vlink_claim.aleo")).unwrap();
      let program3 = Program::<MainnetV0>::from_str(include_str!("./resources/wrapped_credits.aleo")).unwrap();
      let program4 = Program::<MainnetV0>::from_str(include_str!("./resources/validator_oracle.aleo")).unwrap();
      let program5 = Program::<MainnetV0>::from_str(include_str!("./resources/delegator1.aleo")).unwrap();
      let program6 = Program::<MainnetV0>::from_str(include_str!("./resources/delegator2.aleo")).unwrap();
      let program7 = Program::<MainnetV0>::from_str(include_str!("./resources/delegator3.aleo")).unwrap();
      let program8 = Program::<MainnetV0>::from_str(include_str!("./resources/delegator4.aleo")).unwrap();
      let program9 = Program::<MainnetV0>::from_str(include_str!("./resources/delegator5.aleo")).unwrap();
      let program10 = Program::<MainnetV0>::from_str(include_str!("./resources/paleo_token.aleo")).unwrap();
      let program11 = Program::<MainnetV0>::from_str(include_str!("./resources/pondo_protocol_token.aleo")).unwrap();
      let program12 = Program::<MainnetV0>::from_str(include_str!("./resources/pondo_protocol.aleo")).unwrap();

      // Add the program to the process
      process.add_program(&program).unwrap();
      process.add_program(&program2).unwrap();
      process.add_program(&program3).unwrap();
      process.add_program(&program4).unwrap();
      process.add_program(&program5).unwrap();
      process.add_program(&program6).unwrap();
      process.add_program(&program7).unwrap();
      process.add_program(&program8).unwrap();
      process.add_program(&program9).unwrap();
      process.add_program(&program10).unwrap();
      process.add_program(&program11).unwrap();
      process.add_program(&program12).unwrap();

      let transaction = Transaction::<MainnetV0>::from_str(include_str!("./resources/pondo_deposit.txt")).unwrap();
      let execution = transaction.execution().unwrap();

      let (total_cost, (storage_cost, execution_cost)) = execution_cost(&process, &execution).unwrap();

      // Print all the costs
      println!("Total cost: {}", total_cost);
      println!("Storage cost: {}", storage_cost);
      println!("Execution cost: {}", execution_cost);
    }

    #[test]
    fn test_validator_oracle_cost() {
      // Initialize the process.
      let mut process = Process::<MainnetV0>::load().unwrap();

      // Fetch the large program to deploy.
      let program = Program::<MainnetV0>::from_str(include_str!("./resources/validator_oracle.aleo")).unwrap();

      // Add the program to the process
      process.add_program(&program).unwrap();

      let transaction = Transaction::<MainnetV0>::from_str(include_str!("./resources/update_data.txt")).unwrap();
      let execution = transaction.execution().unwrap();

      let (total_cost, (storage_cost, execution_cost)) = execution_cost(&process, &execution).unwrap();

      // Print all the costs
      println!("Total cost: {}", total_cost);
      println!("Storage cost: {}", storage_cost);
      println!("Execution cost: {}", execution_cost);
    }

    #[test]
    fn test_tx_cost() {
      let transaction = r#"{
    "type": "execute",
    "id": "at1xwxqsqtzh3y7fkx3uyt2mtkdlta7szw938dj0qf49jxl25c4zvfqe32fla",
    "execution": {
        "transitions": [
            {
                "id": "au14zgfrh4d59k4g3r95f7x703k5qlxkd3l3yn3mm09n46fhqdxngxqsudntq",
                "program": "credits.aleo",
                "function": "transfer_public_as_signer",
                "inputs": [
                    {
                        "type": "public",
                        "id": "1746390454163988750063166698377630712453394630540231871254036859480344848901field",
                        "value": "aleo1tjkv7vquk6yldxz53ecwsy5csnun43rfaknpkjc97v5223dlnyxsglv7nm"
                    },
                    {
                        "type": "public",
                        "id": "6809406067451222365049973293192742918874595358013627307809674989418890555703field",
                        "value": "1000000u64"
                    }
                ],
                "outputs": [
                    {
                        "type": "future",
                        "id": "1798878455985144152563307768930848820879230250724456572015677481311610276461field",
                        "value": "{\n  program_id: credits.aleo,\n  function_name: transfer_public_as_signer,\n  arguments: [\n    aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm,\n    aleo1tjkv7vquk6yldxz53ecwsy5csnun43rfaknpkjc97v5223dlnyxsglv7nm,\n    1000000u64\n  ]\n}"
                    }
                ],
                "tpk": "771968463219726583644834542667723453765374518636061781187100416966160018348group",
                "tcm": "7419637889273028190561035304533017254330199460294342264203336511056222712473field",
                "scm": "4578870646840355930698894916219770549405836460533899125207148438741684637412field"
            },
            {
                "id": "au1mrt5nh8m76msvcdvjr3zlee20ncj8u8cdsz7c2ndptkz8lq7n5rssmz7mu",
                "program": "token_registry.aleo",
                "function": "mint_public",
                "inputs": [
                    {
                        "type": "public",
                        "id": "1439966549852373189758225041146170667725503107109506094239051466153478616905field",
                        "value": "3443843282313283355522573239085696902919850365217539366784739393210722344986field"
                    },
                    {
                        "type": "public",
                        "id": "1696260835997195713127796402673088412048043209695384433697027419461454930551field",
                        "value": "aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm"
                    },
                    {
                        "type": "public",
                        "id": "828598675908178297499644542679894440541590094120259367704482944932272716871field",
                        "value": "1000000u128"
                    },
                    {
                        "type": "public",
                        "id": "7160566684282393504271071222168689424159605342372585476475655535323765481419field",
                        "value": "4294967295u32"
                    }
                ],
                "outputs": [
                    {
                        "type": "future",
                        "id": "1553263928259168973731514395308793098845594223067594743978768099097022205136field",
                        "value": "{\n  program_id: token_registry.aleo,\n  function_name: mint_public,\n  arguments: [\n    3443843282313283355522573239085696902919850365217539366784739393210722344986field,\n    aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm,\n    1000000u128,\n    4294967295u32,\n    aleo1tjkv7vquk6yldxz53ecwsy5csnun43rfaknpkjc97v5223dlnyxsglv7nm,\n    5783861720504029593520331872442756678068735468923730684279741068753131773333field,\n    4339750626578500203528653953873890933250112957639433785431875614975442816931field\n  ]\n}"
                    }
                ],
                "tpk": "4892793766038399706863414482920198219981398149640173834623970925590211317524group",
                "tcm": "2410262646503508140362424809650805567190410390558619684736713882932618454812field",
                "scm": "4578870646840355930698894916219770549405836460533899125207148438741684637412field"
            },
            {
                "id": "au1zl0d6d8jcuxdmr9xatv9qruqr0m23l2a68xuj4vwe6fftu30xgyq00x922",
                "program": "wrapped_credits.aleo",
                "function": "deposit_credits_public_signer",
                "inputs": [
                    {
                        "type": "public",
                        "id": "7776766217932017886124631422746720188925867859544931894624912175539206535184field",
                        "value": "1000000u64"
                    }
                ],
                "outputs": [
                    {
                        "type": "future",
                        "id": "1758941001712937315335570772158972483402322070988931314374583892784257454811field",
                        "value": "{\n  program_id: wrapped_credits.aleo,\n  function_name: deposit_credits_public_signer,\n  arguments: [\n    {\n      program_id: credits.aleo,\n      function_name: transfer_public_as_signer,\n      arguments: [\n        aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm,\n        aleo1tjkv7vquk6yldxz53ecwsy5csnun43rfaknpkjc97v5223dlnyxsglv7nm,\n        1000000u64\n      ]\n    },\n    {\n      program_id: token_registry.aleo,\n      function_name: mint_public,\n      arguments: [\n        3443843282313283355522573239085696902919850365217539366784739393210722344986field,\n        aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm,\n        1000000u128,\n        4294967295u32,\n        aleo1tjkv7vquk6yldxz53ecwsy5csnun43rfaknpkjc97v5223dlnyxsglv7nm,\n        5783861720504029593520331872442756678068735468923730684279741068753131773333field,\n        4339750626578500203528653953873890933250112957639433785431875614975442816931field\n      ]\n    }\n  \n  ]\n}"
                    }
                ],
                "tpk": "1569860901869045677974959111327155862884422144428222577393450168835175589032group",
                "tcm": "3552516249267844303273392015864434008671027760010855896046753252046334665990field",
                "scm": "4578870646840355930698894916219770549405836460533899125207148438741684637412field"
            },
            {
                "id": "au15lyjs4vcz49n3qm67t06tp7pnal67we56p0hgycnfkaur9399yqs7hc5gv",
                "program": "token_registry.aleo",
                "function": "transfer_from_public_to_private",
                "inputs": [
                    {
                        "type": "public",
                        "id": "4993113950860626238958125210113758450465424180309618524012136166230582200864field",
                        "value": "3443843282313283355522573239085696902919850365217539366784739393210722344986field"
                    },
                    {
                        "type": "public",
                        "id": "7713175788615977103688508567217956111522554803546974921243949791675814094125field",
                        "value": "aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm"
                    },
                    {
                        "type": "private",
                        "id": "6597003386686008987135064781944779206238686254076017570938216281262396802312field",
                        "value": "ciphertext1qgqz8d7tllkk9zk46ns48vy2mdrhpx4t86s5aqcxprpm8caaw2cz6pufjrqtrw52x2fv87ugymfg9dlywhzsgayrqul5kufa8su65md3pyahyd9q"
                    },
                    {
                        "type": "public",
                        "id": "1745686336528666595177292998182744652909703223147920328162986089426783939755field",
                        "value": "1000000u128"
                    },
                    {
                        "type": "public",
                        "id": "5010127674071748072689675643206066621412477038393018810925201156758702891506field",
                        "value": "false"
                    }
                ],
                "outputs": [
                    {
                        "type": "record",
                        "id": "5165744803725039386994323139386691827023030163650747782791071214439333733995field",
                        "checksum": "941587437091664302252557343195368420600519080428616232668595758795123598787field",
                        "value": "record1qyqsq74qdlqgx3cauaq9c7kjema4ly37s66v0mhatkvptcu0j64m04sgqsrxzmt0w4h8ggcqqgqspurrwnun6hts8n75erqapqrc0gut9fryv534vjrwtgps57mdekcdpp6x76m9de0kjezrqqpqyqxn2w7sfqqmuh7237ce9cxdum0374sjwtvq9c236gaspje5kdgjzpuxxhn57twpf0vpxg0mcan2g52l4s3q696v6kdum4jffxhf3ffq68m90p6x2unwv9k97ct4w35x7unf0fshg6t0de0hyet3w45hyetyyvqqyqgq34wyy6r3ufnpsfy2avqcxh8v4ws8dnupc0fksnf9yt9uvlack5zpqct4w35x7unf0fjkghm4de6xjmprqqpqzqzxe8g83laksk9yrs5x3ych7f2qy0ehh42a7d5jnjuqmvkkfvd5q4v26lmpjxr83hsw9d56u8yup0r8nw2dr3u4lfqgpywnvuu4q3fqx2ruwvd"
                    },
                    {
                        "type": "future",
                        "id": "225677601906795975019009429073456509021373014244145776251818301344005008541field",
                        "value": "{\n  program_id: token_registry.aleo,\n  function_name: transfer_from_public_to_private,\n  arguments: [\n    3443843282313283355522573239085696902919850365217539366784739393210722344986field,\n    aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm,\n    1000000u128,\n    false,\n    2741015466476092639755462704311245734202763800690829554387501669335135041371field,\n    4339750626578500203528653953873890933250112957639433785431875614975442816931field\n  ]\n}"
                    }
                ],
                "tpk": "465461760213038802344859491391451731388690462168925041150549918892516850702group",
                "tcm": "6740151706280273450111827458405525293017575747303309364364223019016812496546field",
                "scm": "4578870646840355930698894916219770549405836460533899125207148438741684637412field"
            },
            {
                "id": "au1y4w409pn5vzh6zec5heygkva65cy5ymx3xdfcekwl7wk6l6xyc8snf87jr",
                "program": "token_registry.aleo",
                "function": "transfer_private_to_public",
                "inputs": [
                    {
                        "type": "public",
                        "id": "7936384033755729897714932555569194702574319177743131403735372500331009521086field",
                        "value": "aleo1t74eg228rmmjuzwckhvh0mhfw2nfeqhtx4pt4jrkxalz8p7nyvzsqcq3cv"
                    },
                    {
                        "type": "public",
                        "id": "4755139358403323165309306724205921902056687786672795242864821940982349199035field",
                        "value": "1000000u128"
                    },
                    {
                        "type": "record",
                        "id": "7712262878346950367535664714783803522638656486630251529065640130538320108035field",
                        "tag": "1222730180104844887300233859179228904344628439905470819617030817330353098851field"
                    }
                ],
                "outputs": [
                    {
                        "type": "record",
                        "id": "5590726271449061729938362640093930185638887167459092606363510219309601802900field",
                        "checksum": "8182399591018504800027402007264809539571957368731766959241149515825000607537field",
                        "value": "record1qyqsqlkznzmd6m3p7pmqh0x9q5c4h4lep7n9x5p22pp025yacgqef0sqqsrxzmt0w4h8ggcqqgqspyaa2tkcwfmyke2qyrevtzex3lvjlk79lcq03nmfta5xr2fwv7cwpp6x76m9de0kjezrqqpqyq9ktymytutkkgwza3mndgukcn7zwrl0r6cdzx4eruld6p9r3x09pyh03quj5y3u4h970qfla90gudf9xaj9u7tl3zfk5wcrx7c4fjssz8m90p6x2unwv9k97ct4w35x7unf0fshg6t0de0hyet3w45hyetyyvqqyqgq7p6qaehlhk4efct96rx270ezy508pulruhfv6kha9l2pywqs6g8pqct4w35x7unf0fjkghm4de6xjmprqqpqzqx6ps5fwy7spxaysch4je0pvflz3ae9ehhagxlqy4zt539qkpkyzzetfqpn4kqep6lmva0zqd29fa6hu8wjlaz7werstlr9rvww606su0gargl"
                    },
                    {
                        "type": "future",
                        "id": "2204826092540908803097749774973029439443223022033765936575463683452587307286field",
                        "value": "{\n  program_id: token_registry.aleo,\n  function_name: transfer_private_to_public,\n  arguments: [\n    3443843282313283355522573239085696902919850365217539366784739393210722344986field,\n    aleo1t74eg228rmmjuzwckhvh0mhfw2nfeqhtx4pt4jrkxalz8p7nyvzsqcq3cv,\n    1000000u128,\n    4294967295u32,\n    false,\n    6332753889417248324835106105307338377575749284686365609643768815832134328110field\n  ]\n}"
                    }
                ],
                "tpk": "5767086691999586485584109908010543473631448734944755332919163410758747407994group",
                "tcm": "5118774995754622355852825918229715759726294647311959080805656963460260918856field",
                "scm": "4578870646840355930698894916219770549405836460533899125207148438741684637412field"
            },
            {
                "id": "au1ltsarjntdyqv5g3yffnq559y6yadv8u2lp2sa55q7750trdv6uyq6mkehx",
                "program": "token_registry.aleo",
                "function": "transfer_public_to_private",
                "inputs": [
                    {
                        "type": "public",
                        "id": "20329534244015422785184796746205184471797776563410996643184083692963685774field",
                        "value": "6088188135219746443092391282916151282477828391085949070550825603498725268775field"
                    },
                    {
                        "type": "private",
                        "id": "3459681214856718959372430810938391391151005752286870239860010277403708292988field",
                        "value": "ciphertext1qgqgkdexp0wvn6vhcec347prryhrppfaqqtzmh0j2qz328gj2xl62zcct9zdqlax0sw97qseser97cagy57fazt4nrgc85k85kp84f6czqz5ajc8"
                    },
                    {
                        "type": "public",
                        "id": "7075138826727371884715874062889903216536881893485867811965939409082935151562field",
                        "value": "2436858u128"
                    },
                    {
                        "type": "public",
                        "id": "1899986464632666096968324922775642821752205377106470891414144354530364690581field",
                        "value": "false"
                    }
                ],
                "outputs": [
                    {
                        "type": "record",
                        "id": "2508512819055815477432978210902102586730200752546116571628640094053804148842field",
                        "checksum": "4192396397743285989410129302411165669370802348047790749810606954905120451725field",
                        "value": "record1qyqsqqgpmn86jv5q39zt74rs68vd2lmmgc7cdj9d852kqjmf59mcpmqfqsrxzmt0w4h8ggcqqgqsqh3fh8esmyss7w0xfcynv30u2gc0ad49u2y4fewhwf09fjvjwyc0pp6x76m9de0kjezrqqpqyqztdca3qd27aj6rxu3f637gmqfjkklnm38jjrzlwwsnc5ukk9yezq7yanmt5zpxsyv25qkqhzjpk5hgshz5jgvuuhetrey6fuz3h40sy8m90p6x2unwv9k97ct4w35x7unf0fshg6t0de0hyet3w45hyetyyvqqyqgqhz6yvfn0u7jtqemtslqw7yczjxu99rfaztewurf7wyhwd83avuqpqct4w35x7unf0fjkghm4de6xjmprqqpqzqreyswnftd9lcej0sgzu372m8nu4fxgjgy958qft02gdwl2tgsapdtafuh65lv5e2mn46qwe07sjcqpeuqxweuqwms23qgmlf9afafqj0gkw7v"
                    },
                    {
                        "type": "future",
                        "id": "5276578574086170482375473941034386409611728101121157562402904524897779657653field",
                        "value": "{\n  program_id: token_registry.aleo,\n  function_name: transfer_public_to_private,\n  arguments: [\n    6088188135219746443092391282916151282477828391085949070550825603498725268775field,\n    2436858u128,\n    aleo1t74eg228rmmjuzwckhvh0mhfw2nfeqhtx4pt4jrkxalz8p7nyvzsqcq3cv,\n    false,\n    4017271997399872127133630661595143306637299890633468624080159721317948532970field\n  ]\n}"
                    }
                ],
                "tpk": "5763728995287322354622502837399898942527543689433511253315830536249695946540group",
                "tcm": "7769402534695989851301853450284745996303791200719250058861958616214644151076field",
                "scm": "4578870646840355930698894916219770549405836460533899125207148438741684637412field"
            },
            {
                "id": "au16mrr20pn22kc50hpf2gvmghr9k4a8eqye66fhxnjc6wccjdkxgxsa734vp",
                "program": "arcn_whitelist.aleo",
                "function": "is_caller_eligible",
                "inputs": [
                    {
                        "type": "public",
                        "id": "6637102487733384520108027179201728637970387002972128214876348827547804649606field",
                        "value": "aleo1p05sx0kvkcnkvulckc9kewt9utm4hzz8jvqf8z0zrjxrrceaxgysm6nrxr"
                    }
                ],
                "outputs": [
                    {
                        "type": "future",
                        "id": "2347142979691744958314026092152666721022436242488153574616517657917977470718field",
                        "value": "{\n  program_id: arcn_whitelist.aleo,\n  function_name: is_caller_eligible,\n  arguments: [\n    aleo1p05sx0kvkcnkvulckc9kewt9utm4hzz8jvqf8z0zrjxrrceaxgysm6nrxr\n  ]\n}"
                    }
                ],
                "tpk": "3448890738652129077638480858274630667219302635684529806535330435476640769558group",
                "tcm": "922314695615594855022797834341964302027393336083389206626578210720012501654field",
                "scm": "4578870646840355930698894916219770549405836460533899125207148438741684637412field"
            },
            {
                "id": "au1sc9egrfja45nj4kwg73jsr9hdyd9h3npc77dgdklr7y2zwj3cvrq7p60h0",
                "program": "arcn_pool_v2_2_2.aleo",
                "function": "swap_amm",
                "inputs": [
                    {
                        "type": "public",
                        "id": "6485956943998974340016510393005373657873312963367939626386718417018435100349field",
                        "value": "3593632309927409194215146717781580749800998687192184818060433399389998091992field"
                    },
                    {
                        "type": "private",
                        "id": "2246990208891784653413373225785088274332902902511289897568326081692043441373field",
                        "value": "ciphertext1qgqpmvqpc6w7num7vn8ssndndc4k47hw627c2j45npe4snshnwdhxpjzzvmr7tdyhjegyzf2n9nfjlp8l6t95wf84ldztuhwgu3dekrepvcjjwk4"
                    },
                    {
                        "type": "external_record",
                        "id": "4802454548487398870528361594641805239001849309700681898375378417451629169836field"
                    },
                    {
                        "type": "public",
                        "id": "350516004571356602288958551588522542497600273182978401612937953300809779339field",
                        "value": "1000000u128"
                    },
                    {
                        "type": "public",
                        "id": "6105557766221551802955820024195270759682611551920139229064292173070345594600field",
                        "value": "6088188135219746443092391282916151282477828391085949070550825603498725268775field"
                    },
                    {
                        "type": "public",
                        "id": "6087239883606371648656418558355094356635814027384354922446228905873068707443field",
                        "value": "2436858u128"
                    },
                    {
                        "type": "public",
                        "id": "4236361145952768084342585410856428510382567903064628093768212108071061226088field",
                        "value": "357327307field"
                    },
                    {
                        "type": "public",
                        "id": "4301237484573693640568301992399238179328654521320246028169988893471534709564field",
                        "value": "false"
                    }
                ],
                "outputs": [
                    {
                        "type": "external_record",
                        "id": "4624854019090714663989436182439627755869926302825811240558484827594826995727field"
                    },
                    {
                        "type": "external_record",
                        "id": "4990087236010993402336900707189498440226729057956962607967603141858372507566field"
                    },
                    {
                        "type": "record",
                        "id": "4531893213944821907145407873780530475727141336668329325151338361965087991428field",
                        "checksum": "866316431521797419575494997112316425011375657497315496739513454534769414988field",
                        "value": "record1qyqsp6pcgp0lx34q7hmd9ctumf0fxd7kwac94c3g47lu94e6tpp8cqgtqgy8gmmtv4h976tygvqqyqsq28eqj3djlf3haa8zd65cf4uj36zm3g3cy79pa0gejdx7fepefu8nf57c4cg6lapxqqz93s2szvd3vczg860hf4vpslh3lmevkqrgxpq8wehh2cmgv4eyxqqzqgqgg3fxfuy230zye8a903k8fgqszm843kntuc0j3u2jde7x28d6gr23p936jrr4ykd0n6amh3npp9hch75hhamp9gujv27swhlz0p03qr3epu6hkxd3w5lawu8zg2drkc5y4yvhejm3xu3lnglel8c34cppqf2p5jv"
                    },
                    {
                        "type": "future",
                        "id": "2248325894304913749241876557949577503713329747206393566759554465504771259896field",
                        "value": "{\n  program_id: arcn_pool_v2_2_2.aleo,\n  function_name: swap_amm,\n  arguments: [\n    {\n      program_id: token_registry.aleo,\n      function_name: transfer_private_to_public,\n      arguments: [\n        3443843282313283355522573239085696902919850365217539366784739393210722344986field,\n        aleo1t74eg228rmmjuzwckhvh0mhfw2nfeqhtx4pt4jrkxalz8p7nyvzsqcq3cv,\n        1000000u128,\n        4294967295u32,\n        false,\n        6332753889417248324835106105307338377575749284686365609643768815832134328110field\n      ]\n    },\n    {\n      program_id: token_registry.aleo,\n      function_name: transfer_public_to_private,\n      arguments: [\n        6088188135219746443092391282916151282477828391085949070550825603498725268775field,\n        2436858u128,\n        aleo1t74eg228rmmjuzwckhvh0mhfw2nfeqhtx4pt4jrkxalz8p7nyvzsqcq3cv,\n        false,\n        4017271997399872127133630661595143306637299890633468624080159721317948532970field\n      ]\n    },\n    {\n      program_id: arcn_whitelist.aleo,\n      function_name: is_caller_eligible,\n      arguments: [\n        aleo1p05sx0kvkcnkvulckc9kewt9utm4hzz8jvqf8z0zrjxrrceaxgysm6nrxr\n      ]\n    },\n    3593632309927409194215146717781580749800998687192184818060433399389998091992field,\n    3443843282313283355522573239085696902919850365217539366784739393210722344986field,\n    1000000u128,\n    6088188135219746443092391282916151282477828391085949070550825603498725268775field,\n    2436858u128,\n    357327307field\n  ]\n}"
                    }
                ],
                "tpk": "4802182348567315671463274826096013323825565777505543283033178130296520387412group",
                "tcm": "2897399162004041007063542474776427020476088194917711005302740747993928547504field",
                "scm": "4578870646840355930698894916219770549405836460533899125207148438741684637412field"
            },
            {
                "id": "au1ph49h9l3yttherfdyga4r4fqu4t9ylzwag7xqrykyevjdkzcmvzqmy89aq",
                "program": "arcn_compliance_v1.aleo",
                "function": "report",
                "inputs": [
                    {
                        "type": "public",
                        "id": "3945233737305287749067875557687947363914646417825655904826010564451347100440field",
                        "value": "aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm"
                    },
                    {
                        "type": "public",
                        "id": "3910199104595633797211192565585926161223432420944456106677671959602035016994field",
                        "value": "aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm"
                    },
                    {
                        "type": "public",
                        "id": "6775214082212942666950819933583457379528772375678581615612324257926661777886field",
                        "value": "3443843282313283355522573239085696902919850365217539366784739393210722344986field"
                    },
                    {
                        "type": "public",
                        "id": "504611657241401762283216782988793686459864779008050858723189612525233557729field",
                        "value": "6088188135219746443092391282916151282477828391085949070550825603498725268775field"
                    },
                    {
                        "type": "public",
                        "id": "7425195550636708956346086560442074017987009492287771193703021322081830743880field",
                        "value": "1000000u128"
                    },
                    {
                        "type": "public",
                        "id": "5049987471858100068155398079183754198075416270830806641241947146131751749479field",
                        "value": "2436858u128"
                    }
                ],
                "outputs": [],
                "tpk": "3535230035125582994659212718037045766065355064873083407518279630506272072996group",
                "tcm": "3811818744759727347033154053144262630444252695480028605741610455116003093828field",
                "scm": "4578870646840355930698894916219770549405836460533899125207148438741684637412field"
            },
            {
                "id": "au1y9l8rnv9qrvv8wkzm49l8m9cas3yshgns0wd3kdzxm0r7nrw25rq83vg9d",
                "program": "token_registry.aleo",
                "function": "transfer_private_to_public",
                "inputs": [
                    {
                        "type": "public",
                        "id": "6750533745583014831774550463636478135223482719523491923509911551528106788097field",
                        "value": "aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm"
                    },
                    {
                        "type": "public",
                        "id": "1340025885952561596211224924939883283099036092434696317972759867024115711503field",
                        "value": "2436858u128"
                    },
                    {
                        "type": "record",
                        "id": "2427298430526650031748777578854080572597079826755309744697462584223203635791field",
                        "tag": "2683462196728026492966571486444911943188866733990057265381322726696840108075field"
                    }
                ],
                "outputs": [
                    {
                        "type": "record",
                        "id": "7681670785138983948836161440045969441021933878827448109878511698616892515514field",
                        "checksum": "2254943337566988801534134976657853864021157015690105544762268768610253164007field",
                        "value": "record1qyqspj28jznjfpdleadxz3anzuz5stva43gjdyclpxcu9r0n5v2dz0gjqsrxzmt0w4h8ggcqqgqsptzgh2dsqm0ku2jnvak02nu8sn2mxjga5agm30sp70r7j8v3zgg3pp6x76m9de0kjezrqqpqyqzcga30c5evegm0qnlefrdpujcpc07c6du2dlapwuxcelwzcq4ypddtxkqlgwj764cdheng7adk6mxqnpgck7g6u4hpccyr0l3vc933z8m90p6x2unwv9k97ct4w35x7unf0fshg6t0de0hyet3w45hyetyyvqqyqgqn3qpr4ns2ntu4uhdr8z8pjqfjr2uxvkppfga5s706vjgz6fh3urpqct4w35x7unf0fjkghm4de6xjmprqqpqzqql502zsrku8xa67w3zpj4gaxsfeq0ww4lypuzrtxv8tss6yjakqrez7u2nq94ke49qscz9kp9asje68s3ntjhlhjlzjr2te25tzc5q7rk4y06"
                    },
                    {
                        "type": "future",
                        "id": "2805210028935089553357615293226505663805875929463794898610209168815276394322field",
                        "value": "{\n  program_id: token_registry.aleo,\n  function_name: transfer_private_to_public,\n  arguments: [\n    6088188135219746443092391282916151282477828391085949070550825603498725268775field,\n    aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm,\n    2436858u128,\n    4294967295u32,\n    false,\n    7824963753535188452594405979523313282144230850314810722991015101935100333111field\n  ]\n}"
                    }
                ],
                "tpk": "5847364429513511577360356371106683882322181729755869136406907842042629198598group",
                "tcm": "1373293254278491691115168668136940913118458769057424219294925235123265278926field",
                "scm": "4578870646840355930698894916219770549405836460533899125207148438741684637412field"
            },
            {
                "id": "au1ymwuqjzu09w5u9xghvr8n27qhdkhtynnyl53qend7vldjfjk3c8q2lq56t",
                "program": "arcn_pool_v2_2_2.aleo",
                "function": "transfer_voucher",
                "inputs": [
                    {
                        "type": "record",
                        "id": "5411987166894318654356513844968576457701926540290989396779937503242657862246field",
                        "tag": "5022867595063523617567076632325065294630947310372435236492067582961000450245field"
                    },
                    {
                        "type": "private",
                        "id": "8236838403723731271708152329425738407033372864662890100687267670162984258650field",
                        "value": "ciphertext1qgqtfagk9eaw9gmg99zs9fsqsjxy4aqr0qk6xd28jwju6sajrldzqpsdl8ccwjlmjx43l05qfsfysd5rxyj94u6dqt6zgv6z86dcz2jxp5896f0j"
                    }
                ],
                "outputs": [
                    {
                        "type": "record",
                        "id": "6071880844312592550235561978238001166819009239149392799435876220314411584197field",
                        "checksum": "6628286212269802417618866107131193117253336442221007339874159573412872102048field",
                        "value": "record1qyqsp82434d7g50xz9xhzmujqayp58f460f4n4fgtup0m88jdl65esg0qgy8gmmtv4h976tygvqqyqsqsj2l5s89lknfcnwdfx9r4wnhk9r8vnhdw5k9gm5vhfpt0d7zjc8sj4qeyef6xzzjqwgd7qy8emwd8xkf94zg43ydxtk5tpwpzxur7rq8wehh2cmgv4eyxqqzqgq85jt98yx0na030pjuugeg3h0cnf43msjgm4xfnadzs87yxsjxgyypq3prk0vuj8pfxwwjhqgt636w3py8s950m0gka64mzhf8slc3peyqg4tmwcq6ynk9aynh87l54x734qr87kw7wtq70xwp7lek9jssqvc5u04"
                    }
                ],
                "tpk": "2498499796512733106158045432815238800474313750079495763302363110257154294308group",
                "tcm": "4794125152889343346307696493635804208460232965756442026991741292798725297554field",
                "scm": "4578870646840355930698894916219770549405836460533899125207148438741684637412field"
            },
            {
                "id": "au1spyp249g23gk0qsqys2tteqczcc3e3h9rkha8tht6xr9fl7t4vqswtw9gm",
                "program": "arcn_puc_in_helper_v2_2_4.aleo",
                "function": "swap_amm_credits_in",
                "inputs": [
                    {
                        "type": "public",
                        "id": "5526637124287918094038119143201520819166038048049474187497887222805637329693field",
                        "value": "3593632309927409194215146717781580749800998687192184818060433399389998091992field"
                    },
                    {
                        "type": "public",
                        "id": "2212438002809019702115278203894371458037718322561191508690546956931891803058field",
                        "value": "aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm"
                    },
                    {
                        "type": "public",
                        "id": "5221889566988920096127458406134426308237739332872980910982098416704750395712field",
                        "value": "1000000u128"
                    },
                    {
                        "type": "public",
                        "id": "3025271379097019200331445119705978278840982644040140994261449031229336687142field",
                        "value": "6088188135219746443092391282916151282477828391085949070550825603498725268775field"
                    },
                    {
                        "type": "public",
                        "id": "3883703623454105010518064397158957704298047124498369581609010543691032166419field",
                        "value": "false"
                    },
                    {
                        "type": "public",
                        "id": "2771573638678571434211668730007947894661344660963599634858753231219452666804field",
                        "value": "2436858u128"
                    },
                    {
                        "type": "public",
                        "id": "3736576270110725925662955343387706971048592262533544425483441238597872375067field",
                        "value": "357327307field"
                    }
                ],
                "outputs": [
                    {
                        "type": "external_record",
                        "id": "7103771280445420653663448924256003537184619989578303978389023524734243159724field"
                    },
                    {
                        "type": "future",
                        "id": "6483991242841127143675341877290496066384061223235689032064779186526720416548field",
                        "value": "{\n  program_id: arcn_puc_in_helper_v2_2_4.aleo,\n  function_name: swap_amm_credits_in,\n  arguments: [\n    {\n      program_id: wrapped_credits.aleo,\n      function_name: deposit_credits_public_signer,\n      arguments: [\n        {\n          program_id: credits.aleo,\n          function_name: transfer_public_as_signer,\n          arguments: [\n            aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm,\n            aleo1tjkv7vquk6yldxz53ecwsy5csnun43rfaknpkjc97v5223dlnyxsglv7nm,\n            1000000u64\n          ]\n        },\n        {\n          program_id: token_registry.aleo,\n          function_name: mint_public,\n          arguments: [\n            3443843282313283355522573239085696902919850365217539366784739393210722344986field,\n            aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm,\n            1000000u128,\n            4294967295u32,\n            aleo1tjkv7vquk6yldxz53ecwsy5csnun43rfaknpkjc97v5223dlnyxsglv7nm,\n            5783861720504029593520331872442756678068735468923730684279741068753131773333field,\n            4339750626578500203528653953873890933250112957639433785431875614975442816931field\n          ]\n        }\n      \n      ]\n    },\n    {\n      program_id: token_registry.aleo,\n      function_name: transfer_from_public_to_private,\n      arguments: [\n        3443843282313283355522573239085696902919850365217539366784739393210722344986field,\n        aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm,\n        1000000u128,\n        false,\n        2741015466476092639755462704311245734202763800690829554387501669335135041371field,\n        4339750626578500203528653953873890933250112957639433785431875614975442816931field\n      ]\n    },\n    {\n      program_id: arcn_pool_v2_2_2.aleo,\n      function_name: swap_amm,\n      arguments: [\n        {\n          program_id: token_registry.aleo,\n          function_name: transfer_private_to_public,\n          arguments: [\n            3443843282313283355522573239085696902919850365217539366784739393210722344986field,\n            aleo1t74eg228rmmjuzwckhvh0mhfw2nfeqhtx4pt4jrkxalz8p7nyvzsqcq3cv,\n            1000000u128,\n            4294967295u32,\n            false,\n            6332753889417248324835106105307338377575749284686365609643768815832134328110field\n          ]\n        },\n        {\n          program_id: token_registry.aleo,\n          function_name: transfer_public_to_private,\n          arguments: [\n            6088188135219746443092391282916151282477828391085949070550825603498725268775field,\n            2436858u128,\n            aleo1t74eg228rmmjuzwckhvh0mhfw2nfeqhtx4pt4jrkxalz8p7nyvzsqcq3cv,\n            false,\n            4017271997399872127133630661595143306637299890633468624080159721317948532970field\n          ]\n        },\n        {\n          program_id: arcn_whitelist.aleo,\n          function_name: is_caller_eligible,\n          arguments: [\n            aleo1p05sx0kvkcnkvulckc9kewt9utm4hzz8jvqf8z0zrjxrrceaxgysm6nrxr\n          ]\n        },\n        3593632309927409194215146717781580749800998687192184818060433399389998091992field,\n        3443843282313283355522573239085696902919850365217539366784739393210722344986field,\n        1000000u128,\n        6088188135219746443092391282916151282477828391085949070550825603498725268775field,\n        2436858u128,\n        357327307field\n      ]\n    },\n    {\n      program_id: token_registry.aleo,\n      function_name: transfer_private_to_public,\n      arguments: [\n        6088188135219746443092391282916151282477828391085949070550825603498725268775field,\n        aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm,\n        2436858u128,\n        4294967295u32,\n        false,\n        7824963753535188452594405979523313282144230850314810722991015101935100333111field\n      ]\n    }\n  \n  ]\n}"
                    }
                ],
                "tpk": "6825195796208289312514340856945660905578026152053045325880938724745270972640group",
                "tcm": "4447623725028005489162366510034925941650509074905510492941285203691607983474field",
                "scm": "4578870646840355930698894916219770549405836460533899125207148438741684637412field"
            }
        ],
        "global_state_root": "sr1tg6a63xpx5ljc8muevty0rks2cklghe4d0w6rvc9ghld6rvllv9qdckekm",
        "proof": "proof1qyxqqqqqqqqqqqqpqqqqqqqqqqqqzqqqqqqqqqqqqyqqqqqqqqqqqqgqqqqqqqqqqqqsqqqqqqqqqqqpqqqqqqqqqqqqyqqqqqqqqqqqqvqqqqqqqqqqqqgqqqqqqqqqqqqsqqqqqqqqqqqpqqqqqqqqqqqqzqqqqqqqqqqqhyf2yudw96w84twuyqpm35gs9hccyepql0phc34pc9vqgq5hp77uzmgmdejsjqqwcg33wez0r5sgrfs4507fpmm6f5s0hxvkncxfsnqlgcmgx63nsgtmpjfn47mjzsphfm5sy6f5ketu0dyxzh8j746gq8rafeahj6v5sq4awe9q3c9u6equ5q7r9handqc2v0p2uev9wgqed726smhwthtcdjp6hv5cg6kk5qqtdydyxl8crnckx785rv9ylvupfhexcynusxf4h3kyhdhvq6je77p0ptdv3hdxe7w4d9k6gxjswqqmgydd3qk2e68ufm2uuwfu3l8j87dcnpxm3j2p03lzgeedlehxpkkv3qnwx8xak9rarztke0eeafcqlavqz3v8n5cxsgggg68xf7l28phtm6j7wd74r4vlya6scxd4xmdp48gz4wnq8exd04jfe4t7q3sqzes0a3emtt639lj3y2qmvsvq7pvclyx8rqjaum0uwjd9z59u7cl7vqjze8n9hmzknhghstqdwtspszn0p0kg9dcxltzux5qd09ukxmrxnhd8xzg2gtyn6chmhxv5vagtwywl4n4usacu38nrmdzwc6efaqx4td5hrexd02j5yrjduqhr9gj9l4qn3ju96sqll7p8qm6dhnetzlraz2tkfnurdzxs37dha6r5ykqh8jznc6hu3ky30y5qrtwuu22sz4yzyc74a4s00gcn9xet9p62texjyummlv9ta47w92qpj4f0apgqf0gdhv358hxchm7cufwdkvsufgz4e50ztzkl8r5ve4h2nucrhc0ks47eznxgj0qdyc8u85p9s6qgqyptrlpjk3vzs8934u3xr2gslgwm8qt980x373rlw5dzpjsl8v5r9g3s2nylhzqv0auw7xk8vln5sz9y43gvkv2px7cdr43ejjwsyvmcvegtnnefm9ldnuccn35j83v8gh0gtj3sykcwl27dhw3m20x03q9gl5l9j48gp7kw5lm5p4mr008ws7rfkryxfsewhzltf888nz6nhkzngrar6sdd6xr3pjvzh5twm5q2ekdhe92yhs6eghxeywrx83alnd67ttngzccym3lps7rer4qgra2dxyzhkhcfkz8edryxe42t2lvpqywhdm4q9kd8zwl4jxyf6ha7phpukf3x2x34uhnvgeazjkue66mtkcu9gm38hd82q90t4xrygj9ksqwfjxgd2dqf3scj7thfrgv3y9xx56ae84fx62aeg2umefzhkfezhs47u2gpef3wesqppaxa24q6ekq25rlx2r0m62uqrf9ttjsuzrhmypvpvf64vvdmgtdvnhersd8sewtmh9f6w4kz757mskgyc9zn4asqwt0mg0jx0jyngytne8fpuuwardgymq82hmr327l2v3mcechvj5cqvtjc4g3n3qteva7snmv8t0lqptxalhfqg0yrh3mcpglgwn0xsssy35dpr857ze5vx7p2u2sctk2dvkd6n0lcvukm57jnjls49qsqqy6zn382dpzvy622cgqmqmz5y7ael0y2djy2h6z6m6tg3d34p2rqgrelex2fy3j7z76ssuy7m4er7qzt7syh2wgk2tcrqkurgt8f0cd5zq9rdlqfuw6kaaazjqpwae43khyq3j82tsjlf4mx0v7aaxzmtkqsqzasypc72k8fcvn2se995v39gjnqp9mws2da9x4mxveutkz4d62v5qqwadht5tvvmjdwua5veyqq230t7uewp257aqtvm50fjd4zcptxkxjycrut2wsdh3wmegx0htf8fraamzmm9sj32k25tqvtrdxgzzu7el80wtkl5y6frx70gpwwm2fexv8nlfucjs764v7p0c2gpqd78mwlvd3j62nhzu3mmngjvrdrsr73fp8mvjtwwtf0p0ulxf0f2axnt7znyc9nz4pykeseazvcc09c2h2vh4ng0nnkrltl5w9a7zd83qfn37fzs2m8hya0nuu7lemcseaj9an3csyftfl8nkchpnv3fdjwrg0m8xjtrtq3fjemd489836rskq4pfz40dhrhlw0uc7hqd6h7yprwg70m9e96quysjqtpq97v6afw20rueh4ygxkluz2ghg4vuetg0yq34fvdm3jwl6ph99uc7sxesapvwjgjvgsxd8aydjys50qgk4mqv2flwxvldusth2mgqvx6f5hj29sqtz7zekn0hteqglgvqeve0rd7y8yvlf5yjr52jayy0svzx5sw2kzuyxd6a797er8v7g3n6wzq5znsr4rtc2vpgztncsf3rm6d78hrxm2l3pv5qtj9zmx3et3tyx2nwgfjgeevf6t39lzypf9qkwv66l2gqyg8fv4ucdt4fvrsk6wmgpmavaq4e38fjpyv6w2mgxwypt2cz3q5k99qvzls3ra3vr030wnqg625wqjraptp8h36hfrpzqaht0nngwp94vqdatw5y6arzvp0z5wvjzr562sx9m0wv677m0vxzk2kf7gyvgqpufsd8ys534qa3q73fjdtr3pxv2r7xz25q59qysfkajxpu56z5a8ml6qcacdcdsehzvh2ju57mygp53h6055emsxk7thhcz6y5rne8ufhfd6p7shxxle9fm4xdh8jp88tghadkqhxs0e0rm22wz6zzggs8klzdrulxllcglu72egaas338at2mptwud74kmlekfx3spvmgp5sp072x42c64tm9g43x7xr5n9kq89n6e275n6c44f94rrstlra9ccjjqpuavvh24wh5unfm2xzsta0drnsu7tllwfrn6ldgugupycyxqcuddm9dh0nk75472cdayj32xc8ljmh77sqzp8vjc308euew2kssvx3q6m9m7vjl9fefyfntrxks5ptaja64sy0nm7hy3fx2nmp07aycl2nwjch3nx4p499mq4wpydg4lt4uzn7s6frccswqpkyy3xtjjcqy2uxg3kh7yqwc4f83nzga9exjrzd8ctxp0hrzpd90n8aawamm8eedykud3y75mgpc2rnk6s23qksy0xrwzhngp84sjlxsscesu763xjupejm7sguvht4ey943cpn66fhmle0ptf2z644axt5fpkl0kfyq26t44nm50e2ctsrp9ks8wfvqvpr5vd0je8hmr29x50c8jpk7xdtp6p4hlf67vdzlla5n02zkl5gvq67fpuvjkwztsy8f2thwxjn2fgaa709t483dcscm0g7uzry2dktlugjxy08nlvvlshc3swv02a28sp7vjf38y6kpy6yndux78vnhuwaugmxyxem0qv77sxzwc5kwgccuax00jr6vg3664gu5gv9ml62hyspgw2vvka8w6r79xq8eh7lzkmhptsxzxge0ykyy772j9ev9r5g3wud60edqsfy8jppaqr55q7w862spxp5du2f2uga3p49vsuwac79q6q2dsf4mqmc9fd8xg5esc452eut6e2ttsx9qw8q0khga6wa0cduqqm9azuc82f85fpxq4r78t24gjua26ys8q7n2x5t8f2kzxqlhwnxc75j3xt53gz2ss7xsf4hgua3uqqmgn59jpmqa3amsn67xghmyf6zepagzds8kzttlra4nhvglummushfrqgdlfvv0ctl5jjtchs3vvqe80lgjdyxxdruv4jxelk40d6u854g7t4j5xtfyz7r3zzljn90fnuy23hmreyujsss97zldhd6z7gprmuz3pqtd0g4sdqk4qxkmx4pntsj9dem064zanhgrq3lgy24xk9ysjlc2tv8z5f37wexe2dz5dtsyrh7wzpdr0kmr6lh5gyml7c05d2yqrw0c65qr99lrh7ck49maxhsrc7eypl59hd76x46xra8uka9q84r8k5wrl6jsautmf6z9trysupun6jm5vg7n000nwrslk6acr0lqlcpxahm4cjp3rllhgfylutjcqx7um4jgpntywm48u6ay5yg6ghref30kwtra6kpa5fkf4p53yccwzn8595gxfsm7z53se4eedsnxcqphz80ed5p8c5fq0nnr47pae6d3cepz2zyulg6xlp6wupmm3ks033mxy5qevykfmk4j7ejr3zej0sqwuselqpp4fnsxhjl47uhyw85swkl2udf807rgvx9vyw978cadfgzh8nze87ll3a3e2eejendksyqyp4rtcjxn0gvkfpav8wjfv62mz9l7t79ah3yrw5kkjcnmr0eddvdnwdft743r66c0uk397kyzm9lqdtfws7s33r6dj674g6s8x57zt0dzxk7e40w22qlp7236dxdkt9pud57rsdtfjn3tauvg6urf5upjg4sqluze3d965ljhhw4am2g7ysavdmnedurmwamlfpmd3x095h58vyavqq257xlkjaa8gyyqp8pys2k3e44hu6dtgjrul9n0d9enzg9pjp470f3egkgvckg7kcsarlusy6prjes3hn7z5pqq0dyvlzalyg4txdudkpvszv2p6ta626jj4kuqjjcdn25c0965g8j0848p0e7zkky5z8u0zc7xkqu2gq0zl5pjzcpjs3vvtsu2ywse87z4mcwpej46m9z8dvq793zrpwcz6x3xv90yxs43sdcte69zdcewg4t7ar30j9u8jty9pt7eekcjzzkfghqtkzy6sdyl9x3j8qnfnxcrccqk4sdm9rjzhfnd8fkuly6r3luj2ycjvzwqpe94wqp36tcwetnmctd6wedpwsedal3cpskpwa23uja0shralsgprk5xfhxf9me0e94m2sfzakdn3n7acwgm4lqcnj7u4aszff9l6npv8alqa5aerqdqrsxezu9dvn0aaglhvljcvgrak9rl96cgryc6d3ytn9uyuvcv06rm33jp5ahj60l7r8j879vh8svqsz3whu0zvyxhqgfkygn8p2pwfnadfx9jlvvylssv88k4jw49xa7g74cawkhcs35vgfuu4rc8rk59pwgdzd9nejncw99tuec0ec77yuh58020hypks87r3yem3lu7vsxgqj7ypuhl7thn43rqg07735npj9erdneu4tfecupxuxgpwxkwjsmcl9rxrsfagn8v5gl4jjmldzfhufnw03yewe8ml3pa0djtgjkrgm382scrm8gkh03h3kkz6e58rwwme2vrerg7atgfq9ljgrx8km28sx0xq4u9ydjw5k9szdkyy9c7eczyl36f3ne0454q8p6gfw82fsdxp43t3usfmg4yhza0g8hyfg3ctd2tc86gj2lpsyuy58thztqre62rz8cetwa237mlfar59sxupz7ld94pshl0jarrh5pen9u0dc67ej6fqesv427svtncrwv7xdl7trle84jx6xnd7lmg9pr0fr2k2efyf9umsm6nagjah3z2n0h6ympk9gycyvpa0qkpxxc2q2v0daamwgtdlp9ue5w5j8h0uevdk5qkwhnmdtyg9tnuc8j5r5dg8vv7c9dpx09z7ak6dve0lnagde9x3uj7my9pxcaxantgpczut4yzdj68djdh0w4ug98ysy2myt2zzudh5ljpvsq6k83u9wlnzeu9ypqjwaxjcgf0y299qucp3shrm9lapg56e0wv9f5ljdk3svr460g4cs9p4whl034xecz59gty9y3ay80j0pxyf8l9wj0ycn46dlwwq9nfcq6nr86wzrxd9mpy2q3kmewr2z767nwp60wgclymzusylmvd3npgg7pe8ul37qzv5844n7u0cqsvdz04jcu0w97ez5s3yf0ykn99wd5q4ek4h8t29ptwjqfvner4uukfh3ph07p2n56l9vavz39ec3m9vvqm905ltsg5uzcdsjzevpu3q0xzutgvfhpkla86s9sqzhtghhde6se7sl0pwjx7vgylfnp6ckjgtl0qjx6va2agkghk0rp7vvr6zw09cqr007698d6w54ml6qfnnaxm28fnexq0fnhsk4z9v322yz7528ryfrr3q05sg26kehl9ekpk76cwm8g6mlhuzfmuvcyvvc0t49ema7uzx6x7ad9pyenagkfzuh5zs0zwz8f7gks62fuwxnnp6yn7e009mqp893sl99fhzqvmhvnlkz5m73chy2z3lap8psdd4pyrea3sqhq5xqpvm4eae5c6et3duzp4rg9nl69aqywx47yt6jrwsur2acpn0ysqsx0neu3v6dgqryrtkmjg7wdr7m0dq3qrxwnvldkuwruzfapfhgcyxyfcat7wfd59s3tzut0w7kfpydd6fhszpsxh36gckz5sqkfeyrwrh8sy4hpn5qfsc3ch49ytzzsdlwxy2hcqquls3f9td9ae9h6kjlpeedlx9lrwa2nvk2x4xsx0y58zhs89xxka8an6ptlvkx7qqvurds6n0n9dtycauuk24chyhu36hcessdt8ccv5exlrxmpaz8glzzegqsz4xls3faex4t53v746ykqm8a2h4un806dmd67rxvhwjer95wpqpp49wl8qqgdtlkrm36zz5e22ty94jjx35vdq9q7dsdgd5pfp4rgrvhgemv6u4avvpd2w08ssf0l8v50epfgaqc2cla3syyd2985z83p3cftnj0a6swt4wu96n905lapk5ppf9tqnkatklerwaaq7usgsu3p5qynfgz72qxld93p7ayslac5pa2uzcx0n28vfyzny9t6sgg4wsq7mlt2nzvac5ku9lp8u6e9vx6xxzr97g34tcw2r7tvg5qtpc7gsq0s0yc7zaqumt6ry0jd8lkdkmkvfsfd3lzrtg44dg7e082mkuwsrpkndzre3g7xzj4s3cwq58uyafur3nn7yqpmcwpwhlvczgc45e2q7rj8l9t3tz6y7f7jnexn6nft57k6nxpt3jdcevj87j0tkwsg30q63j7e38xu9vg0758m5avyd5nx5nxwlky4ym34pxlnsxs8wz0h0cv7nxhr4ul7v0czyt95cpfru8a2dyajvw32pgttm90yrntw5lwcgr4pcesfww8rnsdcvph0kfdhnlnhm4203egecrp37gkpchrhhu55p5nqrga4nmt396l4w74l5rvx3nnagzmpsghhc0phnsguvv76t9fprfkk4kxpz7xddhqcc6vykuh6c3x92ueeq4a5cv0wuk6mp9d5k0s6twn7mg3lqqz682ar92karnm7ytxvslfcgxnkass9c09zyqymsgv6mg40pnptltz0cq06j3qgc5upnk4wj7hnadpquthe90wp6pf35pawfvfj32uwzrpa6ccklpl24fpq4k0hzzvk7mzy7sfxlude0ghjqn76qnkw0vzle4pqt5xnpv4mvpuvem0dx2tmh2s2alnj8vy73dnpdje4wtznhdhxcmru89ugcmsfnhs33y2tc2j58ysq9k7nr3c623sqpt3dacqlq6yz0tvcqmmhmyewesf3s3gw9vzrz5t6z66e7ux7vsy28tl4nxtt3pja9ung3qt8xj5px5vrz03gk6nmx2p6ypuwqxl0yr80t3yl6zj2twduhjvl5p7086zumqxpy8jw6zg46mjhgt0pwylzz60c2c5ltm8jpygr0p5am7hapw6wwf4dt2xjy76w88wse3cu726qpfpc4k5xyqyud53pfjg4y2afyxa4hykagaedhm8ldydc6qn2mvqtldhnzt9u6rw5pkvv5th6jll0cgru7arg0a0856vfcu047xygzqjadk6xurxvl3vk7gdt4xpczddfd83hkhclpdpvgza0tjlpvgnpv8fpq4a8tsk2gue8fscjp0v3h5m7x8tqrsswltrxkyznqlp8m045y38k9kd5qmvm5yx4z8hfpv3kucfccf9zkr4adkukgrc77s6s8e4qtte7kn7ya8d4yuaxuxm780wmjweqrylx8jpmel4d5w7lkaprz8serx9sh9lj7jal8hztv2nwmm4v5ug5ywxp2y58yxq56234dpgjjcgx4lcy2zntf6cz9jzkgcvv7u80kstuu0040qduq60dqh307ehuyqv294ddxudaz9e3nthgpj6g9ywh9ttndx2u4ty3xlynsmsnyfz2qlemz3pjddganh52nfdhhrc7yy89alkn9jts4cxcsh3h6rn272qz24pe3dg7z6084dcp9qr4uzlu7d32kznequk0fp5jjersj2j8da3zvmsyrjvyn2pwtrx9cpdgp50pxkumvhqvl26flv0vqrgr9r9a2gd5pu4huqcj27q7kn37x4x7wqcwre0j6ej8f9lfllrjqysyndl7gg9uegqh4u4pltvwkkdd2zu4qug8ctfsuw8ek42tjmtqphxm0pmyzpf7l068w0zpeumpwn9gsdgffamc3ual602exxvjnydkljwvejtpuh7u6jf4ezrwhhnh4ge7g9yleeqgh47kte25t9pssmnqjew23uqhs0xr8lttygftg0c3rtz5w4tr05vqvsle39xv44jxc54j9skg2cwdthyprccgm063pjww9ghpx8sj4nl3sw9qaxt8wvpjnu2sr384spzs2vfg3e7crvrr6rah6c6l3kpk093l4jyuym5pltz9scun4algqvze0fs0aty8jp44ftmj6wwyfepucp4py4844wjtgm3k467zgtdpps4we6xd2a0zn9m5ju2nka5aaewnscfp587fghg8p5pplkc8cgqtp5g8ywr6ajq2y8qpzhlxk77yzwc7pa5ma26cu3005ltskwj3tgpgvfdl8455x2aj8jmn2asj9em6kupqzkm72pgs3xhq25u6ykcm5xheh9fyd3rpf3f9spcvjnnlsmv6qdq82gmlzqpdewt2fvqdntxxrcgl6wehxvzqqvkuz2qu4cmxpqu35het5lfz0elzudvfr0gqg3tqkk9reqq2dexk7573mjm2jf755l4wrnruhvktjhc9wv6gngewpgsaz5dqynhx83scavwm6qjkmwytf5r6mah4mxv85756z0a6u7xkcsr7ykgfy4jpjn74guf6nh0cquvmpwuutrh58e7mt7nllldkd4gvsx3wtqgydtnn6qts2szlqavssrnexsr0zyq2n7l7sfq0an30adrjzlnp3trz7pqnzmvstyh0v96a9eh95y3669dvz4h9gqvmr4tym7npt2ks240pua8krk9w5a9ljw70r404s73srtvwll0xg9ratja0e53q7rttysucpgln5r2w5ynjcwfkh42gqt07kh5zymk9arft69l7wgvpe5l898rm0847ag9tcrgmjk7r9rxx46laah9prgyffd0c9h6dsxfvmgs3anz4t9pu3lqqrzv64526x6lu3u7j4pl3rj52nhhrllrxpznga9gc4hp6n3pqklamr6e6fugcqftch5c64zmg4t44l7r7lt2pm77zkzaltjpakac69yyyt2z9yw02ve9xmrt9qvgch3hnxt46mxs0enkrpnz9xm9huvv775clkuaw47u4xesnqweqkteyqalu09glyg9nkhnsr4u4fy52z8jspqu5vuw4ynlcytlt2xthpq3z9mdsv359gzh9quzhwtfxpjyfavnay88pxc80lz5xn4dydvdxzp4hflarxgyzp2jfhgs79csutcc2femc5tzeu8c4knxnnwdxfwsc0wqwd4eagnnzqdqfhs2xq2kl8twes3x3skv3rz9tdufenaq20hphthdldmvf7hq7xxus50u49c8ey0yc9gl58nchu7n4ryv37qhtpa9ed6sc2zvf6gdzelu2869442m2marzskwg90f3v3jps8jpxz4ycnuwrtmpfnnfqykkkk8ptn4s3lp8ycj52hmul8dwcxw5ham5dcg0ly7thnh77xruzfgfhpkk8fm3t8llu47ep0t579e654qc2v389yctrmt4743ep2vqnv9x25tcvkuygj9k0tum7syz0a8f82qhanh3e3hujuzlsfl7nsqppl54yh85nkkdaavxhzex7j92ppfne669hzvsqy4vrv4k66s8as82n4jfqyghqaxsejz48ssekchxjd3fhyatr9dahdsnljs5zxgfqxg02qdfzqnfz8d8ahapj9qavszttrkeft0vpsv8ws7pgxr92xgsqt3l4qxl664vymejsj65mnussrerx4rzr7093c63gn7e3hfzwccpvl7ak6x2qd9res4a9klrfafjl6rr8p343jh3s4fqhpphty3rjvqjsd2266h6tkrce5fuu8wsey3vyd9wkxwq6t4yvm07e6u8lk0nmqrqvqqqqqqqqqqqzkpmw64058l3kh3vrlspn52wmkf7pnjnxxglunwst0gevu84g88605l4x78d5cj23gkqn6g8vmssqq8q6jzt9fgrhccq40h9qupr7vx2hyksjpewnd8gulna9tl4jjjkxrexs6xs0dcc86v308c7dll2zypq8sxhdtc4chrnld4ctnd9mtnmcy2dq9jjlxsjn9szmy87d6k23gsc2fltm3y2kws96mx0prv5rptad4f7v52rrqejp2er2zqpesvuer2pe8h455yansu8p86c77650rfqqqqw94peh"
    },
    "fee": {
        "transition": {
            "id": "au1eqkuu4mha4z7xsnc7lr32gdfce83jx0wr42pk800sc0djfcqucxsuny54d",
            "program": "credits.aleo",
            "function": "fee_public",
            "inputs": [
                {
                    "type": "public",
                    "id": "8206559322585476760641939660933277215813074311617896801993247327065641774290field",
                    "value": "900000u64"
                },
                {
                    "type": "public",
                    "id": "3285590888130722975480355073816506563369639341989081853622098927384434713247field",
                    "value": "0u64"
                },
                {
                    "type": "public",
                    "id": "287384522767440607561002416722701243889041847115642499907255624264398124202field",
                    "value": "430902441202160333082062895525623521931285435920484048695564057909785907319field"
                }
            ],
            "outputs": [
                {
                    "type": "future",
                    "id": "589886878474677907084012175218512412269553801174779516690175076390962693149field",
                    "value": "{\n  program_id: credits.aleo,\n  function_name: fee_public,\n  arguments: [\n    aleo1lg85lqkd2lzlw4mqxcd43fvjx4epeh2fcvlkqr4z5y6xqy2psvxq9pgtjm,\n    900000u64\n  ]\n}"
                }
            ],
            "tpk": "7750094482715553215750104546202429304150699867405550816911938013932388986152group",
            "tcm": "4155778272043327235577842264270857264510579951269546903154617156874525092532field",
            "scm": "839719563223053343402167778341501983074047533513188222795613652045600536060field"
        },
        "global_state_root": "sr1epldhh7zz0mytk6hnxt9j5cm5y628jy8g000qwgwxqz0uwpzecrspy8jj8",
        "proof": "proof1qyqsqqqqqqqqqqqpqqqqqqqqqqqrkg06m9trll7y8megazjg3msqk547l79l9gh9shgmpjprgqggfskrjwz8gn9c9l3vp0qzmqvn4fqpqyt0rdjmxkqwpgsd9fhg83p5jk9j9mva8dmj839c5zcn537jr3qy3kaunz6fvzchjk68mape9f9zgqxc6ccqk4hypd8hc8muj2frf29weud67v0cujn3yhdyd5e4lh3fdrqx4vqcxtv2m0tpj4nmhanj4wqfva9jg3kaj3cn7xpzf7s2ncuc555a3fmj4hpz4nrjd83vvqvx7jharpdrdde6ul0jcwnhlf00t8gqs7ex9yudw85rqmfmmp945svfv30m3t4s36wcdqxpagpzwhjz4uljd6hzdw52phx4ljesl8p3uu9cztnurmslw5qyy5rlvvtmzaqv4adjyd33gufxms48678yfzf48v4w7en64chu7yjkaaufxrn6f922sy7c50lgqaemf2l25cl992fn4rhtgkqt8va9yu59p3ph2ukmls59kjnxrrh0tgwl6qjrjn29ek2fyqr2qumkh4dng6lj30qhk0eqf493c8y94jg3zqtpdznrlh4lq6jnnyh46xssweh5theuyyvrlukfvjqullchqxkkr9l0unpffg326vgdu0fdpwczu39q5scst2aaxeu3xuswsclxq9tl8mhmjwfm6esgn0vqzszw23fyfxn6xg09ffn0k8arus0c9wqjyvzkn5mr8uzxd4kn8szgz40f6z3auuj8uxnafdlnfs2h8tw063t0995pvh8rhw2s06d6sr6lax3vh52jrc5965evg7v3s5d5hmtk38dazxl25smfz8rck22yqlxe3qkyzmp6r3ypfcpl4au4pqawgq7ekdylcxchys3u40z68syqyu6j7ntsp93l8dutrx25hdp05xfk2ta2k79k874n475yatn5w5q0fznqkjcrf6xatc6kaxgtwymkpu6wsw5axganap90l4nc49448szpmjkfdfpjyyah3mj9vqaxl49wnz5cmm82pfqlhsw8r6f4jvj82yy0434yxdmuepevx7ktf66c4hne4y2x7fum5x6jfjsj75xp33qape45nd89anq5ehn8ne7w69h3ac8h2ruwn6ztpgs87xtgu0gzpfjqudn3jjjj4q3n3mgdln5f664z4psz5pq7uah6rucwkf834qnygmg9qvqqqqqqqqqqqzg3nys0jfhyl0ax4urntgspykam33stylgdndnn9ykew60e4v0n9q39hcuk9p7verf67cqv9ncrqyq8r8zzk0a5zlxa6llvqlz7u3h23a4zkurtchwmkun6qzlc0lgwnu6hvzvlmssc5cg03qw9uxz46dvpqywxqh0nutem43mlkgxngmdqcqhxag53umher0589hs0f5qgygcqthc5khz97cndyuxy2fms8upf0xcmuenc6urw2dw6hyckmdjercu2y6t66mylvev67dkafkulyf4qqyqqyrar8l"
    }
}"#;
    let transaction = Transaction::<MainnetV0>::from_str(transaction).unwrap();
    // Calculate and print out the transaction cost.
    let cost = execution_storage_cost::<MainnetV0>(transaction.execution().unwrap().size_in_bytes().unwrap());
    println!("Transaction cost: {}", cost);

    }

    #[test]
    fn test_storage_costs_compute_correctly() {
        // Test the storage cost of an execution.
        let threshold = MainnetV0::EXECUTION_STORAGE_PENALTY_THRESHOLD;

        // Test the cost of an execution.
        let mut process = Process::load().unwrap();

        // Get the program.
        let program = Program::from_str(SIZE_BOUNDARY_PROGRAM).unwrap();

        // Get the program identifiers.
        let under_5000 = Identifier::from_str("under_five_thousand").unwrap();
        let over_5000 = Identifier::from_str("over_five_thousand").unwrap();

        // Get execution and cost data.
        let execution_under_5000 = get_execution(&mut process, &program, &under_5000, ["2group"].into_iter());
        let execution_size_under_5000 = execution_under_5000.size_in_bytes().unwrap();
        let (_, (storage_cost_under_5000, _)) = execution_cost(&process, &execution_under_5000).unwrap();
        let execution_over_5000 = get_execution(&mut process, &program, &over_5000, ["2group"].into_iter());
        let execution_size_over_5000 = execution_over_5000.size_in_bytes().unwrap();
        let (_, (storage_cost_over_5000, _)) = execution_cost(&process, &execution_over_5000).unwrap();

        // Ensure the sizes are below and above the threshold respectively.
        assert!(execution_size_under_5000 < threshold);
        assert!(execution_size_over_5000 > threshold);

        // Ensure storage costs compute correctly.
        assert_eq!(storage_cost_under_5000, execution_storage_cost::<MainnetV0>(execution_size_under_5000));
        assert_eq!(storage_cost_over_5000, execution_storage_cost::<MainnetV0>(execution_size_over_5000));
    }
}
