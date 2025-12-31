---
sidebar_position: 1
---

# 🚪 Main

This page contains the base configuration options for Ashell.

It allows you to configure things like the log level, the monitor(s) used to
render the status bar, and the bar’s position.

All these configurations are defined in the root of the `toml` file.

## Log Level

The log level controls the verbosity of logs.

You can set it to a general level like `debug`, `info`, `warn`, or `error`,
or specify fine-grained control to enable logs from specific modules
in the codebase, e.g., `ashell::services::network=debug`.

See more about [log levels](https://docs.rs/env_logger/latest/env_logger/#enabling-logging).

:::warning

This configuration **requires** restarting Ashell to take effect.

:::

### Log Examples

Set the global log level to `debug` for all modules:

```toml
log_level = "debug"
```

Set the log level for the `ashell` module only:

```toml
log_level = "ashell=debug"
```

Set the log level to `warn` for all modules, `info` for Ashell modules,
and `debug` only for the network service:

```toml
log_level = "warn,ashell=info,ashell::services::network=debug"
```

To understand all possible module names you can use, check
the [source code](https://github.com/MalpenZibo/ashell).  
The `src` folder is the root of the `ashell` module, and every directory
or file under it declares a module or submodule.

For example, the file `src/modules/media_player.rs` maps to the module `ashell::modules::media_player`.

:::warning

Don’t confuse Ashell features (called “modules”) with Rust modules
(defined with `mod.rs` or in files).  
In this configuration, we're referring to Rust modules.

:::

## Outputs

You can configure which monitor(s) should display the status bar.

It can render on all monitors, only on the active one
(the focused monitor when Ashell starts), or on a list of specified monitors.

### Output Examples

Render the status bar on all monitors:

```toml
outputs = "All"
```

Render the status bar on the active monitor:

```toml
outputs = "Active"
```

Render the status bar on a specific list of monitors:

```toml
outputs = { Targets = ["DP-1", "eDP-1"] }
```

## Position

You can set the default position of the status bar to either `Top` or `Bottom`.
This will be used if no specific bars are configured using `[[bar]]`.

### Position Examples

Set the default bar position to the top:

```toml
position = "Top"
```

Set the default bar position to the bottom:

```toml
position = "Bottom"
```

## Multiple Bars

Ashell supports defining multiple bars, each with its own position, appearance, and modules.
This is done using the `[[bar]]` array of tables in your configuration.

If you define any `[[bar]]` sections, the global `position` setting is ignored for those bars (each bar must specify its position),
but global `modules` and `appearance` settings are used as defaults if not overridden within the bar configuration.

:::info
The `outputs` configuration is global. All configured bars will be rendered on the monitors specified by `outputs`.
:::

### Multi-Bar Example

Create a top bar with "Islands" style and specific modules, and a bottom bar with "Solid" style using default modules:

```toml
# Top Bar
[[bar]]
position = "Top"
[bar.appearance]
style = "Islands"
[bar.modules]
left = ["Workspaces"]
center = ["WindowTitle"]
right = ["Clock", "Settings"]

# Bottom Bar
[[bar]]
position = "Bottom"
[bar.appearance]
style = "Solid"
# Uses global modules configuration if [bar.modules] is omitted
```

## Close menu with esc

You can enable the use of the `Esc` key to close the menu.

:::warning

With these features enabled ashell will use the keyboard
in an exclusive way when a menu is open.

That means other applications will not be able to use
the keyboard when the menu is open.

:::

```toml
enable_esc_key = true
```
