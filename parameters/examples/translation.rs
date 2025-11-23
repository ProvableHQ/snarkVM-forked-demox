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

use snarkvm_algorithms::crypto_hash::sha256::sha256;
use snarkvm_circuit::Aleo;
use snarkvm_console::{
    network::{CanaryV0, MainnetV0, Network, TestnetV0},
    prelude::ToBytes,
    program::{Identifier, ProgramID},
};
use snarkvm_synthesizer::{Process, program::StackTrait};

use anyhow::Result;
use rand::thread_rng;
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    str::FromStr,
};

fn checksum(bytes: &[u8]) -> String {
    hex::encode(sha256(bytes))
}

fn versioned_filename(filename: &str, checksum: &str) -> String {
    match checksum.get(0..7) {
        Some(sum) => format!("{filename}.{sum}"),
        _ => filename.to_string(),
    }
}

/// Writes the given bytes to the given versioned filename.
fn write_remote(filename: &str, version: &str, bytes: &[u8]) -> Result<()> {
    let mut file = BufWriter::new(File::create(PathBuf::from(&versioned_filename(filename, version)))?);
    file.write_all(bytes)?;
    Ok(())
}

/// Writes the given bytes to the given filename.
fn write_local(filename: &str, bytes: &[u8]) -> Result<()> {
    let mut file = BufWriter::new(File::create(PathBuf::from(filename))?);
    file.write_all(bytes)?;
    Ok(())
}

/// Writes the given metadata as JSON to the given filename.
fn write_metadata(filename: &str, metadata: &Value) -> Result<()> {
    let mut file = BufWriter::new(File::create(PathBuf::from(filename))?);
    file.write_all(&serde_json::to_vec_pretty(metadata)?)?;
    Ok(())
}

/// Synthesizes the circuit keys for the credits.aleo credits record translation circuit. (cargo run --release --example translation [network])
pub fn translation<N: Network, A: Aleo<Network = N>>() -> Result<()> {
    let rng = &mut thread_rng();
    let process = Process::<N>::setup::<A, _>(rng)?;
    let credits_stack = process.get_stack(ProgramID::<N>::from_str("credits.aleo").unwrap())?;
    let credits_record_name = Identifier::<N>::from_str("credits").unwrap();
    let proving_key = credits_stack.get_translation_proving_key(&credits_record_name)?;
    let verifying_key = credits_stack.get_translation_verifying_key(&credits_record_name)?;

    // TODO(dynamic_dispatch): similar to the inclusion circuit, prove and verify the verifying key as a sanity check.

    // Initialize a vector for the commands.
    let mut commands = vec![];

    let proving_key_bytes = proving_key.to_bytes_le()?;
    let proving_key_checksum = checksum(&proving_key_bytes);

    let verifying_key_bytes = verifying_key.to_bytes_le()?;
    let verifying_key_checksum = checksum(&verifying_key_bytes);

    let metadata = json!({
        "prover_checksum": proving_key_checksum,
        "prover_size": proving_key_bytes.len(),
        "verifier_checksum": verifying_key_checksum,
        "verifier_size": verifying_key_bytes.len(),
    });

    println!("{}", serde_json::to_string_pretty(&metadata)?);
    write_metadata(&format!("translation_credits.metadata"), &metadata)?;
    write_remote(&format!("translation_credits.prover"), &proving_key_checksum, &proving_key_bytes)?;
    write_local(&format!("translation_credits.verifier"), &verifying_key_bytes)?;

    commands.push(format!("upload \"{}\"", versioned_filename(&format!("translation_credits.prover"), &proving_key_checksum)));

    // Print the commands.
    println!("\nNow, perform the following operations:\n");
    for command in commands {
        println!("{command}");
    }
    println!();

    Ok(())
}

/// Run the following command to generate the translation circuit keys.
/// `cargo run --example translation [network]`
pub fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        println!("Invalid number of arguments. Given: {} - Required: 1", args.len() - 1);
        return Ok(());
    }

    match args[1].as_str() {
        "mainnet" => {
            translation::<MainnetV0, snarkvm_circuit::AleoV0>()?;
        }
        "testnet" => {
            translation::<TestnetV0, snarkvm_circuit::AleoTestnetV0>()?;
        }
        "canary" => {
            translation::<CanaryV0, snarkvm_circuit::AleoCanaryV0>()?;
        }
        _ => panic!("Invalid network"),
    };

    Ok(())
}
