# Desktop setup for USB TouchDeck

TouchDeck connects through USB-Serial/JTAG. The running desktop app mirrors the deck's keys into the host session. Bluetooth is not supported. Wi-Fi and battery support are postponed.

## Running a distributed app versus building it

A distributed app does not need Node, pnpm, Rust, or a C++ compiler. Windows needs the [Microsoft WebView2 Runtime](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution). Linux needs a graphical desktop, the matching WebKitGTK/GTK runtime libraries, USB serial access, and permission to create a virtual keyboard through `/dev/uinput`.

Source builds use **Node.js 24**, matching [app CI](../.github/workflows/app.yml), and the **exact pnpm version in [app/package.json](../app/package.json)** (currently `12.12.1`). Use stable Rust, at least the workspace's declared minimum in [app/Cargo.toml](../app/Cargo.toml). Firmware build prerequisites are separate from desktop setup.

## Windows source-build setup

Use Windows with [PowerShell 7](https://learn.microsoft.com/en-us/powershell/scripting/install/installing-powershell-on-windows) and Microsoft's App Installer/winget available. From the checkout root:

```powershell
pwsh -NoProfile -File app/scripts/setup-windows.ps1
```

This command **installs missing development tools**. Review it before running it; installers may need administrator consent or a restart. The script does not elevate itself, change execution policy, or persist PATH directly. It sets the default rustup toolchain to `stable-msvc`, adds clippy/rustfmt, installs missing MSVC C++ Build Tools and WebView2, and checks every native command's exit code. Microsoft's [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/#windows) describe the C++ workload and WebView2 requirements.

If Node is absent, setup asks winget for available LTS versions and installs the newest **24.x** entry with an explicit version. If the source no longer offers 24.x, it stops with a manual Node 24 installation link. An existing Node from another major is rejected: select/install Node 24, rather than letting setup replace a version-manager selection. pnpm is installed or updated through npm to the manifest pin. On npm 12 or later, setup passes `--allow-scripts=pnpm` so pnpm's installer can replace its placeholder with the native binary; earlier npm versions omit that flag. Setup reads the selected npm shim before invoking pnpm and repairs a shim targeting the extensionless placeholder without executing it. Its final version must match exactly. Other shims that exit successfully but print no version fail with a repair message.

Setup appends installed Machine/User PATH entries and standard Cargo/npm/Node locations to **its own process PATH**, preserving existing entries and their priority. This makes fresh installs usable in that run. A child script cannot update the terminal that launched it: **open a new PowerShell after setup**, then verify:

```powershell
node --version
pnpm --version
rustc --version
```

If a tool is missing or the version is wrong, inspect resolution before retrying:

```powershell
Get-Command node,pnpm,npm,rustup -All
```

Select Node 24 in your version manager or remove the obsolete PATH/shim entry using that tool's normal management procedure. For pnpm, repair npm first if necessary, then run this command on npm 12 or later (use the manifest's version if it changes):

```powershell
npm install --global pnpm@12.12.1 --allow-scripts=pnpm --foreground-scripts
```

Check `npm --version`; on npm 11, omit `--allow-scripts=pnpm` because older releases do not support it. npm 12 [blocks unapproved install scripts](https://github.com/npm/cli/blob/v12.0.0/CHANGELOG.md), and [`--allow-scripts=pnpm`](https://docs.npmjs.com/cli/v12/using-npm/config#allow-scripts) approves pnpm's scripts for this install. When those scripts are blocked, Windows may show a **Pick an app** dialog and print no pnpm version because the npm shim targets an extensionless shell placeholder. If pnpm still prints nothing after repair, repair/remove the stale shim shown by `Get-Command pnpm -All`; a zero exit code alone is insufficient. Do not lower execution policy to make a broken PowerShell shim work; setup uses native/`.cmd` launchers.

Rerunning setup retains working Node/pnpm versions and skips their installers. Rustup's default/component operations are repeatable. MSVC and WebView2 installations are skipped when detected and checked again after an installer succeeds. A nonzero installer exit, incomplete registration, restart requirement, or native command failure stops setup; resolve the reported problem and rerun. A successful version summary does not replace a full app build.

## Linux source-build dependencies

Ubuntu **24.04** is the Linux CI target. Its build packages are:

```sh
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libudev-dev
```

This matches [the workflow](../.github/workflows/app.yml) and [Tauri's Linux prerequisites](https://v2.tauri.app/start/prerequisites/#linux), with `libudev-dev` for serial discovery. CI additionally installs `xvfb` to run GUI tests without a physical display; it is not a desktop runtime prerequisite. Other distributions need their equivalent packages. Install Node 24, the pinned pnpm, and stable Rust separately using their upstream instructions.

From `app/`, the ordinary source build is:

```sh
pnpm install --frozen-lockfile
pnpm tauri build
```

Do not enable the `e2e` feature for a distributed app.

## Linux runtime libraries and device access

Build headers and compilers are not runtime permissions. A Linux distribution of the app needs its linked shared libraries and their dependencies, including WebKitGTK **4.1**, GTK 3, and libudev. On Ubuntu 24.04, WebKitGTK's runtime package is [`libwebkit2gtk-4.1-0`](https://packages.ubuntu.com/noble/libwebkit2gtk-4.1-0); the serial backend uses `libudev1`. Let the app package's dependency metadata and distribution package manager resolve the full set. A portable archive does not imply that all those libraries are bundled.

Before using a connected deck, inspect the actual nodes without opening them or generating input (replace `ttyACM0` with the deck's node):

```sh
id
ls -l /dev/ttyACM0 /dev/uinput
getfacl /dev/ttyACM0 /dev/uinput
udevadm info --attribute-walk --name=/dev/ttyACM0
test -r /dev/ttyACM0 && test -w /dev/ttyACM0
test -w /dev/uinput
```

`getfacl` is supplied by the distribution's ACL utilities. These checks inspect access; they do not prove a successful serial connection or virtual keyboard creation.

- **Serial:** the app needs read/write access to the deck's serial node. Its discovery filter is Espressif VID `303a`, PID `1001` ([code](../app/crates/td-link/src/lib.rs)). Close serial monitors and flash tools before connecting; another process may hold the port. Permission denial can also appear as a busy port. Debian's [`dialout` group](https://wiki.debian.org/SystemGroups) grants serial access, but covers other serial devices too. Prefer an administrator-managed device-specific ACL or a dedicated group rule matching this deck, including its actual USB serial attribute when available. Log out and back in after any group membership change.
- **Keyboard input:** the kernel must provide `uinput`, and the app's user must be allowed to write **`/dev/uinput`**, the exact path opened by [the Linux input backend](../app/crates/td-host/src/input_linux.rs). A system exposing only `/dev/input/uinput` needs an administrator-managed compatible device path. If the module is absent, an administrator must enable it through the distribution's module configuration. Serial access alone does not grant keyboard access. The [kernel's uinput documentation](https://docs.kernel.org/input/uinput.html) explains the interface.

For an administrator who chooses dedicated groups, the following are **rule templates**, not automatic setup. Create the groups, explicitly add only trusted users, replace the serial placeholder with the inspected value, and review existing rules first. Local rules belong in `/etc/udev/rules.d/` and need the distribution's normal rule reload/device-event handling:

```udev
SUBSYSTEM=="tty", ATTRS{idVendor}=="303a", ATTRS{idProduct}=="1001", ATTRS{serial}=="REPLACE_WITH_THIS_DECK_SERIAL", GROUP="touchdeck-serial", MODE="0660"
SUBSYSTEM=="misc", KERNEL=="uinput", GROUP="touchdeck-uinput", MODE="0660"
```

See the [udev rule and permission reference](https://www.freedesktop.org/software/systemd/man/latest/udev.html). Granting uinput access allows processes under that account to inject system input, regardless of which physical device triggered it. Restrict it to a trusted account/session. Do not make device nodes world-writable, grant access to all input devices, disable desktop security controls, or run the desktop app as root. If local policy disallows virtual keyboard creation, leave that permission unavailable; mirrored typing will not work.

## Optional Sources credential storage

The CLIProxyAPI management key is stored through the OS credential store, not in the ordinary Sources JSON configuration ([implementation](../app/src-tauri/src/sources.rs)). The locked `keyring` 4.2 backend uses **Windows Credential Manager** on Windows, **Keychain Services** on macOS, and **Secret Service** on Linux ([upstream platform mapping](https://docs.rs/keyring/4.2.0/keyring/v1/index.html)). This is a credential-store mapping, not a claim that every app feature supports macOS.

On Linux, run the app in the intended user's desktop login session with a session D-Bus and an available Secret Service provider, such as GNOME Keyring or a compatible configured wallet. Its default collection must be usable/unlocked when saving or reading the key; an unlock prompt may be required. See the [Secret Service specification](https://specifications.freedesktop.org/secret-service/latest/description.html). Minimal desktops, headless sessions, containers, and CI may lack that service. Install/configure the provider through the distribution's documented desktop setup if you want this optional source. Do not work around credential-store errors by saving keys in scripts, logs, or world-readable files.

## Verification boundary

Linux CI builds the app and runs mock GUI tests under Xvfb. [The app deliberately uses fake input for Linux mock tests](../app/src-tauri/src/commands.rs), and mock Sources use an in-memory key store. Those checks establish neither real serial permissions, real `/dev/uinput` operation, nor a usable desktop credential store. The setup changes were verified with isolated Windows process fixtures; no real installers, Linux runtime session, device permission changes, or real key injection were exercised in that validation.

The focused setup tests require Windows and PowerShell 7, with no Pester or installed Node/Rust/pnpm dependency:

```powershell
pwsh -NoProfile -File app/scripts/tests/setup-windows.test.ps1
```

Each case starts a fresh process with temporary tool stubs and a guarded tool resolver. Installer calls, tool versions, PATH additions, and native failures are simulated; the suite does not install tools or modify host settings.
