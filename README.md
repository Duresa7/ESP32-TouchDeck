# ESP32 TouchDeck

A customizable touch deck built with an ESP32.

I made this project to get around the limitations of numpads and other
keyboards that lock useful features behind proprietary software or prevent
full customization. I want a flexible, hardware-controlled interface that I
can customize to fit the way I work.

## Project goals

- Provide customizable controls without vendor lock-in.
- Support a flexible touch-based interface.
- Keep the project open to hardware and firmware improvements.

## Hardware

- Waveshare ESP32-S3-Touch-LCD-5B (ESP32-S3, 16 MB flash, 8 MB PSRAM,
  1024x600 touch display)

## Building

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

The commands are `help`, `version`, `heap`, `uptime`, and `reboot`, plus the
three the display adds, which `help` lists too. An unknown command answers with
`"ok":false` and an `error`.

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

The [framed app link](schema/protocol/link-v1.md) shares the debug console's
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
`link selftest` checks the [golden frames](schema/protocol/golden-frames.json)
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
`landscape`. The idle timeout reaches the power gate at boot. The orientation is
stored and reported, and the display applies it in a later milestone.

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
  timeout applies at once.

The local verification harness wraps the transfers as `serial store put <path>
--from <file>` and `serial store get <path> --out <file>`, checking the CRC-32
both ways.

## Display

On boot the panel shows the deck, drawn with LVGL 9: dark rounded tiles on a
near-black background, with page dots at the bottom. It has two pages,
`F-Keys` and `Media`, and a swipe left or right changes the page. A swipe past
either end bounces back. Tiles on `F-Keys` send F1 to F12, and Shift, Ctrl, Alt,
and Win arm a modifier for the next key. Tiles on `Media` send previous, play or
pause, next, mute, volume down, and volume up. The reports go through the key
engine to the console key log, because no USB keyboard exists yet. A page is
`{id, title, rows, cols, tiles[]}`, and the built-in pages are one `const` table in
`firmware/components/td_ui_core/ui_pages.c`. Every color, size, shadow, and
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

## Layout format

The versioned [layout format](schema/README.md) describes pages, tiles, icons,
and actions as JSON. [builtin.json](schema/examples/builtin.json) preserves the
milestone 1 pages. [dev.json](schema/examples/dev.json) covers the action types
and a dashboard for the PC app.

Validate the examples or your own layout with Node.js:

```sh
npm ci --prefix schema
node schema/check-layouts.mjs
node schema/check-layouts.mjs path/to/layout.json
node --test schema/
```

Every failure the validator prints starts with a stable rule key, such as
`label.blank`. [schema/README.md](schema/README.md) lists the keys and the rules
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
A `file:<id>` icon is a 64 by 64 bitmap in [schema/icon-format.md](schema/icon-format.md).

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

## License

TouchDeck is licensed under the [Apache License 2.0](LICENSE).
