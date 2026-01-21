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

#![allow(clippy::type_complexity)]

mod bytes;
mod serialize;
mod string;

use crate::Transaction;
use console::{
    network::prelude::*,
    program::{Address, Identifier, ProgramID},
    types::{Field, U8},
};
use snarkvm_synthesizer_program::Program;
use snarkvm_synthesizer_snark::{Certificate, VerifyingKey};

#[derive(Clone)]
pub struct Deployment<N: Network> {
    /// The edition.
    edition: u16,
    /// The program.
    program: Program<N>,
    /// The mapping of function names to their verifying key and certificate.
    verifying_keys: Vec<(Identifier<N>, (VerifyingKey<N>, Certificate<N>))>,
    /// An optional checksum for the program.
    /// This field creates a backwards-compatible implicit versioning mechanism for deployments.
    /// Before the migration height where this feature is enabled, the checksum will **not** be allowed.
    /// After the migration height where this feature is enabled, the checksum will be required.
    program_checksum: Option<[U8<N>; 32]>,
    /// An optional owner for the program.
    /// This field creates a backwards-compatible implicit versioning mechanism for deployments.
    /// Before the migration height where this feature is enabled, the owner will **not** be allowed.
    /// After the migration height where this feature is enabled, the owner will be required.
    program_owner: Option<Address<N>>,
    /// An optional set of translation verifying keys for the program's records.
    /// This field should only be set if the program contains records.
    /// This field creates a backwards-compatible implicit versioning mechanism for deployments.
    /// Before the migration height where this feature is enabled, the translation verifying keys will **not** be allowed.
    /// After the migration height where this feature is enabled, the translation verifying keys will be required.
    translation_verifying_keys: Option<Vec<(Identifier<N>, (VerifyingKey<N>, Certificate<N>))>>,
}

impl<N: Network> PartialEq for Deployment<N> {
    fn eq(&self, other: &Self) -> bool {
        self.edition == other.edition
            && self.program == other.program
            && self.verifying_keys == other.verifying_keys
            // Note. These fields were added after the original implementation.
            // This is safe since the deployment ID is computed off a hash of all fields.
            // All uses of `PartialEq` and `Eq` of `Deployment` use the deployment ID.
            && self.program_checksum == other.program_checksum
            && self.program_owner == other.program_owner
            && self.translation_verifying_keys == other.translation_verifying_keys
    }
}

impl<N: Network> Eq for Deployment<N> {}

impl<N: Network> Deployment<N> {
    /// Initializes a new deployment.
    pub fn new(
        edition: u16,
        program: Program<N>,
        verifying_keys: Vec<(Identifier<N>, (VerifyingKey<N>, Certificate<N>))>,
        program_checksum: Option<[U8<N>; 32]>,
        program_owner: Option<Address<N>>,
        translation_verifying_keys: Option<Vec<(Identifier<N>, (VerifyingKey<N>, Certificate<N>))>>,
    ) -> Result<Self> {
        // Construct the deployment.
        let deployment =
            Self { edition, program, verifying_keys, translation_verifying_keys, program_checksum, program_owner };
        // Ensure the deployment is ordered.
        deployment.check_is_ordered()?;
        // Return the deployment.
        Ok(deployment)
    }

