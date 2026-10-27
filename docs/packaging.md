# Production packaging

The supported recipe produces an unsigned **Windows x64 portable candidate**. It builds the production app without
the `e2e` feature, its `td-statusline.exe` source helper, and radio-free firmware from the same clean Git revision.
It does not publish a release. Rebuild from the final integrated head after Wi-Fi controls are removed and the
displayed-orientation fixes land; artifacts built before then are candidates, not the finished USB product.

## Build requirements

Use an existing Windows x64 development environment: Rust with the MSVC target and C++ build tools, Node 24 or later,
the pnpm version pinned in `app/package.json`, Python 3.10 or later, and ESP-IDF 6.1 with the ESP32-S3 tools.
Activate ESP-IDF in the build shell so `IDF_PATH`, Python and the toolchain PATH match that installation.
No installer or host configuration changes are performed by the packaging script. It needs normal dependency
download access on a cold build. Firmware dependencies remain governed by `firmware/dependencies.lock`.

From a clean, committed checkout, in PowerShell:

```powershell
git rev-parse HEAD
Set-Location app
pnpm install --frozen-lockfile
# Run with the activated ESP-IDF Python; --out must be a new directory outside the checkout.
python scripts/package_windows.py build --out E:/touchdeck-artifacts/final-head-001 `
  --build-lock E:/ESP32-TouchDeck/.verify/m3-build.lock
```

`npm run package:windows -- --out <new-directory>` invokes the same recipe. If a pnpm shim silently returns no
version, invoke the already installed JavaScript entry point explicitly, e.g.
`node <global-node-modules>/pnpm/bin/pnpm.mjs install --frozen-lockfile`; do not treat an empty version as success.
The Python interpreter must still be the activated ESP-IDF interpreter.

The script atomically acquires the shared firmware build lock and fails immediately if another owner holds it.
Retry after that owner releases it; never delete a foreign lock. It releases its own lock on success or failure.
Each run uses fresh external Cargo and firmware build directories, preventing reuse of an e2e executable. Cargo
uses `--locked --no-default-features`; Tauri embeds the Vite production output and skips installer bundling.
The selected normal/build dependency graph must exclude the optional WebDriver plugin. Firmware config and the
actual linked component list must exclude Wi-Fi/Bluetooth and retain USB-Serial/JTAG. The existing
`firmware/tools/make_bundle.py` creates and reads back the `.tdfw`; no alternate firmware format is introduced.

## Files delivered

```text
touchdeck-<source-sha-prefix>-windows-x64-candidate.zip
  touchdeck.exe
  td-statusline.exe
  firmware/touchdeck-<firmware-version>.tdfw
  manifest.json
  SHA256SUMS
  build/production-dependencies.txt
  LICENSE.txt
  licenses/LICENSE-lucide.txt
  PACKAGING.md
```

The manifest records the full source revision, target, empty app feature selection, tool versions, firmware image
metadata and radio audit, and each payload file's size and SHA-256. `SHA256SUMS` also covers the manifest; a sibling
`.zip.sha256` covers the entire archive. Checksums detect corruption, not publisher authenticity. ZIP entry order,
timestamps and permissions are fixed. Compilers/ESP-IDF may embed build time or paths: this is a repeatable build
recipe, not a claim of bit-identical binaries across machines or builds.

Verify from the matching source checkout:

```powershell
python app/scripts/package_windows.py verify E:/touchdeck-artifacts/final-head-001/touchdeck-<sha>-windows-x64-candidate.zip
python firmware/tools/make_bundle.py --check E:/touchdeck-artifacts/final-head-001/payload/firmware/touchdeck-<version>.tdfw
Get-FileHash E:/touchdeck-artifacts/final-head-001/touchdeck-<sha>-windows-x64-candidate.zip -Algorithm SHA256
```

The verifier checks the exact archive contents, hashes, executable source marker, production dependency graph and
firmware metadata. Build output is under `<out>/build`; the unpacked distributable is `<out>/payload`. Keep build
logs and validation evidence with the candidate. Neither build tools, frontend `node_modules`, test fixtures nor
WebDriver are distributed. Frontend assets, layout validation code, built-in icon index and SVGs are embedded in
the executable. ROM recovery uses the linked Rust implementation and needs no external esptool/Python at runtime.

## Run the portable app

Extract the entire ZIP to a writable folder and run `touchdeck.exe`. Keep `td-statusline.exe` beside it so Sources
can find the helper without a PATH change. The PC must already have the Microsoft Edge WebView2 Evergreen runtime
and the Microsoft Visual C++ runtime required by its MSVC build. This archive does not install or repair either.
It is unsigned; SmartScreen or enterprise execution policy may require local approval. No installer, signing,
auto-update or fresh-machine runtime installation has been validated by this recipe.

"Portable" describes deployment: user settings, approvals, logs, source state and custom icon caches still use
per-user application directories. Moving the folder does not migrate those files. Do not overwrite a running
copy; source-helper settings may contain its absolute path and need user review when the folder moves.
The app must remain running and connected for USB-mirrored keyboard actions. This build does not add standalone
USB HID or wireless support. The firmware bundle is selected in Update/Recover; packaging never flashes it.

For a safe packaging smoke check, use the script from the matching source checkout:

```powershell
pwsh -NoProfile -File app/scripts/smoke_windows.ps1 -PackageDir '<extracted-folder>' -Scratch E:/touchdeck-artifacts/smoke-001
# Inspect Editor/built-in icons and Sources helper discovery through the production window.
pwsh -NoProfile -File app/scripts/smoke_windows.ps1 -Scratch E:/touchdeck-artifacts/smoke-001 -Stop
```

The launcher sets mock transport and fake input, uses an empty external working directory, and redirects temporary
files and source config/state. `TOUCHDECK_APP_DATA_DIR` explicitly redirects Tauri config/log/cache and WebView
storage before initialization; it must be an absolute directory (created if absent) and fails closed on an invalid
path. With that variable unset the app preserves its normal paths. `APPDATA` alone cannot override Tauri's Windows
Known Folder paths. The smoke script snapshots those default app/source paths before launch and after stopping
only its recorded child, failing if their contents change. It does not measure all OS/WebView incidental writes.
The override by itself does not redirect source state: retain the launcher's separate source/temp overrides.
Do not start an e2e driver, install a status line, change provider credentials or trigger a real action during smoke.

Prove the helper independently without touching a real configuration:

```powershell
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('touchdeck-helper-' + [guid]::NewGuid())
'{"session_id":"packaging-fixture","model":{"display_name":"fixture"}}' |
  & '<extracted-folder>/td-statusline.exe' --dir $scratch
Get-Content (Join-Path $scratch 'packaging-fixture.json')
```

Repeat the build and isolated smoke check on the final integrated head; retain its SHA, archive hash, firmware
image hash, commands/results and platform limitations. Linux/macOS archives and platform installers are not
implemented or validated here. Real input, board updates and physical panel checks are outside packaging checks.
