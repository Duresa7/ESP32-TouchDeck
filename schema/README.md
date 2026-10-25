# TouchDeck layout format

[layout.schema.json](layout.schema.json) defines version 1 in JSON Schema draft
2020-12. Layouts describe pages on the deck. They contain no executable code.
The app executes the actions that require the PC.

App-side actions and widgets dim on the deck while the app is offline.

## Validator commands

The validator uses Node.js with ES modules and Ajv 8. Its only installed packages
are Ajv and Ajv's dependencies. From the repository root, the commands are:

```sh
npm ci --prefix schema
node schema/check-layouts.mjs
node --test schema/
node schema/check-layouts.mjs path/to/layout.json
```

Every invocation checks all JSON files recursively under `schema/examples/`,
plus the supplied file paths. Repeated paths count once. Paths supplied on the
command line are relative to the current directory. The bundled examples resolve
relative to the script, so the command also works from another directory.

The default successful summary is:

```text
2 valid, 12 invalid as expected, 0 problems
```

The command prints one `ok` line for each example and a final `wall time` in
milliseconds. The time includes Node startup, imports, schema compilation,
file reads, parsing, validation, and output through the summary.
Exit code `0` means all checks passed. Exit code `1` means at least one file
failed, an invalid example passed, an expectation did not match, or a file could
not be read or parsed. The validator never launches programs or sends keys.

`validateLayout(value)` is an exported function in `check-layouts.mjs`.
It returns an array of error strings, with an empty array for a valid layout.
It does not change the input. `checkLayouts(paths, write)` checks the example
files and extra paths, writes status lines, and returns the exit code.

## Layout

All fields listed here are required. Every object rejects unknown properties.
Numbers that describe cells, spans, or HID usages are integers. Strings use
Unicode code points for length limits, as JSON Schema specifies.

| Field | Value |
| --- | --- |
| `schema` | Integer `1`. A different version is invalid. |
| `name` | Nonempty display name. |
| `orientation` | `landscape` or `portrait`. Each layout describes one orientation. |
| `pages` | Array of 1 to 8 pages, in swipe order. |

Page and tile IDs share one namespace across the entire layout. Every ID matches
`^[a-z0-9_]{1,15}$`. An action's page target refers to a page ID in that layout.
The first page is the initial page.

### Upload and string limits

The upload limit is 32768 bytes of compact UTF-8 JSON. Compact JSON has no
whitespace outside strings, as `JSON.stringify(layout)` with no spacing produces.
The byte count is `Buffer.byteLength(JSON.stringify(layout), 'utf8')`.
Formatting whitespace in an input file does not count. Whitespace inside strings
and JSON escape sequences do count. Exactly 32768 compact bytes is valid.
The validator rejects 32769 or more and reports both the actual size and the limit.
The app always uploads compact JSON.

These limits are the shared contract for the Rust validator in M2-10, the editor's
**Push** in M2-11, and the firmware parser in M2-04. Each must reject layouts above
the compact byte limit and enforce 1 to 8 pages. Every page title and grid tile
label must be nonempty. Titles allow at most 24 Unicode code points, and labels
allow at most 16. `maxLength` counts code points, not UTF-8 bytes or UTF-16 code
units. The firmware must store whatever UTF-8 the schema allows.

## Pages

| Field | Grid page | Dashboard page |
| --- | --- | --- |
| `id` | Unique ID. | Unique ID. |
| `title` | 1 to 24 characters. | 1 to 24 characters. |
| `kind` | `grid` | `dashboard` |
| `rows` | 1 to 8. | Absent. |
| `cols` | 1 to 8. | Absent. |
| `tiles` | Array of 0 to 64 tiles. | Absent, including an empty array. |
| `source` | Absent. | Nonempty source identifier, such as `cliproxyapi`. |

A grid may have empty cells. A dashboard has no grid or tiles. The app supplies
its rows from the named source. Source identifiers remain open to plugins.

## Tiles

| Field | Value |
| --- | --- |
| `id` | Unique ID. |
| `label` | 1 to 16 characters. |
| `row`, `col` | Zero-based cell coordinates, each 0 to 7. |
| `rowSpan`, `colSpan` | Number of cells occupied, each 1 to 8. |
| `icon` | `null`, one of the six `lv:` names below, or `file:` followed by exactly 16 hexadecimal digits. |
| `action` | Exactly one action object from the table below. |

A tile fits only when `row + rowSpan <= rows` and `col + colSpan <= cols`.
No two tiles may occupy the same cell. Adjacent tile edges may touch.
File icon hashes accept uppercase and lowercase hexadecimal digits. A file icon
names a bitmap in deck storage. The validator checks the name, not file presence.

The six built-in icon names reproduce milestone 1's media icons.

