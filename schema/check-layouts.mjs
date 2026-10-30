import { readFileSync, readdirSync } from 'node:fs';
import { dirname, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import Ajv2020 from 'ajv/dist/2020.js';

const schemaDirectory = dirname(fileURLToPath(import.meta.url));
const examplesDirectory = resolve(schemaDirectory, 'examples');
const invalidDirectory = resolve(examplesDirectory, 'invalid');
const schema = JSON.parse(readFileSync(resolve(schemaDirectory, 'layout.schema.json'), 'utf8'));
const ajv = new Ajv2020({ allErrors: true, strict: true, allowUnionTypes: true, verbose: true });
ajv.addVocabulary(['x-rules']);
const checkSchema = ajv.compile(schema);

const generalRules = { type: 'shape.type', required: 'shape.missing-field', additionalProperties: 'shape.unknown-field', 'false schema': 'shape.unknown-field', if: 'icon.format' };
const codeRules = [
  'file.utf8',
  'file.json',
  'json.duplicate-member',
  'json.integer-form',
  'string.malformed-unicode',
  'layout.too-large',
  'id.duplicate',
  'action.page-missing',
  'tile.bounds',
  'tile.overlap',
  'tile.streams',
];

const renderStreams = { graph: { value: ['number', 'series'] } };

export function* schemaNodes(node, property) {
  if (node === null || typeof node !== 'object') return;
  yield [node, property];
  for (const [name, child] of Object.entries(node.properties ?? {})) yield* schemaNodes(child, name);
  for (const child of Object.values(node.$defs ?? {})) yield* schemaNodes(child);
  for (const child of node.oneOf ?? []) yield* schemaNodes(child);
  if (node.items) yield* schemaNodes(node.items);
}

export const ruleKeys = new Set([
  ...Object.values(generalRules),
  ...codeRules,
  ...[...schemaNodes(schema)].flatMap(([node]) => Object.values(node['x-rules'] ?? {})),
]);

const fail = (key, message) => ({ key, message });
const escapePointer = (segment) => String(segment).toWellFormed().replaceAll('~', '~0').replaceAll('/', '~1');

const isObject = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
const selectBranch = (definition, property, value) => definition.oneOf.findIndex((candidate) => candidate.properties[property].const === value[property]);
const ownsMember = (definition, selected, member) => Object.hasOwn(definition.oneOf[selected]?.properties ?? {}, member);

function matchesDiscriminator(error, layout) {
  const pagePath = error.instancePath.match(/^\/pages\/(\d+)(?:\/([^/]*))?/);
  if (!pagePath) return true;
  const page = layout.pages[Number(pagePath[1])];
  if (isObject(page) && pagePath[2] !== undefined && !ownsMember(schema.$defs.page, selectBranch(schema.$defs.page, 'kind', page), pagePath[2])) return false;
  const branch = error.schemaPath.match(/^#\/oneOf(?:\/(\d+))?(?:\/|$)/);
  const actionPath = error.instancePath.match(/^\/pages\/\d+\/tiles\/(\d+)\/action(?:\/([^/]*)|$)/);
  const definition = actionPath ? schema.$defs.action : schema.$defs.page;
  const property = actionPath ? 'type' : 'kind';
  const value = actionPath ? page.tiles[Number(actionPath[1])].action : page;
  if (!isObject(value)) return !branch || (error.keyword === 'type' && branch[1] === '0');
  const selected = selectBranch(definition, property, value);
  if (actionPath && actionPath[2] !== undefined && !ownsMember(definition, selected, actionPath[2])) return false;
  if (!branch) return true;
  if (selected < 0) return error.keyword === 'oneOf';
  return branch[1] !== undefined && Number(branch[1]) === selected;
}

function structuralErrors(layout) {
  const errors = new Map();
  for (const error of checkSchema.errors.filter((candidate) => candidate.keyword !== 'if' && matchesDiscriminator(candidate, layout))) {
    const key = error.parentSchema['x-rules']?.[error.keyword] ?? generalRules[error.keyword];
    if (!key) throw new Error(`schema constraint ${error.keyword} at ${error.schemaPath} has no rule key`);
    const detail = error.params.additionalProperty ?? error.params.missingProperty;
    const message = `${error.instancePath || '/'} ${error.message} (${error.keyword})${detail === undefined ? '' : ` ${JSON.stringify(detail)}`}`;
    errors.set(`${key} ${message}`, fail(key, message));
  }
  return [...errors.values()];
}

function malformedStrings(root) {
  const errors = [];
  const pending = [['', root]];
  while (pending.length) {
    const [pointer, value] = pending.pop();
    if (typeof value === 'string') {
      if (!value.isWellFormed()) errors.push(fail('string.malformed-unicode', `${pointer || '/'} is not well-formed Unicode`));
    } else if (Array.isArray(value)) {
      for (let index = value.length - 1; index >= 0; index--) pending.push([`${pointer}/${index}`, value[index]]);
    } else if (value !== null && typeof value === 'object') {
      const members = Object.entries(value);
      for (const [name] of members) {
        if (!name.isWellFormed()) errors.push(fail('string.malformed-unicode', `${pointer || '/'} has the member name ${JSON.stringify(name)}, which is not well-formed Unicode`));
      }
      for (let index = members.length - 1; index >= 0; index--) pending.push([`${pointer}/${escapePointer(members[index][0])}`, members[index][1]]);
    }
  }
  return errors;
}

function semanticErrors(layout) {
  const errors = [];
  const compactBytes = Buffer.byteLength(JSON.stringify(layout), 'utf8');
  if (compactBytes > 32768) {
    errors.push(fail('layout.too-large', `layout compact UTF-8 JSON is ${compactBytes} bytes; maximum is 32768 bytes`));
  }
  const ids = new Map();
  const pageIds = new Set(layout.pages.map((page) => page.id));
  const addId = (id, location) => {
    if (ids.has(id)) {
      errors.push(fail('id.duplicate', `duplicate id "${id}" at ${location}; first used at ${ids.get(id)}`));
    } else {
      ids.set(id, location);
    }
  };

  for (const [pageIndex, page] of layout.pages.entries()) {
    const pageLocation = `page ${pageIndex} "${page.id}"`;
    addId(page.id, pageLocation);
    if (page.kind === 'dashboard') continue;

    const occupied = new Map();
    for (const [tileIndex, tile] of page.tiles.entries()) {
      addId(tile.id, `${pageLocation} tile ${tileIndex} "${tile.id}"`);
      if (tile.action.type === 'page' && !pageIds.has(tile.action.page)) {
        errors.push(fail('action.page-missing', `page "${page.id}" tile "${tile.id}" references missing page "${tile.action.page}"`));
      }
      const bindings = renderStreams[tile.render] ?? {};
      const streams = tile.streams ?? {};
      for (const [id, type] of Object.entries(streams)) {
        if (!Object.hasOwn(bindings, id) || !bindings[id].includes(type)) errors.push(fail('tile.streams', `page "${page.id}" tile "${tile.id}" cannot bind stream "${id}" as ${JSON.stringify(type)}`));
      }
      for (const id of Object.keys(bindings)) {
        if (!Object.hasOwn(streams, id)) errors.push(fail('tile.streams', `page "${page.id}" tile "${tile.id}" needs stream "${id}"`));
      }
      if (tile.row + tile.rowSpan > page.rows || tile.col + tile.colSpan > page.cols) {
        errors.push(fail('tile.bounds', `page "${page.id}" tile "${tile.id}" exceeds grid bounds ${page.rows}x${page.cols}`));
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
        errors.push(fail('tile.overlap', `page "${page.id}" tiles "${other}" and "${tile.id}" overlap`));
      }
    }
  }
  return errors;
}

export function validateLayout(layout) {
  const errors = malformedStrings(layout);
  if (!checkSchema(layout)) return [...errors, ...structuralErrors(layout)];
  return [...errors, ...semanticErrors(layout)];
}

const jsonToken = /[ \t\n\r]+|"(?:[^"\\]|\\.)*"|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|[{}[\]:,]|true|false|null/y;