    /// Checks that the deployment is ordered.
    pub fn check_is_ordered(&self) -> Result<()> {
        let program_id = self.program.id();

        // Ensure that the appropriate optional fields are present.
        // The call to `Deployment::version` implicitly performs this check.
        self.version()?;
        // Validate the deployment based on the program checksum.
        if let Some(program_checksum) = self.program_checksum {
            ensure!(
                program_checksum == self.program.to_checksum(),
                "The program checksum in the deployment does not match the computed checksum for '{program_id}'"
            );
        }
        // Ensure the program contains functions.
        ensure!(
            !self.program.functions().is_empty(),
            "No functions present in the deployment for program '{program_id}'"
        );
        // Ensure the number of functions does not exceed the maximum.
        ensure!(
            self.program.functions().len() <= N::MAX_FUNCTIONS,
            "Deployment has too many functions (maximum is '{}')",
            N::MAX_FUNCTIONS
        );
        // Ensure the number of records does not exceed the maximum.
        ensure!(
            self.program.records().len() <= N::MAX_RECORDS,
            "Deployment has too many records (maximum is '{}')",
            N::MAX_RECORDS
        );

        // Ensure the deployment contains verifying keys.
        ensure!(
            !self.verifying_keys.is_empty(),
            "No verifying keys present in the deployment for program '{program_id}'"
        );
        // Ensure the number of functions matches the number of verifying keys.
        if self.program.functions().len() != self.verifying_keys.len() {
            bail!("Deployment has an incorrect number of verifying keys, according to the program.");
        }
        // Ensure the function and verifying keys correspond.
        for ((function_name, function), (name, _)) in self.program.functions().iter().zip_eq(&self.verifying_keys) {
            // Ensure the function name is correct.
            if function_name != function.name() {
                bail!("The function key is '{function_name}', but the function name is '{}'", function.name())
            }
            // Ensure the function name with the verifying key is correct.
            if name != function.name() {
                bail!("The verifier key is '{name}', but the function name is '{}'", function.name())
            }
        }
        // Ensure there are no duplicate verifying keys.
        ensure!(
            !has_duplicates(self.verifying_keys.iter().map(|(name, ..)| name)),
            "A duplicate function name was found"
        );

        // If the translation verifying keys are present, ensure they are well-formed.
        if let Some(translation_verifying_keys) = &self.translation_verifying_keys {
            // Ensure the number of program records is non-zero.
            ensure!(
                !self.program.records().is_empty(),
                "No records present in the deployment for program '{program_id}'"
            );
            // Ensure the number of records matches the number of translation verifying keys.
            ensure!(
                self.program.records().len() == translation_verifying_keys.len(),
                "Expected {} records, but {} translation verifying keys were provided.",
                self.program.records().len(),
                translation_verifying_keys.len()
            );
            // Ensure the records and translation verifying keys correspond.
            for ((record_name, record), (name, _)) in self.program.records().iter().zip_eq(translation_verifying_keys) {
                // Ensure the record name is correct.
                if record_name != record.name() {
                    bail!("The record key is '{record_name}', but the record name is '{}'", record.name())
                }
                // Ensure the record name with the translation verifying key is correct.
                if name != record.name() {
                    bail!("The translation verifying key is '{name}', but the record name is '{}'", record.name())
                }
            }
            // Ensure there are no duplicate translation verifying keys.
            ensure!(
                !has_duplicates(translation_verifying_keys.iter().map(|(name, ..)| name)),
                "A duplicate translation record name was found"
            );
        }

        Ok(())
    }

    /// Returns the size in bytes.
    pub fn size_in_bytes(&self) -> Result<u64> {
        Ok(u64::try_from(self.to_bytes_le()?.len())?)
    }

    /// Returns the number of program functions in the deployment.
    pub fn num_functions(&self) -> usize {
        self.program.functions().len()
    }

    /// Returns the edition.
    pub const fn edition(&self) -> u16 {
        self.edition
    }

    /// Returns the program.
    pub const fn program(&self) -> &Program<N> {
        &self.program
    }

    /// Returns the program checksum, if it was stored.
    pub const fn program_checksum(&self) -> Option<[U8<N>; 32]> {
        self.program_checksum
    }

    /// Returns the program owner, if it was stored.
    pub const fn program_owner(&self) -> Option<Address<N>> {
        self.program_owner
    }

    /// Returns the program.
    pub const fn program_id(&self) -> &ProgramID<N> {
        self.program.id()
    }

    /// Returns the verifying keys.
    pub const fn verifying_keys(&self) -> &Vec<(Identifier<N>, (VerifyingKey<N>, Certificate<N>))> {
        &self.verifying_keys
    }

    /// Returns the translation verifying keys.
    pub const fn translation_verifying_keys(&self) -> &Option<Vec<(Identifier<N>, (VerifyingKey<N>, Certificate<N>))>> {
        &self.translation_verifying_keys
    }

    /// Returns the sum of the variable counts in this deployment.
    pub fn num_combined_variables(&self) -> Result<u64> {
        self.num_combined_function_variables()?
            .checked_add(self.num_combined_translation_variables()?)
            .ok_or_else(|| anyhow!("Overflow when counting total variables for '{}'", self.program_id()))
    }

    /// Returns the sum of the variable counts for all functions in this deployment.
    pub fn num_combined_function_variables(&self) -> Result<u64> {
        // Initialize the accumulator.
        let mut num_combined_variables = 0u64;
        // Iterate over the translation verifying keys.
        for (_, (vk, _)) in &self.verifying_keys {
            // Add the number of variables.
            num_combined_variables = num_combined_variables
                .checked_add(vk.num_variables())
                .ok_or_else(|| anyhow!("Overflow when counting variables for '{}'", self.program_id()))?;
        }
        // Return the number of combined variables.
        Ok(num_combined_variables)
    }

