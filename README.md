# winarrange

`winarrange` is an on-demand window arranger for macOS and Windows. It arranges
windows only when explicitly invoked and does not run a background window
manager.

It is not a tiling window manager or daemon: it does not watch window events,
replace Spaces / Virtual Desktops, or resize anything by itself.

> Nothing happens until the user runs `winarrange`.

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

# Temporarily exclude the focused window from future arrangements
winarrange toggle-float
```

Preview any arrangement without moving windows:

```console
winarrange --focused-process --dry-run
```

## Supported platforms

- macOS on Apple Silicon (arm64)
- Windows x64

Linux and Intel Mac are not supported in v0.4.

## Install from GitHub Releases

Download the archive for your platform from the
[latest GitHub Release](https://github.com/konnta0/winarrange/releases/latest).
The archive contains only `winarrange` (macOS) or `winarrange.exe` (Windows).

Each release also provides `checksums.txt`. Verify the SHA-256 checksum before
running the downloaded executable:

```console
# macOS
shasum -a 256 winarrange-v0.4.0-macos-arm64.tar.gz

# Windows PowerShell
Get-FileHash .\winarrange-v0.4.0-windows-x64.zip -Algorithm SHA256
```

Compare the result with the corresponding entry in `checksums.txt`.

### macOS

Extract the archive and place the executable in a directory on `PATH`:

```console
tar -xzf winarrange-v0.4.0-macos-arm64.tar.gz
mkdir -p "$HOME/.local/bin"
install -m 755 winarrange "$HOME/.local/bin/winarrange"
```

Ensure `$HOME/.local/bin` is included in your `PATH`, then check the install:

```console
winarrange --version
winarrange --help
```

The v0.4 binary is not signed or notarized. On first launch, macOS may block it
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
runtime and does not require a Rust installation. The v0.4 binary is unsigned,
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

## Focus and activate one process instance

Use an exact PID when several application instances have the same process name,
bundle identifier, and window title:

```console
winarrange list --json
winarrange focus --pid 12345
```

`focus` selects one visible window belonging to that PID. If the PID has more
than one visible window, the frontmost one is selected. On macOS, winarrange
both raises the window and makes that exact process instance the active
application. On Windows, it makes the selected window the active foreground
window. Preview the selection without changing focus with:

```console
winarrange focus --pid 12345 --dry-run
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
