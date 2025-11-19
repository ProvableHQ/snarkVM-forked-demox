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

use crate::{CallStack, Registers, Stack, stack::Address};
use aleo_std::prelude::{finish, lap, timer};
use console::{
    account::Field,
    network::prelude::*,
    program::{
        DynamicFuture, DynamicRecord, Identifier, InputID, Literal, Plaintext, ProgramID, Register, Request, Value,
        ValueType,
    },
    types::{Group, U16},
};
use snarkvm_synthesizer_program::{
    Call, CallDynamic, CallOperator, Operand, RegistersCircuit as _, RegistersSigner as _, RegistersTrait as _,
    StackTrait,
};
use snarkvm_utilities::dev_eprintln;

use std::sync::Arc;

mod dynamic;
pub use dynamic::*;

mod standard;
pub use standard::*;

pub trait CallTrait<N: Network> {
    /// Evaluates the instruction.
    fn evaluate<A: circuit::Aleo<Network = N>, R: CryptoRng + Rng>(
        &self,
        stack: &Stack<N>,
        registers: &mut Registers<N, A>,
        rng: &mut R,
    ) -> Result<()>;

    /// Executes the instruction.
    fn execute<A: circuit::Aleo<Network = N>, R: CryptoRng + Rng>(
        &self,
        stack: &Stack<N>,
        registers: &mut Registers<N, A>,
        rng: &mut R,
    ) -> Result<()>;
}
