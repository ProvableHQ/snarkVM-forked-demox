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
    /// Verifies the given deployment is ordered.
    #[inline]
    pub fn verify_deployment<A: circuit::Aleo<Network = N>, R: Rng + CryptoRng>(
        &self,
        consensus_version: ConsensusVersion,
        deployment: &Deployment<N>,
        rng: &mut R,
    ) -> Result<()> {
        let timer = timer!("Process::verify_deployment");

        // Retrieve the program ID.
        let program_id = deployment.program().id();
        // Check if this deployment version requires the program to already exist.
        // - V3: Deployments with translation VKs (used for upgrades at V14+).
        // - V4: Amendments that update VKs without changing edition or owner.
        let version = deployment.version()?;
        let requires_existing_program = matches!(version, DeploymentVersion::V3 | DeploymentVersion::V4);
        // If the deployment requires an existing program, verify that it exists.
        // If the edition is zero (and no existing program required), verify that the program does not exist.
        // Otherwise, verify that the program exists.
        if requires_existing_program {
            ensure!(
                self.contains_program(program_id),
                "Program '{program_id}' does not exist, but deployment requires an existing program (V3/V4)"
            );
        } else {
            match deployment.edition().is_zero() {
                true => ensure!(
                    !self.contains_program(program_id),
                    "Program '{program_id}' already exists, but the deployment edition is zero"
                ),
                false => ensure!(
                    self.contains_program(program_id),
                    "Program '{program_id}' does not exist, but the deployment edition is non-zero"
                ),
            }
        }

        // Ensure the program is well-formed, by computing the stack.
        // Note: The program owner is intentionally not set, since `program_owner` is an operand
        //   that is only available in a finalize scope.
        let stack = if requires_existing_program {
            // For V3/V4 deployments, use the existing edition instead of incrementing.
            let existing_stack = self.get_stack(program_id)?;
            let stack = Stack::new_raw(self, deployment.program(), *existing_stack.program_edition())?;
            stack.initialize_and_check(self)?;
            stack
        } else {
            Stack::new(self, deployment.program())?
        };
        lap!(timer, "Compute the stack");

        // Ensure the verifying keys are well-formed and the certificates are valid.
        let verification = stack.verify_deployment::<A, R>(consensus_version, deployment, rng);
        lap!(timer, "Verify the deployment");

        finish!(timer);
        verification
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type CurrentAleo = circuit::network::AleoV0;

    /// Use `cargo test profiler --features timer` to run this test.
    #[ignore]
    #[test]
    fn test_profiler() -> Result<()> {
        let rng = &mut TestRng::default();

        // Initialize the process.
        let process = Process::load()?;

        // Fetch the large program to deploy.
        let large_program = Program::from_str(include_str!("./resources/large_functions.aleo"))?;

        // Create a deployment for the program.
        let deployment = process.deploy::<CurrentAleo, _>(&large_program, rng)?;

        // Verify the deployment.
        assert!(process.verify_deployment::<CurrentAleo, _>(ConsensusVersion::V8, &deployment, rng).is_ok());

        bail!("\n\nRemember to #[ignore] this test!\n\n")
    }
}
