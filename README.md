# winarrange

`winarrange` is an on-demand window arranger for macOS and Windows. It changes
windows only when you explicitly invoke a CLI command or configured global
hotkey. Its optional daemon connects hotkeys to actions; it does not watch or
automatically manage windows.

It is not a tiling window manager: neither the CLI nor optional hotkey daemon
watches window events, replaces Spaces / Virtual Desktops, or resizes anything
without an explicit action.

> Nothing happens until the user explicitly invokes a winarrange action.

The primary use case is keeping several Unity Editor instances in a predictable
grid without adopting a resident tiling window manager.

## Quick start

```console
# Arrange eligible windows on the current Space / Virtual Desktop
winarrange

# Arrange only windows belonging to the currently focused application
winarrange --focused-process

# Focus one window for an exact process instance
winarrange focus --pid 12345

# Move through windows of the currently focused application
winarrange focus --next

# Temporarily exclude the focused window from future arrangements
winarrange toggle-float

# Start the optional global-hotkey daemon
winarrange daemon start
```

Preview any arrangement without moving windows:

```console
winarrange --focused-process --dry-run
```

## Supported platforms

- macOS on Apple Silicon (arm64)
- Windows x64

Linux and Intel Mac are not supported in v0.7.

## Install from GitHub Releases

Download the archive for your platform from the
[latest GitHub Release](https://github.com/konnta0/winarrange/releases/latest).
The archive contains only `winarrange` (macOS) or `winarrange.exe` (Windows).

Each release also provides `checksums.txt`. Verify the SHA-256 checksum before
running the downloaded executable:

```console
# macOS
shasum -a 256 winarrange-v0.7.0-macos-arm64.tar.gz

# Windows PowerShell
Get-FileHash .\winarrange-v0.7.0-windows-x64.zip -Algorithm SHA256
```

Compare the result with the corresponding entry in `checksums.txt`.

### macOS

Extract the archive and place the executable in a directory on `PATH`:

```console
tar -xzf winarrange-v0.7.0-macos-arm64.tar.gz
mkdir -p "$HOME/.local/bin"
install -m 755 winarrange "$HOME/.local/bin/winarrange"
```

Ensure `$HOME/.local/bin` is included in your `PATH`, then check the install:

```console
winarrange --version
winarrange --help
```

The v0.7 binary is not signed or notarized. On first launch, macOS may block it
because the developer cannot be verified. After verifying the checksum and
trying to run it once, follow Apple's
[Open Anyway instructions](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unidentified-developer-mh40616/mac)
in **System Settings → Privacy & Security**. Do not disable Gatekeeper.

winarrange also needs **Accessibility** permission to inspect and move windows.
Open **System Settings → Privacy & Security → Accessibility**, add and enable
the installed `winarrange` executable (or the terminal/hotkey application that
launches it), then rerun the command. Without permission, winarrange exits with
a clear error and does not move windows.

PID-based `focus` uses macOS System Events as the final application-activation
step. macOS may also ask the launching application for permission to automate
System Events the first time this command is used. If denied, winarrange reports
the activation error instead of claiming that the application became active.

### Windows

Extract `winarrange.exe` from the ZIP into a directory of your choice and add
that directory to your user `PATH`. Open a new PowerShell window and check the
install:

```console
winarrange --version
winarrange --help
```

The executable is built natively for Windows x64 with a statically linked C
runtime and does not require a Rust installation. The v0.7 binary is unsigned,
so Microsoft Defender SmartScreen may ask you to confirm the first run. Verify
the checksum before proceeding.

## Arrange windows

Arrange every eligible window on the current Space / Virtual Desktop:

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

## Focus and navigate windows

Use an exact PID when several application instances have the same process name,
bundle identifier, and window title:

```console
winarrange list --json
winarrange focus --pid 12345
```

`focus` selects one visible window belonging to that PID. If the PID has more
than one visible window, the first in stable title/PID/window-ID order is
selected. On macOS, winarrange
both raises the window and makes that exact process instance the active
application. On Windows, it makes the selected window the active foreground
window. Preview the selection without changing focus with:

```console
winarrange focus --pid 12345 --dry-run
```

Focus by a case-insensitive partial title match, optionally limited to a
process. If multiple windows match, winarrange reports that fact and chooses the
first window in the same stable order used for arrangement:

```console
winarrange focus --title Client-02
winarrange focus --process Unity --title Client-02
```

Cycle through windows belonging to the focused application. Navigation wraps
around and includes floating and ignored windows because those states affect
arrangement only:

```console
winarrange focus --next
winarrange focus --prev
winarrange focus --next --process Unity
```

All focus modes support `--dry-run`, which prints the selected PID and native
window ID without changing focus.

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

[[profile.unity.slot]]
title = "Server"
position = 0

[[profile.unity.slot]]
title = "Client-01"
position = 1

[profile.browser]
process = "Google Chrome"
layout = "grid"
sort = "title"

[[ignore]]
type = "process"
value = "Calculator"

[[hotkey]]
keys = "cmd+shift+a"
action = "arrange"
```

Run a profile with an unambiguous subcommand:

```console
winarrange profile unity
winarrange profile unity --columns 3 --gap 16
```

Inspect profiles and locate the configuration file from scripts:

```console
winarrange profile list
winarrange profile show unity
winarrange config path
```

Slot rules pin matching windows to zero-based grid cells. Title matching is
case-insensitive and partial; rules are evaluated in file order. Unmatched
windows fill the remaining cells in stable title/PID/window-ID order. A missing,
ignored, or floating window does not reserve its configured slot, so the
remaining layout is recalculated from the windows currently eligible for
arrangement. Rules are applied independently on each monitor.

Duplicate positions, negative positions, empty slot titles, and invalid grid
values are rejected when the configuration is loaded.

## Global hotkeys and daemon

Global hotkeys are opt-in. Add only the bindings you want to `config.toml`:

```toml
# macOS example
[[hotkey]]
keys = "cmd+shift+a"
action = "arrange"

[[hotkey]]
keys = "cmd+shift+space"
action = "toggle-float"

[[hotkey]]
keys = "cmd+shift+j"
action = "focus-next-all"

[[hotkey]]
keys = "cmd+shift+k"
action = "focus-prev-all"

[[hotkey]]
keys = "cmd+shift+enter"
action = "toggle-zoom"

[[hotkey]]
keys = "cmd+shift+u"
action = "profile"
profile = "unity"
```

Use `action = "arrange-focused"` instead when the shortcut should arrange
only windows belonging to the currently focused application.

`focus-next-all` and `focus-prev-all` navigate every detected window across
applications and include floating and ignored windows. `toggle-zoom` centers a
non-floating focused window at roughly half the monitor area; invoking it again
restores the original bounds.

On Windows use `win` in place of `cmd`, for example
`win+shift+a`. `cmd`, `win`, `ctrl`, `alt`/`option`, and `shift` are
case-insensitive. Hotkey actions are parsed as structured winarrange actions;
they cannot execute shell commands or arbitrary programs.

Run in the foreground while configuring or troubleshooting:

```console
winarrange daemon run
```

Manage the background process:

```console
winarrange daemon start
winarrange daemon status
winarrange daemon reload
winarrange daemon restart
winarrange daemon stop
```

`reload` validates the entire new configuration before replacing registered
hotkeys. If parsing or registration fails, the previous bindings are restored.
Actions execute serially on the daemon event loop, and one failed action is
logged without terminating the daemon. The daemon listens only on a local Unix
socket (macOS) or named pipe (Windows); it opens no network port.

Install or remove login-time autostart for the current user:

```console
winarrange daemon install
winarrange daemon uninstall
```

macOS uses `~/Library/LaunchAgents/io.github.konnta0.winarrange.plist`.
Windows uses the current user's `Run` registry key; it is not a Windows
Service. Uninstalling autostart does not remove the binary, configuration,
profiles, ignore rules, or floating state.

Daemon logs are written to `daemon.log` beside `config.toml`. On macOS,
Accessibility permission applies to whichever installed winarrange binary the
daemon runs. Hotkey registration itself does not arrange or focus any window.

Effective values are selected in this order:

```text
CLI option > profile > global config > built-in default
```

When `default_profile` is configured, bare `winarrange` uses that profile.
Without it, bare `winarrange` keeps the original behavior and arranges all
eligible windows.

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
winarrange list --json
```

The text output includes each PID; `--verbose` also includes the native window
ID. `--json` provides `pid`, `window_id`, process, title, monitor, state, and
bounds for scripts that need to resolve a specific process instance.

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
- Incomplete final rows keep the same cell width and leave trailing grid cells
  empty instead of stretching windows.
- `gap` applies between windows and `margin` applies around each monitor's work
  area.
- Applications that enforce a large minimum size are clamped back inside their
  monitor after resizing.
- Floating and ignored windows are left completely untouched.

Temporary state lives alongside the config in `state.json`.

## Development

Build from source with Rust 1.85 or newer:

```console
cargo build --release
cargo fmt --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

Platform APIs are isolated under `src/platform/`; filtering, ordering,
configuration precedence, state, and layout logic do not depend on Win32,
Accessibility, or CoreGraphics.
