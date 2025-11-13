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

extern crate criterion;

use criterion::{Criterion, criterion_group, criterion_main};

use snarkvm_console_network::MainnetV0;
use snarkvm_console_program::{Ciphertext, Group, Record};
use snarkvm_console_types::{Field, Scalar};
use snarkvm_utilities::{TestRng, Uniform};

type CurrentNetwork = MainnetV0;

pub fn benchmark_hash_functions(c: &mut Criterion) {
    let rng = &mut TestRng::default();
    let randomizer = Scalar::rand(rng);
    let field1 = Field::<CurrentNetwork>::from_u64(u64::rand(rng));
    let field2 = Field::<CurrentNetwork>::from_u64(u64::rand(rng));
    let nonce = Group::<CurrentNetwork>::rand(rng);

    c.bench_function("is_owner", |b| {
        b.iter(|| {
            Record::<CurrentNetwork, Ciphertext<CurrentNetwork>>::is_owner_direct(field1, randomizer, nonce, field2)
        });
    });

    // benchmark is_owner_direct_precompute
    c.bench_function("is_owner_precompute", |b| {
        b.iter(|| {
            Record::<CurrentNetwork, Ciphertext<CurrentNetwork>>::is_owner_direct_precompute(
                field1, randomizer, nonce, field2,
            )
        });
    });
}

criterion_group!(benches, benchmark_hash_functions);
criterion_main!(benches);
