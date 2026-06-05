<p align="center">
  <img src="img/bulletty.png" alt="bulletty" />
</p>
<h1 align="center">bulletty</h1>
<p align="center">The TUI RSS/Atom feed reader that lets you decide where to store your data.</p>

<p align="center">
  <img src="img/screenshot.gif" alt="bulletty" />
</p>

**bulletty** is a TUI feed reader and aggregator (RSS and Atom). Read your subscriptions directly in your terminal. It downloads the entries for offline reading so all the data is local and yours: your subscriptions, highlights, comments, etc. All in a universal format: Markdown. Back up and sync your `data` directory your own way. 

It's in active development.

## 🔨 Features

 - Subscribe to **RSS** and **Atom** feed types
 - All your feed sources and entries are stored in Markdown in one place: `$HOME/.local/share/bulletty/`
 - Automatically download new entries
 - Add articles to the **Read Later** category
 - Read articles with the embedded cozy Markdown reader
 - Import/export **OPML** feed list
 - Supports multiple beautiful colorful **themes**
 - Hooks: you can set hooks to sync your feed library before bulletty opens and after it closes; or tell it how to open links 

## 🚀 Install

<a href="https://repology.org/project/bulletty/versions">
    <img src="https://repology.org/badge/vertical-allrepos/bulletty.svg" alt="Packaging status" align="right">
</a>

[Download bulletty pre-built binaries](https://github.com/CrociDB/bulletty/releases)

### 🐧 Arch Linux (and derivatives like Manjaro, Parabola)

```shell
pacman -S bulletty
```

### 🍺 Homebrew (macOS / Linux)

```shell
brew install bulletty
```

### ❄️ Nix

```shell
nix profile install nixpkgs#bulletty
```

Or using the traditional command:

```shell
nix-env -iA nixpkgs.bulletty
```

### 🚩 NetBSD

```shell
pkgin install bulletty
```

### 📦 Snap

[![bulletty](https://snapcraft.io/bulletty/badge.svg)](https://snapcraft.io/bulletty)

```shell
sudo snap install bulletty
```

### 🚚 Through Cargo

It requires **cargo 1.90+**:

```shell
cargo install bulletty
```

To install the latest version in the repository:

```shell
cargo install --git https://github.com/CrociDB/bulletty.git
```

### ☂️ Pre-requisites

**bulletty** runs in most platforms, however there are some pre-requisites to have it run the best way possible:

- Use a modern terminal emulator such as **Kitty**, **Ghostty**, **Alacritty**, **WezTerm**, **Windows Terminal**, etc. They provide modern features and true color support, and are usually very fast and hardware-rendered
- Use a [NerdFont](http://nerdfonts.com/). They are patched versions of common coding fonts with several icons

## 🚄 Usage

### 🗞️ Adding new feed sources

For now, you can only add new feed sources via the CLI:

```shell
bulletty add https://crocidb.com/index.xml [Category]
```

If no category is passed, the feed source will be added to the `General` category. **bulletty** will synchronize all your sources when you open the TUI, by just invoking `bulletty`.

More on the CLI commands with:

```shell
bulletty help
```

### 🧩 TUI

On any screen, you can press question mark `?` and it will show you the available commands for that screen. Also, on the bottom right, it shows the most important commands for that context.

In general, it supports `j/k/up/down` to select items, navigate and scroll, as well as `g/G/Home/End` to go to the beginning/end of a list or file and `Enter` and `q/Esc` to navigate into and out of Categories and Entries. In order to open an Entry externally, press `o`.

## 💌 Don't know what to subscribe to?

[HN Personal Websites](https://hnpwd.github.io/) is a good repository of blogs that constantly show up on [Hacker News](https://news.ycombinator.com/). Subscribing to all of them is simple:

```shell
wget https://hnpwd.github.io/hnpwd.opml
bulletty import hnpwd.opml
```

## 🏫 Philosophy

The whole idea is to help bring back the decentralized internet. You subscribe to the sources you like the most and you get their content whenever it's available. When you get it, it's local, it's yours. **bulletty** will generate a Markdown file of each entry from each source. You can read through the embedded reader, directly in your terminal, or using any text editor.

All your feed data will be at `$HOME/.local/share/bulletty/`, in this structure:

```shell
[~/.local/share/bulletty]$ tree
.
└── categories
    ├── Programming
    │   ├── bruno-croci
    │   │   ├── .feed.toml
    │   │   ├── about.md
    │   │   ├── demystifying-the-shebang-kernel-adventures.md
    │   │   ├── from-ides-to-the-terminal.md
    │   │   ├── i-wrote-a-webserver-in-haskell.md
    │   │   ├── ...
    ├── General
    │   ├── another-website
    │   │   ├── .feed.toml
    │   │   ├── some-post.md
    │   │   ├── ...

```

All that needs to be done is to synchronize the `bulletty` directory to save your data, similar to an Obsidian vault.

## ✂️ Third Party Tools

One of the nice things about this open data structure is that people can build external tools that interact with the bulletty feed library.

 - [**convert_bulletty_to_pdf.py**](https://gist.github.com/thefranke/e7b80eca835275f355fd2f0dbe080e7b): a python script that exports bulletty's _Read Later_ articles into PDF, by @thefranke


## 📜 Feature Roadmap

 - Highlight
 - Notes
 - Web view
 - Mouse support
 - Image support
 - PDF/Epub article export


## 💻 Build

```shell
git clone https://github.com/CrociDB/bulletty.git
cd bulletty
cargo build --release
```

Build with `nix` is also possible with:

```shell
nix run .#release
```

### Notes on building on Windows

bulletty requires the `openssl` crate to build, but it's known to be a little complicated to build on Windows. If it fails on a regular build, it's probably because whatever `perl` version it's trying to use is not suitable to build it. In that case, try installing [Strawberry Perl](https://strawberryperl.com/) and make sure that `openssl` uses the one you just install to build:

```powershell
$env:OPENSSL_SRC_PERL = "C:\Strawberry\perl\bin\perl.exe"
```

Then `cargo build` should work normally.

## 👩‍💻 Contributing to bulletty

I am very open to contributions to help make **bulletty** the best feed reader out there. For more information on how to contribute, refer to the **CONTRIBUTING.md**.

## 📃 License

Copyright (c) Bruno Croci

This project is licensed under the MIT license ([LICENSE] or <http://opensource.org/licenses/MIT>)

[LICENSE]: ./LICENSE
