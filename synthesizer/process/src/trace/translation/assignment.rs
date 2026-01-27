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

use circuit::{Aleo, Field as CircuitField, traits::ToFields};

use super::*;

/// Computes the ID of a console record (dynamic or external) given its field representation.
pub fn compute_console_external_record_id<N: Network>(
    function_id: Field<N>,
    record_fields: Vec<Field<N>>,
    tvk: Field<N>,
    index: U16<N>,
) -> Result<Field<N>> {
    let mut preimage = Vec::new();
    preimage.push(function_id);
    preimage.extend(record_fields);
    preimage.push(tvk);
    preimage.push(index.to_field()?);

    N::hash_psd8(&preimage)
}

/// Computes the ID of a circuit record (dynamic or external) given its field representation.
fn compute_record_id<A: Aleo>(
    function_id: CircuitField<A>,
    record_fields: Vec<CircuitField<A>>,
    tvk: CircuitField<A>,
    index: CircuitField<A>,
) -> CircuitField<A> {
    let mut preimage = Vec::new();
    preimage.push(function_id);
    preimage.extend(record_fields);
    preimage.push(tvk);
    preimage.push(index);

    A::hash_psd8(&preimage)
}

/// An assignment for the record translation circuit.
#[derive(Clone, Debug)]
pub struct TranslationAssignment<N: Network> {
    /// The static record (whether external or not).
    pub(super) record_static: Record<N, Plaintext<N>>,
    /// The dynamic record.
    pub(super) record_dynamic: DynamicRecord<N>,
    /// The ID of the program where the static record is defined (whether external or not), to be embedded as a constant.
    pub(super) program_id: ProgramID<N>,
    /// The function ID of the callee in the dynamic call.
    pub(super) function_id: Field<N>,
    /// The name of the static record (to be embedded as a constant).
    pub(super) record_name: Identifier<N>,
    /// True if translation is happening for an input to `dynamic.call` (static record is being produced) or an output of `dynamic.call` (static record is being consumed).
    pub(super) is_input: bool,
    /// Whether the value type corresponding to the static record is `Record` or that of an `ExternalRecord`.
    pub(super) static_is_external: bool,
    /// The index of this translation within the current batch.
    pub(super) translation_index: u16,
    /// The view key of the transition containing the dynamic call.
    pub(super) tvk: Field<N>,
    /// Index of the input operand or output destination that contains the (dynamic and static) record.
    // Note that the first three dynamic.call operands are reserved for call-related data, *however* this operand index still starts at 0 and is the same for caller and callee.
    pub(super) input_output_index: u16,
    /// The ID of the dynamic record.
    pub(super) id_dynamic: Field<N>,
    /// The ID of the static record:
    /// - If the static record is external, this is its `InputID` = `OutputID`.
    /// - If the static record is not external, this is
    ///    - Its `InputID`, i. e. its serial number, if the record is an input.
    ///    - Its `OutputID`, i. e. its commitment, if the record is an output.
    pub(super) id_static: Field<N>,
    /// The record view key of the static record. Irrelevant if `static_is_external` is true.
    pub(super) record_view_key: Field<N>,
    /// The additional point used to produce the serial number. Irrelevant if `is_input` is false or `static_is_external` is true.
    pub(super) gamma: Group<N>,
}

impl<N: Network> TranslationAssignment<N> {
    /// Initializes a new translation assignment.
    pub fn new(
        record_static: Record<N, Plaintext<N>>,
        record_dynamic: DynamicRecord<N>,
        program_id: ProgramID<N>,
        function_id: Field<N>,
        record_name: Identifier<N>,
        is_input: bool,
        static_is_external: bool,
        translation_index: u16,
        tvk: Field<N>,
        input_output_index: u16,
        id_dynamic: Field<N>,
        id_static: Field<N>,
        record_view_key: Option<Field<N>>,
        gamma: Option<Group<N>>,
    ) -> Self {
        Self {
            record_static,
            program_id,
            function_id,
            record_name,
            record_dynamic,
            is_input,
            static_is_external,
            translation_index,
            tvk,
            input_output_index,
            id_dynamic,
            id_static,
            record_view_key: record_view_key.unwrap_or_else(Field::zero),
            gamma: gamma.unwrap_or_else(Group::zero),
        }
    }

