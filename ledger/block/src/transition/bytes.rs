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

impl<N: Network> FromBytes for Transition<N> {
    /// Reads the output from a buffer.
    fn read_le<R: Read>(mut reader: R) -> IoResult<Self> {
        // Read the version.
        let version = u8::read_le(&mut reader)?;
        // Validate the version.
        if version != 1 && version != 2 {
            return Err(error(format!("Invalid transition version: {version}")));
        }

        // Read the transition ID.
        let transition_id = N::TransitionID::read_le(&mut reader)?;
        // Read the program ID.
        let program_id = FromBytes::read_le(&mut reader)?;
        // Read the function name.
        let function_name = FromBytes::read_le(&mut reader)?;

        // Read the number of inputs.
        let num_inputs: u8 = FromBytes::read_le(&mut reader)?;
        // Ensure the number of inputs is within bounds.
        if num_inputs as usize > N::MAX_INPUTS {
            return Err(error(format!(
                "Transition (from 'read_le') has too many inputs ({} > {})",
                num_inputs,
                N::MAX_INPUTS
            )));
        }
        // Read the inputs.
        let mut inputs = Vec::with_capacity(num_inputs as usize);
        for _ in 0..num_inputs {
            // Read the input.
            inputs.push(FromBytes::read_le(&mut reader)?);
        }

        // Read the number of outputs.
        let num_outputs: u8 = FromBytes::read_le(&mut reader)?;
        // Ensure the number of outputs is within bounds.
        if num_outputs as usize > N::MAX_OUTPUTS {
            return Err(error(format!(
                "Transition (from 'read_le') has too many outputs ({} > {})",
                num_outputs,
                N::MAX_OUTPUTS
            )));
        }
        // Read the outputs.
        let mut outputs = Vec::with_capacity(num_outputs as usize);
        for _ in 0..num_outputs {
            // Read the output.
            outputs.push(FromBytes::read_le(&mut reader)?);
        }

        // Read the transition public key.
        let tpk = FromBytes::read_le(&mut reader)?;
        // Read the transition commitment.
        let tcm = FromBytes::read_le(&mut reader)?;
        // Read the signer commitment.
        let scm = FromBytes::read_le(&mut reader)?;

        // If the version is 2, read the caller metadata. V1 transitions have no caller metadata.
        let caller_metadata = match version {
            1 => None,
            2 => {
                // Read the is_dynamic flag.
                let is_dynamic = bool::read_le(&mut reader)?;
                // If the metadata is dynamic, then read the inputs and outputs.
                if is_dynamic {
                    // Read the caller inputs.
                    let caller_inputs =
                        (0..num_inputs).map(|_| FromBytes::read_le(&mut reader)).collect::<Result<Vec<_>, _>>()?;
                    // Read the caller outputs.
                    let caller_outputs =
                        (0..num_outputs).map(|_| FromBytes::read_le(&mut reader)).collect::<Result<Vec<_>, _>>()?;
                    // Construct the caller metadata.
                    Some(
                        TransitionCallerMetadata::new_dynamic(caller_inputs, caller_outputs)
                            .map_err(|e| error(e.to_string()))?,
                    )
                } else {
                    Some(TransitionCallerMetadata::new_static())
                }
            }
            // SAFETY: Version is validated above to be 1 or 2.
            _ => unreachable!(),
        };

        // Construct the candidate transition.
        let transition = Self::new(program_id, function_name, inputs, outputs, tpk, tcm, scm, caller_metadata)
            .map_err(|e| error(e.to_string()))?;
        // Ensure the transition ID matches the expected ID.
        match transition_id == *transition.id() {
            true => Ok(transition),
            false => Err(error("Transition ID is incorrect, possible data corruption")),
        }
    }
}

