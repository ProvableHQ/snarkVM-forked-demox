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
| `notify_mapping_update` | Called when a mapping key-value is inserted/updated during canonical finalize; args are serialized to bytes for object-safety |
| `notify_staking_reward` | Called once per staker per block during staking reward distribution |
| `history_enabled` / `history_staking_rewards_enabled` | Flags plugins use to opt in to data streams |

### `plugins/slipstream_plugin_manager`
Manages loaded plugins and their backing `libloading::Library` handles.

- **`LoadedSlipstreamPlugin`** — wrapper holding a boxed plugin + its name; implements `Deref`/`DerefMut`
- **`SlipstreamPluginManager`**
  - `unload()` — fires `on_unload()` on each plugin then drops the libraries
  - `history_mappings_enabled()` / `history_staking_rewards_enabled()` — aggregate opt-in checks
  - `notify_mapping_update()` — fan-out broadcast to all interested plugins
- **`SlipstreamService`** — async service wrapping the manager (separate file)

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

`SlipstreamPluginService::new()` takes a slice of config file paths:
```rust
let service = SlipstreamPluginService::new(&[
    PathBuf::from("/etc/aleo/plugins/my_plugin.json5"),
])?;
```

> Errors from plugin callbacks (`notify_mapping_update`, `notify_staking_reward`) are logged as warnings and never propagated — a misbehaving plugin will not crash the node.