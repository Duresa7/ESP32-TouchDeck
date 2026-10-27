# TouchDeck app

The desktop app for the deck, on Windows and Linux. It finds the deck on its
USB-Serial/JTAG port, connects over the [framed app link](../schema/protocol/link-v1.md),
shows the build the deck runs, and gives the debug console a pane. The window title
names the git commit the app was built from.

The product connects over USB. Wi-Fi and Bluetooth are deferred to a post-product update.

## Layout

| Path | What it holds |
| --- | --- |
| `crates/td-core` | Pure Rust with no I/O. The frame codec and stream decoder, the link messages, the layout format with the rule keys of [schema/README.md](../schema/README.md), and the deck settings. |
| `crates/td-link` | The transport over a serial port or the in-process mock deck, the session, discovery, and the `td-link` CLI. |
| `src-tauri` | The Tauri shell and its commands: `list_decks`, `connect`, `disconnect`, `deck_info`, and `console_send`. |
| `src` | The Svelte 5 window. |
| `e2e` | WebdriverIO specs and their runner. |

## What the PC needs

On Windows, run `pwsh -NoProfile -File app\scripts\setup-windows.ps1`. It installs
what is missing:

- Rust from `rustup` with the `stable-msvc` toolchain, `clippy`, and `rustfmt`.
- The MSVC tools and the Windows SDK, from the Visual Studio C++ build tools.
- WebView2, which Windows 11 already ships.
- Node.js 24 and pnpm 12.

On Linux, install the packages that [the CI workflow](../.github/workflows/app.yml)
installs. Those are WebKitGTK 4.1, libudev, and the build tools.

## Commands

Run these from `app/`.

```sh
pnpm install
pnpm tauri dev                      # the window, against the deck on USB
TOUCHDECK_MOCK=1 pnpm tauri dev     # the window, against the mock deck
pnpm lint                           # svelte-check, warnings fail
pnpm test                           # node --test for the frontend and the spec picker
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
pnpm tauri build --features e2e     # target/release/touchdeck with the WebDriver server
pnpm e2e                            # every spec for this OS, against the built app
```

In PowerShell, set the mock with `$env:TOUCHDECK_MOCK = '1'`.

## The mock deck

`TOUCHDECK_MOCK=1` makes the app list one deck, `mock`, that speaks protocol 1 in
process. It answers `hello`, `ping`, `link.state`, `icon.list` in pages of 32,
`settings.get`, `settings.put`, and `widget.set`, answers `version` and `link state`
on the console, prints a heartbeat line every 2 seconds, and records every message
it receives. Tests also make it emit `press` and `key.report` events, go silent,
or hang up. Each app PR that adds firmware behavior extends the mock to match, so
the WebDriver specs never need the board.

## td-link

`td-link` is a second link client beside the Python one in the verification harness.

```sh
cargo run --release -p td-link -- discover
cargo run --release -p td-link -- --port COM3 hello
cargo run --release -p td-link -- --mock ping --count 1000
cargo run --release -p td-link -- console "version"
cargo run --release -p td-link -- events --seconds 10
```

It prints JSON. It exits `0` on success, `1` when the deck answers with `ok` false,
and `2` on any other failure.

## End-to-end tests

The `e2e` cargo feature adds [tauri-plugin-wdio-webdriver](https://crates.io/crates/tauri-plugin-wdio-webdriver),
a W3C WebDriver server inside the app on port 4445 or `TAURI_WEBDRIVER_PORT`. Never
ship a build with that feature. `pnpm e2e` starts the built app for each spec, runs
the spec with WebdriverIO, and writes `<spec>.json` and any screenshots to
`TD_E2E_OUT`, which defaults to `target/e2e`. `TD_E2E_SPEC=<name>` runs one spec, and
`TD_E2E_ATTACH_PORT=<port>` drives an app that is already running instead of
starting one.

A spec named `name.spec.ts` runs on every OS, `name.win.spec.ts` only on Windows, and
`name.linux.spec.ts` only on Linux.
