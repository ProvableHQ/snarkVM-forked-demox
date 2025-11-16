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

use circuit::{Aleo, Poseidon2, Poseidon8, merkle_tree::MerkleTree, traits::{ToField, ToFields}};

use super::*;

type CircuitLH<A> = Poseidon8<A>;
type CircuitPH<A> = Poseidon2<A>;
type ConsoleLH<N> = console::algorithms::Poseidon8<N>;
type ConsolePH<N> = console::algorithms::Poseidon2<N>;

pub type RecordMerkleTree<A> = MerkleTree<A, CircuitLH<A>, CircuitPH<A>, RECORD_DATA_TREE_DEPTH>;

/// An assignment for the record translation circuit.
#[derive(Clone, Debug)]
pub struct TranslationAssignment<N: Network> {
    /// The static record.
    pub(super) record_static: Record<N, Plaintext<N>>,
    /// The ID of the program where the static record is defined.
    pub(super) program_id: ProgramID<N>,
    /// The function ID of the caller.
    pub(super) function_id: Field<N>,
    /// The name of the static record.
    pub(super) record_name: Identifier<N>,
    /// The dynamic record representing the static one.
    pub(super) record_dynamic: DynamicRecord<N>,
    /// True if the dynamic record is being translated to the static one, false if translation is happening in the opposite direction. 
    pub(super) to_static_record: bool,
    /// The number of times a translation circuit has been invoked in the current batch.
    pub(super) translation_count: u16,
    /// The view key of the transaction which produces or consumes the dynamic record.
    pub(super) tvk: Field<N>,
    /// Index of the input operand or output destination that contains the dynamic record.
    // Note that the first three dynamic call operands are reserved for
    // call-related data, *however* this operand index still starts at 0.
    pub(super) operand_index: u16,
    /// The ID of the dynamic record.
    pub(super) id_dynamic: Field<N>,
    /// The commitment (if producing `record_static`) or serial number (if consuming `record_static`) of the static record.
    pub(super) id_static: Field<N>,
    /// The record view key of the static record.
    pub(super) record_view_key: Field<N>,
    /// The additional point used to produce the record commitment and serial number.
    /// Irrelevant if `to_static_record` is false.
    pub(super) gamma: Group<N>,
} 

impl<N: Network> TranslationAssignment<N> {
    /// Initializes a new translation assignment.
    pub fn new(
        record_static: Record<N, Plaintext<N>>,
        program_id: ProgramID<N>,
        function_id: Field<N>,
        record_name: Identifier<N>,
        record_dynamic: DynamicRecord<N>,
        to_static_record: bool,
        translation_count: u16,
        tvk: Field<N>,
        register_index: u16,
        id_dynamic: Field<N>,
        id_static: Field<N>,
        record_view_key: Field<N>,
        gamma: Group<N>,
    ) -> Self {
        Self {
            record_static,
            program_id,
            function_id,
            record_name,
            record_dynamic,
            to_static_record,
            translation_count,
            tvk,
            operand_index: register_index,
            id_dynamic,
            id_static,
            record_view_key,
            gamma,
        }
    }