| Name | LVGL glyph |
| --- | --- |
| `lv:prev` | `LV_SYMBOL_PREV` |
| `lv:play_pause` | `LV_SYMBOL_PLAY` followed by `LV_SYMBOL_PAUSE` |
| `lv:next` | `LV_SYMBOL_NEXT` |
| `lv:mute` | `LV_SYMBOL_MUTE` |
| `lv:volume_mid` | `LV_SYMBOL_VOLUME_MID` |
| `lv:volume_max` | `LV_SYMBOL_VOLUME_MAX` |

## Actions

Each action has a `type` and exactly the fields for that type. Fields from another
type are invalid. The schema uses `oneOf` with a constant `type` in each branch.

| `type` | Other required fields | Runs on | Meaning |
| --- | --- | --- | --- |
| `key` | `usage`, `mods` | Deck | Tap a keyboard HID usage from 1 to 255 with the listed modifiers. `mods` is a duplicate-free subset of `ctrl`, `shift`, `alt`, and `win`. An empty array is allowed. |
| `mod` | `mod` | Deck | Apply the modifier `ctrl`, `shift`, `alt`, or `win`. A tap arms the next key, a double tap locks it, and a tap on a locked modifier clears it. |
| `consumer` | `usage` | Deck | Tap a consumer HID usage from 1 to 65535. |
| `page` | `page` | Deck | Jump to the existing page with that ID, including a dashboard. |
| `none` | None. | Deck | Do nothing. |
| `launch` | `path`, `args`, `cwd` | App | Start the nonempty executable path with an array of string arguments. `cwd` is a working-directory string. An empty string inherits the app's working directory. |
| `open` | `target` | App | Open a nonempty file path or URL with the operating system's default handler. |
| `command` | `program`, `args`, `cwd`, `shell` | App | Run the nonempty program with an array of string arguments and a working-directory string. An empty `cwd` inherits the app's working directory. Boolean `shell` chooses whether the app invokes a shell. The app asks before a new or changed command runs. |
| `widget` | `source`, `field`, `account` | App | Display a reading from a source for an account. All three fields are nonempty strings. Source plugins interpret field and account identifiers. |

The deck owns the key engine and its console key log. The milestone 2 app mirrors
key reports to the PC. Deck-side describes action ownership, not a USB HID sink.
The schema does not check installed programs, URLs, accounts, or source availability.

For widget actions, `claude-status` supplies fields such as `context_pct` and
`cost_usd`. The example account `current` selects the current session.
`cliproxyapi` supplies fields such as `status` and `recent_10m`.
The example account `default` must match an account configured in the app.

## Examples

[builtin.json](examples/builtin.json) is the firmware's built-in fallback.
Its `fkeys` page is 4 by 4, with F1 to F12 and Shift, Ctrl, Alt, and Win.
Its `media` page is 2 by 3, with the six original consumer actions and icons.
The 22 tile IDs, labels, cell positions, and usages match the milestone 1 tables.

[dev.json](examples/dev.json) has three grid pages and a CLIProxyAPI dashboard.
It covers all nine action types, a horizontal span, a vertical span, a key chord,
a dashboard jump, three Claude status widgets, and one CLIProxyAPI widget.
Its command tile describes `cmd /c echo TouchDeck` on Windows.

Later milestone lanes depend on these fixture IDs and roles. `f5` sends F5 with
usage 62, and `shift` applies Shift on the same page. `chord` is a key with `ctrl`
and `shift`. `launch`, `command`, and `open` each use their matching app action.
The `claude-status` widgets `context`, `cost`, and `model` use fields `context_pct`,
`cost_usd`, and `model`. The `proxy` widget uses `cliproxyapi` field `status`.
The `accounts` dashboard uses source `cliproxyapi`, and `accounts_jump` opens it.

The twelve [invalid examples](examples/invalid/) each break one rejection rule.
Each is strict JSON. Its first line has the form:

```json
{"name": "expected: must be <= 255 (maximum)",
```

The rest of the file completes that layout object. The `name` field holds the
expected error substring. It adds no metadata property or JSON comment.
Only files in the bundled `examples/invalid/` directory receive this treatment.
An invalid file supplied from elsewhere must fail, even when its name starts
with `expected: `. An unreadable file or malformed JSON always counts as a problem.

## Errors

Structural errors contain a JSON Pointer, the Ajv message, and the schema keyword
in parentheses. Missing or extra fields also include the quoted field name.
Ajv is pinned by the lockfile. Known page kinds and action types report errors
from the selected branch. Unknown kinds or types report `oneOf`.

The semantic checks run only after structural validation succeeds. They report
the compact byte limit first, then page and tile errors in order with these
stable message forms:

```text
layout compact UTF-8 JSON is <actual> bytes; maximum is 32768 bytes
duplicate id "<id>" at <location>; first used at <location>
page "<page>" tile "<tile>" references missing page "<target>"
page "<page>" tile "<tile>" exceeds grid bounds <rows>x<cols>
page "<page>" tiles "<first>" and "<second>" overlap
```

A location is `page "<id>"` or `page "<page>" tile "<id>"`.
The duplicate check includes page IDs and tile IDs. Overlap reports both tile
IDs once per pair, even when the pair shares several cells.
