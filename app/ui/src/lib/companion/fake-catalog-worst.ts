import type { CatalogHit } from './api.ts';

const NAMES: readonly [string, 'simple-icons' | 'selfhst', string | null][] = [
  ['Calibre-Web-Automated-Book-Downloader', 'selfhst', null],
  ['X', 'simple-icons', '000000'],
  ['КиноПоиск', 'simple-icons', 'FF5500'],
  ['Metro de la Ciudad de México', 'simple-icons', 'F77E1C'],
  ['大众点评', 'simple-icons', 'FF6633'],
  ['.NET', 'simple-icons', '512BD4'],
  ['카카오톡', 'simple-icons', 'FFCD00'],
  ['Initiative (Project Management)', 'selfhst', null],
  ['Île-de-France Mobilités', 'simple-icons', '67B4E7'],
  ['tado°', 'simple-icons', 'FFA900'],
  ['/e/', 'simple-icons', null],
  ['San Francisco Municipal Railway', 'simple-icons', 'BA0C2F'],
  ['ŠKODA', 'simple-icons', '0E3A2F'],
  ['1&1', 'simple-icons', '003D8F'],
  ['Social Security Administration', 'selfhst', null],
  ['アメブロ', 'simple-icons', '2D8C3C'],
];

export function worstCaseHits(): CatalogHit[] {
  return NAMES.map(([name, source, hex], i) => ({
    source,
    name,
    id: `worst-${i}`,
    aliases: [],
    hex,
    tags: ['logo'],
    category: source === 'selfhst' ? 'Self-Hosted' : null,
    svg: true,
    light: false,
    dark: false,
    license: source === 'selfhst' ? 'CC-BY-4.0' : 'CC0-1.0',
    license_url: null,
    guidelines: null,
    attribution_url: 'https://selfh.st/icons/',
    attribution: 'Logos belong to their owners.',
  }));
}
