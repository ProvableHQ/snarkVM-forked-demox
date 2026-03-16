# Aleo Slipstream Plugin Interface

This crate enables a plugin to be added into a SnarkVM runtime to
take actions at the time of mapping updates at block finalization;
for example, saving historical mappings state and staking data to an external database. The plugin must
implement the `SlipstreamPlugin` trait. Please see the details of the
`slipstream_plugin_interface.rs` for the interface definition.