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

use crate::slipstream_manager::SlipstreamPluginManager;

use std::{
    path::PathBuf,
    sync::{Arc, RwLock},
};
use thiserror::Error;

/// The service managing the Slipstream plugin workflow.
pub struct SlipstreamPluginService {
    plugin_manager: Arc<RwLock<SlipstreamPluginManager>>,
}

impl SlipstreamPluginService {
    /// Initializes the service from a list of plugin config files.
    ///
    /// Each config file must be a JSON5 file with a `libpath` field pointing to the
    /// shared library that implements `SlipstreamPlugin`.
    pub fn new(config_files: &[PathBuf]) -> Result<Self, SlipstreamPluginServiceError> {
        let mut manager = SlipstreamPluginManager::new();
        for path in config_files {
            manager.load_plugin(path).map_err(|e| SlipstreamPluginServiceError::FailedToLoadPlugin(e.to_string()))?;
        }
        Ok(Self { plugin_manager: Arc::new(RwLock::new(manager)) })
    }

    /// Returns a clone of the shared plugin manager handle.
    pub fn plugin_manager(&self) -> Arc<RwLock<SlipstreamPluginManager>> {
        self.plugin_manager.clone()
    }

    /// Unloads all plugins and shuts down the service.
    pub fn join(self) {
        match self.plugin_manager.write() {
            Ok(mut manager) => manager.unload(),
            Err(e) => {
                tracing::warn!("Slipstream: plugin manager lock poisoned during shutdown, attempting recovery: {e}");
                e.into_inner().unload();
            }
        }
    }
}

#[derive(Error, Debug)]
pub enum SlipstreamPluginServiceError {
    #[error("Failed to load a Slipstream plugin: {0}")]
    FailedToLoadPlugin(String),
}
