use std::{fs::OpenOptions, time::Instant};

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use console::{
    account::{Address, PrivateKey},
    network::{prelude::*, MainnetV0},
    program::{Identifier, Literal, Plaintext, ProgramID, Value},
    types::{I128, I16, I32, I64, I8, U128, U16, U32, U64, U8},
};
use ledger_store::{
    FinalizeStorage, FinalizeStore
};
use synthesizer_program::FinalizeStoreTrait;

type CurrentNetwork = MainnetV0;

fn write_to_csv(benchmark_name: &str, size: usize, time_per_key: f64) {
  let mut file = OpenOptions::new()
      .create(true)
      .append(true)
      .open("./benchmark_results.csv")
      .expect("Failed to open CSV file");

  writeln!(file, "{},{},{}", benchmark_name, size, time_per_key * 1_000_000.0)
      .expect("Failed to write to CSV file");
}

fn generate_nested_array_string(depth: usize, elements_per_array: usize) -> String {
  // Base string for individual elements in the array.
  let element = "1field";

  // Helper function to generate a single level of the array.
  fn generate_array_level(elements_per_array: usize, element: &str) -> String {
      let elements = vec![element; elements_per_array].join(", ");
      format!("[{}]", elements)
  }

  // Generate the nested arrays by iteratively embedding them.
  let mut result = generate_array_level(elements_per_array, element);

  for _ in 1..depth {
      result = format!("[{}]", result);
  }

  // Wrap the entire result in raw string quotes (r"").
  format!(r#"{}"#, result)
}

fn bench_update_key_value(c: &mut Criterion) {
    // Initialize the RNG.
    let rng = &mut TestRng::default();

    // Initialize a new finalize store.
    let finalize_store = {
      let temp_dir = tempfile::tempdir().expect("Failed to open temporary directory").into_path();
      let program_rocksdb = ledger_store::helpers::rocksdb::FinalizeDB::open_testing(temp_dir, None).unwrap();
      FinalizeStore::from(program_rocksdb).unwrap()
  };
  
    let program_id = ProgramID::from_str("credits.aleo").unwrap();
    let account_mapping = Identifier::from_str("account").unwrap();

    // Initialize the account mapping, ignoring the result.
    let _ = finalize_store.initialize_mapping(program_id, account_mapping);

    // Generate N unique keys upfront.
    let num_keys = 100; // Adjust this as needed.
    let keys: Vec<Plaintext<CurrentNetwork>> = (0..num_keys)
        .map(|_| {
            let private_key = PrivateKey::<CurrentNetwork>::new(&mut TestRng::default()).unwrap();
            let address = Address::try_from(private_key).unwrap();
            Plaintext::from(Literal::Address(address))
        })
        .collect();

    let small_struct_string = format!(
        r#"{{
            owner: aleo1d5hg2z3ma00382pngntdp68e74zv54jdxy249qhaujhks9c72yrs33ddah,
            data0: {},
            data1: {},
            data2: {},
            data3: {}
        }}"#,
        generate_nested_array_string(2, 2),
        generate_nested_array_string(2, 2),
        generate_nested_array_string(2, 2),
        generate_nested_array_string(2, 2),
    );
    let large_struct_string = format!(
        r#"{{
            owner: aleo1d5hg2z3ma00382pngntdp68e74zv54jdxy249qhaujhks9c72yrs33ddah,
            data0: {},
            data1: {},
            data2: {},
            data3: {},
            data4: {},
            data5: {},
            data6: {},
            data7: {},
            data8: {},
            data9: {},
            data10: {},
            data11: {}
        }}"#,
        generate_nested_array_string(12, 12),
        generate_nested_array_string(12, 12),
        generate_nested_array_string(12, 12),
        generate_nested_array_string(12, 12),
        generate_nested_array_string(12, 12),
        generate_nested_array_string(12, 12),
        generate_nested_array_string(12, 12),
        generate_nested_array_string(12, 12),
        generate_nested_array_string(12, 12),
        generate_nested_array_string(12, 12),
        generate_nested_array_string(12, 12),
        generate_nested_array_string(12, 12),
    );
    let small_array_string = generate_nested_array_string(1, 1);
    let medium_array_string = generate_nested_array_string(16, 16);
    let large_array_string = generate_nested_array_string(32, 32);

    // Define the different types of values to test, along with their type names.
    let test_values: Vec<(&'static str, Value<CurrentNetwork>)> = vec![
        ("small_struct", Value::from_str(&small_struct_string).unwrap()),
        ("large_struct", Value::from_str(&large_struct_string).unwrap()),
        ("array_1x1", Value::from_str(&small_array_string).unwrap()),
        ("array_16x16", Value::from_str(&medium_array_string).unwrap()),
        ("array_32x32", Value::from_str(&large_array_string).unwrap()),
        ("I8", Value::from(Literal::I8(I8::new(42)))),
        ("I16", Value::from(Literal::I16(I16::new(42)))),
        ("I32", Value::from(Literal::I32(I32::new(42)))),
        ("I64", Value::from(Literal::I64(I64::new(42)))),
        ("I128", Value::from(Literal::I128(I128::new(42)))),
        ("U8", Value::from(Literal::U8(U8::new(42)))),
        ("U16", Value::from(Literal::U16(U16::new(42)))),
        ("U32", Value::from(Literal::U32(U32::new(42)))),
        ("U64", Value::from(Literal::U64(U64::new(42)))),
        ("U128", Value::from(Literal::U128(U128::new(42)))),
    ];

  // Loop over each value type and benchmark the get and set operations.
  for (type_name, value) in test_values.into_iter() {
      let value_clone = value.clone();

      // Benchmark the set operation with different keys.
      c.bench_with_input(
          BenchmarkId::new(format!("Set key value - {}", type_name), num_keys),
          &keys,
          |b, keys| {
              b.iter_custom(|iterations| {
                  let start = Instant::now();
                  for _ in 0..iterations {
                      for key in keys.iter() {
                          finalize_store
                              .update_key_value(program_id, account_mapping, key.clone(), value_clone.clone())
                              .unwrap();
                      }
                  }
                  let duration = start.elapsed();

                  // Write the results to a CSV file.
                  let value_size = value.to_bytes_le().unwrap().len();
                  let time_per_key = duration.as_secs_f64() / (iterations as f64 * num_keys as f64);
                  write_to_csv(&format!("set-{}", type_name), value_size, time_per_key);

                  // Divide total time by the number of keys for accurate per-key timing.
                  duration / num_keys as u32
              });
          },
      );

      // Benchmark the get operation with different keys.
      c.bench_with_input(
          BenchmarkId::new(format!("Get key value - {}", type_name), num_keys),
          &keys,
          |b, keys| {
              b.iter_custom(|iterations| {
                  let start = Instant::now();
                  for _ in 0..iterations {
                      for key in keys.iter() {
                          let result = finalize_store
                              .get_value_speculative(program_id, account_mapping, key)
                              .unwrap();
                          black_box(result); // Prevent optimization.
                      }
                  }
                  let duration = start.elapsed();

                  // Write the results to a CSV file.
                  let value_size = value.to_bytes_le().unwrap().len();
                  let time_per_key = duration.as_secs_f64() / (iterations as f64 * num_keys as f64);
                  write_to_csv(&format!("get-{}", type_name), value_size, time_per_key);

                  // Divide total time by the number of keys for accurate per-key timing.
                  duration / num_keys as u32
              });
          },
      );
  }
}

criterion_group!(
  name = update_key_value;
  config = Criterion::default().sample_size(10);
  targets = bench_update_key_value
);
criterion_main!(update_key_value);