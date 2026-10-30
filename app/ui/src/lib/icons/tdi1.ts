export const TDI1_SIDE = 64;
const HEADER = 12;
const COLOR = TDI1_SIDE * TDI1_SIDE * 2;
const FILE_BYTES = HEADER + COLOR + TDI1_SIDE * TDI1_SIDE;
const HEAD = [0x54, 0x44, 0x49, 0x31, 64, 0, 64, 0, 1, 0, 0, 0];

export function isTdi1(file: Uint8Array): boolean {
  return file.length === FILE_BYTES && HEAD.every((byte, i) => file[i] === byte);
}

export function decodeTdi1(file: Uint8Array): Uint8ClampedArray | null {
  if (!isTdi1(file)) return null;
  const rgba = new Uint8ClampedArray(TDI1_SIDE * TDI1_SIDE * 4);
  for (let i = 0; i < TDI1_SIDE * TDI1_SIDE; i++) {
    const value = file[HEADER + i * 2] | (file[HEADER + i * 2 + 1] << 8);
    const r = value >> 11;
    const g = (value >> 5) & 0x3f;
    const b = value & 0x1f;
    rgba[i * 4] = (r << 3) | (r >> 2);
    rgba[i * 4 + 1] = (g << 2) | (g >> 4);
    rgba[i * 4 + 2] = (b << 3) | (b >> 2);
    rgba[i * 4 + 3] = file[HEADER + COLOR + i];
  }
  return rgba;
}

export function encodeTdi1(pixel: (x: number, y: number) => readonly [number, number, number, number]): Uint8Array {
  const file = new Uint8Array(FILE_BYTES);
  file.set(HEAD);
  for (let y = 0; y < TDI1_SIDE; y++) {
    for (let x = 0; x < TDI1_SIDE; x++) {
      const [r, g, b, a] = pixel(x, y);
      const i = y * TDI1_SIDE + x;
      const value = ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3);
      file[HEADER + i * 2] = value & 0xff;
      file[HEADER + i * 2 + 1] = value >> 8;
      file[HEADER + COLOR + i] = a;
    }
  }
  return file;
}
