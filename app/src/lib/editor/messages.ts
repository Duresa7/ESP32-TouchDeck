// The editor's refusals and errors in plain words. The layout rules speak in rule keys and JSON pointers, such as
// `overlaps quiet` or `label.too-long: /pages/0/tiles/2/label ...`, which stay available as the detail.
import type { Layout } from './validate.ts';

export const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

function tileLabel(layout: Layout | null | undefined, id: string): string | null {
  for (const page of layout?.pages ?? []) {
    if (page.kind !== 'grid') continue;
    const tile = page.tiles.find((t) => t.id === id);
    if (tile) return tile.label;
  }
  return null;
}

const FIELDS: Record<string, string> = {
  path: 'Choose the app to open.',
  target: 'Enter the file, folder, or web address to open.',
  program: 'Enter the program to run.',
  source: 'Choose where the usage numbers come from.',
  field: 'Choose which number to show.',
};

/** A sentence for why the editor refused a change, from the rule's own reason. */
export function friendlyRefusal(reason: string, layout?: Layout | null): string {
  let match: RegExpExecArray | null;
  if ((match = /^overlaps (\S+)$/.exec(reason))) {
    const label = tileLabel(layout, match[1]);
    return `That spot is taken${label ? ` by “${label}”` : ''}. Pick an empty spot.`;
  }
  if ((match = /^outside the (\d+)x(\d+) grid$/.exec(reason))) {
    return `That doesn't fit. This page has ${plural(Number(match[1]), 'row')} and ${plural(Number(match[2]), 'column')}.`;
  }
  if (reason === 'no free cell on this page') return 'This page is full. Make its grid bigger or add another page.';
  if ((match = /^a page holds at most (\d+) tiles$/.exec(reason))) return `A page can hold up to ${match[1]} buttons.`;
  if ((match = /^a layout holds at most (\d+) pages$/.exec(reason))) return `A layout can have up to ${match[1]} pages.`;
  if (reason === 'a layout keeps at least 1 page') return 'A layout needs at least one page.';
  if ((match = /^grid\.too-small: (\S+)$/.exec(reason))) {
    const label = tileLabel(layout, match[1]);
    return `The grid can't shrink past${label ? ` “${label}”` : ' a button'}. Move or remove it first.`;
  }
  const key = reason.split(':')[0];
  if ((match = /^text\.empty: \/pages\/\d+\/tiles\/\d+\/action\/(\w+)/.exec(reason)) && FIELDS[match[1]]) return FIELDS[match[1]];
  switch (key) {
    case 'label.too-long':
      return 'Button names can be up to 16 characters.';
    case 'label.blank':
      return 'Give the button a name.';
    case 'title.too-long':
      return 'Page names can be up to 24 characters.';
    case 'title.blank':
      return 'Give the page a name.';
    case 'name.too-long':
      return 'Layout names can be up to 24 characters.';
    case 'name.blank':
      return 'Give the layout a name.';
    case 'grid.size':
      return 'A page grid can have 1 to 8 rows and 1 to 8 columns.';
    case 'layout.too-large':
      return 'This layout is too big for the deck. Remove some buttons or pages.';
    case 'string.control-character':
      return 'Remove tabs, line breaks, and other special characters.';
    case 'action.page-missing':
      return 'A button goes to a page that no longer exists.';
    case 'key.usage-range':
    case 'consumer.usage-range':
      return 'Pick a key from the list.';
    default:
      return "The deck can't take that change.";
  }
}

/** A sentence for a layout file the editor could not read. */
export function friendlyFileError(name: string, key: string): string {
  if (key === 'file.json' || key === 'file.utf8') return `“${name}” isn't an Open TouchDeck layout file, so nothing changed.`;
  return `“${name}” has a problem the deck can't take, so nothing changed.`;
}