    /// Returns the sum of the variable counts for all translations in this deployment.
    pub fn num_combined_translation_variables(&self) -> Result<u64> {
        // Initialize the accumulator.
        let mut num_combined_variables = 0u64;
        // Iterate over the translation verifying keys.
        if let Some(translation_vks) = &self.translation_verifying_keys {
            for (_, (vk, _)) in translation_vks {
                // Add the number of variables.
                num_combined_variables = num_combined_variables
                    .checked_add(vk.num_variables())
                    .ok_or_else(|| anyhow!("Overflow when counting variables for '{}'", self.program_id()))?;
            }
        }
        // Return the number of combined variables.
        Ok(num_combined_variables)
    }

    /// Returns the sum of the constraint counts in this deployment.
    pub fn num_combined_constraints(&self) -> Result<u64> {
        self.num_combined_function_constraints()?
            .checked_add(self.num_combined_translation_constraints()?)
            .ok_or_else(|| anyhow!("Overflow when counting total constraints for '{}'", self.program_id()))
    }

    /// Returns the sum of the constraint counts for all functions in this deployment.
    pub fn num_combined_function_constraints(&self) -> Result<u64> {
        // Initialize the accumulator.
        let mut num_combined_constraints = 0u64;
        // Iterate over the translation verifying keys.
        for (_, (vk, _)) in &self.verifying_keys {
            // Add the number of constraints.
            num_combined_constraints = num_combined_constraints
                .checked_add(vk.circuit_info.num_constraints as u64)
                .ok_or_else(|| anyhow!("Overflow when counting constraints for '{}'", self.program_id()))?;
        }
        // Return the number of combined constraints.
        Ok(num_combined_constraints)
    }

    /// Returns the sum of the constraint counts for all translations in this deployment.
    pub fn num_combined_translation_constraints(&self) -> Result<u64> {
        // Initialize the accumulator.
        let mut num_combined_constraints = 0u64;
        // Iterate over the translation verifying keys.
        if let Some(translation_vks) = &self.translation_verifying_keys {
            for (_, (vk, _)) in translation_vks {
                // Add the number of constraints.
                num_combined_constraints = num_combined_constraints
                    .checked_add(vk.circuit_info.num_constraints as u64)
                    .ok_or_else(|| anyhow!("Overflow when counting constraints for '{}'", self.program_id()))?;
            }
        }
        // Return the number of combined constraints.
        Ok(num_combined_constraints)
    }

    /// Returns the deployment ID.
    pub fn to_deployment_id(&self) -> Result<Field<N>> {
        Ok(*Transaction::deployment_tree(self)?.root())
    }
}

impl<N: Network> Deployment<N> {
    /// Sets the program checksum.
    /// Note: This method is intended to be used by the synthesizer **only**, and should not be called by the user.
    #[doc(hidden)]
    pub fn set_program_checksum_raw(&mut self, program_checksum: Option<[U8<N>; 32]>) {
        self.program_checksum = program_checksum;
    }

    /// Sets the program owner.
    /// Note: This method is intended to be used by the synthesizer **only**, and should not be called by the user.
    #[doc(hidden)]
    pub fn set_program_owner_raw(&mut self, program_owner: Option<Address<N>>) {
        self.program_owner = program_owner;
    }

    /// Sets the translation verifying keys.
    /// Note: This method is intended to be used by the synthesizer **only**, and should not be called by the user.
    #[doc(hidden)]
    pub fn set_translation_verifying_keys_raw(
        &mut self,
        translation_verifying_keys: Option<Vec<(Identifier<N>, (VerifyingKey<N>, Certificate<N>))>>,
    ) {
        self.translation_verifying_keys = translation_verifying_keys;
    }

    /// An internal function to return the implicit deployment version.
    /// This function implicitly checks that the deployment checksum and owner is well-formed.
    #[doc(hidden)]
    pub(super) fn version(&self) -> Result<DeploymentVersion> {
        match (
            self.program_checksum.is_some(),
            self.program_owner.is_some(),
            self.translation_verifying_keys().is_some(),
        ) {
            (false, false, false) => Ok(DeploymentVersion::V1),
            (false, false, true) => bail!(
                "The deployment contains translation verifying keys, but neither the program checksum nor the program owner are present."
            ),
            (true, true, false) => Ok(DeploymentVersion::V2),
            (true, true, true) => Ok(DeploymentVersion::V3),
            (true, false, _) => {
                bail!("The program checksum is present, but the program owner is absent.")
            }
            (false, true, _) => {
                bail!("The program owner is present, but the program checksum is absent.")
            }
        }
    }
}

// An internal enum to represent the deployment version.
#[derive(Copy, Clone, Eq, PartialEq)]
pub(super) enum DeploymentVersion {
    // Inactive after consensus version >= V9.
    V1 = 1,
    // Active after consensus version >= V9.
    V2 = 2,
    // Active after consensus version >= V14.
    V3 = 3,
}