const integerToken = /^(?:0|-?[1-9]\d*)$/;
const maxNesting = 16;

function nestsTooDeep(text) {
  let depth = 0;
  jsonToken.lastIndex = 0;
  for (let match; (match = jsonToken.exec(text)); ) {
    if (match[0] === '{' || match[0] === '[') {
      if (++depth > maxNesting) return true;
    } else if (match[0] === '}' || match[0] === ']') {
      depth--;
    }
  }
  return false;
}

function scanSource(text) {
  const errors = [];
  const containers = [];
  const valuePointer = () => {
    const top = containers.at(-1);
    if (!top) return '';
    return `${top.pointer}/${top.members ? escapePointer(top.member) : top.index}`;
  };
  jsonToken.lastIndex = 0;
  for (let match; (match = jsonToken.exec(text)); ) {
    const token = match[0];
    const top = containers.at(-1);
    if (token === '{') {
      containers.push({ pointer: valuePointer(), members: new Set(), member: null, expectMember: true });
    } else if (token === '[') {
      containers.push({ pointer: valuePointer(), index: 0 });
    } else if (token === '}' || token === ']') {
      containers.pop();
    } else if (token === ',') {
      if (top.members) top.expectMember = true;
      else top.index++;
    } else if (token[0] === '"' && top?.members && top.expectMember) {
      top.member = JSON.parse(token);
      top.expectMember = false;
      if (top.members.has(top.member)) {
        errors.push(fail('json.duplicate-member', `${top.pointer || '/'} repeats the member name ${JSON.stringify(top.member)}`));
      }
      top.members.add(top.member);
    } else if (/^-?\d/.test(token) && !integerToken.test(token)) {
      errors.push(fail('json.integer-form', `${valuePointer() || '/'} is written as ${token}; numbers must be JSON integers`));
    }
  }
  return errors;
}

