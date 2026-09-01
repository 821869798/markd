# mkd

[![Crates.io](https://img.shields.io/badge/crates.io-mkd-orange)](https://crates.io/crates/mkd)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](./LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-black)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey)](#)

**[English](#english) | [中文文档](./README.zh-CN.md)**

---

## English

`mkd` is a cross-platform directory bookmark tool. A Rust TUI manages, searches, and selects bookmarks, while a registered shell function turns the selection into a real `cd` in your current shell — registered like `zoxide`, but with manually curated bookmarks and category management.

### Install

**Option 1: Download a prebuilt binary (recommended)**

Grab the archive for your platform from the [Releases page](https://github.com/OWNER/mkd/releases), extract it, and put `mkd` (or `mkd.exe`) somewhere on your `PATH`:

| Platform | Archive |
| --- | --- |
| Windows x64 | `mkd-x86_64-pc-windows-msvc.zip` |
| Windows ARM64 | `mkd-aarch64-pc-windows-msvc.zip` |
| macOS ARM64 (Apple Silicon) | `mkd-aarch64-apple-darwin.tar.gz` |
| Linux x64 | `mkd-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `mkd-aarch64-unknown-linux-gnu.tar.gz` |

Example (Linux/macOS):

```bash
VERSION=v0.1.0
curl -LO https://github.com/OWNER/mkd/releases/download/$VERSION/mkd-$(uname -m)-unknown-linux-gnu.tar.gz
# macOS: mkd-aarch64-apple-darwin.tar.gz
tar -xzf mkd-*.tar.gz
sudo mv mkd-*/mkd /usr/local/bin/
mkd --version
```

**Option 2: Install from source with Cargo**

With the Rust toolchain installed, from the source directory:

```console
cargo install --path .
```

Make sure Cargo's bin directory (usually `~/.cargo/bin`, on Windows `%USERPROFILE%\.cargo\bin`) is on your `PATH`.

### Quick start

1. **Register the shell function** (one-time, per shell):

```console
mkd setup          # auto-detects your shell and asks for confirmation
```

Then **open a new terminal window** (or reload your profile) and confirm the function is loaded — `Get-Command mkd` on PowerShell should report `Function`, not `Application`.

2. **Add your first bookmarks**:

```console
mkd add .                          # bookmark the current directory
mkd add ~/projects/api --name api --category work
```

3. **Jump**:

```console
mkd          # opens the TUI
```

Use `j/k` or arrows to move, `/` to fuzzy-search, `Enter` (or double-click) to jump. The most-used directories rank higher automatically.

4. **Organize with groups** — in the TUI, press `c` to create a group, select a bookmark and press `a` to pick a group from a popup list (`↑↓` + Enter). Or from the command line:

```console
mkd category add work
mkd list --category work
```

5. **Uninstall registration** when no longer needed:

```console
mkd setup --remove
```

### Shell registration

Register the shell function after installing:

```console
mkd setup
```

Omitting the shell lets mkd auto-detect. You can also be explicit, with flags to preview, write without confirmation, or remove:

```console
mkd setup bash --dry-run
mkd setup powershell --yes
mkd setup --remove zsh
```

`--dry-run` only prints the plan; repeated runs update the same managed block instead of duplicating; `--remove` deletes only mkd's own block. A backup `<profile>.mkd-backup` is created before modifying an existing profile, and malformed mkd markers abort without touching the file.

| Shell | Auto-registration target | Reload in current session |
| --- | --- | --- |
| Bash | `~/.bashrc` | `source ~/.bashrc` |
| Zsh | `~/.zshrc` | `source ~/.zshrc` |
| Fish | `~/.config/fish/config.fish` | `source ~/.config/fish/config.fish` |
| PowerShell | queries `$PROFILE.CurrentUserCurrentHost` at runtime | `. $PROFILE.CurrentUserCurrentHost` |

Manual fallback:

```bash
eval "$(mkd init bash)"        # Bash
eval "$(mkd init zsh)"         # Zsh
mkd init fish | source          # Fish
mkd init powershell | Out-String | Invoke-Expression   # PowerShell
```

### Daily usage

```console
mkd add . --name markd --category work
mkd          # open the TUI
```

| Action | Key / Mouse |
| --- | --- |
| Move | `j` / `k` or arrows |
| Switch panes | `Tab`, or click a pane |
| Fuzzy search | `/`, or click the search bar |
| Jump to directory | `Enter`, or double-click |
| Assign a group | `a` — arrow keys in the picker popup, Enter to confirm |
| New group | `c` (type the name) |
| Rename / delete group | `r` / `D` (press `D` twice) |
| Rename / delete bookmark | `e` / `d` (press `d` twice) |
| Copy path to clipboard | `y` |
| Help overlay | `h` |
| Cancel / quit | `Esc` |

Press `h` any time for the full shortcut list. Invalid directories are marked in the list and cannot be jumped to, but can still be deleted.

CLI equivalents:

```console
mkd category add work
mkd add . --name markd --category work
mkd list --category work
mkd remove markd
```

### Data file

Bookmarks live in a JSON file under the platform's `data_local_dir` (e.g. `%LOCALAPPDATA%\mkd\data\bookmarks.json` on Windows, `~/.local/share/mkd/bookmarks.json` on Linux). Override with `MKD_DATA_FILE`. Writes are atomic (temp file + rename); a corrupt file is reported, never silently overwritten.

### Limitations (v1)

- No cloud sync, sharing, cross-device merge, or plugins
- No cross-process file locking; avoid editing the same data file from concurrent sessions
- The TUI requires an interactive terminal with raw mode and mouse support
- Paths must be existing directories; CR/LF in paths is rejected so shell output stays a single safe line

## License

[MIT](./LICENSE)