    // Internal auxiliary function which actually constructs the translation
    // circuit in `A`. The publicly exposed function `to_circuit_assignment`
    // ejects the resulting `Assignment` from the R1CS, but having direct access
    // to `A` while the constraint system is still loaded facilitates testing.
    pub(crate) fn to_circuit_assignment_internal<A: Aleo<Network = N>>(&self) -> Result<()> {
        // Ensure the circuit environment is clean.
        assert_eq!(A::count(), (0, 1, 0, 0, (0, 0, 0)));
        A::reset();

        // ******** Initial constants

        // Inject the program ID as `Mode::Constant`.
        let circuit_program_id = circuit::ProgramID::<A>::constant(self.program_id);

        // Inject the record name as `Mode::Constant`.
        let circuit_record_name = circuit::Identifier::<A>::constant(self.record_name);

        // ******** Public inputs and field-name constants

        // Inject the translation-direction flag as `Mode::Public`.
        let circuit_to_static_record = circuit::Boolean::<A>::new(circuit::Mode::Public, self.to_static_record);
        
        // Inject the calling function id as `Mode::Public`.
        let circuit_function_id = circuit::Field::<A>::new(circuit::Mode::Public, self.function_id);

        // Inject the translation count as `Mode::Public`.
        let _circuit_translation_count = circuit::U16::<A>::new(circuit::Mode::Public, console::types::U16::<N>::new(self.translation_count));

        // Inject the register index as `Mode::Public`.
        let circuit_register_index = circuit::U16::<A>::new(circuit::Mode::Public, console::types::U16::<N>::new(self.operand_index));
        
        // Inject the commitment or serial number of the static record as `Mode::Public`.
        let circuit_id_static = circuit::Field::<A>::new(circuit::Mode::Public, self.id_static);

        // Inject the ID of the dynamic record as `Mode::Public`.
        let circuit_id_dynamic = circuit::Field::<A>::new(circuit::Mode::Public, self.id_dynamic);

        // ******** Private inputs
        
        // Inject the static record as `Mode::Private`.
        let circuit_record_static = circuit::Record::<A, circuit::Plaintext<A>>::new(circuit::Mode::Private, self.record_static.clone());

        // Inject the dynamic as `Mode::Private`.
        let circuit_record_dynamic = circuit::DynamicRecord::<A>::new(circuit::Mode::Private, self.record_dynamic.clone());

        // Inject the transition view key as `Mode::Private`.
        let circuit_tvk = circuit::Field::<A>::new(circuit::Mode::Private, self.tvk);

        // Inject the record view key of the static record as `Mode::Private`.
        let circuit_record_view_key = circuit::Field::<A>::new(circuit::Mode::Private, self.record_view_key);

        // Inject the additional point used to produce the record commitment as `Mode::Private`.
        let circuit_gamma = circuit::Group::<A>::new(circuit::Mode::Private, self.gamma);

        // ******** Computing the IDs of the dynamic and static records
        
        let actual_id_dynamic = circuit_record_dynamic.to_id(
            circuit_function_id,
            circuit_tvk,
            circuit_register_index,
        );

        let circuit_static_commitment = circuit_record_static.to_commitment(
            &circuit_program_id,
            &circuit_record_name,
            &circuit_record_view_key,
        );

        let circuit_static_serial_number = circuit::Record::<A, circuit::Plaintext<A>>::serial_number_from_gamma(&circuit_gamma, circuit_static_commitment.clone());

        let actual_id_static = circuit::Field::<A>::ternary(
            &circuit_to_static_record,
            &circuit_static_serial_number,
            &circuit_static_commitment,
        );

        // ******** Merkelizing the static-record data

        let console_leaf_hasher = ConsoleLH::<A::Network>::setup("DynamicRecordLeafHasher").unwrap();
        let console_path_hasher = ConsolePH::<A::Network>::setup("DynamicRecordPathHasher").unwrap();
        let circuit_leaf_hasher = CircuitLH::<A>::constant(console_leaf_hasher.clone());
        let circuit_path_hasher = CircuitPH::<A>::constant(console_path_hasher.clone());

        let circuit_leaves = circuit_record_static.data().iter().map(|(identifier, entry)| {
            let mut leaf = vec![identifier.to_field()];
            leaf.extend(entry.to_fields());
            leaf
        }).collect::<Vec<Vec<circuit::Field<A>>>>();

        let circuit_tree = RecordMerkleTree::<A>::new(circuit_leaf_hasher, circuit_path_hasher, &circuit_leaves).unwrap();
        let circuit_data_root = circuit_tree.root();

        // ******** Assertions

        A::assert_eq(circuit_record_static.owner().to_group(), circuit_record_dynamic.owner().to_group());
        A::assert_eq(circuit_record_static.nonce(), circuit_record_dynamic.nonce());
        A::assert_eq(circuit_record_static.version(), circuit_record_dynamic.version());
        A::assert_eq(circuit_data_root, circuit_record_dynamic.root());
        A::assert_eq(actual_id_static, circuit_id_static);
        A::assert_eq(actual_id_dynamic, circuit_id_dynamic);

        Ok(())
    }

    /// The circuit for record-translation verification
    ///
    /// # Operation outline
    /// The `[[ ]]` notation is used to denote public inputs or constants.
    /// ```ignore
    ///     cm = commit(static_record, [[program_id]], [[record_name]], record_view_key)
    ///     sn = serial_number(cm, gamma)
    ///     internal_id_static_record = to_static_record ? sn : cm
    ///     internal_id_dynamic_record = HashPSD8([[calling_function_id]] | dynamic_record | tvk | [[register_index]])
    /// 
    ///     assert static_record.owner == dynamic_record.owner
    ///     assert static_record.nonce == dynamic_record.nonce
    ///     assert static_record.version == dynamic_record.version
    ///     assert merkleize(static_record) == dynamic_record.root
    ///     assert [[id_static_record]] == internal_id_static_record
    ///     assert [[id_dynamic_record]] == internal_id_dynamic_record
    /// ```
    pub fn to_circuit_assignment<A: circuit::Aleo<Network = N>>(&self) -> Result<circuit::Assignment<N::Field>> {
        self.to_circuit_assignment_internal::<A>()?;
        Stack::log_circuit::<A>(format_args!("Translation circuit for dynamic record with nonce {}", self.record_static.nonce()));
        Ok(A::eject_assignment_and_reset())
    }
}
