export interface Spot {
  section: number;
  index: number;
}

export type Move = Spot | 'search' | null;

const nextFilled = (sizes: readonly number[], from: number, step: 1 | -1) => {
  for (let s = from + step; s >= 0 && s < sizes.length; s += step) if (sizes[s] > 0) return s;
  return null;
};

export function moveSpot(sizes: readonly number[], at: Spot, key: string, cols: number): Move {
  const size = sizes[at.section] ?? 0;
  const col = at.index % cols;
  switch (key) {
    case 'ArrowRight': {
      if (at.index + 1 < size) return { section: at.section, index: at.index + 1 };
      const next = nextFilled(sizes, at.section, 1);
      return next === null ? null : { section: next, index: 0 };
    }
    case 'ArrowLeft': {
      if (at.index > 0) return { section: at.section, index: at.index - 1 };
      const prev = nextFilled(sizes, at.section, -1);
      return prev === null ? null : { section: prev, index: sizes[prev] - 1 };
    }
    case 'ArrowDown': {
      if (at.index + cols < size) return { section: at.section, index: at.index + cols };
      if (Math.floor(at.index / cols) < Math.floor((size - 1) / cols)) return { section: at.section, index: size - 1 };
      const next = nextFilled(sizes, at.section, 1);
      return next === null ? null : { section: next, index: Math.min(col, sizes[next] - 1) };
    }
    case 'ArrowUp': {
      if (at.index - cols >= 0) return { section: at.section, index: at.index - cols };
      const prev = nextFilled(sizes, at.section, -1);
      if (prev === null) return 'search';
      const lastRow = Math.floor((sizes[prev] - 1) / cols) * cols;
      return { section: prev, index: Math.min(lastRow + col, sizes[prev] - 1) };
    }
    case 'Home': {
      const first = nextFilled(sizes, -1, 1);
      return first === null ? null : { section: first, index: 0 };
    }
    case 'End': {
      const last = nextFilled(sizes, sizes.length, -1);
      return last === null ? null : { section: last, index: sizes[last] - 1 };
    }
    default:
      return null;
  }
}
