# Use TouchDeck over USB

Use this guide to install TouchDeck, connect your deck, and customize its controls.
You need the Waveshare ESP32-S3-Touch-LCD-5B, a USB data connection, and a Windows
or Linux desktop session.

Keep the app running and connected for keyboard and media input, PC actions, and
live Sources. The app mirrors USB key reports into the host session. The deck
does not act as a standalone USB HID keyboard. Page navigation works on the deck
without the app. Wi-Fi, Bluetooth, battery support, and standalone USB HID
(direct TinyUSB) are deferred to the post-product update.

## Obtain and install the desktop app

This section is for people who run the app. If you build it from source, use
[Build from source (developers)](#build-from-source-developers) instead.

**Windows installer.** Once a release is published, its installer will be at
[TouchDeck-windows-x64-setup.exe](https://github.com/Duresa7/ESP32-TouchDeck/releases/latest/download/TouchDeck-windows-x64-setup.exe)
on [the latest GitHub release](https://github.com/Duresa7/ESP32-TouchDeck/releases/latest).
No release has been published yet, so that link does not work today. When it does,
run the installer. It installs for your user, by default under
`%LOCALAPPDATA%/TouchDeck`, with `touchdeck.exe`, the `td-statusline.exe` helper
beside it, and a `firmware/` folder with the matching `.tdfw` bundle. You need no Node, pnpm, Rust,
Python, or ESP-IDF. If Windows lacks the WebView2 runtime, the installer downloads
it, so that step needs network access. The [packaging guide](packaging.md) lists
the installer's contents.

Until a release exists, use the source build below. Linux has no installer. Run
`touchdeck` from a source build and keep `td-statusline` beside it. The
[desktop setup guide](setup.md) lists the Linux runtime libraries, the serial and
`/dev/uinput` permissions, and the desktop credential store that the optional
CLIProxyAPI Source needs.

Start `touchdeck.exe` on Windows or `touchdeck` on Linux. The window title
identifies the source revision the app was built from.

### Build from source (developers)

Source builds are for development and for use before a release exists. On Windows,
run `pwsh -NoProfile -File app/scripts/setup-windows.ps1` from the checkout root.
It installs missing developer tools, so read [the setup guide](setup.md) first.
Then follow [Build the desktop app](../app/README.md#build-the-desktop-app). The
packaging recipe in the [packaging guide](packaging.md) produces installer
candidates. A candidate is not a published release.

## Install or update deck firmware

Desktop app installation and deck firmware installation are separate. The app's
**Update** view takes a `.tdfw` bundle, not the desktop installer or the app update
metadata. Use the bundle that accompanies the intended firmware build for this
board. A published release is planned to list a `touchdeck-<version>.tdfw` asset,
and the Windows installer places one in its `firmware/` folder. Neither exists
until a release is published, so build a bundle yourself until then.

If you build firmware yourself, use ESP-IDF 6.1 and the
[firmware build instructions](../README.md#building). From the checkout root,
after building `firmware/`, create and check a bundle:

```sh
python firmware/tools/make_bundle.py --build-dir firmware/build --out-dir firmware/build
python firmware/tools/make_bundle.py --check firmware/build/touchdeck-<version>.tdfw
```

Replace `<version>` with the filename printed by the first command. The bundle
contains the application image, bootloader, partition table, and a manifest with
image hashes. Keep the bundle intact.

To update a deck that runs TouchDeck firmware:

1. Close serial monitors, `td-link`, and flash tools that use the deck's port.
2. Open **Update** in the desktop app.
3. Select the `.tdfw` file under **Firmware bundle**.
4. Set **Deck port** to the deck's port, such as `COM3` or `/dev/ttyACM0`.
5. Select **Update deck**. Keep USB connected through transfer, reboot, and confirmation.
6. Wait for **Firmware updated**, or **Already on this build** if the image matches.
7. Return to **Deck**, select **Refresh**, and **Connect** again.

The app closes its ordinary connection while the updater owns the port. A normal
update writes the inactive application slot and verifies the image before reboot.
The firmware confirms a successful boot. An image that resets before confirmation
can roll back to the previous slot. If the app reports **Failed**, read the reason
before retrying. Reaching 100% transfer alone does not establish a confirmed boot.

If the deck does not run a working TouchDeck image, use **Recover deck** with the
same bundle and the correct USB port. This uses the ESP32-S3 ROM downloader over
USB-Serial/JTAG and writes the bundled bootloader, partition table, and application.
It resets the boot selection and keeps the saved-layout storage partition for
the supported partition map. Use this path for initial installation or broken
firmware. It is not the ordinary app update path or a layout reset. The desktop
recovery implementation needs no separate Python or esptool at runtime.

The deck's ten-second touch hold is a different recovery feature. It boots the
built-in layout and default settings from RAM while keeping stored files. See
[Recovery](../README.md#recovery) for its behavior.

## Connect and keep the USB port available

1. Connect the board's USB-Serial/JTAG interface with a data cable.
2. Start the app and open **Deck**.
3. Select **Refresh**, then **Connect** beside the deck's port.
4. Check that the header says **Connected to** your port. The Deck pane shows the
   firmware version, image identity, and recovery state.

Only one client can own the serial port. **In use by another program** means you
must close the other app, monitor, or flashing session before retrying. On Linux,
also check serial-device permissions. **No answer to hello** does not establish
that firmware is healthy. Check the cable, port, and firmware before using ROM
recovery. Select **Disconnect** before opening a separate serial tool.

Keep the app running after connecting. Mirrored keys go to the focused desktop
application. If typing fails, check **Actions > Key monitor** and the Linux
`/dev/uinput` permissions described in the setup guide. App actions and widgets
dim on the deck when the app is offline. Key tiles do not dim, but still need
the connected app to type into the PC.

## Import, edit, and push a layout

The Editor keeps a separate draft for **Landscape** and **Portrait**. These
buttons choose the draft and storage slot, not the display's physical orientation.

1. Open **Editor** and choose an orientation.
2. Select **Import** and open a layout JSON file. Start with
   [builtin.json](../schema/examples/builtin.json) for keys and media controls, or
   [dev.json](../schema/examples/dev.json) for action and Source examples.
3. Select a page tab, then a tile. Change its **Label**, **Icon**, and **Action**
   in the side panel.
4. Drag a tile to move it. Drag its corner to resize it, or use **Row**,
   **Column**, **Rows**, and **Columns**. Overlaps and invalid edits are refused.
5. Select **Add tile** for a free cell or **Add page** for another grid page.
   **Remove page** asks for a second **Click to remove** selection.
6. Select **Export** to save the current orientation's draft before replacing it.
7. With the deck connected, select **Push**. Read the status at the bottom,
   including the layout the deck actually shows and any unavailable icons.

Import selects the orientation declared inside the file. **New** replaces the
current orientation's draft with a new layout. **Pull** reads the selected
orientation's stored file from the deck. Choose **Replace** to accept it or
**Cancel** to keep your draft. A deck using only the built-in layout may have no
stored file to pull. Import the built-in example in that case.

Push writes the file's orientation slot. Pushing the currently displayed
orientation reloads it. Pushing the other slot saves it for a later orientation
change and leaves the displayed layout, its actions, and its widgets running.
Export each orientation separately. The Editor has no undo or autosave
and does not save drafts across app restarts.

New pages use a fixed 3-row, 4-column grid. To change layout or page names, grid
dimensions, or create a dashboard, edit the JSON outside the app and import it.
See the [layout reference](../schema/README.md) for limits and rules. From a source
checkout, validate examples and your own file without executing its actions:

```sh
npm ci --prefix schema
node schema/check-layouts.mjs path/to/layout.json
```

The validator checks the repository examples too. A valid layout fits within
32768 bytes of compact JSON. A failed import keeps the previous draft.

## Change the displayed orientation and idle timeout

1. Push a layout for the orientation you want to display.
2. Open **Settings** and choose **Orientation**.
3. Set **Screen off after, in seconds** to a whole number from 1 to 86400.
4. Select **Save**. The timeout takes effect immediately.
5. When offered, select **Reboot deck** to apply the saved orientation. The status
   line says the orientation applies after the deck reboots.
6. Reconnect from **Deck** after the reboot.

A saved orientation takes effect only after the deck reboots. Until then the deck
keeps displaying the previous orientation, and the app's actions and widget
Sources follow the layout the deck actually displays, not the saved setting.
Pushing a layout to the other orientation's slot stores it without changing the
displayed layout, the actions, or live widgets. Pushing to the displayed
orientation reloads the display and rebinds both.

A portrait file in storage does not rotate the current display. Portrait uses a
600 by 1024 canvas with the panel turned 90 degrees clockwise. If the displayed
slot has no valid layout, the firmware shows its built-in layout. That layout has
no PC actions, so the app keeps the action definitions it already had.

## Configure PC actions and command approvals

Set a tile's **Action** in Editor, then push the layout. Use **Actions** to inspect
the loaded action layout, **Key monitor**, and **Action log**. The monitor records
mirrored reports and, when its text area has focus, received key events. For an
initial key check, choose F13 in the Editor and focus that monitor before pressing
the tile.

Use the action fields as follows:

- `key`, `mod`, and `consumer` select keys, modifiers, and media controls. The key
  picker offers F1 through F24, letters, and digits.
- `page` selects another page in the same layout. `none` leaves a tile inactive.
- `launch` starts the executable at **Path** with optional arguments and a
  **Working folder**.
- `open` opens a file, folder, or URL in its default desktop application.
- `command` runs **Program** with optional arguments, a working folder, and
  **Run in a shell**. Put each argument on its own line. Shell mode still passes
  arguments as data; it does not interpret the field as a shell script.
- `widget` binds **Source**, **Field**, and **Account** to a reading described below.

Only `command` actions use the command-approval dialog. Review its program,
arguments, directory, and shell setting before choosing **Allow once**,
**Always allow**, or **Deny**. Always allow remembers that exact command on this
PC. Changing any of those four values asks again. Launch and open actions run
without this dialog, so review imported layouts before using their tiles.

**Actions > Load layout file** loads local action definitions for troubleshooting.
It does not upload a file or change the deck display. Use Editor **Push** for the
normal workflow, and keep local action definitions consistent with the displayed
layout. Check the Action log for denied, failed, started, and completed actions.

## Choose icons and keep them portable

Select a tile, then use **Icon** for built-in `lv:` symbols or **Choose icon** for
the searchable bundled icon set. A suggested icon can be selected explicitly.
For a `launch` tile, **Use app icon** extracts an icon from its configured program
and saves it in this PC's icon cache.

Push sends referenced bitmap icons that the deck lacks, if those icons exist in
the app's cache or bundled set. Read the Push result for **icon not in the cache**
errors. A layout JSON stores `file:<id>` references, not the image bytes. Pull
reads the layout file and does not retrieve bitmap files into the PC cache.

For transfer to another PC, prefer `lv:` symbols or the bundled icon set.
Re-extract custom application icons on the destination PC or preserve their cache
files separately. Update executable paths for that PC too. **Remove unused icons**
deletes deck icons not referenced by either stored orientation or the drafts
currently open in Editor. An exported file elsewhere is not protected by that
check. Keep custom icon backups before pruning.

## Set up optional Sources

Sources feed widget tiles and imported dashboard pages while the app runs.
Open **Sources** to check readings and errors. While the
deck is connected, the app takes widget bindings from the layout the deck displays,
so a push to the other orientation's slot does not interrupt the feeds. To load
bindings from a file instead, set **Tiles to feed > Layout file** to the full path
of the layout that the deck actually displays, then select **Save**. This loads
local bindings; it does not push a layout to the deck. Keep that file aligned with
your pushed layout.

### Connect the local status-line Source

1. Keep `td-statusline.exe` beside the Windows app, or `td-statusline` beside the
   Linux app. A source build needs the separate helper build in
   [app/README.md](../app/README.md#build-the-desktop-app). If no sibling helper is
   found, the displayed entry expects `td-statusline` on PATH.
2. In **Claude Code status line**, turn **On** and select **Save** at the bottom
   of Sources.
3. Check **Settings file**. The default follows `CLAUDE_CONFIG_DIR`, or
   `~/.claude/settings.json` when that variable is unset.
4. Select **Show diff**. Review the proposed status-line entry, then select
   **Install** to write it. This step changes that provider's settings file.
5. Use the provider so it emits a status-line update. Check that Sources shows a
   session and readings instead of **Waiting for a status line update**.

Install wraps an existing status-line command with `--then` so that command keeps
running. Before the first replacement of an existing file, it keeps a
`settings.json.touchdeck.bak` beside the settings file if no such backup exists.
Review the entry again if you move the app or helper.

For a widget, choose Source `claude-status`, leave Account empty, and set Field to
`model`, `context_pct`, `cost_usd`, `five_hour_pct`, or `seven_day_pct`. The app reads
the newest session file. Fields appear only when the status-line document supplies
them. Missing rate-limit fields are not inferred from account credentials. Data
older than ten minutes becomes stale.

### Connect CLIProxyAPI

1. Use an existing CLIProxyAPI service with its management API enabled. TouchDeck
   does not install the service or sign accounts into it.
2. Turn **On** in the CLIProxyAPI card and set **Base URL**. The default is
   `http://localhost:8317`. Set **Poll every (s)** as needed, then select **Save**.
3. Enter the service's management key and select **Save key**. This is separate
   from the Sources configuration Save. The key uses the OS credential store.
4. Select **Refresh now** and inspect the accounts table or the reported error.
5. In Editor, set a widget's Source to `cliproxyapi`. Set Field to `status`,
   `success`, `failed`, or `recent_10m`. Copy the exact **Account** value shown
   in Sources for a single account, or leave Account empty for the aggregate.
6. Push the matching layout and check the Sources feed status.

This Source reads `GET /v0/management/auth-files`. Counts reflect fields supplied
by that response. `recent_10m` uses its newest recent-request bucket. An individual
account's bar is relative to the busiest account, not a quota percentage. No
remaining quota or credit balance is provided. After three failed polls, cached
readings become stale and the deck dims the affected widgets. A widget tap asks
the Source to refresh. Imported CLIProxyAPI dashboard pages show up to eight
account rows. Other Source dashboards are not implemented.

## Back up layouts and restore them

1. Before replacing an Editor draft, select **Export** and retain the JSON file.
   Repeat for Landscape and Portrait.
2. To back up the deck's stored files, choose an orientation, select **Pull**, and
   accept **Replace**. Export that result, then repeat for the other orientation.
3. Check that each exported file exists in the save or download location. Reimport
   it to check the layout before relying on the backup.
4. Preserve custom icon cache files separately, along with any local layout files
   named in Sources. Record the idle timeout and saved orientation separately.
5. On the destination PC, import the JSON, review paths and actions, restore or
   re-extract custom icons, and push each orientation. Reconfigure Sources and
   review command approvals on that PC.

Export saves the selected orientation as formatted JSON. The app uses the webview's
save picker when available, otherwise a browser download. Check the actual file
location rather than relying on the **Exported** status alone. A layout export
does not include bitmap files, device settings, source configuration, management
keys, command approvals, or logs.

Sources configuration lives in `%APPDATA%/touchdeck/sources.json` on Windows and
`${XDG_CONFIG_HOME:-~/.config}/touchdeck/sources.json` on Linux. Status-line state
uses `%LOCALAPPDATA%/touchdeck/claude-status` or
`${XDG_STATE_HOME:-~/.local/state}/touchdeck/claude-status`. The app stores approvals
and icon caches in its separate per-user Tauri config and cache directories.
Moving the executable folder does not move these files. Re-enter the management
key through Sources on another PC; it is not in the JSON configuration.

## Update the desktop app

Desktop app updates and deck firmware updates are separate. This section covers
the app. The deck's firmware, including ROM recovery, is in
[Install or update deck firmware](#install-or-update-deck-firmware).

The app can check for its own updates in **Settings > App updates**:

1. Export open drafts from **Editor**. They are not saved when the app closes.
2. Open **Settings** and find **App updates**. It shows the installed app version
   and notes that deck firmware is updated in the **Update** tab.
3. Select **Check for updates**. The app reports **You're on the latest app
   version**, or the new version with its release notes. If it cannot reach a
   release, it shows the reason and you can keep using the current version.
4. Select **Download update**. TouchDeck stays open while it downloads the signed
   installer and verifies its signature.
5. Select **Disconnect** in **Deck**. The install button is disabled while a deck
   is connected, and installation is refused during deck transfers.
6. Select **Install and close TouchDeck**. Windows starts the installer and the
   app closes. Open TouchDeck again when the installer finishes.

An update keeps your per-user settings, command approvals, Sources configuration,
and icon cache. It does not change layouts stored on the deck or flash the board.
After restarting, reconnect in **Deck**, and review the Sources helper path if the
installation folder changed.

The app update path needs a published release signed with the project's updater
key. No release exists yet, so **Check for updates** finds no update until the
first signed release is published. Source builds carry no updater key and report
that release signing is not configured. The first
installation always comes from the installer on GitHub Releases, once a release is
published. Source and portable copies replace by hand: close the app, then rebuild
or extract the new copy.
