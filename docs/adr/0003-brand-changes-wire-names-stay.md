# The brand changes, but the wire names stay

The product is Open TouchDeck everywhere a user looks: the title bar, installer, About, docs and the Device's boot screen. Names users never see keep the old ones: the `touchdeck` product id in the USB handshake, `.tdfw` Firmware bundles, the `td-*` module names, and the GitHub repo `ESP32-TouchDeck`.

Changing the handshake id or the bundle format would break Devices running older Firmware. Keeping the repo name keeps the URL that v0.2.0 checks for updates working without a redirect.
