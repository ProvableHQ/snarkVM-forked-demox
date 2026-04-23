# Aleo Slipstream Plugin Interface

This crate enables a plugin to be added into a SnarkVM runtime to
take actions at the time of mapping updates at block finalization;
for example, saving historical mappings state and staking data to an external database. The plugin must
implement the `SlipstreamPlugin` trait. Please see the details of the
`slipstream_plugin_interface.rs` for the interface definition.

# Components

### `plugins/slipstream_plugin_interface`
Defines the `SlipstreamPlugin` trait — the interface all plugins must implement.

| Method | Description |
|---|---|
| `on_load` / `on_unload` | Lifecycle hooks |
| `subscribed_events` | Returns the event types a plugin subscribes to |
| `on_broadcast` | Called once per mapping (if the event kind is in the subscribed list), broadcasts the event to the plugin|

### `plugins/slipstream_plugin_manager`
Manages loaded plugins and their backing `libloading::Library` handles.

- **`LoadedSlipstreamPlugin`** — wrapper holding a boxed plugin + its name; implements `Deref`/`DerefMut`
- **`SlipstreamPluginManager`**
  - `from_config_files` - takes a vec of paths to plugin config files and loads them into the manager
  - `unload()` — fires `on_unload()` on each plugin then drops the libraries
  - `any_plugin_subscribes()` — aggregate opt-in checks
  - `broadcast()` — fan-out broadcast to all interested plugins
  - `list_plugins()` - return the names of all loaded plugins

---

## Plugin Config File (JSON5)

Each plugin requires a config file:
```json5
{
  "libpath": "/path/to/libmy_plugin.so",  // required; relative paths resolve from the config file's dir
  "name": "my_plugin"                      // optional; overrides the plugin's name() return value
}
```

---

## Plugin Library Convention

The shared library (`.so` / `.dylib` / `.dll`) must export a C function:
```rust
#[no_mangle]
pub extern "C" fn _create_plugin() -> *mut dyn SlipstreamPlugin {
    Box::into_raw(Box::new(MyPlugin::new()))
}
```

---

## Startup

`SlipstreamPluginManager::from_config_files()` takes a slice of config file paths and returns a manager object:
```rust
let manager = SlipstreamPluginManager::from_config_files(&[
    PathBuf::from("/etc/aleo/plugins/my_plugin.json5"),
])?;
```

> Errors from plugin callbacks (`on_broadcast`) are logged as warnings and never propagated — a misbehaving plugin will not crash the node.