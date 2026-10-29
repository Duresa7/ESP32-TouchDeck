import { counts, type Layout } from './validate.ts';

export const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

export function describeLayout(layout: Layout): string {
  const { pages, tiles } = counts(layout);
  return `${plural(pages, 'Page')}, ${plural(tiles, 'Tile')}`;
}

function tileLabel(layout: Layout | null | undefined, id: string): string | null {
  for (const page of layout?.pages ?? []) {
    if (page.kind !== 'grid') continue;
    const tile = page.tiles.find((t) => t.id === id);
    if (tile) return tile.label;
  }
  return null;
}

const FIELDS: Record<string, string> = {
  path: 'Choose an app.',
  target: 'Enter a file, folder or web address.',
  program: 'Enter a program.',
  source: 'Choose an Integration.',
  field: 'Choose a value to show.',
};

export function friendlyRefusal(reason: string, layout?: Layout | null, device = 'the Device'): string {
  let match: RegExpExecArray | null;
  if ((match = /^overlaps (\S+)$/.exec(reason))) {
    const label = tileLabel(layout, match[1]);
    return label ? `That spot is taken by “${label}”. Pick an empty one.` : 'That spot is taken. Pick an empty one.';
  }
  if ((match = /^outside the (\d+)x(\d+) grid$/.exec(reason))) {
    return `That doesn't fit. This Page has ${plural(Number(match[1]), 'row')} and ${plural(Number(match[2]), 'column')}.`;
  }
  if (reason === 'no free cell on this page') return 'This Page is full. Make its grid bigger or add a Page.';
  if ((match = /^a page holds at most (\d+) tiles$/.exec(reason))) return `A Page holds up to ${match[1]} Tiles.`;
  if ((match = /^a layout holds at most (\d+) pages$/.exec(reason))) return `A Layout holds up to ${match[1]} Pages.`;
  if (reason === 'a layout keeps at least 1 page') return 'A Layout needs at least one Page.';
  if ((match = /^grid\.too-small: (\S+)$/.exec(reason))) {
    const label = tileLabel(layout, match[1]);
    return `The grid can't shrink past ${label ? `“${label}”` : 'a Tile'}. Move or remove it first.`;
  }
  if ((match = /^text\.empty: \/pages\/\d+\/tiles\/\d+\/action\/(\w+)/.exec(reason)) && FIELDS[match[1]]) return FIELDS[match[1]];
  switch (reason.split(':')[0]) {
    case 'label.too-long':
      return 'Tile labels can be up to 16 characters.';
    case 'label.blank':
      return 'Give the Tile a label.';
    case 'title.too-long':
      return 'Page names can be up to 24 characters.';
    case 'title.blank':
      return 'Give the Page a name.';
    case 'name.too-long':
      return 'Layout names can be up to 24 characters.';
    case 'name.blank':
      return 'Give the Layout a name.';
    case 'grid.size':
      return 'A Page grid has 1 to 8 rows and 1 to 8 columns.';
    case 'layout.too-large':
      return `This Layout is too big for ${device}. Remove some Tiles or Pages.`;
    case 'string.control-character':
      return 'Remove tabs, line breaks and other special characters.';
    case 'action.page-missing':
      return 'A Tile goes to a Page that no longer exists.';
    case 'key.usage-range':
    case 'consumer.usage-range':
      return 'Pick a key from the list.';
    default:
      return 'That change breaks a Layout rule. See Details.';
  }
}

export function friendlyFileError(file: string, key: string): string {
  if (key === 'file.json' || key === 'file.utf8') return `“${file}” isn't a Layout file. Nothing changed.`;
  return `“${file}” breaks a Layout rule. Nothing changed. See Details.`;
}

export const PLUG_IN = "Check that it's plugged in, then retry.";
