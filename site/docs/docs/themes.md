---
title: Themes Reference
summary:
  Guide to creating and using TOML-based Base16 color themes for the bulletty
  RSS reader
show_datetime: false
---

**bulletty** RSS reader supports customizable color themes using the `base16`
color scheme format. This document explains how themes work and how to create
your own.

## 📝 Theme File Format

Theme files are written in [TOML](https://toml.io/) format and stored in
the Library under `themes/`. Each theme contains:

```toml
scheme = "Theme Name"
author = "Your Name"

# Background colors (00-06)
base00 = "RRGGBB"  # Main background (status bar, tags)
base01 = "RRGGBB"  # Sidebar background
base02 = "RRGGBB"  # Feed list background

# Foreground colors (03-06)
base03 = "RRGGBB"  # Hints, secondary text (dates, authors)
base04 = "RRGGBB"  # Descriptions, tertiary text
base05 = "RRGGBB"  # Main text (content, paragraphs)
base06 = "RRGGBB"  # Read posts, status bar text

# Accent colors (07-0F)
base07 = "RRGGBB"  # Reserved for future use
base08 = "RRGGBB"  # Selected items, headings
base09 = "RRGGBB"  # Unread posts
base0A = "RRGGBB"  # Reserved for future use
base0B = "RRGGBB"  # Reserved for future use
base0C = "RRGGBB"  # Inline code text
base0D = "RRGGBB"  # URLs and links
base0E = "RRGGBB"  # Reserved for future use
base0F = "RRGGBB"  # Reserved for future use
```

## 🎯 Color Reference

This reference is based on the GitHub Dark Dimmed theme:

| Variable | Hex Color | Usage                                         |
| -------- | --------- | --------------------------------------------- |
| `base00` | `#1e232a` | Status bar background, tags background        |
| `base01` | `#1e232a` | Sidebar background                            |
| `base02` | `#212830` | Feed list background                          |
| `base03` | `#545d68` | Hints, post date & author (secondary text)    |
| `base04` | `#768390` | Post descriptions, tertiary text              |
| `base05` | `#adbac7` | Active sidebar item, post content, paragraphs |
| `base06` | `#adbac7` | Read posts, status bar sections               |
| `base07` | `#00ff00` | Reserved                                      |
| `base08` | `#539bf5` | Selected post/item, headings                  |
| `base09` | `#f69d50` | Unread posts                                  |
| `base0A` | `#00ff00` | Reserved                                      |
| `base0B` | `#00ff00` | Reserved                                      |
| `base0C` | `#8ddb8c` | Inline code text                              |
| `base0D` | `#bc7cff` | URLs and links                                |
| `base0E` | `#00ff00` | Reserved                                      |
| `base0F` | `#00ff00` | Reserved                                      |

## 👀 Preview

**Main Screen**

![Main Screen](https://github.com/user-attachments/assets/9e583491-9b6f-4000-8f05-3982e763f963)

**Reader Screen**

![Reader Screen](https://github.com/user-attachments/assets/12b2a895-6e13-46de-9a84-468a5b1e86f8)

## 🔗 Base16 Compatibility

**bulletty** RSS Reader uses the
[base16](https://github.com/chriskempson/base16) color scheme standard. This
means you can adapt colors from any `base16`-compatible editor theme (Neovim, VS
Code, Vim, Alacritty, etc.) to **bulletty**.

To adapt an existing `base16` theme:

> Most `base16` theme files are written in **YAML**. **bulletty** uses **TOML**.
> YAML uses `:` to associate keys and values, while TOML uses `=`. That's a
> conversion you'll need to perform when adapting a `base16` theme.

1. Copy the hex colors from the theme.
2. Map them to the `bulletty` variables using the table above.
3. Adjust unused slots (base07, 0A, 0B, 0E, 0F) as needed.
