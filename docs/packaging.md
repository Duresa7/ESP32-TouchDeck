# Windows distribution and app updates

End users download the [Windows x64 installer](https://github.com/Duresa7/ESP32-TouchDeck/releases/latest/download/TouchDeck-windows-x64-setup.exe) from the latest published GitHub release and run it. That link becomes usable when the release owner publishes the first release with that asset. The NSIS installer defaults to `%LOCALAPPDATA%/TouchDeck`, per user. It installs `touchdeck.exe` and `td-statusline.exe` as siblings, with `firmware/`, `licenses/`, `LICENSE.txt`, `PACKAGING.md` and `install-manifest.json`.

Users need no Node, pnpm, Rust, Python, ESP-IDF, compiler, or developer setup script. The installer embeds the Microsoft WebView2 bootstrapper: if WebView2 is missing it downloads/installs the Evergreen runtime, requiring network access and possibly local approval. An existing runtime is reused. Rust binaries use a static MSVC CRT. This is not a fully offline installer. The companion ZIP assumes WebView2 is already available. See [developer setup](https://github.com/Duresa7/ESP32-TouchDeck/blob/main/docs/setup.md) only for source builds; packaging never runs that setup script on this host.

## App updates versus deck firmware

Open **Settings → App updates → Check for updates**. A newer release displays its version and plain-text release notes. **Download update** downloads the installer with byte progress and verifies its signature and signed version before offering installation. Current-version, offline/unavailable, invalid-signature, network and installer-error states remain visible. Checking and downloading leave TouchDeck running.

Before **Install and close TouchDeck**, export any unsaved layout edits, disconnect the deck and finish deck operations. A backend reservation excludes installation from active deck transfers and blocks new transfers while installation starts. A connected deck also prevents installation. Windows then starts the installer and closes TouchDeck; installation progress appears in the installer. TouchDeck does not automatically relaunch. Open it again when installation finishes. The separate **Update** tab selects `.tdfw` bundles for the deck, not desktop app updates.

The application identifier remains `io.github.duresa7.touchdeck`. Upgrades replace binaries/resources and retain per-user config, approvals, source settings/state and icon cache. They do not modify the deck's stored layouts. No profile deletion or explicit uninstall is part of updating. Unsaved editor state is in memory: export before closing. The helper stays beside the app; changing the install folder may require reviewing its absolute-path source configuration. Actual installed upgrade/preservation needs a disposable Windows environment; source/fixture checks alone do not establish it.

Updater signatures authenticate artifacts using the embedded public key. `requireSignedVersion` binds metadata version to the signed version, preventing an old artifact from being advertised under a newer number. TLS and signature checks remain enabled. Updater signatures are separate from Windows Authenticode signing: this work supplies no Authenticode certificate, SmartScreen reputation or store approval.

## Uninstall

The status-line Install on the Sources page writes the absolute path of `td-statusline.exe` into Claude Code's `settings.json`, so it must be undone before the helper is deleted. Users can select **Remove** in Sources. An uninstaller calls the helper itself, before it deletes any file:

```text
td-statusline.exe --unwrap [--settings <file>]
```

- `--settings` defaults to `%CLAUDE_CONFIG_DIR%\settings.json`, else `%USERPROFILE%\.claude\settings.json` (`$HOME` first when set). It is the only flag allowed with `--unwrap`. A custom settings file chosen in Sources is not read from the source config, so pass it explicitly if needed.
- If the file's `statusLine` command runs `td-statusline`, the helper writes back the command it wrapped with `--then`, or removes the `statusLine` entry when there was none. Only the command string, or that one member, changes. Every other byte stays the same. It writes through a temporary file and a rename. `settings.json.touchdeck.bak` is left in place.
- It never reads stdin and never waits for input, so it can run with no console. One line of status goes to stdout, and writing it never fails the run.
- Exit codes: `0` when it restored the command, removed the entry, or had nothing to do (the file is missing, has no entry, or the entry does not run the helper); `1` when the file cannot be read, is not a JSON object, has a `--then` command it cannot parse, or cannot be written, in which case the file is unchanged; `2` for bad arguments.
- Skip it when the uninstaller runs in update mode. An update keeps the helper in the same place.

Source config and status-line state live in the Tauri identifier folders, `%APPDATA%\io.github.duresa7.touchdeck` and `%LOCALAPPDATA%\io.github.duresa7.touchdeck`, never under the install folder, so the uninstaller's **Delete app data** option removes them. The first start after an upgrade moves `sources.json` and `claude-status\*.json` out of the old `%APPDATA%\touchdeck` and `%LOCALAPPDATA%\touchdeck` folders. Because NTFS ignores case, the second of those is the install folder. The CLIProxyAPI management key stays in Windows Credential Manager (`touchdeck` / `cliproxyapi-management-key`) after uninstall.

## Release-owner bootstrap and publication

The repository contains no durable private key or disposable test public key. The base updater `pubkey` is empty; development candidates report that release signing is not configured. The release recipe injects the selected public key through an external build configuration. Before the first real release, the owner must generate/back up a durable Tauri updater keypair, configure `TAURI_UPDATER_PUBLIC_KEY` and matching secrets `TAURI_SIGNING_PRIVATE_KEY` and optionally `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in the `release` GitHub environment. Losing that private key prevents updates to clients trusting it; do not replace it casually. Root owns durable secret bootstrap and publication.

The **windows release** workflow (`.github/workflows/release-windows.yml`) builds the installer, helper and matching radio-free firmware on a disposable Windows runner, then verifies the signed installer with the real Tauri updater against local HTTPS fixtures. A pull request that changes the release inputs builds a `--test-signing` candidate with no secrets and keeps it as a workflow artifact. A manual run is a dry run with the durable key. Pushing a `vX.Y.Z` tag builds with the durable key and publishes. The build job holds only a read token. A separate publish job, which runs no build code, creates a draft, uploads every asset, checks them and only then publishes the release as latest. It refuses to replace a published release. A release contains:

```text
TouchDeck-windows-x64-setup.exe
TouchDeck-windows-x64-setup.exe.sig
latest.json
release-manifest.json
SHA256SUMS
touchdeck-<firmware-version>.tdfw
touchdeck-<source-sha-prefix>-windows-x64-candidate.zip
```

Clients check `https://github.com/Duresa7/ESP32-TouchDeck/releases/latest/download/latest.json`; metadata points to the versioned release's signed installer. Keep installer/signature/metadata together. Because the release is published only after all of its assets are uploaded, that URL never points at a half-uploaded release. `app/scripts/release_version.py check` fails when `app/package.json`, `app/src-tauri/Cargo.toml`, the app entry in `app/Cargo.lock` and `tauri.conf.json` disagree; the workflow also rejects a tag differing from the app version and a tagged commit that is not on `main`.

## Shipping an update

1. Bump every app version in one command (Python 3.11+), then open a pull request with the result and merge it:

   ```sh
   python app/scripts/release_version.py bump X.Y.Z
   python app/scripts/release_version.py check
   ```

2. Tag the merged commit on `main` with an annotated tag. Its message becomes the release notes that **Settings → App updates** shows; a tag without a message gets `TouchDeck vX.Y.Z`.

   ```sh
   git switch main
   git pull --ff-only
   git tag -a vX.Y.Z -m "<notes>"
   git push origin vX.Y.Z
   ```

3. The tag push runs the **windows release** workflow. It checks the tag against the app version, builds, signs with the durable key, verifies the installer against the local HTTPS fixtures and publishes the release. Installed apps then find the update in **Settings → App updates**.

To rehearse a release without publishing, open the Actions tab, choose **windows release**, then **Run workflow**, and give it a commit or tag on `main` (default `main`). That dry run builds and verifies with the durable key and keeps the assets as a workflow artifact. It never creates a tag or a release. From a terminal:

```sh
gh workflow run release-windows.yml --ref main -f ref=main
```

The updater signing key (`TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) and `TAURI_UPDATER_PUBLIC_KEY` live in the `release` GitHub environment, which only `main` and `v*` tags can deploy to, and dry runs only build commits already on `main`.

Never regenerate the updater signing key. Every installed app trusts the public key it was built with, so assets signed by a new key are rejected and those installs can no longer update themselves. If a tag's run fails for a transient reason, re-run it from the Actions tab; the publish job reuses its own leftover draft. If the source needs a fix, land it on `main` and ship the next patch version instead of moving the tag.

## Reproduce the build

Use an existing Windows x64 development environment: Rust/MSVC C++ tools, Node 24+, the pinned pnpm version, Python 3.10+ and ESP-IDF 6.1 with ESP32-S3 tools. Activate ESP-IDF so `IDF_PATH`, Python and toolchain PATH match. Build only a clean committed source revision, using a new output directory outside the checkout:

```powershell
git rev-parse HEAD
Set-Location app
pnpm install --frozen-lockfile
# The existing signing environment supplies the private key; never put it in command-line arguments.
python scripts/package_windows.py build --out E:/touchdeck-artifacts/final-head-001 `
  --public-key-file E:/secure-public-material/touchdeck-updater.pub `
  --notes-file E:/touchdeck-artifacts/release-notes.md `
  --build-lock E:/ESP32-TouchDeck/.verify/m3-build.lock
```

`npm run package:windows -- --out ...` runs the same recipe. An empty pnpm version is not success; use the already installed `node <global-node-modules>/pnpm/bin/pnpm.mjs` entry point if necessary. Local packaging does not install tools or alter host configuration. Cold builds download dependencies and Tauri's NSIS/WebView bootstrapper build inputs.

For an unpublished candidate, pass **`--test-signing` instead of `--public-key-file`**, with no durable signing environment. It generates an external disposable key, captures/discards the signer's key output, embeds only the test public key in that candidate's external config/binary, and deletes the private key after signing. The installer is named `TouchDeck-windows-x64-TEST-ONLY-setup.exe` and the manifest records `test_signing:true`. Never publish these assets or copy the test key into shipping config. `--portable-only` produces a diagnostic ZIP; portable-only is not the product release path.

The recipe atomically holds shared `m3-build.lock` during firmware compilation, and removes only its own lock on success/failure. Retry after another owner releases it; never delete a foreign lock. Fresh external Cargo/firmware directories prevent e2e binary reuse. Cargo uses `--locked --no-default-features` and static CRT; the production dependency graph excludes WebDriver. Tauri embeds frontend assets and built-in icons. Firmware config and actual linked components exclude Wi-Fi/Bluetooth and retain USB-Serial/JTAG. The existing `firmware/tools/make_bundle.py` creates/checks `.tdfw`; its format is unchanged.

The companion ZIP contains app/helper, install resources, firmware, `manifest.json`, `SHA256SUMS` and `build/production-dependencies.txt`. Manifests identify the full source SHA, target/features, tools, resources, firmware image/ELF hashes and radio audit. Release checksums cover uploaded assets. Checksums detect corruption, not publisher authenticity. ZIP order/times/permissions are fixed; compiler paths/times can vary, so cross-machine bit identity is not promised.

```powershell
python app/scripts/package_windows.py verify '<out>/touchdeck-<sha>-windows-x64-candidate.zip'
python firmware/tools/make_bundle.py --check '<out>/payload/firmware/touchdeck-<version>.tdfw'
python app/scripts/verify_updater.py --assets '<out>/release-assets' --out E:/touchdeck-artifacts/https-fixture-001 `
  --cargo-target '<out>/build/app' --openssl 'C:/Program Files/Git/usr/bin/openssl.exe'
```

The fixture uses the real Tauri updater and actual signed installer over local HTTPS, trusting a test CA only in that client. It never changes the OS trust store or disables TLS. It accepts the correct signature and rejects invalid TLS, altered payload bytes, mismatched signed version, malformed metadata and offline endpoints; it checks current-version behavior. It never executes the installer. This does not prove an installed upgrade or the eventual published GitHub endpoint.

## Isolated production smoke

Extract the companion ZIP and use scripts from its source revision:

```powershell
pwsh -NoProfile -File app/scripts/smoke_windows.ps1 -PackageDir '<extracted-folder>' -Scratch E:/touchdeck-artifacts/smoke-001
# Inspect Editor/icons, Sources helper discovery and Settings/App updates in the production window.
pwsh -NoProfile -File app/scripts/smoke_windows.ps1 -Scratch E:/touchdeck-artifacts/smoke-001 -Stop
```

The launcher uses mock transport, fake input, an empty working directory and scratch source/temp paths. `TOUCHDECK_APP_DATA_DIR` redirects Tauri config/log/cache and WebView storage before initialization; it requires an absolute directory and fails closed if invalid. It also redirects source config (`config/`) and status-line state (`state/`) for the app and `td-statusline` when `TOUCHDECK_CONFIG_DIR` and `TOUCHDECK_STATE_DIR` are unset, and a run with it set never moves files out of the old source folders. Unset preserves normal paths. `APPDATA` alone cannot override Windows Known Folders. Source/temp paths have separate launcher overrides. Default app/source paths are snapshotted before/after; changes fail smoke. This does not measure all OS/WebView incidental writes. Installer execution is disabled in mock sessions. Do not install a status line, change credentials or trigger real input.

```powershell
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('touchdeck-helper-' + [guid]::NewGuid())
'{"session_id":"packaging-fixture","model":{"display_name":"fixture"}}' |
  & '<extracted-folder>/td-statusline.exe' --dir $scratch
Get-Content (Join-Path $scratch 'packaging-fixture.json')
```

Repeat final-head builds/checks after dependent fixes land. Current artifacts are candidates. Board flashing, real input and physical checks are excluded. Linux/macOS installers/updater delivery are not implemented here. Fresh-machine installation, real upgrade/preservation and published endpoint behavior must remain unverified without their evidence.

References: [official Tauri updater guide](https://v2.tauri.app/plugin/updater/) and [JavaScript updater API](https://v2.tauri.app/reference/javascript/updater/). Backend-owned commands keep installation behind the reservation; direct plugin installer commands are not granted to the frontend capability.
