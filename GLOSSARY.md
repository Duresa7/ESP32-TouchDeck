# Open TouchDeck

A touchscreen deck for your desk, and the desktop app that designs and drives it.

## Language

### Hardware

**Device**:
One physical Open TouchDeck. It has a name the user gives it, a hardware identity that never changes, and its own Layout.
_Avoid_: deck, your deck, board

**New board**:
A board connected to the computer that doesn't run Open TouchDeck Firmware yet. It becomes a Device once Firmware is installed and it's named.
_Avoid_: blank deck, recovery deck

**Firmware**:
The software running on a Device. It belongs to that Device and is updated from the app.
_Avoid_: deck software

### Computer

**Companion**:
The background program on the user's computer that runs the Devices, Integrations and actions, and serves the app's UI to the desktop window or a browser.
_Avoid_: daemon, service, backend, host

### Layout

**Layout**:
All the Pages of one Device, with their Tiles.
_Avoid_: config, profile, buttons

**Page**:
One screen of Tiles on a Device.

**Tile**:
One slot on a Page. It shows something and can run an action when tapped.
_Avoid_: key, slot

**Live tile**:
A Tile whose content keeps changing with data from an Integration, such as a graph or now playing.
_Avoid_: widget, dynamic tile

**Preset tile**:
A ready-made Tile that an Integration offers, such as a CPU graph, a volume bar or now playing.
_Avoid_: widget

**Asset**:
An icon, image or animation prepared for a Device and stored on it.
_Avoid_: resource, media file

### Integrations

**Integration**:
A connection to something outside the app that supplies data or accepts commands, such as a CLIProxyAPI hub, the PC, media playback, Proxmox or UniFi.
_Avoid_: source, plugin
