# Use TouchDeck over USB

Use this guide to install TouchDeck, connect your deck, and set up its buttons.
You need the Waveshare ESP32-S3-Touch-LCD-5B, a USB data cable, and a Windows
or Linux desktop session.

Keep the app running and connected for keyboard and media keys, buttons that open
apps or run commands, and live AI usage. The app types the deck's key presses into
your computer. The deck does not act as a standalone USB keyboard. Swiping between
pages works on the deck without the app. Bluetooth is not supported. Wi-Fi, battery
support, and standalone USB HID (direct TinyUSB) are deferred to the post-product
update.

The app has three screens, chosen at the top: **Buttons**, where you set up the
deck, **AI usage**, and **Settings**. The deck indicator at the top right shows
whether your deck is connected and opens the list of decks.

## Obtain and install the desktop app

This section is for people who run the app. If you build it from source, use
[Build from source (developers)](#build-from-source-developers) instead.

**Windows installer.** Once a release is published, its installer will be at
[TouchDeck-windows-x64-setup.exe](https://github.com/Duresa7/ESP32-TouchDeck/releases/latest/download/TouchDeck-windows-x64-setup.exe)
on [the latest GitHub release](https://github.com/Duresa7/ESP32-TouchDeck/releases/latest).
No release has been published yet, so that link does not work today. When it does,
run the installer. It installs for your user, by default under
`%LOCALAPPDATA%/TouchDeck`, with `touchdeck.exe` and a `firmware/` folder with the matching `.tdfw` bundle. You need no Node, pnpm, Rust,
Python, or ESP-IDF. If Windows lacks the WebView2 runtime, the installer downloads
it, so that step needs network access. The [packaging guide](packaging.md) lists
the installer's contents.

Until a release exists, use the source build below. Linux has no installer. Run
`touchdeck` from a source build. The
[desktop setup guide](setup.md) lists the Linux runtime libraries, the serial and
`/dev/uinput` permissions, and the desktop credential store that the optional
CLIProxyAPI connection needs.

Start `touchdeck.exe` on Windows or `touchdeck` on Linux. The window title, and
**Settings > Troubleshooting**, show the source revision the app was built from.

### Build from source (developers)

Source builds are for development and for use before a release exists. On Windows,
run `pwsh -NoProfile -File app/scripts/setup-windows.ps1` from the checkout root.
It installs missing developer tools, so read [the setup guide](setup.md) first.
Then follow [Build the desktop app](../app/README.md#build-the-desktop-app). The
packaging recipe in the [packaging guide](packaging.md) produces installer
candidates. A candidate is not a published release.

## Connect your deck

1. Plug the board's USB-Serial/JTAG port into your computer with a data cable.
2. Start the app. If it finds the deck, the Buttons screen says
   **Your deck is plugged in**. Select **Connect** there.
3. Or select the deck indicator at the top right, which reads
   **Deck not connected**, and select **Connect** beside your deck.
4. The indicator turns green and reads **Deck connected**.

The deck list names what it found on each port. **Another program is using it**
means you must close the other app, serial monitor, or flashing tool first. On
Linux, also check serial-device permissions. **Didn't answer yet** does not prove
the firmware is healthy. Check the cable, the port, and the firmware before using
the deck's repair path. Select **Disconnect** in the same list before opening a
separate serial tool. If the connection drops, the indicator reads
**Connection lost**, and the list explains what to try.

**Settings > Troubleshooting** shows the connected deck's details: its software
version and build ID, which slot it runs from, safe mode, and the USB port.

Keep the app running after connecting. The deck's keys go to whichever window has
focus. If typing fails, check **Settings > Troubleshooting > Test your keys** and
the Linux `/dev/uinput` permissions described in the setup guide. Buttons that
open apps, run commands, or show AI usage grey out on the deck while the app is
offline. Key buttons do not grey out, but still need the connected app to type
into the computer.

## Set up your buttons

The Buttons screen draws your deck exactly as the deck draws it: the same
1024 by 600 screen (600 by 1024 in portrait), the same Montserrat letters at the
same sizes, the same colors, and the same rules for shrinking a long name. What you
see is what the deck will show, scaled to fit the window.

The app keeps a separate layout for **Landscape** and **Portrait**. These buttons
choose which layout you edit and which slot on the deck it goes to, not which way
the deck is turned.

1. Open **Buttons** and choose **Landscape** or **Portrait**.
2. With nothing open yet, pick a start: **Start from your deck** loads the
   buttons your deck has now, **Start with examples** gives you media controls and
   everyday shortcuts, and **Start blank** gives you one empty page. **Open a
   layout file…** opens a layout you saved before, such as
   [builtin.json](../schema/examples/builtin.json) for keys and media controls or
   [dev.json](../schema/examples/dev.json) for apps, commands, and AI usage.
3. Select a page at the top of the deck, then a button. Change its
   **Name on the button**, what it does under **When pressed**, and its
   **Picture** in the panel on the right.
4. Drag a button to move it, and drag its corner dot to resize it. With a button
   selected, the arrow keys move it, and Shift with an arrow key resizes it. Exact
   numbers are under **Position and size**. A move onto another button, or past the
   grid's edge, is refused and the status line says why.
5. Select an empty square, or **Add button**, to add a button there. **Add page**
   adds a page. With no button selected, the panel shows the page: its
   **Page name**, its **Grid** of rows and columns, and **Remove this page**, which
   asks you to select it a second time.
6. Before replacing what you have, open the **Layout** menu and select
   **Save to a file…**.
7. With the deck connected, select **Send to deck**. The status line says what the
   deck now shows, and whether any picture could not be sent.

The panel warns when a name uses a letter the deck can't show, such as an accented
letter or an emoji. The deck draws an empty box for each, and so does the preview.

### Try it before you send it

Select **Try it** (or press **P**) to use the preview the way you use the deck.
Swipe sideways, or press the left and right arrow keys, to change pages. The
pages slide with the deck's own timing. Tap a button to see its pressed glow, a
page button jumps to its page, and Ctrl, Shift, Alt, and Windows buttons show the
deck's held and locked looks. AI usage buttons and pages show example numbers.
**Actual pixels** shows one deck pixel per screen pixel. Select **Edit** to go back.

### Use the whole screen

With no button selected, the panel's **All pages** section has
**Use the whole screen**. Turn it on to hide the page name at the top left and the
page dots at the bottom, so the buttons fill the screen. You still swipe between
pages. The gap between buttons stays the same. An AI usage page with no rows yet
shows nothing at all on a full-screen layout, because it has no page name to show.

The setting is saved in the layout as `"fullScreen": true`, and the app writes it
only when it is on, so a layout without it is unchanged. It needs deck software
that knows the setting. Older deck software refuses such a layout, and the status
line's **Details** give the deck's reason. See
[full-screen.json](../schema/examples/full-screen.json) for an example.

### Opening, loading, and saving

Opening a file switches to the orientation written inside it.
**Layout > Start blank** replaces the current orientation's layout with an empty
one, and **Start with examples** with the example set. **Load from deck** reads the
selected orientation's layout from the deck. Select **Replace** to use it or
**Keep mine** to keep what you have. A deck that runs only its built-in layout may
have nothing to load. Open the built-in example in that case.

Sending writes the layout's orientation slot. Sending the orientation the deck
shows reloads it. Sending the other orientation saves it for later and leaves the
shown layout, its buttons, and its AI usage running. Layout writers refuse a file
whose declared orientation differs from the target slot. The CLI's optional
`put-layout --orientation` must match the JSON; edit the file's `orientation`
member to move it to the other slot. Save each orientation to a file separately.
The app has no undo or autosave, and it does not keep your layouts across restarts.

The panel names the layout under **All pages > Layout name**. Pages added in the
app are grid pages. To add an AI usage page, which lists every account, edit the
JSON outside the app and open it. See the [layout reference](../schema/README.md)
for limits and rules. From a source checkout, check examples and your own file
without running its actions:

```sh
npm ci --prefix schema
node schema/check-layouts.mjs path/to/layout.json
```

The checker covers the repository examples too. A valid layout fits within 32768
bytes of compact JSON. A file that fails to open leaves your layout as it was, and
the status line's **Details** shows the rule it broke.

## Change which way the deck stands and when its screen turns off

1. Send a layout for the orientation you want the deck to show.
2. Open **Settings > Your deck** and choose **Which way it stands**.
3. Set **Turn the screen off after** to a whole number of seconds from 1 to 86400.
   The hint below it says it in minutes and hours.
4. Select **Save to deck**. The screen timeout takes effect at once.
5. When offered, select **Restart deck now** to turn the screen. The status line
   says the deck needs a restart first.
6. Connect again once the deck's buttons are back.

A saved orientation takes effect only after the deck restarts. Until then the deck
keeps showing the previous orientation, and the app's buttons and AI usage follow
the layout the deck actually shows, not the saved setting. Sending a layout to the
other orientation's slot stores it without changing the shown layout, its buttons,
or live AI usage. Sending to the shown orientation reloads the screen and both.

A portrait file in storage does not turn the current screen. Portrait uses a
600 by 1024 screen with the panel turned 90 degrees clockwise. If the shown slot
has no valid layout, the deck shows its built-in layout. That layout has no
buttons that open apps or show AI usage. On reconnect the app swaps the previous
layout's actions and AI usage for that built-in layout.

## Buttons that work on your computer, and command approvals

Choose what a button does under **When pressed**, then send the layout. The choices
are grouped by where they happen:

- **Press a key or shortcut** picks a key (letters, digits, and F1 to F24) and the
  keys to hold down with it. The hint spells out the shortcut, such as
  **Ctrl + Shift + F5**.
- **Control music and volume** picks play or pause, next, previous, stop, mute, or
  volume up and down.
- **Hold Ctrl, Shift, Alt, or Windows** makes a modifier button. Tap it once and
  it's held for your next key. Tap it twice quickly and it stays on until you tap
  it again.
- **Open an app** starts the app at its path, with optional arguments and a folder
  to start in under **More options**.
- **Open a file, folder, or website** opens it the way double-clicking it would.
- **Run a command** runs a program with arguments, one per line, in an optional
  folder, and optionally through the system shell. Shell mode still passes
  arguments as data; it does not run the field as a shell script.
- **Go to another page** jumps to a page of the same layout.
- **Show AI usage** shows a number from your CLIProxyAPI hub, described below.
- **Nothing** shows the button but does nothing, which is handy as a label.

Only **Run a command** asks before it runs. The dialog shows the exact program,
its arguments, its folder, and whether it uses the system shell. Choose
**Run once**, **Always allow**, or **Don't run**. Always allow remembers that exact
command on this computer, and changing any of those four values asks again.
**Open an app** and **Open a file, folder, or website** run without asking, so
check a layout from someone else before using its buttons.

**Settings > Troubleshooting > Button activity** shows which layout the app uses
for these buttons and what happened at each press: started, finished, didn't work,
or not run. **Use a layout file instead…** loads local button actions for
troubleshooting. It does not send anything to the deck or change its screen. Use
**Send to deck** for the normal workflow, and keep local actions in step with the
shown layout. **Test your keys** shows each key the deck sends and what this
computer typed. To check a key, give a button F13, click in the box, and press it.

## Pictures on buttons

Select a button, then use **Picture**. **Symbol** offers the deck's built-in
media symbols. **Choose a picture** opens the searchable gallery that comes with
the app. **Suggested** offers a picture that fits what the button does. For an
**Open an app** button, **Use the app's icon** takes the icon from that app and
saves it on this computer.

Sending a layout also sends the pictures the deck lacks, if they are on this
computer. The status line says when a picture couldn't be found, and that button
shows only its name. A layout file stores `file:<id>` references, not the images.
**Load from deck** reads the layout and does not copy pictures back to this
computer.

To move a layout to another computer, prefer the built-in symbols or the gallery.
Take app icons again on the new computer, or copy their cache files separately,
and update the app paths for that computer. **Clear unused pictures from the deck**
deletes pictures that neither stored orientation nor the layouts open in the app
use. A saved file elsewhere is not protected by that check, so keep copies of
custom pictures before clearing.

## Show AI usage on the deck

AI usage buttons and AI usage pages show how much of your AI plans you've used.
TouchDeck reads the numbers from your CLIProxyAPI hub while the app runs. Open
**AI usage** to check readings and problems. While the deck is connected, the app
fills the AI usage buttons of the layout the deck shows, so sending to the other
orientation's slot does not interrupt them. To fill the buttons of a saved file
instead, set **Which buttons to fill > Layout file** to the full path of the
layout the deck actually shows, then select **Save**. This does not send a layout
to the deck. Keep that file in step with what you sent.

Earlier pre-release builds could wrap your Claude Code status line. Before removing
that build, run its old `td-statusline --unwrap` helper (with `--settings <file>` for
a custom settings file), or restore the `settings.json.touchdeck.bak` beside your
Claude settings. Current builds do not edit Claude settings.

### Connect CLIProxyAPI

TouchDeck shows your Codex and Claude usage limits through your own
[CLIProxyAPI hub](https://github.com/router-for-me/CLIProxyAPI).

1. Run CLIProxyAPI and enable its management API with a management secret key.
   Allow remote management only if the hub is not on this computer.
2. Sign your Codex and Claude accounts into CLIProxyAPI using its login commands
   or management center. TouchDeck does not handle provider sign-ins.
3. In **AI usage**, turn on **Connect to your CLIProxyAPI hub**, set
   **Hub address**, and select **Save**. A hub on this computer can use
   `http://127.0.0.1:8317`. Use HTTPS for a hub elsewhere; plain HTTP sends the
   management key unencrypted, and the page warns you. HTTPS uses the operating
   system's certificate trust store, including private CAs installed in the
   Windows certificate store.
4. Enter the hub's **Management key** and select **Save key**, then **Check now**.
   The key is kept in the computer's secure password store (Windows Credential
   Manager), not in the configuration file.
5. The table shows each account's plan, how much of each limit is used, and when it
   resets. On the Buttons screen, choose **When pressed > Show AI usage**, then what
   to **Show** and which **Account**. Copy an account from the table to show only
   that account. The account includes the service, such as
   `codex:alice@example.com` or `claude:alice@example.com`, so the same email can
   be used with both.
6. Send the layout to fill its AI usage buttons and pages.

The numbers a button can show are the 5-hour and 7-day limits used
(`five_hour_pct`, `seven_day_pct`), the time until each resets (`five_hour_reset`,
`seven_day_reset`), and the plan name (`plan`). Reset times are the time left at
the last check. Codex windows of up to one day use the 5-hour numbers; longer
windows use the 7-day numbers. **All accounts** shows the highest percentage for
each window. Plans and reset times belong to single accounts. A missing window is
unavailable, not zero.

Account status (`status`), requests that worked and failed (`success`, `failed`),
and requests in the last 10 minutes (`recent_10m`) are also available. The
all-accounts status is the worst one, and the all-accounts counts are summed.
`recent_10m` uses the newest bucket, with each account's bar relative to the
busiest account. Gemini and other services show status and counts only for now.
Disabled accounts are listed but not checked. AI usage pages show up to eight rows,
with the service, the account name, and the usage percentages.

The hub is checked every 120 seconds by default, and at most every 60. Each check
lists the hub's accounts and asks the hub to fetch usage from Codex and Claude, so
it makes real calls to those services. **Check now** and taps on AI usage buttons
share a 15-second limit; extra requests in that time are ignored. A failure on one
service affects only that account and shows a short reason. After three failed
checks, the numbers count as stale and the deck greys out its AI usage buttons.

Uninstalling leaves the management key in Windows Credential Manager. Select
**Forget key** in AI usage before uninstalling to remove it.

## Back up layouts and restore them

1. Before replacing a layout in the app, open **Layout** and select
   **Save to a file…**, and keep the JSON file. Repeat for Landscape and Portrait.
2. To back up what's stored on the deck, choose an orientation, select
   **Layout > Load from deck**, and choose **Replace**. Save that to a file, then
   repeat for the other orientation.
3. Check that each saved file exists where you saved it. Open it again to check the
   layout before relying on the backup.
4. Keep custom picture cache files separately, along with any local layout files
   named in AI usage. Note the screen timeout and saved orientation separately.
5. On the new computer, open the JSON, check paths and actions, restore or take
   again any custom pictures, and send each orientation. Set up AI usage again and
   review command approvals on that computer.

Saving writes the selected orientation as formatted JSON. The app uses the
webview's save dialog when it can, otherwise a browser download. Check the actual
file location rather than relying on the **Saved** message alone. A saved layout
does not include pictures, deck settings, AI usage settings, management keys,
command approvals, or logs.

AI usage settings live in
`%APPDATA%/io.github.duresa7.touchdeck/sources.json` on Windows and
`${XDG_CONFIG_HOME:-~/.config}/io.github.duresa7.touchdeck/sources.json` on Linux.
These are the app's per-user Tauri folders, which also hold approvals; the picture
cache is in the matching Tauri cache folder. Moving the app's folder does not move
these files. Enter the management key again in AI usage on another computer; it is
not in the JSON file.

## Install or update deck firmware

The desktop app and the deck's own software update separately. **Settings >
Deck software** takes a `.tdfw` bundle, not the desktop installer or the app update
metadata. Use the bundle that comes with the deck software you want for this
board. A published release is planned to list a `touchdeck.tdfw` asset, and the
Windows installer places it at `firmware/touchdeck.tdfw`. Its version and SHA-256
are recorded in `install-manifest.json`. Neither exists until a release is
published, so build a bundle yourself until then.

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

To update a deck that runs TouchDeck software:

1. Close serial monitors, `td-link`, and flash tools that use the deck's port.
2. Open **Settings > Deck software**.
3. Select **Choose file…** and pick the `.tdfw` file.
4. Keep the deck plugged in and select **Update deck**. The app sends the software,
   restarts the deck, and checks that the new software started.
5. Wait for **Done. Your deck is running the new software.**, or
   **Your deck already has this version** if it matches.
6. Connect again from the deck indicator.

The app closes its ordinary connection while the update owns the port. A normal
update writes the inactive application slot and checks the image before the
restart. The deck confirms a good start. An image that resets before confirming
can roll back to the previous slot, and the app says so. If the update doesn't
finish, the message says what happened and **Details** has the reason; read it
before trying again. **Update log** lists every step in the update's own words.
Reaching 100% alone does not prove a confirmed start.

If the deck does not run working TouchDeck software, use **Repair deck** under
**Repair the deck**, with the same bundle and the right **USB port**, such as
`COM3` or `/dev/ttyACM0`. It uses the ESP32-S3 ROM downloader over USB-Serial/JTAG
and writes the bundled bootloader, partition table, and application. It resets the
boot selection and keeps the saved-layout storage partition for the supported
partition map. Use it for a first install or broken software. It is not the
ordinary update path or a layout reset. It needs no separate Python or esptool.

The deck's ten-second touch hold is a different recovery feature. It starts the
built-in layout and default settings from RAM while keeping stored files. See
[Recovery](../README.md#recovery) for its behavior.

## Update the desktop app

This section covers the app. The deck's own software, including its repair path,
is in [Install or update deck firmware](#install-or-update-deck-firmware).

The app can check for its own updates in **Settings > This app**:

1. Save open layouts to files. They are not kept when the app closes.
2. Open **Settings > This app**. It shows the installed version.
3. Select **Check for updates**. The app says **You're on the latest app version**,
   or shows the new version with its release notes. If it cannot reach a release,
   it says why, and you can keep using the current version.
4. Select **Download update**. TouchDeck stays open while it downloads the signed
   installer and checks its signature.
5. Disconnect the deck from the deck indicator. The install button is off while a
   deck is connected, and installing is refused during deck transfers.
6. Select **Install and close TouchDeck**. Windows starts the installer and the app
   closes. Open TouchDeck again when the installer finishes.

An update keeps your per-user settings, command approvals, AI usage settings, and
picture cache. It does not change layouts stored on the deck or touch the deck's
software. After restarting, connect again.

The app update path needs a published release signed with the project's updater
key. No release exists yet, so **Check for updates** finds no update until the
first signed release is published. Source builds carry no updater key and say that
release signing is not configured. The first installation always comes from the
installer on GitHub Releases, once a release is published. Source and portable
copies are replaced by hand: close the app, then rebuild or extract the new copy.
