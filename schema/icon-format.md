# TouchDeck icon file format

A tile whose `icon` is `file:<id>` shows the bitmap stored on the deck at
`/storage/icons/<id>.bin`. `<id>` is the 16 lowercase hexadecimal digits from the
layout. Every icon file uses the `TDI1` format below. The deck refuses any other
file and draws the tile's label alone.

## Layout of the file

A `TDI1` file is exactly 12300 bytes. Every number is little-endian.

| Offset | Size | Field | Value |
| --- | --- | --- | --- |
| 0 | 4 | `magic` | The ASCII bytes `TDI1`. |
| 4 | 2 | `width` | `64` as u16. |
| 6 | 2 | `height` | `64` as u16. |
| 8 | 2 | `format` | `1` as u16, meaning RGB565 followed by A8. |
| 10 | 2 | `reserved` | `0` as u16. |
| 12 | 8192 | color | 64 by 64 RGB565 pixels, row-major from the top-left pixel. Each pixel is a u16. |
| 8204 | 4096 | alpha | 64 by 64 A8 values in the same order. `0` is transparent and `255` is opaque. |

The two planes match LVGL's `LV_COLOR_FORMAT_RGB565A8` with a stride of 128 bytes,
so the deck draws the bytes after the header as they are.

## Converting from 8-bit color

A packer converts each 8-bit RGBA pixel by truncation. It keeps the high bits of
each channel and copies alpha unchanged:

```text
rgb565 = ((r8 >> 3) << 11) | ((g8 >> 2) << 5) | (b8 >> 3)
a8     = a8
```

These rules match the screenshot harness, which recovers `r5 = r8 >> 3`,
`g6 = g8 >> 2`, and `b5 = b8 >> 3` from a PNG pixel.

## Golden file

[fixtures/icon-64.tdi](fixtures/icon-64.tdi) is the reference file. Every packer
must reproduce it byte for byte from the RGBA image below, where `x` and `y` run
from 0 to 63 and `/` is integer division:

```text
r8 = x * 255 / 63
g8 = y * 255 / 63
b8 = (x XOR y) * 255 / 63
a8 = 255 when 16 <= x < 48 and 16 <= y < 48
     160 when (x + y) is even, outside that square
     0   otherwise
```

The opaque 32 by 32 square in the middle lets a screenshot compare the drawn
pixels with the file's pixels without any blending. Its SHA-256 is
`be1044446a68262dd7fce4f01f9a711430a6944b6e646c017673a2b67c8e1245`.
