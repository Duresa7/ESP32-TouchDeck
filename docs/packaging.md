# Windows installer and app updates

End users download the [Windows x64 installer](https://github.com/Duresa7/ESP32-TouchDeck/releases/latest/download/OpenTouchDeck-windows-x64-setup.exe) from the latest GitHub release and run it. It installs for the current user only and needs no administrator rights, Node, Rust, Python or ESP-IDF.

## What it installs

The installer is built with [electron-builder](https://www.electron.build/nsis) (NSIS, assisted mode, per user). It always installs to `%LOCALAPPDATA%\Programs\Open TouchDeck`; the folder can't be changed, so the app never lands somewhere other users can write.

| Path | What it is |
|---|---|
| `Open TouchDeck.exe` | The Electron window |
| `td-companion.exe` | The Companion, with the UI and the Firmware bundle built in |
| `resources\app.asar` | The window's code |
| `resources\firmware\touchdeck.tdfw` | The same Firmware bundle the Companion carries, for picking by hand |
| `resources\install-manifest.json` | Source revision, version and the Firmware bundle's hashes |
| `resources\LICENSE.txt`, `resources\licenses\` | Licenses |
| `Uninstall Open TouchDeck.exe` | The uninstaller |

It adds Start menu and desktop shortcuts and an entry in **Installed apps**. On its first start the Companion adds the per-user Run value `Open TouchDeck` so it starts at sign-in (see [companion-api.md](companion-api.md#tray-and-startup-implementation-decisions-79)).

User data stays outside the install folder: `%LOCALAPPDATA%\io.github.duresa7.touchdeck` (Companion data and its runtime file), `%APPDATA%\io.github.duresa7.touchdeck` and `%APPDATA%\Open TouchDeck` (the window).

## Install, update and uninstall

- **Install or reinstall.** Before it replaces files, the installer stops a running Companion through its own shutdown route: it reads the runtime file, checks the Companion's HMAC proof before it sends the pairing token, takes `system.lock` and calls `system.shutdown`. It waits up to 15 seconds, then ends only processes whose path is exactly the installed `td-companion.exe`. If a Device is busy with a Job such as a Firmware update, the installer stops and says so. It then closes the window.
- **Downgrade.** An installer older than the installed version refuses to run (exit code 5). Uninstall first to go back.
- **Update.** Open TouchDeck checks `latest.json`, downloads the installer, and checks its signature with the updater key built into the app (ADR 0005) before and right before running it. It takes `system.lock`, stops the Companion, starts the installer with `/S --updated --force-run` and quits. The installer replaces the files silently and starts Open TouchDeck again.
- **Uninstall.** Stops the Companion and the window the same way, removes the files, shortcuts, the uninstall entry, the Run value (only when it points at this install) and the startup marker, so a later install turns startup at sign-in back on. It **keeps user data** unless you answer Yes when it asks, or pass `--delete-app-data` to a silent uninstall. Integration secrets in Windows Credential Manager stay either way.

Installers are not Authenticode-signed: there's no certificate, so Windows SmartScreen may warn. The update signature is separate from Authenticode and is always checked.

## Update signing

The update key is the existing updater key ([ADR 0005](adr/0005-updater-keeps-the-signing-key.md)). The release build signs the installer with the Tauri CLI signer (`tauri signer sign --app-version X.Y.Z`), which writes the minisign signature the Electron updater verifies, with the version bound in its trusted comment. Never regenerate or replace the key: every installed copy trusts it.

The key lives in the `release` GitHub environment: `TAURI_UPDATER_PUBLIC_KEY` (variable), `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (secrets). Only `main` and `v*` tags can deploy to it.

A release contains:

```text
OpenTouchDeck-windows-x64-setup.exe
OpenTouchDeck-windows-x64-setup.exe.sig
latest.json
release-manifest.json
touchdeck.tdfw
SHA256SUMS
```

`latest.json` holds the version, notes, date and, for `windows-x86_64`, the installer URL, its signature and its size. Installed apps read `https://github.com/Duresa7/ESP32-TouchDeck/releases/latest/download/latest.json`.

## Shipping an update

1. Bump every app version in one command (Python 3.11+), open a pull request with the result and merge it. `check` fails when `app/package.json` and `app/electron/package.json` disagree.

   ```sh
   python app/scripts/release_version.py bump X.Y.Z
   python app/scripts/release_version.py check
   ```

2. Tag the merged commit on `main` with an annotated tag. Its message becomes the release notes.

   ```sh
   git switch main
   git pull --ff-only
   git tag -a vX.Y.Z -m "<notes>"
   git push origin vX.Y.Z
   ```

3. The tag push runs **windows release** (`.github/workflows/release-windows.yml`). It runs the app, Firmware and installer lifecycle checks on the tagged commit without secrets, builds and signs with the durable key, and a separate publish job, which runs no build code, uploads every asset to a draft and only then publishes it as latest.

To rehearse without publishing, run **windows release** by hand on a commit on `main`. It builds and signs with the durable key and keeps the assets as a workflow artifact:

```sh
gh workflow run release-windows.yml --ref main -f ref=main
```

If a tag's run fails for a transient reason, re-run it; the publish job reuses its own draft. If the source needs a fix, ship the next patch version instead of moving the tag.

## Installer lifecycle check

`installer-lifecycle.yml` runs on a disposable GitHub-hosted Windows runner as part of every **windows release** run. Run it by hand to check an installer change on a branch:

```sh
gh workflow run installer-lifecycle.yml --ref <branch>
```

It builds a test-signed candidate with a throwaway key that is deleted after signing, plus a second installer at the next patch version signed with the same throwaway key. Then `app/scripts/installer_lifecycle.ps1`:

1. installs silently and opens the app: the window and the Companion start, and the Companion registers the Run value;
2. runs the real Electron updater (`app/scripts/lifecycle_update.mjs`, connected to the installed Companion as the window is) against a loopback feed that serves the newer installer with one byte changed, and expects a signature failure with nothing installed;
3. updates to the newer installer through the same updater: lock, shutdown, `/S --updated --force-run`, restart, user data kept byte for byte;
4. runs the older installer over the newer one and expects it to refuse;
5. uninstalls silently with the Companion running: its runtime file is gone (it shut down cleanly), and the files, uninstall entry and Run value are gone while user data stays;
6. reinstalls and uninstalls with `--delete-app-data`, and expects the data folders gone.

The script refuses to run outside GitHub Actions: it writes the real per-user registry. It uploads only a sanitized log and summary.

## Build the installer

Use Windows x64 with Rust (MSVC), Node 24, the pinned pnpm, Python 3.11+ and ESP-IDF 6.1 with ESP32-S3 tools, and activate ESP-IDF. Build a clean committed revision into a new folder outside the checkout:

```powershell
Set-Location app
pnpm install --frozen-lockfile
# The signing environment supplies the private key; never put it on the command line.
python scripts/package_windows.py build --out E:/touchdeck-artifacts/build-001 `
  --public-key-file E:/secure-public-material/touchdeck-updater.pub `
  --notes-file E:/touchdeck-artifacts/release-notes.md
```

The recipe builds the Firmware and audits it radio-free, builds the UI, builds the Companion with `cargo build --release --locked -p td-companion` and the Firmware bundle embedded, refuses a Companion with test hooks, builds the Electron code with the updater public key, packages it with electron-builder and signs the installer.

For an unpublished candidate, pass `--test-signing` instead of `--public-key-file`, with no signing environment. It generates a throwaway key, builds the installer named `OpenTouchDeck-windows-x64-TEST-ONLY-setup.exe` and the next-patch installer for the lifecycle check, and deletes the private key. Never publish these assets. On a PC without ESP-IDF, a test-signed candidate can reuse a checked bundle with `--firmware-bundle <file.tdfw>`; releases always build the Firmware.

The recipe holds the shared `.verify/m3-build.lock` while it compiles Firmware and removes only its own lock. Never delete another owner's lock.

The installer enforces current-user mode and `%LOCALAPPDATA%\Programs\Open TouchDeck` after resolving command-line and stored installation settings. `/allusers` and `/D=` cannot change that destination. A busy Device stops the installer with exit code 3. If the Companion passes authentication proof but the helper cannot confirm an install lock, the installer exits 4 and asks you to close Open TouchDeck from the tray before retrying.
