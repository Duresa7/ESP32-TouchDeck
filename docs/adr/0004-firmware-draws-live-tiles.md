# The Firmware draws live Tiles; the computer never streams frames

The Companion never renders the Device's screen and streams it over USB. Instead:

- **Icons, images and GIFs** are resized by the Companion to the Device's Tile size, stored on the Device, and played by the Firmware.
- **Live Tiles** (graphs, the volume bar, now playing) receive small value updates, about once a second, and the Firmware draws them.
- **Photo frame pictures** are streamed one compressed picture at a time from a folder the user picks, and are not kept on the Device. When the Companion isn't running, the photo frame shows a clock.

Streaming rendered frames would be simpler to build but doesn't fit the link. One full 1024×600 frame at 16-bit color is about 1.2 MB, about a second's worth of USB-Serial/JTAG bandwidth, and the same link carries taps and actions.

The cost is that every new kind of live Tile needs Firmware support. Older Firmware shows such Tiles as "Needs Firmware x.y".
