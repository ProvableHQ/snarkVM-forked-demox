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

#[macro_use]
extern crate criterion;

use snarkvm_algorithms::{AlgebraicSponge, crypto_hash::PoseidonSponge};
use snarkvm_curves::bls12_377::Fq;
use snarkvm_utilities::{TestRng, Uniform};

use criterion::Criterion;

#[cfg(feature = "cuda")]
fn benchmark_poseidon_cuda(c: &mut Criterion) {
  let rng = &mut TestRng::default();
  let mut sponge = PoseidonSponge::<Fq, 3, 1>::new();

  let mut input = Vec::with_capacity(100);
  for _ in 0..100 {
      input.push(Fq::rand(rng));
  }
    c.bench_function("Poseidon CUDA absorb 100 elements", |b| {
        b.iter(|| {
            sponge.absorb_native_field_elements(&input);
        })
    });
}

#[cfg(feature = "cuda")]
criterion_group! {
  name = cuda_benchmarks;
  config = Criterion::default().measurement_time(std::time::Duration::from_secs(10));
  targets = benchmark_poseidon_cuda
}

#[cfg(feature = "cuda")]
criterion_main!(cuda_benchmarks);
