import { readdirSync, readFileSync } from 'node:fs';
import { join, sep } from 'node:path';
import { describe, expect, it } from 'vitest';

const src = join(import.meta.dirname, '..');
const sheets = readdirSync(src, { recursive: true, encoding: 'utf8' })
  .filter((file) => file.endsWith('.css'))
  .map((file) => ({ file: file.split(sep).join('/'), text: readFileSync(join(src, file), 'utf8') }));
const tokens = sheets.find((sheet) => sheet.file === 'styles/tokens.css')!.text;
const components = sheets.filter((sheet) => sheet.file !== 'styles/tokens.css');

const transitions = (text: string) => [...text.matchAll(/\btransition(?:-duration|-delay|-timing-function)?\s*:\s*([^;}]+)/g)].map((m) => m[1].trim());
const reducedBlock = tokens.slice(tokens.indexOf('@media (prefers-reduced-motion: reduce)'), tokens.indexOf('}\n}', tokens.indexOf('@media (prefers-reduced-motion: reduce)')));
const reducedValue = (name: string) => new RegExp(`${name}:\s*([^;]+);`).exec(reducedBlock)?.[1].trim();
const defined = new Set([...tokens.matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1]));

describe('motion tokens', () => {
  it('give every transition its duration and easing; only an instant 0ms is written in place', () => {
    const raw = components.flatMap(({ file, text }) =>
      transitions(text)
        .filter((value) => /\d*\.?\d+m?s\b|cubic-bezier|steps\(|\b(?:ease(?:-in|-out|-in-out)?|linear)\b/.test(value.replace(/var\(--[\w-]+\)/g, '').replace(/\b0ms\b/g, '')))
        .map((value) => `${file}: ${value}`),
    );
    expect(raw).toEqual([]);
  });

  it('define every motion token a transition uses', () => {
    const used = components.flatMap(({ text }) => transitions(text).flatMap((value) => [...value.matchAll(/var\((--[\w-]+)\)/g)].map((m) => m[1])));
    expect(used.length).toBeGreaterThan(0);
    expect([...new Set(used)].filter((name) => !defined.has(name))).toEqual([]);
  });

  it('keep the press, entry and lift amounts in the tokens', () => {
    const literal = components.flatMap(({ file, text }) => [...text.matchAll(/\bscale(?:\(|:\s*)0\.\d+|translateY\(-?\d/g)].map((m) => `${file}: ${m[0]}`));
    expect(literal).toEqual([]);
  });

  it('answer reduced motion in one place', () => {
    expect(components.filter(({ text }) => text.includes('prefers-reduced-motion: reduce')).map(({ file }) => file)).toEqual([]);
  });

  it('remove movement under reduced motion and keep the fades', () => {
    for (const scale of ['--press-scale', '--press-scale-device', '--enter-scale', '--enter-scale-tooltip']) expect(reducedValue(scale), scale).toBe('1');
    for (const duration of ['--duration-lift', '--duration-chevron', '--duration-switch', '--duration-expand', '--duration-collapse', '--duration-label', '--duration-label-out', '--delay-label', '--duration-settle', '--duration-progress']) {
      expect(reducedValue(duration), duration).toBe('0ms');
    }
    expect(reducedValue('--duration-popover')).toBe('120ms');
    expect(reducedValue('--duration-dialog')).toBe('140ms');
    for (const fade of ['--duration-light', '--duration-tooltip', '--duration-exit', '--duration-hover', '--duration-theme']) expect(reducedValue(fade), fade).toBeUndefined();
  });
});
