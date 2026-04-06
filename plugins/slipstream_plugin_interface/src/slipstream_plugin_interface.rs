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

/// The interface for Aleo Slipstream plugins. A plugin must implement
/// the `SlipstreamPlugin` trait to work with the runtime. In addition,
/// the dynamic library must export a `C` function `_create_plugin` that
/// creates the implementation of the plugin.
use anyhow::Result;
use std::any::Any;

pub trait SlipstreamPlugin: Any + Send + Sync + std::fmt::Debug {
    /// Returns the name of the plugin.
    fn name(&self) -> &'static str;

    /// The callback called when a plugin is loaded by the system, used for
    /// doing whatever initialization is required by the plugin. The
    /// `_config_file` contains the name of the config file (JSON format) with
    /// a `libpath` field indicating the full path of the shared library.
    fn on_load(&mut self, _config_file: &str, _is_reload: bool) -> Result<()> {
        Ok(())
    }

    /// The callback called right before a plugin is unloaded by the system.
    /// Used for doing cleanup before unload.
    fn on_unload(&mut self) {}

    /// Called when a mapping key-value pair is inserted or updated during canonical finalize.
    /// All arguments are serialized to bytes to keep the trait object-safe.
    fn notify_mapping_update(
        &self,
        _program_id: &[u8],
        _mapping_name: &[u8],
        _key: &[u8],
        _value: &[u8],
        _block_height: u32,
    ) -> Result<()> {
        Ok(())
    }

    /// Called once per staker per block during staking reward distribution.
    fn notify_staking_reward(
        &self,
        _staker: &[u8],
        _validator: &[u8],
        _reward: u64,
        _new_stake: u64,
        _block_height: u32,
    ) -> Result<()> {
        Ok(())
    }

    /// Returns `true` if the plugin is interested in general mapping update data.
    fn history_enabled(&self) -> bool {
        false
    }

    /// Returns `true` if the plugin is interested in staking reward data.
    fn history_staking_rewards_enabled(&self) -> bool {
        false
    }
}
