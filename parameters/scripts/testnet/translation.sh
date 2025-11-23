# Generates the translation proving and verifying key.

# Inputs: network

cargo run --release --example translation testnet -- --nocapture || exit

mv translation_credits_aleo_credits.metadata ../../src/testnet/resources/credits || exit
mv translation_credits_aleo_credits.prover.* ~/.aleo/resources || exit
mv translation_credits_aleo_credits.verifier ../../src/testnet/resources/credits || exit
