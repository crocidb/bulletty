---
title: Configuration
summary:
    Reference for the local config file for the bulletty RSS reader
show_datetime: false
---

The local configuration file for **bulletty** is written in [TOML](https://toml.io) and stored outside the Library. It's location varies depending on the OS and can be found using the `dirs` command:

```sh 
bulletty dirs local-config
```

## 🔧 General Options
These settings reside at the top level.

### `datapath`
**Required**. Set the directory for storing the Library. Accepts the path as a string.

### `tui_auto_update`
Update the library on launch of the TUI. Accepts a boolean. Defaults to `true`.

### `parallel_feed_updates`
Fetch feed updates concurrently. Accepts a boolean. Defaults to `true`.

## 🪝 Hooks
Hooks are set in the `[hooks]` table and allow you to specify shell commands to run at key lifecycle points. See [Hooks](../hooks/) for specific details.

## 🥫 Example config.toml
```toml
datapath = "~/.local/share/bulletty" # Specify the directory for storing the Library
tui_auto_update = true               # Fetch new content on TUI launch
parallel_feed_updates = true         # Enable concurrent feed updates

[hooks]
before_tui = "some-command" # Specify a shell command to run before the TUI launches
after_tui = "some-command"  # Specify a shell command to run as the TUI terminates
open_link = "xdg-open %s"   # Define a command template for opening URLs
```
