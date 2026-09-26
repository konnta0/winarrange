# winarrange

`winarrange` is an on-demand window arrangement CLI for macOS and Windows. It
uses the operating system's existing Spaces / Virtual Desktops and only changes
windows when you explicitly run a command.

> Nothing happens until the user runs `winarrange`.

There is no daemon, event watcher, global hotkey service, automatic reflow, or
virtual desktop manager. The primary use case is keeping several Unity Editor
instances in a predictable grid without adopting a resident tiling window
manager.

## Status

Version 0.1 targets:

- macOS on Apple Silicon
- Windows x64

Linux is not supported. Build from source with Rust 1.82 or newer.

## Install

```console
cargo install --path .
```

On macOS, grant Accessibility access to the installed `winarrange` executable,
or to the terminal/hotkey application that launches it, in **System Settings →
Privacy & Security → Accessibility**. Missing access is reported as an error.

## Usage

Arrange all eligible windows on the currently visible Space / Virtual Desktop:

```console
winarrange
```

Limit arrangement to an application/process (matching is case-insensitive and
supports partial names):

```console
winarrange --process Unity
```

Preview the exact operation without changing windows:

```console
winarrange --process Unity --dry-run
```

Temporarily exclude the focused window from subsequent arrangement:

```console
winarrange float
winarrange unfloat
winarrange toggle-float
```

These commands only update state. They never move the focused window or reflow
other windows. Run `winarrange` separately when you want a new arrangement.
Temporary floating identity includes both the process ID and native window ID,
so it is not inherited when an application is restarted.

Manage persistent process ignore rules:

```console
winarrange ignore add --process Calculator
winarrange ignore remove --process Calculator
winarrange ignore list
```

## Behavior

- Only visible, non-minimized, ordinary top-level windows on the current
  Space / Virtual Desktop are candidates.
- Each monitor is arranged independently inside its work area, preserving the
  menu bar, Dock, or taskbar.
- Windows are never moved to a different monitor.
- Titles provide stable ordering; process ID and native window ID break ties.
- A compact grid is selected automatically. A short final row expands to use
  the full width (for example, three windows use `2 + 1`).
- Windows marked floating and applications matching an ignore rule are left
  completely untouched.

Configuration and temporary state live in the platform-appropriate user data
directory:

- macOS: `~/Library/Application Support/winarrange/`
- Windows: `%APPDATA%\winarrange\`

## Hotkeys

Global hotkeys are intentionally outside this project. Bind `winarrange` and
`winarrange toggle-float` using the launcher or keyboard automation tool of your
choice.

## Development

```console
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

Platform APIs are isolated under `src/platform/`; filtering, ordering, state,
and layout logic do not depend on Win32, Accessibility, or CoreGraphics.

