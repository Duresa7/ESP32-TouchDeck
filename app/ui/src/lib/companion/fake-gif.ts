const FRAMES = 8;
const DELAY_CS = 10;
const CLEAR = 4;
const END = 5;
const CODE_BITS = 3;
const LOOP = [0x21, 0xff, 0x0b, ...[...'NETSCAPE2.0'].map((c) => c.charCodeAt(0)), 0x03, 0x01, 0x00, 0x00, 0x00];

const u16 = (n: number) => [n & 0xff, n >> 8];

function packCodes(pixels: Uint8Array): number[] {
  const out: number[] = [];
  let acc = 0;
  let bits = 0;
  const put = (code: number) => {
    acc |= code << bits;
    bits += CODE_BITS;
    while (bits >= 8) {
      out.push(acc & 0xff);
      acc >>= 8;
      bits -= 8;
    }
  };
  for (let i = 0; i < pixels.length; i += 2) {
    put(CLEAR);
    put(pixels[i]);
    if (i + 1 < pixels.length) put(pixels[i + 1]);
  }
  put(END);
  if (bits > 0) out.push(acc & 0xff);
  return out;
}

function subBlocks(data: number[]): number[] {
  const out: number[] = [];
  for (let at = 0; at < data.length; at += 255) {
    const chunk = data.slice(at, at + 255);
    out.push(chunk.length, ...chunk);
  }
  out.push(0);
  return out;
}

function frame(width: number, height: number, index: number): Uint8Array {
  const pixels = new Uint8Array(width * height);
  const bar = Math.max(2, Math.floor(width / 5));
  const x0 = Math.floor((index * (width + bar)) / FRAMES) - bar;
  const band = Math.floor(height / 3);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const inBar = x >= x0 && x < x0 + bar;
      const inBand = y >= band && y < height - band;
      pixels[y * width + x] = inBar ? (inBand ? 2 : 1) : inBand ? 3 : 0;
    }
  }
  return pixels;
}

/** A small, valid, looping GIF the fake Companion returns for a prepared GIF: a bar sweeping across in four colours picked by `seed`. */
export function fakeGif({ seed, width, height }: { seed: number; width: number; height: number }): Uint8Array {
  const tint = (shift: number, base: number) => base + ((seed >>> shift) & 0x3f);
  const palette = [
    [tint(0, 20), tint(6, 28), tint(12, 48)],
    [tint(18, 150), tint(24, 120), tint(2, 60)],
    [tint(8, 190), tint(14, 190), tint(20, 190)],
    [tint(4, 40), tint(10, 70), tint(16, 120)],
  ].flat();
  const out = [0x47, 0x49, 0x46, 0x38, 0x39, 0x61, ...u16(width), ...u16(height), 0x91, 0, 0, ...palette, ...LOOP];
  for (let i = 0; i < FRAMES; i++) {
    out.push(0x21, 0xf9, 0x04, 0x04, ...u16(DELAY_CS), 0x00, 0x00);
    out.push(0x2c, 0, 0, 0, 0, ...u16(width), ...u16(height), 0x00, 0x02, ...subBlocks(packCodes(frame(width, height, i))));
  }
  out.push(0x3b);
  return new Uint8Array(out);
}
