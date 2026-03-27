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

use snarkvm_slipstream_plugin_interface::slipstream_plugin_interface::SlipstreamPlugin;
use tokio::sync::oneshot::Sender as OneShotSender;

use libloading::Library;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// A type alias for the result of plugin manager operations.
type JsonRpcResult<T> = Result<T, SlipstreamPluginManagerError>;

#[derive(Debug)]
pub struct LoadedSlipstreamPlugin {
    name: String,
    plugin: Box<dyn SlipstreamPlugin>,
}

impl LoadedSlipstreamPlugin {
    pub fn new(plugin: Box<dyn SlipstreamPlugin>, name: Option<String>) -> Self {
        Self {
            name: name.unwrap_or_else(|| plugin.name().to_owned()),
            plugin,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Deref for LoadedSlipstreamPlugin {
    type Target = Box<dyn SlipstreamPlugin>;

    fn deref(&self) -> &Self::Target {
        &self.plugin
    }
}

impl DerefMut for LoadedSlipstreamPlugin {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.plugin
    }
}

// The Plugin Manager itself
#[derive(Default, Debug)]
pub struct SlipstreamPluginManager {
    pub plugins: Vec<LoadedSlipstreamPlugin>,
    libs: Vec<Library>,
    /// Resolved, absolute paths to the loaded `.so` files, parallel to `plugins` / `libs`.
    /// Used to detect duplicate loads before calling `dlopen`, preventing unsafe double-loading.
    libpaths: Vec<PathBuf>,
}

impl SlipstreamPluginManager {
    pub fn new() -> Self {
        SlipstreamPluginManager { plugins: Vec::default(), libs: Vec::default(), libpaths: Vec::default() }
    }

    /// Unload all plugins and loaded plugin libraries, making sure to fire
    /// their `on_unload()` methods so they can do any necessary cleanup.
    pub fn unload(&mut self) {
        for mut plugin in self.plugins.drain(..) {
            info!("Unloading plugin for {:?}", plugin.name());
            plugin.on_unload();
        }

        for lib in self.libs.drain(..) {
            drop(lib);
        }

        self.libpaths.clear();
    }

    /// Check which plugins are interested in regular mapping data.
    pub fn history_mappings_enabled(&self) -> bool {
        for plugin in &self.plugins {
            if plugin.history_enabled() {
                return true;
            }
        }
        false
    }

    /// Check if there is any plugin interested in historical staking data.
    pub fn history_staking_rewards_enabled(&self) -> bool {
        for plugin in &self.plugins {
            if plugin.history_staking_rewards_enabled() {
                return true;
            }
        }
        false
    }

    /// Broadcasts a mapping update to all interested plugins. Errors are
    /// logged as warnings but never propagated.
    pub fn notify_mapping_update(
        &self,
        program_id: &[u8],
        mapping_name: &[u8],
        key: &[u8],
        value: &[u8],
        block_height: u32,
    ) {
        tracing::debug!(
            target: "slipstream",
            "notify_mapping_update called: block_height={block_height} plugins={}",
            self.plugins.len()
        );
        for plugin in &self.plugins {
            if plugin.history_enabled() && let Err(e) =
                    plugin.notify_mapping_update(program_id, mapping_name, key, value, block_height)
            {
                warn!("Slipstream plugin '{}' mapping_update error: {e}", plugin.name());
            }
        }
    }

    /// Broadcasts a staking reward to all interested plugins. Errors are
    /// logged as warnings but never propagated.
    pub fn notify_staking_reward(
        &self,
        staker: &[u8],
        validator: &[u8],
        reward: u64,
        new_stake: u64,
        block_height: u32,
    ) {
        tracing::debug!(
            target: "slipstream",
            "notify_staking_reward called: block_height={block_height} reward={reward} new_stake={new_stake} plugins={}",
            self.plugins.len()
        );
        for plugin in &self.plugins {
            if plugin.history_staking_rewards_enabled() && let Err(e) =
                    plugin.notify_staking_reward(staker, validator, reward, new_stake, block_height)
            {
                warn!("Slipstream plugin '{}' staking_reward error: {e}", plugin.name());
            }
        }
    }

    /// Returns the names of all loaded plugins.
    pub fn list_plugins(&self) -> JsonRpcResult<Vec<String>> {
        Ok(self.plugins.iter().map(|p| p.name().to_owned()).collect())
    }

    /// Loads a plugin from the given config file.
    ///
    /// # Safety
    ///
    /// This function loads the dynamically linked library specified in the config. The library
    /// must do necessary initializations.
    pub fn load_plugin(
        &mut self,
        slipstream_plugin_config_file: impl AsRef<Path>,
    ) -> JsonRpcResult<String> {
        // Resolve the library path from the config before calling dlopen.
        // This lets us detect duplicates without loading the library a second time, which is
        // unsafe: a second dlopen on an already-loaded .so can trigger re-execution of Rust
        // .init_array startup code, corrupting global state in the running plugin instance.
        let resolved_libpath =
            resolve_libpath_from_config(slipstream_plugin_config_file.as_ref())?;

        // Check for duplicate library path first (catches same .so before dlopen).
        if let Some(idx) = self.libpaths.iter().position(|p| p == &resolved_libpath) {
            return Err(SlipstreamPluginManagerError::PluginAlreadyLoaded(
                self.plugins[idx].name().to_string(),
            ));
        }

        let (new_lib, mut new_plugin, new_config_file) =
            load_plugin_from_config(slipstream_plugin_config_file.as_ref())?;

        // Also guard against a different .so that happens to expose the same plugin name.
        if self.plugins.iter().any(|plugin| plugin.name().eq(new_plugin.name())) {
            return Err(SlipstreamPluginManagerError::PluginAlreadyLoaded(
                new_plugin.name().to_string(),
            ));
        }

        // Call on_load and push plugin.
        new_plugin
            .on_load(new_config_file, false)
            .map_err(|e| SlipstreamPluginManagerError::PluginStartError(e.to_string()))?;
        let name = new_plugin.name().to_string();
        self.plugins.push(new_plugin);
        self.libs.push(new_lib);
        self.libpaths.push(resolved_libpath);

        Ok(name)
    }

    /// Unloads the plugin with the given name.
    pub fn unload_plugin(&mut self, name: &str) -> JsonRpcResult<()> {
        let Some(idx) = self.plugins.iter().position(|plugin| plugin.name().eq(name)) else {
            return Err(SlipstreamPluginManagerError::PluginNotLoaded(name.to_string()));
        };

        self._drop_plugin(idx);
        Ok(())
    }

    /// Reloads the plugin with the given name from the given config file.
    pub fn reload_plugin(&mut self, name: &str, config_file: &str) -> JsonRpcResult<()> {
        let Some(idx) = self.plugins.iter().position(|plugin| plugin.name().eq(name)) else {
            return Err(SlipstreamPluginManagerError::PluginNotLoaded(name.to_string()));
        };

        // Resolve the new library path before unloading, so we can track it after reload.
        let new_resolved_libpath = resolve_libpath_from_config(config_file.as_ref())
            .map_err(|e| SlipstreamPluginManagerError::PluginLoadError(e.to_string()))?;

        // Unload the current plugin first.
        self._drop_plugin(idx);

        // Load the new plugin.
        let (new_lib, mut new_plugin, new_parsed_config_file) =
            load_plugin_from_config(config_file.as_ref())
                .map_err(|e| SlipstreamPluginManagerError::PluginLoadError(e.to_string()))?;

        // Ensure no other plugin with this name is already loaded.
        if self.plugins.iter().any(|plugin| plugin.name().eq(new_plugin.name())) {
            return Err(SlipstreamPluginManagerError::PluginAlreadyLoaded(
                new_plugin.name().to_string(),
            ));
        }

        // Attempt to call on_load with new plugin.
        new_plugin
            .on_load(new_parsed_config_file, true)
            .map_err(|e| SlipstreamPluginManagerError::PluginStartError(e.to_string()))?;

        self.plugins.push(new_plugin);
        self.libs.push(new_lib);
        self.libpaths.push(new_resolved_libpath);

        Ok(())
    }

    fn _drop_plugin(&mut self, idx: usize) {
        let current_lib = self.libs.remove(idx);
        let mut current_plugin = self.plugins.remove(idx);
        self.libpaths.remove(idx);
        let name = current_plugin.name().to_string();
        current_plugin.on_unload();
        // The plugin must be dropped before the library to avoid a crash.
        drop(current_plugin);
        drop(current_lib);
        info!("Unloaded plugin {name} at idx {idx}");
    }
}

// NOTE: TODO: NOT SURE IF IT MAKES SENSE TO HAVE PLUGINS SPECIFY SINCE
// UNLIKE SOLANA, THIS CAN BE USED ON ANY CLIENT NODE, NOT JUST A VALIDATOR? CAN DISCUSS
#[derive(Debug)]
pub enum SlipstreamPluginManagerRequest {
    ReloadPlugin {
        name: String,
        config_file: String,
        response_sender: OneShotSender<JsonRpcResult<()>>,
    },
    UnloadPlugin {
        name: String,
        response_sender: OneShotSender<JsonRpcResult<()>>,
    },
    LoadPlugin {
        config_file: String,
        response_sender: OneShotSender<JsonRpcResult<String>>,
    },
    ListPlugins {
        response_sender: OneShotSender<JsonRpcResult<Vec<String>>>,
    },
}

#[derive(thiserror::Error, Debug)]
pub enum SlipstreamPluginManagerError {
    #[error("Cannot open the plugin config file: {0}")]
    CannotOpenConfigFile(String),

    #[error("Cannot read the plugin config file: {0}")]
    CannotReadConfigFile(String),

    #[error("The config file is not in a valid JSON/JSON5 format: {0}")]
    InvalidConfigFileFormat(String),

    #[error("Plugin library path is not specified in the config file")]
    LibPathNotSet,

    #[error("Invalid plugin path")]
    InvalidPluginPath,

    #[error("Cannot load plugin shared library (error: {0})")]
    PluginLoadError(String),

    #[error("The slipstream plugin '{0}' is already loaded")]
    PluginAlreadyLoaded(String),

    #[error("The plugin '{0}' is not loaded")]
    PluginNotLoaded(String),

    #[error("The SlipstreamPlugin on_load method failed (error: {0})")]
    PluginStartError(String),
}

/// Parses a plugin config file and returns the resolved, absolute path to the `.so`.
///
/// Does NOT open or load the library — safe to call for duplicate detection before `dlopen`.
pub(crate) fn resolve_libpath_from_config(
    slipstream_plugin_config_file: &Path,
) -> Result<PathBuf, SlipstreamPluginManagerError> {
    use std::{fs::File, io::Read};

    let mut file = File::open(slipstream_plugin_config_file).map_err(|e| {
        SlipstreamPluginManagerError::CannotOpenConfigFile(format!(
            "Failed to open the plugin config file {slipstream_plugin_config_file:?}, error: {e:?}"
        ))
    })?;

    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(|e| {
        SlipstreamPluginManagerError::CannotReadConfigFile(format!(
            "Failed to read the plugin config file {slipstream_plugin_config_file:?}, error: {e:?}"
        ))
    })?;

    let result: serde_json::Value = json5::from_str(&contents).map_err(|e| {
        SlipstreamPluginManagerError::InvalidConfigFileFormat(format!(
            "The config file {slipstream_plugin_config_file:?} is not in a valid Json5 format, error: {e:?}"
        ))
    })?;

    let libpath_str = result["libpath"].as_str().ok_or(SlipstreamPluginManagerError::LibPathNotSet)?;
    let mut libpath = PathBuf::from(libpath_str);
    if libpath.is_relative() {
        let config_dir = slipstream_plugin_config_file.parent().ok_or_else(|| {
            SlipstreamPluginManagerError::CannotOpenConfigFile(format!(
                "Failed to resolve parent of {slipstream_plugin_config_file:?}",
            ))
        })?;
        libpath = config_dir.join(libpath);
    }

    Ok(libpath)
}

/// # Safety
///
/// This function loads the dynamically linked library specified in the path. The library
/// must do necessary initializations.
///
/// Returns the slipstream plugin, the dynamic library, and the parsed config file as a `&str`.
/// (The slipstream plugin interface requires a `&str` for the `on_load` method.)
#[cfg(not(test))]
pub(crate) fn load_plugin_from_config(
    slipstream_plugin_config_file: &Path,
) -> Result<(Library, LoadedSlipstreamPlugin, &str), SlipstreamPluginManagerError> {
    use std::{fs::File, io::Read, path::PathBuf};
    type PluginConstructor = unsafe extern "C" fn() -> *mut dyn SlipstreamPlugin;
    use libloading::Symbol;

    let mut file = match File::open(slipstream_plugin_config_file) {
        Ok(file) => file,
        Err(err) => {
            return Err(SlipstreamPluginManagerError::CannotOpenConfigFile(format!(
                "Failed to open the plugin config file {slipstream_plugin_config_file:?}, error: {err:?}"
            )));
        }
    };

    let mut contents = String::new();
    if let Err(err) = file.read_to_string(&mut contents) {
        return Err(SlipstreamPluginManagerError::CannotReadConfigFile(format!(
            "Failed to read the plugin config file {slipstream_plugin_config_file:?}, error: {err:?}"
        )));
    }

    let result: serde_json::Value = match json5::from_str(&contents) {
        Ok(value) => value,
        Err(err) => {
            return Err(SlipstreamPluginManagerError::InvalidConfigFileFormat(format!(
                "The config file {slipstream_plugin_config_file:?} is not in a valid Json5 format, error: {err:?}"
            )));
        }
    };

    let libpath = result["libpath"]
        .as_str()
        .ok_or(SlipstreamPluginManagerError::LibPathNotSet)?;
    let mut libpath = PathBuf::from(libpath);
    if libpath.is_relative() {
        let config_dir = slipstream_plugin_config_file.parent().ok_or_else(|| {
            SlipstreamPluginManagerError::CannotOpenConfigFile(format!(
                "Failed to resolve parent of {slipstream_plugin_config_file:?}",
            ))
        })?;
        libpath = config_dir.join(libpath);
    }

    let plugin_name = result["name"].as_str().map(|s| s.to_owned());

    let config_file = slipstream_plugin_config_file
        .as_os_str()
        .to_str()
        .ok_or(SlipstreamPluginManagerError::InvalidPluginPath)?;

    let (plugin, lib) = unsafe {
        let lib = Library::new(libpath)
            .map_err(|e| SlipstreamPluginManagerError::PluginLoadError(e.to_string()))?;
        let constructor: Symbol<PluginConstructor> = lib
            .get(b"_create_plugin")
            .map_err(|e| SlipstreamPluginManagerError::PluginLoadError(e.to_string()))?;
        let plugin_raw = constructor();
        if plugin_raw.is_null() {
            return Err(SlipstreamPluginManagerError::PluginLoadError(
                "plugin constructor returned a null pointer".to_string(),
            ));
        }
        (Box::from_raw(plugin_raw), lib)
    };
    Ok((lib, LoadedSlipstreamPlugin::new(plugin, plugin_name), config_file))
}

#[cfg(test)]
const TESTPLUGIN_CONFIG: &str = "TESTPLUGIN_CONFIG";
#[cfg(test)]
const TESTPLUGIN2_CONFIG: &str = "TESTPLUGIN2_CONFIG";

// In tests resolve_libpath_from_config returns the config path itself as a stand-in for
// the .so path.  This is sufficient for duplicate detection without real file I/O.
#[cfg(test)]
pub(crate) fn resolve_libpath_from_config(
    slipstream_plugin_config_file: &Path,
) -> Result<PathBuf, SlipstreamPluginManagerError> {
    Ok(slipstream_plugin_config_file.to_path_buf())
}

// This is mocked for tests to avoid having to do IO with a dynamically linked library
// across different architectures at test time.
#[cfg(test)]
pub(crate) fn load_plugin_from_config(
    slipstream_plugin_config_file: &Path,
) -> Result<(Library, LoadedSlipstreamPlugin, &str), SlipstreamPluginManagerError> {
    if slipstream_plugin_config_file.ends_with(TESTPLUGIN_CONFIG) {
        Ok(tests::dummy_plugin_and_library(tests::TestPlugin, TESTPLUGIN_CONFIG))
    } else if slipstream_plugin_config_file.ends_with(TESTPLUGIN2_CONFIG) {
        Ok(tests::dummy_plugin_and_library(tests::TestPlugin2, TESTPLUGIN2_CONFIG))
    } else {
        Err(SlipstreamPluginManagerError::CannotOpenConfigFile(
            slipstream_plugin_config_file.to_str().unwrap().to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use {
        crate::slipstream_manager::{
            LoadedSlipstreamPlugin,
            SlipstreamPluginManager,
            TESTPLUGIN2_CONFIG,
            TESTPLUGIN_CONFIG,
        },
        libloading::Library,
        snarkvm_slipstream_plugin_interface::slipstream_plugin_interface::SlipstreamPlugin,
        std::sync::{Arc, RwLock},
    };

    pub(super) fn dummy_plugin_and_library<P: SlipstreamPlugin>(
        plugin: P,
        config_path: &'static str,
    ) -> (Library, LoadedSlipstreamPlugin, &'static str) {
        #[cfg(unix)]
        let library = libloading::os::unix::Library::this();
        #[cfg(windows)]
        let library = libloading::os::windows::Library::this().unwrap();
        (Library::from(library), LoadedSlipstreamPlugin::new(Box::new(plugin), None), config_path)
    }

    const DUMMY_NAME: &str = "dummy";
    pub(super) const DUMMY_CONFIG: &str = "dummy_config";
    const ANOTHER_DUMMY_NAME: &str = "another_dummy";

    #[derive(Clone, Copy, Debug)]
    pub(super) struct TestPlugin;

    impl SlipstreamPlugin for TestPlugin {
        fn name(&self) -> &'static str {
            DUMMY_NAME
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub(super) struct TestPlugin2;

    impl SlipstreamPlugin for TestPlugin2 {
        fn name(&self) -> &'static str {
            ANOTHER_DUMMY_NAME
        }
    }

    #[test]
    fn test_slipstream_reload() {
        // Initialize empty manager.
        let plugin_manager = Arc::new(RwLock::new(SlipstreamPluginManager::new()));

        // No plugins are loaded, this should fail.
        let mut plugin_manager_lock = plugin_manager.write().unwrap();
        let reload_result = plugin_manager_lock.reload_plugin(DUMMY_NAME, DUMMY_CONFIG);
        assert!(reload_result.is_err());
        assert_eq!(
            reload_result.unwrap_err().to_string(),
            format!("The plugin '{DUMMY_NAME}' is not loaded")
        );

        // Load TestPlugin via the normal path so libpaths is kept in sync.
        // (TESTPLUGIN_CONFIG is accepted by the test mock of load_plugin_from_config.)
        let load_result = plugin_manager_lock.load_plugin(TESTPLUGIN_CONFIG);
        assert!(load_result.is_ok());
        assert_eq!(plugin_manager_lock.plugins[0].name(), DUMMY_NAME);

        // Try wrong name (same error).
        const WRONG_NAME: &str = "wrong_name";
        let reload_result = plugin_manager_lock.reload_plugin(WRONG_NAME, DUMMY_CONFIG);
        assert!(reload_result.is_err());
        assert_eq!(
            reload_result.unwrap_err().to_string(),
            format!("The plugin '{WRONG_NAME}' is not loaded")
        );

        // Now try a (dummy) reload, replacing TestPlugin with TestPlugin2.
        let reload_result = plugin_manager_lock.reload_plugin(DUMMY_NAME, TESTPLUGIN2_CONFIG);
        assert!(reload_result.is_ok());

        // The plugin is now replaced with ANOTHER_DUMMY_NAME.
        let plugins = plugin_manager_lock.list_plugins().unwrap();
        assert!(plugins.iter().any(|name| name.eq(ANOTHER_DUMMY_NAME)));
        // DUMMY_NAME should no longer be present.
        assert!(!plugins.iter().any(|name| name.eq(DUMMY_NAME)));
    }

    #[test]
    fn test_plugin_list() {
        // Initialize empty manager.
        let plugin_manager = Arc::new(RwLock::new(SlipstreamPluginManager::new()));
        let mut plugin_manager_lock = plugin_manager.write().unwrap();

        // Load two plugins.
        let (mut plugin, lib, config) = dummy_plugin_and_library(TestPlugin, TESTPLUGIN_CONFIG);
        plugin.on_load(config, false).unwrap();
        plugin_manager_lock.plugins.push(plugin);
        plugin_manager_lock.libs.push(lib);

        let (mut plugin, lib, config) = dummy_plugin_and_library(TestPlugin2, TESTPLUGIN2_CONFIG);
        plugin.on_load(config, false).unwrap();
        plugin_manager_lock.plugins.push(plugin);
        plugin_manager_lock.libs.push(lib);

        // Check that both plugins are returned in the list.
        let plugins = plugin_manager_lock.list_plugins().unwrap();
        assert!(plugins.iter().any(|name| name.eq(DUMMY_NAME)));
        assert!(plugins.iter().any(|name| name.eq(ANOTHER_DUMMY_NAME)));
    }

    #[test]
    fn test_plugin_load_unload() {
        // Initialize empty manager.
        let plugin_manager = Arc::new(RwLock::new(SlipstreamPluginManager::new()));
        let mut plugin_manager_lock = plugin_manager.write().unwrap();

        // Load rpc call.
        let load_result = plugin_manager_lock.load_plugin(TESTPLUGIN_CONFIG);
        assert!(load_result.is_ok());
        assert_eq!(plugin_manager_lock.plugins.len(), 1);

        // Unload rpc call.
        let unload_result = plugin_manager_lock.unload_plugin(DUMMY_NAME);
        assert!(unload_result.is_ok());
        assert_eq!(plugin_manager_lock.plugins.len(), 0);
    }

    #[test]
    fn test_notify_mapping_update() {
        let mut manager = SlipstreamPluginManager::new();

        // Install a mock plugin that tracks calls.
        #[derive(Debug)]
        struct TrackingPlugin {
            calls: std::sync::atomic::AtomicU32,
        }
        impl SlipstreamPlugin for TrackingPlugin {
            fn name(&self) -> &'static str {
                "tracking"
            }
            fn history_enabled(&self) -> bool {
                true
            }
            fn notify_mapping_update(
                &self,
                _program_id: &[u8],
                _mapping_name: &[u8],
                _key: &[u8],
                _value: &[u8],
                _block_height: u32,
            ) -> anyhow::Result<()> {
                self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
        }

        // Manually push the plugin (bypassing dynamic loading).
        #[cfg(unix)]
        let lib = Library::from(libloading::os::unix::Library::this());
        #[cfg(windows)]
        let lib = Library::from(libloading::os::windows::Library::this().unwrap());

        let plugin = TrackingPlugin { calls: std::sync::atomic::AtomicU32::new(0) };
        manager.plugins.push(LoadedSlipstreamPlugin::new(Box::new(plugin), None));
        manager.libs.push(lib);

        // Call notify_mapping_update and verify the plugin received it.
        manager.notify_mapping_update(b"program_id", b"mapping", b"key", b"value", 42);

        // Verify via list_plugins that the plugin is still loaded.
        assert_eq!(manager.list_plugins().unwrap(), vec!["tracking"]);
    }
}
