# Firmware reference

This reference is for people who build or change the deck firmware.
To use Open TouchDeck, read the [user guide](usage.md) instead.

## Build and flash

This section builds and flashes firmware from source. To use a matching `.tdfw`
bundle through the app instead, follow [Install or update deck firmware](usage.md#install-or-update-deck-firmware).
Close the desktop app's connection before a flash tool or serial monitor opens
the same port.

The firmware is an [ESP-IDF](https://docs.espressif.com/projects/esp-idf/)
project in the `firmware/` folder, tested with ESP-IDF v6.1. Install ESP-IDF
with the
[ESP-IDF Installation Manager](https://docs.espressif.com/projects/idf-im-ui/),
then either use the ESP-IDF VS Code extension (open the `firmware/` folder and
click Build, Flash and Monitor) or run these from an ESP-IDF terminal:

```sh
cd firmware
idf.py set-target esp32s3   # first time only
idf.py -p COM3 flash monitor
```

Replace `COM3` with your board's serial port. Press `Ctrl+]` to exit the
monitor. Board defaults (flash size, PSRAM, CPU speed, LVGL options) live in
`firmware/sdkconfig.defaults`. The first build downloads LVGL and the LVGL
adapter and cJSON from the ESP-IDF component registry, so it needs a network connection.
`firmware/dependencies.lock` pins the versions.

## Serial console

The firmware runs a debug console on the board's USB port with the prompt
`td> `. Each command answers with one line, `@td ` followed by a JSON object
that holds `cmd`, `ok`, and the command's own fields:

```
td> version
@td {"cmd":"version","ok":true,"project_name":"touchdeck",...}
```

`help` lists the commands in this order:

| Command | Owner | What it does |
|---|---|---|
| `help` | console | Lists the console commands. |
| `version` | console | Reports the app descriptor. |
| `heap` | console | Reports free heap. |
| `uptime` | console | Reports milliseconds since boot. |
| `reboot` | console | Restarts the board. |
| `screenshot` | display | Streams the screen as RGB565 base64 lines, scale 1, 2, or 4. |
| `perf` | display | Measures frames per second. The reply arrives when it is done. |
| `backlight` | display | Turns the backlight on or off. With no argument it reports it. |
| `ui` | ui | Reports the deck and drives its pages and tiles. |
| `layout` | ui | Reports the loaded layout, reloads it from storage, or switches to the built-in one. |
| `touch` | touch | Injects taps and swipes, logs real touches, and reports touch statistics. |
| `keys` | keys | Drives the key engine, reads its state, logs and records its reports, and fuzzes the reducer. |
| `power` | power | Reports the idle screen-off state, or sets the idle timeout until the next reset. |
| `link` | link | Reports the app link state. |
| `store` | storage | Inspects, reads, and writes the files on the storage partition. |
| `settings` | storage | Reads or changes the settings that persist across resets. |
| `@tds` | storage | Carries one base64 line of a `store put`. The host sends it after the put. |
| `recovery` | storage | Reports the recovery hold watcher, or arms recovery and restarts. |
| `update` | update | Reports the firmware slots and update state. |

An unknown command answers with `"ok":false` and an `error`.

A component adds commands with `td_console_register`, which takes an array of
`esp_console_cmd_t`. `app_main` calls it for every component before
`td_console_start`, which seals the list. A registration after that fails with
`ESP_ERR_INVALID_STATE`.

Replies leave through the USB driver and wait for the host when its buffer is
full, so a redrawing display cannot drop them. The prompt and typed echo use
the same writer. Log lines still go through stdio and can be dropped.

Input waits in a 2048-byte queue while a command runs. The queue accepts 2047
bytes, which holds about 250 short lines or seven full-length ones. When a burst
fills it, the firmware drops the next read of up to 64 bytes and then everything
through the next CR or LF, so a line that a drop cut never runs. It reports the
loss before the next prompt:

```
@td {"cmd":"console","ok":false,"error":"input overflow","dropped_bytes":N}
```

`dropped_bytes` counts every byte the firmware threw away, the cut line
included. It counts the rest of a cut line as those bytes arrive, which can be
after the first notice printed, so one loss can print several of these lines.
Add up every one.

The firmware cannot count a loss inside the USB driver. The driver's interrupt
copies each USB packet into a 256-byte ring and ignores the result of
`xRingbufferSendFromISR`. If `td_rx`, the task that empties the ring, cannot
run while about four full packets arrive, the next packet is lost with no
count.

When no line is blank and every line ends in one CR or one LF, the bytes of the
lines that got a reply, line endings included, plus the `dropped_bytes` of every
notice equal the bytes you sent. CR LF breaks that, by up to one byte each time
the queue overflows, in either direction:

- The dropped input can end between a line's CR and its LF. The console reads
  that LF as an empty line, which nothing counts, so the sum comes up one byte
  short.
- The dropped input can start on the LF of a line that already got a reply. That
  LF is counted twice, so the sum runs one byte over.

To check the sum exactly, end every line with LF. A script that sends a burst
should wait for replies after about 250 short lines.

The console reads the USB-Serial/JTAG driver, so it needs USB-Serial/JTAG as
the primary console, which `firmware/sdkconfig.defaults` sets. With another
primary console the firmware still boots, logs that the console is off, and
does not start it.

## App link

The [framed app link](../schema/protocol/link-v1.md) shares the debug console's
USB-Serial/JTAG port. SLIP frames carry CRC-protected payloads of up to 1024
bytes. The `hello`, `ping`, and `link.state` messages identify the running build,
echo a request, and report the connection state. The text command `link state`
reports the same counters. The port stays available for the debug console and
ROM flashing.

The link also moves files and mirrors input. `layout.put` and `layout.get` copy a
layout in chunks, `icon.put`, `icon.list`, and `icon.rm` manage the icons, and
`settings.get`, `settings.put`, and `store.stat` read and change the deck. A put
is checked and validated before it replaces the stored file, and a layout for the
orientation on screen is shown without a reboot. While the app is connected, the
deck sends a `press` event for each app or widget tap and a `key.report` event
for each key report the engine emits, so the app can type the keys on the PC.
`widget.set` fills a widget tile with a value, a caption, and a bar, and
`dashboard.set` fills a dashboard page with up to 8 rows. The deck draws them on
its next 100 ms redraw.

The local verification harness provides `link hello`, `link ping --count 100`,
`link state`, `link raw --hex <frame>`, and `link selftest`. `link scenario <file>`
sends timed console lines and binary frames through one port handle. Console
and link tasks reply independently, so callers match each channel separately.
`link selftest` checks the [golden frames](../schema/protocol/golden-frames.json)
without a board. Live checks still require the board.

## Storage and settings

The `storage` partition holds one [LittleFS](https://github.com/joltwallet/esp_littlefs)
volume, mounted at `/storage` before the console starts. The first boot after the
partition was erased formats it and logs `store: formatted`, and every boot logs
`store: mounted, <used>/<total> bytes`. Flashing the firmware leaves the volume
alone. The paths the firmware uses are `/storage/settings.json`,
`/storage/layouts/<orientation>.json`, `/storage/icons/<id>.bin`, and
`/storage/recovery`.

A write goes to `<path>.tmp`, is flushed with `fsync`, and is renamed over the
target. LittleFS makes the rename atomic across power loss, so a reader sees the
whole old file or the whole new one. A temporary file that a power cut left behind
is removed at the next mount.

Settings are `{"idle_timeout_s": 600, "orientation": "landscape"}`. The deck reads
them before the display starts and logs `settings: idle_timeout_s=<n>
orientation=<o>`. An unknown field is ignored, a missing field takes its default,
a timeout outside 1 to 86400 takes 600, and an unknown orientation takes
`landscape`. The idle timeout reaches the power gate at boot. The orientation
applies at boot. In `portrait` the screen is the panel turned 90 degrees
clockwise, 600 by 1024, and the deck loads `/storage/layouts/portrait.json`, or
the built-in layout when that file is missing. Touch points, `touch` and `ui hold`
injection, and `screenshot` all use the rotated size. The adapter rotates each
frame in software, so portrait draws much slower than landscape.

The `store` and `settings` console commands work on the volume:

- `store stat [path]` reports the volume (`total`, `used`, `files`, `dirs`), a
  directory's counts, or a file's size.
- `store ls <dir>` lists a directory, `store rm <path>` removes a file or an empty
  directory.
- `store put <path> <bytes> <crc32>` takes the file from the host as `@tds ` base64
  lines of 180 raw bytes, up to 65536 bytes. The deck holds the file in memory and
  swaps it in atomically after its last line. It acknowledges every 6 lines, so the
  host never sends more than one window ahead.
- `store get <path>` streams a file as `@tds ` lines between a `start` and an `end`
  reply that carries its CRC-32.
- `settings get` and `settings set idle_timeout_s <n>` or
  `settings set orientation landscape|portrait` read and write the file. A new
  timeout applies at once. A new orientation applies on the next boot, so its
  reply, like `settings.put`'s, carries `reboot_required` true.

The local verification harness wraps the transfers as `serial store put <path>
--from <file>` and `serial store get <path> --out <file>`, checking the CRC-32
both ways.

## Recovery

Holding one finger on the glass for 10 seconds restarts the deck into a known-good
state, whatever the deck shows and whether or not the backlight is on. The hold
arms when one contact stays within 24 px of where it began for 10000 ms. A
release before that, or a move past 24 px, starts over. The recovery watcher reads
the raw contact before the idle gate, so a hold that begins on a dark screen
counts from its first sample, and that same contact wakes the panel.

When the hold arms, the deck stops sending the contact to the pages, so no tile
fires when the finger lifts. It draws a full-screen `Release to reset` band, logs
`recovery: armed`, writes the marker file `/storage/recovery`, and restarts 250 ms
after the lift.

A boot that finds the marker deletes it and runs in recovery mode. The deck shows
the built-in pages and the default settings from RAM, leaves every file in
`/storage` as it was, logs `recovery: mode on, files kept`, and shows a `Recovery`
caption in the title band. The link's `hello` and `layout state` report
`"recovery": true`, and `layout state` still names what is wrong with a stored
layout that does not parse. A layout or settings write by the app, a
`layout use storage`, or a restart leaves recovery mode. A plain `layout reload`
in recovery mode reloads the built-in pages.

The `recovery` console command has two forms:

- `recovery state` replies with `mode`, `watcher` (`IDLE`, `HOLDING` or `ARMED`),
  `held_ms`, and `armed_count`.
- `recovery arm` writes the marker and restarts, as the hold does after the lift. It
  is for tests.

A swipe lasts at most 2000 ms, so `touch begin <x> <y>`, `touch move <x> <y>` and
`touch end` hold an injected contact for as long as a test needs and return at
once. A contact that nobody ends lifts by itself after 60 seconds. `touch stats`
adds `watch_samples`, `watch_ns_avg`, and `watch_ns_max`, which is what the watcher
costs per sample.

## Firmware updates

The app updates the deck's firmware over the link, and the deck rolls back by
itself when the new image does not come up. The flash holds two application slots,
`ota_0` and `ota_1`, of 4 MB each. An update writes the slot the deck is not
running, so the running image stays whole until the very end.

`update.begin {bytes, sha256}` opens the other slot and `update.end` finishes it, with
the image carried by the same chunk transfer as a layout. At `update.end` the deck
checks the SHA-256 of the bytes it wrote, validates the image, sets the new slot as
the boot slot, replies with its name, and restarts 250 ms later. A refusal replies
with `reason` and leaves the boot slot alone: `size`, `sha256`, `incomplete`,
`flash`, `busy`, `unconfirmed`, or `project`. A transfer that stops moving for 2
seconds is dropped, and the update returns to `IDLE`.

The new image boots as `PENDING_VERIFY`. It confirms itself after the first frame,
a layout load, and 10 seconds of uptime, and logs `update: confirmed <slot>`. A
reset before that, from a crash, a watchdog, or a power cut, makes the bootloader
mark the slot `ABORTED` and boot the previous one. While an image waits to confirm,
the deck refuses another update.

The `update` console command has three forms:

- `update state` replies with `running`, `boot`, the state of `ota_0` and `ota_1`
  (`NEW`, `PENDING_VERIFY`, `VALID`, `INVALID`, `ABORTED`, `UNDEFINED`, or `NONE`
  when the slot has no record), `confirmed`, and the update `state`.
- `update confirm` confirms the running image at once. It is for tests.
- `update invalidate` marks the running image invalid and reboots into the other slot,
  when that slot holds a valid image. It is for tests.

`firmware/tools/make_bundle.py` packs a build into `touchdeck-<version>.tdfw`, a zip
with `bootloader.bin`, `partition-table.bin`, `touchdeck.bin`, and a `manifest.json`
that names each file's flash offset, size, and SHA-256 with the image's version and
`elf_sha256`. `python firmware/tools/make_bundle.py --check <bundle>` reads one back.

A flash over the cable writes `ota_0` and resets the boot slot to it, so
`idf.py flash` always recovers the deck. The local verification harness provides
`link update <touchdeck.bin> --wait` and `idf build-crashing`, a build that aborts
two seconds after start, for checking the rollback on the board.

## Display

On boot the panel shows the deck, drawn with LVGL 9: dark rounded tiles on a
near-black background, with page dots at the bottom. It has two pages,
`F-Keys` and `Media`, and a swipe left or right changes the page. A swipe past
either end bounces back. Tiles on `F-Keys` send F1 to F12, and Shift, Ctrl, Alt,
and Win arm a modifier for the next key. Tiles on `Media` send previous, play or
pause, next, mute, volume down, and volume up. The reports go through the key
engine to the console key log and the USB app link. The connected desktop app
mirrors them into the host session. Built-in pages come from embedded JSON in
[builtin.json](../schema/examples/builtin.json), and saved layouts use the same
[layout format](../schema/README.md). Every color, size, shadow, and
animation time of the look is a named token in `firmware/components/ui/ui_theme.c`.

The `ui` component adds the `ui` console command: `ui state` reports the page and
the tile that is pressed, `ui goto <page>` changes the page, `ui tile <id>` gives a
tile's box, and `ui hold <x> <y> <ms>` holds a press on the glass for a test.
`ui state` also reports `online`, whether the app is connected, and `dimmed`, how
many tiles are drawn dim. `ui widget <tile>` and `ui dashboard <page>` report what
the app last sent to a widget tile or a dashboard page.

The display component adds three console commands:

- `screenshot [1|2|4]` streams the screen as RGB565 in `@tdb ` base64 lines and
  ends with a reply that carries a CRC-32. Scale 2 or 4 keeps every second or
  fourth pixel.
- `perf fps <seconds>` redraws the whole screen every frame and replies with the
  frames per second when the time is up.
- `backlight on|off` switches the backlight through the CH422G expander.

The display component hands its commands to `td_console_register`, so `help`
lists them. Until the display has started, `screenshot` and `perf` answer
`display not ready`.

### Fonts

The deck draws its text in six Archivo fonts, generated as LVGL C files into
`firmware/components/ui/fonts/`, with the font's
[SIL Open Font License](../firmware/components/ui/fonts/OFL.txt) next to them.
[docs/design.md](design.md#fonts-on-the-device) says which instance, size and
glyph set each one has. The `lv:` icons are LVGL symbols, which only LVGL's own
Montserrat 48 carries, so they still draw in it.

The source is `Archivo[wdth,wght].ttf`, version 2.001, from the upstream
repository at commit `b5d63988ce19d044d3e10362de730af00526b672`:
<https://github.com/Omnibus-Type/Archivo/blob/b5d63988ce19d044d3e10362de730af00526b672/fonts/variable/Archivo%5Bwdth,wght%5D.ttf>.
`OFL.txt` comes from the same commit.

To regenerate them, install fontTools 4.60.1 (`pip install fonttools==4.60.1`)
and lv_font_conv 1.5.3 (`npm install lv_font_conv@1.5.3`). Then cut the three
instances and convert each font in a folder of its own, and copy the six `.c`
files into `firmware/components/ui/fonts/`:

```sh
fonttools varLib.instancer "Archivo[wdth,wght].ttf" wght=520 wdth=100 -o Archivo-Medium.ttf
fonttools varLib.instancer "Archivo[wdth,wght].ttf" wght=620 wdth=100 -o Archivo-Semibold.ttf
fonttools varLib.instancer "Archivo[wdth,wght].ttf" wght=620 wdth=118 -o Archivo-Nameplate.ttf

R="-r 0x20-0x7E -r 0xA0-0xFF -r 0x2013-0x2014 -r 0x2018-0x2019 -r 0x201C-0x201D -r 0x2022 -r 0x2026 -r 0x2212"
conv() { lv_font_conv --font "$1" --size "$2" --bpp 4 --format lvgl --no-compress --lv-include lvgl.h $3 -o "$4.c"; }
conv Archivo-Medium.ttf 22 "$R" ui_font_medium_22
conv Archivo-Medium.ttf 26 "$R" ui_font_medium_26
conv Archivo-Medium.ttf 44 "-r 0x20-0x7E" ui_font_medium_44
conv Archivo-Semibold.ttf 32 "$R" ui_font_semibold_32
conv Archivo-Semibold.ttf 40 "$R" ui_font_semibold_40
conv Archivo-Nameplate.ttf 26 "$R" ui_font_nameplate_26
```

`--no-compress` stores the glyphs as plain 4-bit bitmaps, as LVGL's built-in
fonts do, so drawing a glyph reads it straight from flash with no
decompression. `--lv-include lvgl.h` makes the files include `lvgl.h` the way the
ESP-IDF LVGL component provides it. The same tool versions and commands give
the same files byte for byte.

### Performance options

Five Kconfig options make the firmware faster or lighter without changing what
the screen shows. Each is on by default, and menuconfig can turn it off:

- `CONFIG_TD_PAY_COPY_DEBT`, in `TouchDeck display`. In the adapter's
  triple-partial mode a refresh that redraws most of the screen, such as a page
  slide's frame, leaves the two other frame buffers behind, and each of the next
  two refreshes copies most of the screen into one of them, about 60 ms each.
  With this option LVGL redraws one unchanged pixel twice once it has been idle
  for 120 ms, so those copies happen before the next touch. A tap half a
  second after a slide then finishes its frame in about 7 to 24 ms instead of
  62 to 66 ms.
- `CONFIG_TD_LV_TICK_CB`, in `TouchDeck display`. LVGL reads its millisecond
  clock from `esp_timer_get_time()`, so the adapter's 1 ms tick timer no longer
  wakes the `esp_timer` task a thousand times a second. Core 0 is about a third
  less busy on a static screen.
- `CONFIG_TD_LAZY_SNAPSHOT_BUF`, in `TouchDeck display`. The 1.2 MB PSRAM buffer
  that `screenshot` renders into exists only while a screenshot or
  `perf fb check` runs.
- `CONFIG_TD_LAZY_KEY_RECORD`, in `TouchDeck keys`. The 40 KB key record is
  allocated by the first `keys record start` instead of at boot. Before that,
  `keys record dump` answers with a count of 0.
- `CONFIG_TD_UI_SKIP_NOOP_SETS`, in `TouchDeck UI`. A widget or dashboard update
  sets a label's text or a widget's text opacity only when the label would
  change, so an update that repeats the shown values draws no frame. A label
  cut with dots is always set again, so a repeated value too long for its tile
  or row still redraws.

### Perf tools (test builds only)

`CONFIG_TD_PERF_TOOLS`, in the menu `TouchDeck perf tools (test builds only)`,
adds measurement commands for performance work. It stays off in `sdkconfig` and
`sdkconfig.defaults`, and while it is off the firmware compiles to the same code
as without it. A perf build goes to a directory of its own with a config of its
own, on top of the shipped defaults. From `firmware/`:

```sh
idf.py -B <abs>/build-perf -D SDKCONFIG=<abs>/build-perf/sdkconfig \
       -D "SDKCONFIG_DEFAULTS=sdkconfig.defaults;perf/tools.defaults" build
```

Flash it with the same `-B` and `-D` options and `-p <port> flash`.
[tools.defaults](../firmware/perf/tools.defaults) turns the tools on, with
FreeRTOS run-time stats on `esp_timer`. A build that also changes other options
lists its own defaults file after it and names itself in
`CONFIG_TD_PERF_VARIANT`. A perf build never formats a storage volume that will
not mount. It logs an error and boots without storage, so the layout and
settings on the deck stay as they are. `perf` then also takes:

- `perf info` reports the build: PSRAM, flash, and CPU clocks, the panel clock,
  bounce buffer lines, frame buffers, tear mode, rotation, LVGL draw units and
  assembly, refresh period, cache sizes, the variant name, which performance
  options are on (`lv_tick_cb`, `lazy_snapshot_buf`, `lazy_key_record`,
  `pay_copy_debt`), `reset_reason`, and `boot_count`, the boots since power-on.
  With `CONFIG_TD_PAY_COPY_DEBT` on, `copy_debt_steps` counts the idle redraws
  it has asked for since boot.
- `perf fps <seconds>` keeps its fields and adds `shown`, `shown_fps`,
  `refreshes`, `render_us_avg`, `render_us_max`, `render_only_us_avg`,
  `render_only_us_max`, `copy_us_avg`, `copy_us_max`, `flush_us_avg`,
  `flush_us_max`, `px`, `cpu0_pct`, and `cpu1_pct`.
- `perf window start` and `perf window stop` report the same over any stretch,
  with `elapsed_ms` and `drawn`, and force no redraw.
- `perf tap <x> <y>` taps like `touch tap` and replies with `press_to_flush_us`,
  `press_to_shown_us`, `release_to_flush_us`, and `release_to_shown_us`, each null
  when no redraw followed within 500 ms. `press_render_us`, `press_copy_us`,
  `press_strips`, and `press_px`, and the same for `release`, describe the
  refresh that flushed. The tap's press and release samples usually reach LVGL
  in one input read, about 1.4 ms apart, so both name the same refresh. In the
  adapter's triple-partial mode the first two partial refreshes after a
  full-screen redraw, such as boot or `perf fb prep`, each copy the whole screen
  into a lagging back buffer, about 60 ms, which `press_copy_us` shows, unless
  `CONFIG_TD_PAY_COPY_DEBT` paid it while idle. `shown_chained` is true, because
  the hook that takes `shown` calls the adapter's own frame-complete callback
  after it.
- `perf mem` reports free, minimum free, and largest block of internal, PSRAM, and
  DMA heap, and each task's core, priority, stack size, and stack high-water mark.
  Minimum free sizes count from the start of `app_main`, because ESP-IDF gives
  the 32 KB internal pool it reserves after startup
  (`CONFIG_SPIRAM_MALLOC_RESERVE_INTERNAL`) a minimum of 0, which would put the
  internal minimum 32 KB below anything in use. `perf mem boot` lists free,
  minimum free, and largest free internal RAM and free PSRAM after each boot
  stage. `perf mem heaps` prints ESP-IDF's per-heap summary of internal RAM to
  the log and replies with the totals. `perf mem last` reports what the previous
  boot saved when it restarted: its latest sample, taken every two seconds, of
  each heap's free and minimum free size and each task's stack high-water mark,
  with `boot`, the boot that saved it, and `uptime_ms`, the sample's time. When
  the previous boot saved nothing it replies `"saved":false`, with `stale_boot`
  when an older boot's record is left. A panic, a watchdog, or a power cut saves
  nothing.
- `perf cpu <seconds>` replies when the time is up with each core's busy share and
  each task's share of one core.
- The task lists of `perf mem`, `perf mem last` and `perf cpu` read each name
  from the running system without stopping the other core. A task that ends at
  that moment is left out or shows a garbled name. The read stops at the name's
  length limit, and the figures beside it are unaffected.
- `perf fb prep` redraws the whole screen three times, so all three panel frame
  buffers hold the same frame. `perf fb check` redraws nothing. It takes a
  snapshot, copies each panel buffer to PSRAM with the LVGL lock held, and
  compares it with the snapshot, turned the way the adapter turns it in portrait.
  Per buffer it replies with `index`, `crc`, `exact`, `max_diff` (the largest
  RGB565 channel step), and `diff_pixels`, plus `submits`, the frames handed to
  the panel since the last prep, and `screen changed` when a frame flushed in
  between. Each copy holds the lock for about 60 ms.
- `perf invalidate <x> <y> <w> <h>` marks one area of the screen for redraw.
- `perf stall <ms>` suspends the LVGL task for up to 10 s and replies at once,
  so a `screenshot` sent next runs into its timeout.
- `perf memtest <kb>` writes six patterns, among them each word's own address,
  over a PSRAM block of its own, reads them back from PSRAM, and replies with
  `errors`, `bytes`, and `us`. It touches no other memory. `kb` goes up to 4096
  and to the largest free PSRAM block that `perf mem` reports.
- `perf membw` replies with memcpy rates in MB/s, SRAM to PSRAM, PSRAM to SRAM,
  and PSRAM to PSRAM, measured on core 1 with the panel running. Its 4 KB SRAM
  buffer shows in the internal minimum free size afterward.
- `perf temp` replies with the chip temperature in degrees Celsius as `temp_c`.
  The first reading turns the sensor on, and it stays on until the next reset.

`drawn` counts LVGL refreshes that flushed, `shown` the panel refreshes that
scanned out a new frame, and `refreshes` every panel scan-out, about 24 a second.
`render` runs from the start of an LVGL refresh to its last flush call, and
`flush` from that call to flush ready. LVGL draws a refresh in strips and the
adapter copies each strip into the frame buffer before the next is drawn, so
`render` includes those copies. `render_only` leaves them out, and `copy` adds up
every strip's flush, the last one's included. `px` counts the pixels flushed. A
core's busy share is 100 % less its idle task's share of run time, and interrupt
time counts toward the task it interrupted.
`firmware/components/display/display_perf.c` says how each figure is taken.

## Layout format

The versioned [layout format](../schema/README.md) describes pages, tiles, icons,
and actions as JSON. [builtin.json](../schema/examples/builtin.json) preserves the
milestone 1 pages. [dev.json](../schema/examples/dev.json) covers the action types
and a dashboard for the PC app.

Validate the examples or your own layout with Node.js:

```sh
npm ci --prefix schema
node schema/check-layouts.mjs
node schema/check-layouts.mjs path/to/layout.json
node --test schema/
```

Every failure the validator prints starts with a stable rule key, such as
`label.blank`. [schema/README.md](../schema/README.md) lists the keys and the rules
for reading a layout file. Send the deck the compact form of a layout, because
it rejects files over 32768 bytes without parsing them first.

App-side actions and widgets dim on the deck while the app is offline. They
draw at 40 percent opacity, and a tap on one logs `ui: offline <tile>` and sends
nothing. Key, modifier, media, and page tiles never dim.

### Layouts on the deck

The firmware embeds `builtin.json` and draws its pages from it, so the deck has
no page table in C. At boot it loads `/storage/layouts/<orientation>.json`. When
that file is missing or invalid, the deck logs `layout: <reason>, using builtin`
and starts on the built-in layout. Its parser accepts and rejects the same files
as `check-layouts.mjs`, and it reports the same rule key.

- `layout state` reports the layout on screen: `source` (`storage` or `builtin`),
  `name`, `orientation`, `pages`, `tiles`, `icons_loaded`, `icons_missing`,
  `bytes`, `load_ms`, and `error`.
- `layout reload` and `layout use storage` load the stored file again.
  `layout use builtin` switches to the built-in layout. A file that fails
  validation never replaces the layout on screen. `layout state` reports why in
  `error`.
- `ui tile <id>` also reports the tile's `action` type and its `icon`.

A `key` tile with `mods` sends its modifiers together with any active ones. A
`page` tile slides to its page. App and widget tiles send a `press` event over
the app link, and log `ui: press <page>/<tile> <kind>` when no link hook is set.
A `file:<id>` icon is a 64 by 64 bitmap in [schema/icon-format.md](../schema/icon-format.md).

## Unit tests

`firmware/test_app` is an ESP-IDF project that runs the [Unity](https://docs.espressif.com/projects/esp-idf/en/latest/esp32s3/api-reference/system/unity.html)
tests of every component. A component adds tests in its own `test/` folder. List
the test sources in `test/CMakeLists.txt` as `set(TD_TEST_SOURCES test_x.c)`.
Run them on the board from an ESP-IDF terminal:

```sh
cd firmware/test_app
idf.py -p COM3 flash monitor
```

The monitor ends with Unity's summary, `N Tests 0 Failures 0 Ignored`, and `OK`.
Flash `firmware/` again afterwards to get the deck firmware back.

## Protocol v2 identity

USB discovery retains the v1 hello and advertises protocols 1 and 2. Selecting
protocol 2 adds the factory MAC identity, persistent Device name, Firmware
version, and capabilities. Identity discovery compares the id with the USB
serial after removing colons and hyphens and folding ASCII case; a mismatch
refuses association. The legacy discovery API remains available.

The `td-link` CLI provides `discover-identities`, `identity`, `name`, and
`set-name "Studio"`. Names default to `Open TouchDeck`, contain 1–48 UTF-8
bytes, and persist in NVS separately from Layouts. See the
[protocol contract](../schema/protocol/link-v2.md) for validation rules.
Only static Tiles and the existing tdi1 format are advertised. Asset, live,
photo, idle, and clock requests remain unsupported in this slice.

The Firmware test app links the link dispatcher to exercise maximum-size hello
replies and NVS persistence. Its pinned UI dependencies and LVGL snapshot and
font settings match the components linked by the production Firmware.