#[cfg(test)]
pub mod test_helpers {
    use super::*;
    use console::network::MainnetV0;
    use snarkvm_synthesizer_process::Process;

    use std::sync::OnceLock;

    type CurrentNetwork = MainnetV0;
    type CurrentAleo = snarkvm_circuit::network::AleoV0;

    pub(crate) fn sample_deployment_v1(edition: u16, rng: &mut TestRng) -> Deployment<CurrentNetwork> {
        static INSTANCE: OnceLock<Deployment<CurrentNetwork>> = OnceLock::new();
        let deployment = INSTANCE
            .get_or_init(|| {
                // Initialize a new program.
                let (string, program) = Program::<CurrentNetwork>::parse(
                    r"
program testing_three.aleo;

mapping store:
    key as u32.public;
    value as u32.public;

function compute:
    input r0 as u32.private;
    add r0 r0 into r1;
    output r1 as u32.public;",
                )
                .unwrap();
                assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");
                // Construct the process.
                let process = Process::load().unwrap();
                // Compute the deployment.
                let deployment = process.deploy::<CurrentAleo, _>(&program, rng).unwrap();
                // Return the deployment.
                // Note: This is a testing-only hack to adhere to Rust's dependency cycle rules.
                Deployment::from_str(&deployment.to_string()).unwrap()
            })
            .clone();
        // Create a new deployment with the desired edition.
        // Note the only valid editions for V1 deployments are 0 and 1.
        Deployment::<CurrentNetwork>::new(
            edition % 2,
            deployment.program().clone(),
            deployment.verifying_keys().clone(),
            None,
            None,
            None,
        )
        .unwrap()
    }

    pub(crate) fn sample_deployment_v2(edition: u16, rng: &mut TestRng) -> Deployment<CurrentNetwork> {
        static INSTANCE: OnceLock<Deployment<CurrentNetwork>> = OnceLock::new();
        let deployment = INSTANCE
            .get_or_init(|| {
                // Initialize a new program.
                let (string, program) = Program::<CurrentNetwork>::parse(
                    r"
program testing_four.aleo;

mapping store:
    key as u32.public;
    value as u32.public;

function compute:
    input r0 as u32.private;
    add r0 r0 into r1;
    output r1 as u32.public;",
                )
                .unwrap();
                assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");
                // Construct the process.
                let process = Process::load().unwrap();
                // Compute the deployment.
                let deployment = process.deploy::<CurrentAleo, _>(&program, rng).unwrap();
                // Return the deployment.
                // Note: This is a testing-only hack to adhere to Rust's dependency cycle rules.
                Deployment::from_str(&deployment.to_string()).unwrap()
            })
            .clone();
        // Create a new deployment with the desired edition.
        Deployment::<CurrentNetwork>::new(
            edition,
            deployment.program().clone(),
            deployment.verifying_keys().clone(),
            deployment.program_checksum(),
            Some(Address::rand(rng)),
            None,
        )
        .unwrap()
    }

    pub(crate) fn sample_deployment_v3(edition: u16, rng: &mut TestRng) -> Deployment<CurrentNetwork> {
        static INSTANCE: OnceLock<Deployment<CurrentNetwork>> = OnceLock::new();
        let deployment = INSTANCE
            .get_or_init(|| {
                // Initialize a new program.
                let (string, program) = Program::<CurrentNetwork>::parse(
                    r"
program testing_five.aleo;

record data:
    owner as address.private;
    one as field.private;
    two as group.public;

mapping store:
    key as u32.public;
    value as u32.public;

function compute:
    input r0 as u32.private;
    add r0 r0 into r1;
    output r1 as u32.public;",
                )
                .unwrap();
                assert!(string.is_empty(), "Parser did not consume all of the string: '{string}'");
                // Construct the process.
                let process = Process::load().unwrap();
                // Compute the deployment.
                let deployment = process.deploy::<CurrentAleo, _>(&program, rng).unwrap();
                // Return the deployment.
                // Note: This is a testing-only hack to adhere to Rust's dependency cycle rules.
                Deployment::from_str(&deployment.to_string()).unwrap()
            })
            .clone();
        // Create a new deployment with the desired edition.
        Deployment::<CurrentNetwork>::new(
            edition,
            deployment.program().clone(),
            deployment.verifying_keys().clone(),
            deployment.program_checksum(),
            Some(Address::rand(rng)),
            deployment.translation_verifying_keys().clone(),
        )
        .unwrap()
    }
}
