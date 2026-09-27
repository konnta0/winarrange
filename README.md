# winarrange

`winarrange` is an on-demand window arrangement CLI for macOS and Windows. It
uses the operating system's existing Spaces / Virtual Desktops and only changes
windows when you explicitly run an arrange command.

> Nothing happens until the user runs `winarrange`.

There is no daemon, event watcher, global hotkey service, automatic reflow, or
virtual desktop manager. The primary use case is keeping several Unity Editor
instances in a predictable grid without adopting a resident tiling window
manager.

## Status

Version 0.2 targets:

- macOS on Apple Silicon
- Windows x64

Linux is not supported. Build from source with Rust 1.85 or newer.

## Install

```console
cargo install --path .
```

On macOS, grant Accessibility access to the installed `winarrange` executable,
or to the terminal/hotkey application that launches it, in **System Settings →
Privacy & Security → Accessibility**. Missing access is reported as an error.

## Daily usage

Arrange every eligible window on the currently visible Space / Virtual Desktop:

```console
winarrange
```

Arrange only windows belonging to the focused application:

```console
winarrange --focused-process
```

Limit arrangement to an explicit application/process. Matching is
case-insensitive and supports partial names:

```console
winarrange --process Unity
```

Override the grid geometry:

```console
winarrange --process Unity --columns 3 --gap 8 --margin 8
```

Every arrange mode supports a non-mutating preview:

```console
winarrange --focused-process --dry-run
winarrange profile unity --dry-run
```

## Floating windows

Temporarily exclude the focused window from subsequent arrangement:

```console
winarrange float
winarrange unfloat
winarrange toggle-float
```

These commands only update state. They never move the focused window or reflow
other windows. Run an arrange command separately when you want a new layout.
Temporary floating identity includes both the process ID and native window ID,
so it is not inherited when an application is restarted.

## Profiles and configuration

Configuration is stored at:

- macOS: `~/Library/Application Support/winarrange/config.toml`
- Windows: `%APPDATA%\winarrange\config.toml`

Example:

```toml
gap = 8
margin = 4
# default_profile = "unity"

[profile.unity]
process = "Unity"
layout = "grid"
sort = "title"
columns = 2
gap = 12

[profile.browser]
process = "Google Chrome"
layout = "grid"
sort = "title"

[[ignore]]
type = "process"
value = "Calculator"
```

Run a profile with an unambiguous subcommand:

```console
winarrange profile unity
winarrange profile unity --columns 3 --gap 16
```

Effective values are selected in this order:

```text
CLI option > profile > global config > built-in default
```

When `default_profile` is configured, bare `winarrange` uses that profile.
Without it, bare `winarrange` keeps the v0.1 behavior and arranges all eligible
windows.

Existing v0.1 `config.json` files are read when `config.toml` does not exist.
The next ignore-rule change writes the configuration in TOML format.

## Ignore rules

```console
winarrange ignore add --process Calculator
winarrange ignore remove --process Calculator
winarrange ignore list
```

## Diagnostics

List windows detected on the current Space / Virtual Desktop:

```console
winarrange list
winarrange list --verbose
```

Show the configuration path, profiles, floating windows, and ignore rules:

```console
winarrange status
```

Diagnostic commands never move or resize windows.

## Behavior

- Only visible, non-minimized, ordinary top-level windows on the current
  Space / Virtual Desktop are candidates.
- Each monitor is arranged independently inside its work area, preserving the
  menu bar, Dock, or taskbar.
- Windows are never moved to a different monitor.
- Titles provide stable ordering; process ID and native window ID break ties.
- A compact grid is selected automatically unless `columns` is specified.
- `gap` applies between windows and `margin` applies around each monitor's work
  area.
- Applications that enforce a large minimum size are clamped back inside their
  monitor after resizing.
- Floating and ignored windows are left completely untouched.

Temporary state lives alongside the config in `state.json`.

## Hotkeys

Global hotkeys are intentionally outside this project. A typical setup binds:

```text
Hotkey A → winarrange --focused-process
Hotkey B → winarrange toggle-float
```

## Development

```console
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

Platform APIs are isolated under `src/platform/`; filtering, ordering,
configuration precedence, state, and layout logic do not depend on Win32,
Accessibility, or CoreGraphics.