impl<N: Network> ToBytes for Transition<N> {
    /// Writes the literal to a buffer.
    fn write_le<W: Write>(&self, mut writer: W) -> IoResult<()> {
        // Write the version based on caller_metadata presence.
        match &self.caller_metadata {
            None => 1u8.write_le(&mut writer)?,
            Some(_) => 2u8.write_le(&mut writer)?,
        }

        // Write the transition ID.
        self.id.write_le(&mut writer)?;
        // Write the program ID.
        self.program_id.write_le(&mut writer)?;
        // Write the function name.
        self.function_name.write_le(&mut writer)?;

        // Write the number of inputs.
        (u8::try_from(self.inputs.len()).map_err(|e| error(e.to_string()))?).write_le(&mut writer)?;
        // Write the inputs.
        self.inputs.write_le(&mut writer)?;

        // Write the number of outputs.
        (u8::try_from(self.outputs.len()).map_err(|e| error(e.to_string()))?).write_le(&mut writer)?;
        // Write the outputs.
        self.outputs.write_le(&mut writer)?;

        // Write the transition public key.
        self.tpk.write_le(&mut writer)?;
        // Write the transition commitment.
        self.tcm.write_le(&mut writer)?;
        // Write the signer commitment.
        self.scm.write_le(&mut writer)?;

        // If the version is V2, write the caller metadata.
        if let Some(caller_metadata) = &self.caller_metadata {
            // Write the is_dynamic flag.
            caller_metadata.is_dynamic().write_le(&mut writer)?;
            // If the metadata is dynamic, write the inputs and outputs.
            // Note that the unwraps are safe, since `is_dynamic()` implies the presence of inputs and outputs.
            if caller_metadata.is_dynamic() {
                // Write the caller inputs.
                for input in caller_metadata.inputs().unwrap() {
                    input.write_le(&mut writer)?;
                }
                // Write the caller outputs.
                for output in caller_metadata.outputs().unwrap() {
                    output.write_le(&mut writer)?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes() -> Result<()> {
        let rng = &mut TestRng::default();

        // Sample the transition.
        let expected = crate::transition::test_helpers::sample_transition(rng);

        // Check the byte representation.
        let expected_bytes = expected.to_bytes_le()?;
        assert_eq!(expected, Transition::read_le(&expected_bytes[..])?);

        Ok(())
    }

    #[test]
    fn test_bytes_dynamic() -> Result<()> {
        let rng = &mut TestRng::default();

        for _ in 0..3 {
            // Sample the transition.
            let static_transition = crate::transition::test_helpers::sample_transition(rng);

            // Create dynamic caller metadata from the static transition's inputs/outputs.
            let caller_metadata = TransitionCallerMetadata::new_dynamic(
                static_transition.inputs().to_vec(),
                static_transition.outputs().to_vec(),
            )
            .unwrap();

            // Create the dynamic transition using `Transition::new` to ensure the ID is computed correctly.
            // Note: The transition ID includes the caller metadata when present.
            let dynamic_transition = Transition::new(
                *static_transition.program_id(),
                *static_transition.function_name(),
                static_transition.inputs().to_vec(),
                static_transition.outputs().to_vec(),
                *static_transition.tpk(),
                *static_transition.tcm(),
                *static_transition.scm(),
                Some(caller_metadata),
            )?;

            // Check the byte representation.
            let expected_bytes = dynamic_transition.to_bytes_le()?;
            assert_eq!(dynamic_transition, Transition::read_le(&expected_bytes[..])?);

            // Verify the inclusion_id differs from the transition ID (since caller_metadata is present).
            assert_ne!(*dynamic_transition.id(), dynamic_transition.inclusion_id());
        }

        Ok(())
    }

    #[test]
    fn test_bytes_static_v2() -> Result<()> {
        let rng = &mut TestRng::default();

        for _ in 0..3 {
            // Sample a V1 transition.
            let v1_transition = crate::transition::test_helpers::sample_transition(rng);

            // Create static caller metadata (V2 with is_dynamic = false).
            let caller_metadata = TransitionCallerMetadata::new_static();

            // Create the V2 static transition.
            let static_v2_transition = Transition::new(
                *v1_transition.program_id(),
                *v1_transition.function_name(),
                v1_transition.inputs().to_vec(),
                v1_transition.outputs().to_vec(),
                *v1_transition.tpk(),
                *v1_transition.tcm(),
                *v1_transition.scm(),
                Some(caller_metadata),
            )?;

            // Verify it's a V2 transition by checking caller_metadata is present.
            assert!(static_v2_transition.caller_metadata().is_some());

            // For static metadata, id != inclusion_id (because id includes the metadata hash).
            assert_ne!(*static_v2_transition.id(), static_v2_transition.inclusion_id());

            // Check the byte representation round-trips correctly.
            let expected_bytes = static_v2_transition.to_bytes_le()?;
            let deserialized = Transition::read_le(&expected_bytes[..])?;
            assert_eq!(static_v2_transition, deserialized);

            // Verify the deserialized transition also has correct inclusion_id.
            assert_eq!(static_v2_transition.inclusion_id(), deserialized.inclusion_id());
        }

        Ok(())
    }
}
