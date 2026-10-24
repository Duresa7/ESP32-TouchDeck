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
project, tested with ESP-IDF v6.1. Install ESP-IDF with the
[ESP-IDF Installation Manager](https://docs.espressif.com/projects/idf-im-ui/),
then either use the ESP-IDF VS Code extension (open this folder and click
Build, Flash and Monitor) or run these from an ESP-IDF terminal:

```sh
idf.py set-target esp32s3   # first time only
idf.py -p COM3 flash monitor
```

Replace `COM3` with your board's serial port. Press `Ctrl+]` to exit the
monitor. Board defaults (flash size, PSRAM) live in `sdkconfig.defaults`.
