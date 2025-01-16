#[macro_use]
extern crate criterion;

use snarkvm_algorithms::{AlgebraicSponge, crypto_hash::PoseidonSponge};
use snarkvm_curves::bls12_377::Fq;
use snarkvm_utilities::{TestRng, Uniform};

use criterion::Criterion;

fn benchmark_poseidon_cpu(c: &mut Criterion) {
    let rng = &mut TestRng::default();
    let mut sponge = PoseidonSponge::<Fq, 3, 1>::new();

    let mut input = Vec::with_capacity(100);
    for _ in 0..100 {
        input.push(Fq::rand(rng));
    }
    
    c.bench_function("Poseidon CPU absorb 100 elements", |b| {
        b.iter(|| {
            sponge.absorb_native_field_elements(&input);
        })
    });
}

criterion_group! {
  name = cpu_benchmarks;
  config = Criterion::default().measurement_time(std::time::Duration::from_secs(10));
  targets = benchmark_poseidon_cpu
}

criterion_main!(cpu_benchmarks);