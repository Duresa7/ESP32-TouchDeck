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
adapter from the ESP-IDF component registry, so it needs a network connection.
`firmware/dependencies.lock` pins the versions.

## Serial console

The firmware runs a debug console on the board's USB port with the prompt
`td> `. Each command answers with one line, `@td ` followed by a JSON object
that holds `cmd`, `ok`, and the command's own fields:

```
td> version
@td {"cmd":"version","ok":true,"project_name":"touchdeck",...}
```

The commands are `help`, `version`, `heap`, `uptime`, and `reboot`. An unknown
command answers with `"ok":false` and an `error`.

## Display

On boot the panel shows a test screen of eight color bars and a `TouchDeck`
label, drawn with LVGL 9. The display component adds three console commands:

- `screenshot [1|2|4]` streams the screen as RGB565 in `@tdb ` base64 lines and
  ends with a reply that carries a CRC-32. Scale 2 or 4 keeps every second or
  fourth pixel.
- `perf fps <seconds>` redraws the whole screen every frame and replies with the
  frames per second when the time is up.
- `backlight on|off` switches the backlight through the CH422G expander.

These three are registered by the display component and do not appear in
`help`.

## Unit tests

`firmware/test_app` is an ESP-IDF project that runs the [Unity](https://docs.espressif.com/projects/esp-idf/en/latest/esp32s3/api-reference/system/unity.html)
tests of every component. A component adds tests in its own `test/` folder. List
the test sources in `test/CMakeLists.txt` as `set(TD_TEST_SOURCES test_x.c)`.
Run them on the board from an ESP-IDF terminal:

```sh
cd firmware/test_app
idf.py -p COM3 flash monitor
```

The monitor ends with Unity's summary, for example `8 Tests 0 Failures 0 Ignored`
and `OK`. Flash `firmware/` again afterwards to get the deck firmware back.

## License

TouchDeck is licensed under the [Apache License 2.0](LICENSE).
