# ESP32 TouchDeck

A customizable touch deck built with an ESP32.

I made this project to get around the limitations of numpads and other
keyboards that lock useful features behind proprietary software or prevent
full customization. I want a flexible, hardware-controlled interface that I
can customize to fit the way I work.

## Use TouchDeck over USB

The desktop app edits layouts, runs PC actions, feeds widgets, and mirrors the
deck's key and media reports into your desktop session. Keep the app running and
connected over USB for typing. The deck does not provide a standalone USB HID
keyboard. Bluetooth is not supported. Wi-Fi, battery support, and standalone USB
HID are deferred to the post-product update.

Start with the [USB user guide](docs/usage.md) to obtain or build the app, install
firmware, connect, edit and push layouts, configure CLIProxyAPI usage limits, and back up your work.
Once a release is published, the Windows installer will be at
[TouchDeck-windows-x64-setup.exe](https://github.com/Duresa7/ESP32-TouchDeck/releases/latest/download/TouchDeck-windows-x64-setup.exe)
on [GitHub Releases](https://github.com/Duresa7/ESP32-TouchDeck/releases/latest).
No release has been published yet. Until one is, follow the
[desktop source-build instructions](app/README.md#build-the-desktop-app).

## Project goals

- Provide customizable controls without vendor lock-in.
- Support a flexible touch-based interface.
- Keep the project open to hardware and firmware improvements.

## Hardware

- Waveshare ESP32-S3-Touch-LCD-5B (ESP32-S3, 16 MB flash, 8 MB PSRAM,
  1024x600 touch display)

## Build from source

See the [firmware reference](docs/firmware.md) and [desktop app guide](app/README.md).

## License

TouchDeck is licensed under the [Apache License 2.0](LICENSE).
