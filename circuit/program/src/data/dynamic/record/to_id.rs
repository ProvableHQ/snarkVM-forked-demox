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

impl<A: Aleo> DynamicRecord<A> {
    /// Returns the ID of the dynamic record.
    pub fn to_id(
        &self,
        function_id: Field<A>,
        tvk: Field<A>,
        index: U16<A>,
    ) -> Field<A> {
        // Construct the preimage as `(function ID || self || tvk || index)`.
        let mut preimage = Vec::new();
        preimage.push(function_id);
        preimage.extend(self.to_fields());
        preimage.push(tvk);
        preimage.push(index.to_field());

        A::hash_psd8(&preimage)
    }
}
