import { readFileSync, readdirSync } from 'node:fs';
import { dirname, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import Ajv2020 from 'ajv/dist/2020.js';

const schemaDirectory = dirname(fileURLToPath(import.meta.url));
const examplesDirectory = resolve(schemaDirectory, 'examples');
const invalidDirectory = resolve(examplesDirectory, 'invalid');
const schema = JSON.parse(readFileSync(resolve(schemaDirectory, 'layout.schema.json'), 'utf8'));
const checkSchema = new Ajv2020({ allErrors: true, strict: true }).compile(schema);

function matchesDiscriminator(error, layout) {
  const branch = error.schemaPath.match(/^#\/oneOf(?:\/(\d+))?(?:\/|$)/);
  const pagePath = error.instancePath.match(/^\/pages\/(\d+)/);
  if (!branch || !pagePath) return true;
  const page = layout.pages[Number(pagePath[1])];
  const actionPath = error.instancePath.match(/\/tiles\/(\d+)\/action(?:\/|$)/);
  const definition = actionPath ? schema.$defs.action : schema.$defs.page;
  const property = actionPath ? 'type' : 'kind';
  const value = actionPath ? page.tiles[Number(actionPath[1])].action : page;
  const selected = definition.oneOf.findIndex((candidate) => candidate.properties[property].const === value?.[property]);
  if (selected < 0) return error.keyword === 'oneOf';
  return branch[1] !== undefined && Number(branch[1]) === selected;
}

export function validateLayout(layout) {
  if (!checkSchema(layout)) {
    return [...new Set(checkSchema.errors.filter((error) => matchesDiscriminator(error, layout)).map(({ instancePath, keyword, message, params }) => {
      const detail = params.additionalProperty ?? params.missingProperty;
      return `${instancePath || '/'} ${message} (${keyword})${detail === undefined ? '' : ` ${JSON.stringify(detail)}`}`;
    }))];
  }

  const errors = [];
  const ids = new Map();
  const pageIds = new Set(layout.pages.map((page) => page.id));
  const addId = (id, location) => {
    if (ids.has(id)) {
      errors.push(`duplicate id "${id}" at ${location}; first used at ${ids.get(id)}`);
    } else {
      ids.set(id, location);
    }
  };

  for (const page of layout.pages) {
    addId(page.id, `page "${page.id}"`);
    if (page.kind === 'dashboard') continue;

    const occupied = new Map();
    for (const tile of page.tiles) {
      addId(tile.id, `page "${page.id}" tile "${tile.id}"`);
      if (tile.action.type === 'page' && !pageIds.has(tile.action.page)) {
        errors.push(`page "${page.id}" tile "${tile.id}" references missing page "${tile.action.page}"`);
      }
      if (tile.row + tile.rowSpan > page.rows || tile.col + tile.colSpan > page.cols) {
        errors.push(`page "${page.id}" tile "${tile.id}" exceeds grid bounds ${page.rows}x${page.cols}`);
        continue;
      }
      const overlaps = new Set();
      for (let row = tile.row; row < tile.row + tile.rowSpan; row++) {
        for (let col = tile.col; col < tile.col + tile.colSpan; col++) {
          const cell = row * page.cols + col;
          if (!occupied.has(cell)) occupied.set(cell, []);
          for (const other of occupied.get(cell)) overlaps.add(other);
          occupied.get(cell).push(tile.id);
        }
      }
      for (const other of overlaps) {
        errors.push(`page "${page.id}" tiles "${other}" and "${tile.id}" overlap`);
      }
    }
  }
  return errors;
}

function exampleFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name)).flatMap((entry) => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? exampleFiles(path) : entry.name.endsWith('.json') ? [path] : [];
  });
}

export function checkLayouts(paths = [], write = console.log) {
  let valid = 0;
  let invalid = 0;
  let problems = 0;
  const files = new Set([...exampleFiles(examplesDirectory), ...paths.map((path) => resolve(path))]);
  for (const path of files) {
    const label = relative(process.cwd(), path);
    try {
      const source = readFileSync(path, 'utf8');
      const layout = JSON.parse(source);
      const errors = validateLayout(layout);
      if (path.startsWith(invalidDirectory + sep)) {
        const firstLine = source.split(/\r?\n/, 1)[0];
        const match = firstLine.match(/^\{\s*"name"\s*:\s*("(?:[^"\\]|\\.)*")\s*,\s*$/);
        const name = match ? JSON.parse(match[1]) : '';
        const expected = name.startsWith('expected: ') ? name.slice('expected: '.length) : '';
        if (!expected || !errors.some((error) => error.includes(expected))) {
          problems++;
          write(`FAIL ${label}: expected ${JSON.stringify(expected)}, got ${errors.length ? errors.join('; ') : 'valid layout'}`);
        } else {
          invalid++;
          write(`ok ${label} (invalid as expected: ${expected})`);
        }
      } else if (errors.length) {
        problems++;
        write(`FAIL ${label}: ${errors.join('; ')}`);
      } else {
        valid++;
        write(`ok ${label}`);
      }
    } catch (error) {
      problems++;
      write(`FAIL ${label}: ${error.message}`);
    }
  }
  write(`${valid} valid, ${invalid} invalid as expected, ${problems} problems`);
  return problems === 0 ? 0 : 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const exitCode = checkLayouts(process.argv.slice(2));
  console.log(`wall time: ${performance.now().toFixed(2)} ms`);
  process.exitCode = exitCode;
}
