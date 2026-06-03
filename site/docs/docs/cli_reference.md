---
title: CLI Reference
summary: The CLI commands reference for bulletty
show_datetime: false
---

Running **bulletty** without any command launches the TUI (Terminal User Interface). All other commands below are for managing feeds and configuration from the command line.

## ⚙️ Global Flags

These flags can be used with any command:

| Flag | Description |
|------|-------------|
| `--no-hooks` | Disable all hooks defined in the config |
| `--version` | Display the current version of bulletty |
| `--help` | Display help information |

## 📋 Commands

### 💠 `list`

Lists all your categories and the feeds in each one, along with their *slugs* (the slugified, space-free names used as directory names in the filesystem).

```
bulletty list
```

### 💠 `add <URL> [CATEGORY]`

Adds a new RSS/Atom feed. Provide the feed URL and, optionally, a category name. If no category is specified, the feed is added to **General**.

```
bulletty add https://example.com/feed.xml
bulletty add https://example.com/feed.xml "Tech News"
```

### 💠 `update`

Checks all registered feeds for new articles and downloads them.

```
bulletty update
```

### 💠 `delete <IDENTIFIER>`

Finds a feed matching the given name, URL, or slug and prompts you for confirmation before deleting it along with all of its articles. If multiple feeds match, you'll be asked to pick which one to delete.

```
bulletty delete "My Blog"
bulletty delete https://example.com/feed.xml
bulletty delete my-blog
```

### 💠 `dirs`

Displays the important directories used by **bulletty**, including the library and logs paths.

```
bulletty dirs
```

#### `dirs library [PATH]`

Without arguments, prints the current library path. When a path is provided, updates the library directory to the specified location. This is useful if you want to sync your library across machines (e.g. via a cloud-synced folder).

```
bulletty dirs library
bulletty dirs library ~/Dropbox/bulletty
```

#### `dirs logs`

Prints the path to the logs directory.

```
bulletty dirs logs
```

#### `dirs local-config`

Prints the path to the local config directory.

```
bulletty dirs local-config
```

#### `dirs themes`

Prints the path to the themes directory.

```
bulletty dirs themes
```

### 💠 `import <OPML_FILE>`

Imports feed sources from an OPML file. Most feed readers can export to this format, making it easy to migrate your subscriptions into **bulletty**.

```
bulletty import feeds.opml
```

### 💠 `export <OPML_FILE>`

Exports all your feed sources to an OPML file, so you can back them up or import them into another reader.

```
bulletty export my_feeds.opml
```

### 💠 `help`

Displays all available commands and their descriptions. You can also pass a command name to get detailed help for that specific command.

```
bulletty help
bulletty help add
```