export function validateSource(bytes) {
  let text;
  try {
    text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  } catch {
    return [fail('file.utf8', 'file is not well-formed UTF-8')];
  }
  if (nestsTooDeep(text)) return [fail('file.json', `JSON nests deeper than ${maxNesting} levels`)];
  let layout;
  try {
    layout = JSON.parse(text);
  } catch (error) {
    return [fail('file.json', error.message)];
  }
  return [...scanSource(text), ...validateLayout(layout)];
}

function splitExpectation(bytes) {
  const end = bytes.indexOf(0x0a);
  const firstLine = new TextDecoder().decode(bytes.subarray(0, end < 0 ? bytes.length : end)).replace(/\r$/, '');
  return { expected: firstLine.match(/^expected: ([a-z][a-z0-9.-]*)$/)?.[1] ?? '', body: end < 0 ? new Uint8Array() : bytes.subarray(end + 1) };
}

function exampleFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name)).flatMap((entry) => {
    const path = resolve(directory, entry.name);
    return entry.isDirectory() ? exampleFiles(path) : entry.name.endsWith('.json') ? [path] : [];
  });
}

// Windows paths are case-insensitive, including the drive letter, so compare them folded.
export const pathKey = (path, platform = process.platform) => (platform === 'win32' ? path.toLowerCase() : path);

const describeErrors = (errors) => (errors.length ? errors.map(({ key, message }) => `${key}: ${message}`).join('; ') : 'valid layout');

export function checkLayouts(paths = [], write = console.log) {
  let valid = 0;
  let invalid = 0;
  let problems = 0;
  const files = new Map();
  for (const path of [...exampleFiles(examplesDirectory), ...paths.map((path) => resolve(path))]) {
    if (!files.has(pathKey(path))) files.set(pathKey(path), path);
  }
  const invalidPrefix = pathKey(invalidDirectory + sep);
  for (const [key, path] of files) {
    const label = relative(process.cwd(), path);
    try {
      const bytes = readFileSync(path);
      if (key.startsWith(invalidPrefix)) {
        const { expected, body } = splitExpectation(bytes);
        const errors = validateSource(body);
        if (!ruleKeys.has(expected)) {
          problems++;
          write(`FAIL ${label}: first line does not name a known rule key as "expected: <key>"`);
        } else if (errors.length !== 1 || errors[0].key !== expected) {
          problems++;
          write(`FAIL ${label}: expected only ${expected}, got ${describeErrors(errors)}`);
        } else {
          invalid++;
          write(`ok ${label} (invalid as expected: ${expected})`);
        }
        continue;
      }
      const errors = validateSource(bytes);
      if (errors.length) {
        problems++;
        write(`FAIL ${label}: ${describeErrors(errors)}`);
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
