# TouchDeck desktop app

TouchDeck runs on Windows and Linux. It connects to the deck over USB-Serial/JTAG,
edits and transfers layouts, runs PC actions, feeds widgets, and updates deck
firmware. The running, connected app mirrors keys and media controls into the host
session. The deck does not type as a standalone USB HID keyboard. Wi-Fi, Bluetooth,
battery support, and standalone USB HID are deferred to the post-product update.

For everyday use, follow the [USB user guide](../docs/usage.md). The window title
identifies the app's source revision. **Deck** reports the connected firmware's
version and image identity. Desktop app updates and deck firmware updates are
separate operations.

## Install a published build

Once a release is published, the Windows installer will be at
[TouchDeck-windows-x64-setup.exe](https://github.com/Duresa7/ESP32-TouchDeck/releases/latest/download/TouchDeck-windows-x64-setup.exe)
on [the latest GitHub release](https://github.com/Duresa7/ESP32-TouchDeck/releases/latest).
No release has been published yet, so use the source-build path below until one
is. The installer installs per user and needs no Node, pnpm, Rust, or ESP-IDF. It
downloads the WebView2 runtime if Windows lacks it. Linux has no installer.

Keep `td-statusline.exe` beside `touchdeck.exe` on Windows, or `td-statusline`
beside `touchdeck` on Linux, for the optional status-line Source. A portable
archive must be fully extracted before you start the app. Per-user settings,
approvals, Sources state, and icon caches live outside the executable directory.
Copying that directory alone does not migrate your setup.

The [desktop setup guide](../docs/setup.md) covers prerequisites and Linux serial,
`/dev/uinput`, and credential-store access. The
[packaging guide](../docs/packaging.md) describes the installer's contents,
candidate artifacts, and release production. A successful source build or
candidate package is not a published release.

Update the installed app from **Settings > App updates**. That is separate from
updating deck firmware in the **Update** tab. The updater signing key is not
configured yet, so app updates become available only after the first signed
release is published.

## Build the desktop app

Use Node.js **24**, pnpm **12.12.1** as pinned in [package.json](package.json),
and stable Rust meeting the minimum in [Cargo.toml](Cargo.toml). Follow the
[desktop setup guide](../docs/setup.md) for platform dependencies.

On Windows, from the checkout root, the setup command is:

```powershell
pwsh -NoProfile -File app/scripts/setup-windows.ps1
```

The script installs missing development tools. Review it before running it, and
open a new PowerShell afterward. It is a developer setup script, not the desktop
app installer. Linux builds also need the WebKitGTK 4.1, GTK, and libudev development
packages described in the setup guide.

From `app/`, build the production app and helper:

```sh
pnpm install --frozen-lockfile
pnpm tauri build
cargo build --release -p td-link --bin td-statusline
```

The executable is `target/release/touchdeck.exe` on Windows or
`target/release/touchdeck` on Linux. The helper is in the same directory after its
separate build. Keep them together for a manual source deployment. Frontend
assets and built-in icons are embedded in the app. Building the app does not flash
the deck or create a published release. Firmware build and `.tdfw` instructions
are in the [USB user guide](../docs/usage.md#install-or-update-deck-firmware).

Do not enable the `e2e` feature for an app you distribute. The packaging recipe
owns installer and release generation, including any updater configuration.

## Develop and check changes

Run these commands from `app/` after installing dependencies:

```sh
pnpm tauri dev
pnpm lint
pnpm test
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

`pnpm tauri dev` opens the app against real USB devices. To use the mock deck
without a board, set the environment before starting the command. In PowerShell:

```powershell
$env:TOUCHDECK_MOCK = '1'
$env:TOUCHDECK_INPUT = 'fake'
pnpm tauri dev
```

In a POSIX shell:

```sh
TOUCHDECK_MOCK=1 TOUCHDECK_INPUT=fake pnpm tauri dev
```

The mock implements the app link in process, including layouts, settings,
icons, Sources messages, and firmware update scenarios. Tests can emit key and
press events, drop messages, or close the connection. Fake input records key
operations without typing into the host session. Mock checks do not verify real
serial permissions, keyboard input, credential storage, or physical panel behavior.

## Repository layout

| Path | Responsibility |
| --- | --- |
| `crates/td-core` | Frame codec, link messages, layout validation, and settings with no I/O. |
| `crates/td-link` | Serial and mock transports, transfers, firmware update and ROM recovery, CLI tools, and `td-statusline`. |
| `crates/td-host` | Host input, actions, approvals, icon processing, and Sources. |
| `src-tauri` | Desktop shell, app commands, and per-user storage integration. |
| `src` | Svelte window and its Deck, Editor, Actions, Settings, Sources, and Update views. |
| `e2e` | WebdriverIO specs and their runner. |

The [layout format](../schema/README.md) and
[framed app link](../schema/protocol/link-v1.md) describe the files and protocol.

## Use the link CLI

`td-link` is another USB client. Close the desktop app's connection and any serial
monitor before using it against the same port. These commands run from `app/`:

```sh
cargo run --release -p td-link --bin td-link -- discover
cargo run --release -p td-link --bin td-link -- --port COM3 hello
cargo run --release -p td-link --bin td-link -- --mock ping --count 1000
cargo run --release -p td-link --bin td-link -- --port COM3 console "version"
cargo run --release -p td-link --bin td-link -- --port COM3 events --seconds 10
```

Replace `COM3` with your deck's port. The CLI prints JSON and exits `0` on success,
`1` when the deck replies with `ok` false, and `2` on another failure.

## Run end-to-end tests

Build a separate test app, then run the specs from `app/`:

```sh
pnpm tauri build --features e2e
pnpm e2e
```

The `e2e` Cargo feature adds a WebDriver server on port 4445 or
`TAURI_WEBDRIVER_PORT`. Never distribute that executable. `pnpm e2e` starts the
built app for each spec and writes results and screenshots to `TD_E2E_OUT`,
defaulting to `target/e2e`. `TD_E2E_SPEC=<name>` selects one spec.
`TD_E2E_ATTACH_PORT=<port>` drives an already running test app.

A spec named `name.spec.ts` runs on every OS, `name.win.spec.ts` only on Windows,
and `name.linux.spec.ts` only on Linux. The Editor export spec substitutes a
save-picker writer. It checks serialized content, not a native save dialog or an
actual file saved by the installed app.