    // Internal auxiliary function which actually constructs the translation
    // circuit in `A`. The publicly exposed function `to_circuit_assignment`
    // ejects the resulting `Assignment` from the R1CS, but having direct access
    // to `A` while the constraint system is still loaded facilitates testing.
    pub(crate) fn to_circuit_assignment_internal<A: Aleo<Network = N>>(&self) -> Result<()> {
        // Ensure the circuit environment is clean.
        ensure!(
            A::count() == (0, 1, 0, 0, (0, 0, 0)),
            "Circuit environment is not clean: expected (0, 1, 0, 0, (0, 0, 0)), got {:?}",
            A::count()
        );
        A::reset();

        // ******** Constants

        // Inject the program ID as `Mode::Constant`.
        let circuit_program_id = circuit::ProgramID::<A>::constant(self.program_id);

        // Inject the record name as `Mode::Constant`.
        let circuit_record_name = circuit::Identifier::<A>::constant(self.record_name);

        // ******** Public inputs

        // Inject the translation-direction flag as `Mode::Public`.
        let circuit_is_input = circuit::Boolean::<A>::new(circuit::Mode::Public, self.is_input);

        // Inject the external-record flag as `Mode::Public`.
        let circuit_static_is_external = circuit::Boolean::<A>::new(circuit::Mode::Public, self.static_is_external);

        // Inject the calling function id as `Mode::Public`.
        let circuit_function_id = circuit::Field::<A>::new(circuit::Mode::Public, self.function_id);

        // Inject the translation index as `Mode::Public`.
        // Note that although the index is not explicitly used in the circuit, the prover and verifier must use the same value for proof verification to succeed.
        let _circuit_translation_index = circuit::Field::<A>::new(
            circuit::Mode::Public,
            console::types::Field::<N>::from_u16(self.translation_index),
        );

        // Inject the register index as `Mode::Public`.
        let circuit_input_output_index = circuit::Field::<A>::new(
            circuit::Mode::Public,
            console::types::Field::<N>::from_u16(self.input_output_index),
        );

        // Inject the commitment or serial number of the non-external record (if
        // `static_is_external`) or the input/output ID of the external record
        // (if not `static_is_external`) as `Mode::Public`.
        let circuit_id_static = circuit::Field::<A>::new(circuit::Mode::Public, self.id_static);

        // Inject the ID of the dynamic record as `Mode::Public`.
        let circuit_id_dynamic = circuit::Field::<A>::new(circuit::Mode::Public, self.id_dynamic);

        // ******** Private inputs (including implicit constants such as record-field names)

        // Inject the static record as `Mode::Private`.
        let circuit_record_static =
            circuit::Record::<A, circuit::Plaintext<A>>::new(circuit::Mode::Private, self.record_static.clone());

        // Inject the dynamic as `Mode::Private`.
        let circuit_record_dynamic =
            circuit::DynamicRecord::<A>::new(circuit::Mode::Private, self.record_dynamic.clone());

        // Inject the transition view key as `Mode::Private`.
        let circuit_tvk = circuit::Field::<A>::new(circuit::Mode::Private, self.tvk);

        // TODO (Compute the circuit RVK using the TVK)
        // Inject the record view key of the static record as `Mode::Private`.
        let circuit_record_view_key = circuit::Field::<A>::new(circuit::Mode::Private, self.record_view_key);

        // Inject the additional point used to produce the record commitment as `Mode::Private`.
        let circuit_gamma = circuit::Group::<A>::new(circuit::Mode::Private, self.gamma);

        // ******** Computing the IDs of the dynamic and static records

        // Compute the ID of the dynamic record.
        let actual_id_dynamic = compute_record_id(
            circuit_function_id.clone(),
            circuit_record_dynamic.to_fields(),
            circuit_tvk.clone(),
            circuit_input_output_index.clone(),
        );

        let circuit_static_commitment =
            circuit_record_static.to_commitment(&circuit_program_id, &circuit_record_name, &circuit_record_view_key);

        let circuit_static_serial_number = circuit::Record::<A, circuit::Plaintext<A>>::serial_number_from_gamma(
            &circuit_gamma,
            circuit_static_commitment.clone(),
        );

        // Input/output ID of the static record if it is not external (serial number or commitment).
        let actual_id_static_non_external =
            circuit::Field::<A>::ternary(&circuit_is_input, &circuit_static_serial_number, &circuit_static_commitment);

        // Input/output ID of the static record if it is external.
        // Note: External records have the same InputID and OutputID formula.
        let actual_id_static_external = compute_record_id(
            circuit_function_id,
            circuit_record_static.to_fields(),
            circuit_tvk,
            circuit_input_output_index,
        );

        let actual_id_static = circuit::Field::<A>::ternary(
            &circuit_static_is_external,
            &actual_id_static_external,
            &actual_id_static_non_external,
        );

        // ******** Merkelizing the static-record data

        let circuit_tree = circuit::DynamicRecord::<A>::merkleize_data(circuit_record_static.data())?;
        let circuit_data_root = circuit_tree.root();

        // ******** Assertions

        A::assert_eq(circuit_record_static.owner().to_group(), circuit_record_dynamic.owner().to_group())?;
        A::assert_eq(circuit_record_static.nonce(), circuit_record_dynamic.nonce())?;
        A::assert_eq(circuit_record_static.version(), circuit_record_dynamic.version())?;
        A::assert_eq(circuit_data_root, circuit_record_dynamic.root())?;
        A::assert_eq(actual_id_static, circuit_id_static)?;
        A::assert_eq(actual_id_dynamic, circuit_id_dynamic)?;

        Ok(())
    }

    /// The circuit for record-translation verification
    ///
    /// # Operation outline
    /// The `[[ ]]` notation is used to denote public inputs or constants.
    /// ```ignore
    ///     cm = commit([[program_id]], [[record_name]], record_static, record_view_key)
    ///     sn = serial_number(cm, gamma)
    ///     actual_id_non_external = is_input ? sn : cm
    ///     actual_id_external =  HashPSD8([[function_id]] | record_static | tvk | [[input_output_index]])
    ///     actual_id_static = is_external ? actual_id_external : actual_id_non_external
    ///     actual_id_dynamic = HashPSD8([[function_id]] | record_dynamic | tvk | [[input_output_index]])
    ///
    ///     assert record_static.owner == record_dynamic.owner
    ///     assert record_static.nonce == record_dynamic.nonce
    ///     assert record_static.version == record_dynamic.version
    ///     assert merkleize(record_static) == record_dynamic.root
    ///     assert [[id_record_static]] == actual_id_static
    ///     assert [[id_record_dynamic]] == actual_id_dynamic
    /// ```
    pub fn to_circuit_assignment<A: circuit::Aleo<Network = N>>(&self) -> Result<circuit::Assignment<N::Field>> {
        self.to_circuit_assignment_internal::<A>()?;
        Stack::log_circuit::<A>(
            format_args!("Translation circuit for dynamic record with nonce {}", self.record_static.nonce()),
            "TranslationAssignment".to_string(),
        );
        Ok(A::eject_assignment_and_reset())
    }
}
