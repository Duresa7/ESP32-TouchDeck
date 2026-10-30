import { truncDiv } from './geometry.ts';

export interface GraphPoint {
  x: number;
  y: number;
}

export interface GraphRange {
  lo: number;
  hi: number;
}

export function graphCeiling(m: number): number {
  let p = 1;
  while (p < m) p *= 10;
  while (p / 10 >= m) p /= 10;
  const q = p / 10;
  if (m <= q * 2) return q * 2;
  if (m <= q * 5) return q * 5;
  return p;
}

export function graphRange(samples: readonly number[]): GraphRange {
  let min = 0;
  let max = 0;
  for (const v of samples) {
    if (v < min) min = v;
    if (v > max) max = v;
  }
  const lo = min < 0 ? -graphCeiling(-min) : 0;
  const hi = max > 0 ? graphCeiling(max) : 0;
  return { lo, hi: hi === lo ? lo + 1 : hi };
}

/** The Firmware's ui_graph_points: the newest sample on the right edge, older ones a cap-th of the width apart. */
export function graphPoints(input: readonly number[], cap: number, w: number, h: number): GraphPoint[] {
  if (input.length === 0 || w < 2 || h < 2 || cap === 0) return [];
  const samples = input.length > cap ? input.slice(input.length - cap) : input;
  const { lo, hi } = graphRange(samples);
  return samples.map((v, i) => {
    const slot = cap - samples.length + i;
    const x = cap === 1 ? w - 1 : truncDiv(slot * (w - 1), cap - 1);
    const y = h - 1 - Math.floor(((v - lo) * (h - 1)) / (hi - lo) + 0.5);
    return { x, y };
  });
}
