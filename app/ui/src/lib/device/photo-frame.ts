export const START_AFTER_S: readonly number[] = [30, 60, 120, 300, 600, 900, 1800, 3600];

export function durationLabel(seconds: number): string {
  if (seconds >= 3600 && seconds % 3600 === 0) return `${seconds / 3600} hr`;
  if (seconds >= 60 && seconds % 60 === 0) return `${seconds / 60} min`;
  return `${seconds} s`;
}

export function startAfterChoices(current: number): number[] {
  return START_AFTER_S.includes(current) ? [...START_AFTER_S] : [...START_AFTER_S, current].sort((a, b) => a - b);
}
