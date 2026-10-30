# Design system

This is the look of Open TouchDeck: the desktop app and the screens the Firmware draws on a Device. Every UI ticket follows it. The tokens live in [`app/ui/src/styles/tokens.css`](../app/ui/src/styles/tokens.css), and this page explains each one, then shows how the parts are built from them.

Words follow [`GLOSSARY.md`](../GLOSSARY.md). Every user-visible string comes from [`docs/copy.md`](copy.md). Strings quoted on this page describe a state; where one differs from the copy guide, the copy guide wins.

## The direction: Backlit

**The app is the desk, and each Device is a lit object sitting on it. Only lit things have color.**

A Device is a 5-inch, 1024 × 600 touchscreen with a black glass bezel. The app draws it as that object, on a quiet desk mat, so the Device is always the brightest and most colorful thing in the window. The chrome around it is the unlit case: solid, small and grey. Color appears only where something glows: the Device's screen, the status lights and your focus.

### Principles

1. **Only lit things have color.** The screen, the status lights and the focus ring glow. Everything else is ink on a surface. Primary buttons are ink, not blue.
2. **The Device is an object, not a card.** It has a bezel, a contact shadow and, in dark mode, light spilling onto the mat. No rounded panel wraps it.
3. **Chrome is small, solid and quiet.** Navigation, the inspector and dialogs never compete with the screen. Surfaces are opaque. No blur and no translucent panels: blur over a busy dark screen hurts legibility and costs GPU time in Electron.
4. **The app borrows the Device's vocabulary.** The pager pill marks the current Page, Preset tiles are drawn in screen colors, and the app and the Firmware share one typeface, Archivo.
5. **Each color has one meaning.** Green means connected. Amber means something waits for you. Blue is the Device's accent and your keyboard focus. Selection in the app's own chrome uses ink, so blue stays unambiguous.
6. **Status is a light plus words.** Never color alone.
7. **Disabled looks different from quiet.** A secondary action and a meaningful empty value use secondary text. Disabled controls lose their edge and fade further.

## Tokens

Components use semantic tokens only. The `--palette-*` values exist so the semantic tokens can derive from them; don't use them in components. Never write a raw color in a component.

Light is the default. Dark follows `prefers-color-scheme`. The Appearance setting forces a theme by setting `data-theme="light"` or `data-theme="dark"` on `<html>`. The two dark blocks in `tokens.css` must stay identical; the contrast script checks this.

### Palette

| Token | Value | Role |
| --- | --- | --- |
| `--palette-ash-felt` | `#e4e6e9` | Light desk mat |
| `--palette-slate-felt` | `#1f252c` | Dark desk mat, lighter than the bezel so the glass reads as the darkest thing |
| `--palette-bezel` | `#15171a` | The Device's black glass, both themes |
| `--palette-screen` | `#081018` | The Device's screen background |
| `--palette-backlight` | `#58a8ff` | The Device's accent, selection on the Device, dark-theme focus |
| `--palette-live` | `#40d890` | Connected |
| `--palette-signal` | `#ffb020` | Something waits for you: updates, Needs Firmware |

### App colors

| Token | Light | Dark | Use |
| --- | --- | --- | --- |
| `--mat` | `#e4e6e9` | `#1f252c` | Window background |
| `--mat-grain` | 0.045 | 0.06 | Opacity of the felt grain over the mat |
| `--surface` | `#f6f7f8` | `#262d35` | Inspector rail, Integrations list |
| `--surface-raised` | `#ffffff` | `#2c343d` | Dialogs, popovers, nav cluster, inputs, secondary buttons |
| `--surface-inset` | `#eceef0` | `#1b2026` | Grouped Settings rows, change list, tracks |
| `--ink` | `#1a1d22` | `#eef1f5` | Primary text and icons |
| `--ink-2` | `#565d68` | `#a3acb8` | Secondary text, enabled secondary actions such as **+ Page**, meaningful empty values such as "Nothing" |
| `--ink-disabled` | `#a3a9b1` | `#5f6873` | Disabled text only |
| `--line` | ink at 11% | ink at 9% | Hairlines, rings around raised surfaces |
| `--line-strong` | ink at 20% | ink at 17% | Secondary button border |
| `--line-control` | `#7f858e` | `#7d8591` | Edges that identify a control: inputs, selects, switch off, slider thumb ring |
| `--fill-hover` | ink at 5.5% | ink at 6% | Hover, segmented track |
| `--fill-active` | ink at 9% | ink at 10% | Current nav button, open trigger, Add Tile while open |
| `--fill-disabled` | ink at 4% | ink at 4% | Disabled button fill |
| `--accent` | `#1f6fd1` | `#58a8ff` | Current Page pill, progress, slider fill |
| `--focus-ring` | `#2f7bd9` | `#58a8ff` | The one focus ring |
| `--live-led` / `--live-ink` | `#2fbf71` / `#17804a` | `#40d890` / `#40d890` | Connected light / switch on |
| `--led-off` | `#8a919b` | `#6e7884` | Not connected light |
| `--signal-led` / `--signal-ink` / `--signal-tint` | `#f5a000` / `#8a5600` / `#fdedc8` | `#ffb020` / `#ffc65a` / amber at 14% | Update dot, Firmware update badge |
| `--danger-ink` | `#b42a32` | `#ff7b80` | Clear Tile |
| `--button-primary-bg` / `-ink` | `#1a1d22` / `#ffffff` | `#eef1f5` / `#15191e` | Primary button, tooltip |
| `--scrim` | `rgba(20, 24, 30, 0.28)` | `rgba(8, 10, 14, 0.55)` | Behind dialogs |

The light `--focus-ring` is a step lighter than `--accent` on purpose. It's the one blue that clears 3:1 both on the light mat and on the dark screen, so the same ring works on a Tile.

### Screen colors

The Device's screen is a backlit LCD, so these are the same in both themes. The Firmware uses the same values. Every one is RGB565-exact; the table with each RGB565 value is under [Device translation](#device-translation).

| Token | Value | Use |
| --- | --- | --- |
| `--screen-bg` | `#081018` | Screen background |
| `--screen-tile` | `#182030` | A Tile |
| `--screen-tile-raised` | `#202c40` | A pressed Tile on the Device, the selected Tile in the editor |
| `--screen-track` | `#303848` | The empty part of bars and progress |
| `--screen-pager-idle` | `#606470` | Pager dots for the other Pages |
| `--screen-outline` | `#282c30` | Ring around a Tile that needs newer Firmware |
| `--screen-ink` | `#f0f0f8` | Labels and values |
| `--screen-ink-2` | `#9098b0` | Page title, Tile labels under an icon, second lines |
| `--screen-accent` | `#58a8ff` | Selection, current pager pill, default graph and bar color |
| `--screen-live` | `#40d890` | Running light |
| `--screen-signal` | `#ffb020` | Stopped light, "Needs Firmware" |
| `--screen-danger` | `#ff5860` | Errors on the Device |

**Tile colors** are the backgrounds a user can pick for an action Tile. Labels on them use `--screen-ink`.

| Token | Value |
| --- | --- |
| `--tile-color-violet` | `#483cc0` |
| `--tile-color-green` | `#107050` |
| `--tile-color-blue` | `#205cb8` |
| `--tile-color-red` | `#b02830` |
| `--tile-color-orange` | `#a05810` |
| `--tile-color-slate` | `#606470` |

**Live tile accents** color a graph line and a bar fill. The default is the backlight.

| Token | Value |
| --- | --- |
| `--tile-accent-backlight` | `#58a8ff` |
| `--tile-accent-violet` | `#a08cff` |
| `--tile-accent-coral` | `#ff8868` |
| `--tile-accent-mint` | `#60d4c0` |

**Offline preview.** `--screen-offline-opacity` (0.7) and `--screen-offline-grayscale` (1) draw a Device that isn't connected: its saved Page at 70% over `--screen-bg`, with all color removed. Unlit things have no color, but the Layout stays readable. This is how the app pictures an unplugged Device; the Firmware never uses it.

### Type

One family, Archivo, at two widths. `--font-ui` is `'Archivo Variable', 'Archivo', 'Bahnschrift', 'Segoe UI', system-ui, sans-serif`.

- **Text** (`--stretch-text`, 100%): everything by default.
- **Nameplate** (`--stretch-plate`, 118%, weight 620): Device names, page titles and dialog titles. Wide, flat-sided lettering, like a badge on hardware.

| Size token | Size / leading | Tracking | Use |
| --- | --- | --- | --- |
| `--text-12` | 12 / 16 | +0.01em | Field labels, captions, badges, Settings row hints |
| `--text-14` | 14 / 20 | 0 | Body, buttons, status |
| `--text-16` | 16 / 22 | −0.004em | Inspector title, Integration name, empty-state body |
| `--text-18` | 18 / 24 | −0.008em | Device names, dialog titles (nameplate) |
| `--text-24` | 24 / 28 | −0.014em | Page titles, one Device's name on Home, empty-state title (nameplate) |
| `--text-36` | 36 / 40 | −0.022em | Reserved for onboarding |

Each size has `--leading-*` and `--track-*` tokens. Weights are `--weight-regular` 420, `--weight-medium` 520 and `--weight-semibold` 620. Numbers that update (volume, progress, megabytes) use `font-variant-numeric: tabular-nums`.

### Spacing, radius and elevation

Spacing is a 4 px grid: `--space-1` 4, `-2` 8, `-3` 12, `-4` 16, `-5` 20, `-6` 24, `-8` 32, `-10` 40, `-12` 48, `-14` 56, `-16` 64.

Radius follows physical hierarchy: the bigger and more physical a thing is, the rounder.

| Token | Value | Use |
| --- | --- | --- |
| `--radius-chip` | 999px | Badges, progress, pager |
| `--radius-control` | 8px | Buttons, inputs, nav buttons |
| `--radius-group` | 12px | Nav cluster, Update icon, grouped rows, popovers |
| `--radius-sheet` | 16px | Dialogs, the Integrations list |
| `--radius-tile` | 18px | Tiles, in screen pixels at 1024 × 600 |

The Device's case is `--bezel` with a 1px `--bezel-edge` highlight (white at 7%). It scales with the width it's drawn at. Multiply each ratio by that width in px: `--bezel-pad` 0.034 is the bezel thickness, `--bezel-radius` 0.049 the case corner and `--bezel-screen-radius` 0.012 the glass corner.

| Token | Use |
| --- | --- |
| `--shadow-device` | A Device on the mat: a 1px contact line, a tight shadow and a wide ambient one |
| `--shadow-device-lifted` | A Device on Home under the pointer: 2px higher, longer shadow |
| `--shadow-float` | The nav cluster |
| `--shadow-float-filter` | The Update icon. It's clipped, which would cut a box-shadow, so it casts this drop-shadow from its wrapper |
| `--shadow-dialog` | Dialogs and popovers, always with a 1px `--line` ring |
| `--spill-opacity` | 0 in light, 0.32 in dark: screen light falling on the mat under a connected Device |

### Focus

`--focus-ring-width` (2px), `--focus-ring-offset` (2px) and `--focus-ring` make the one focus ring. Every focusable element gets it from a single `:focus-visible` rule:

```css
:focus-visible {
  outline: var(--focus-ring-width) solid var(--focus-ring);
  outline-offset: var(--focus-ring-offset);
}
```

A few places change where the ring is drawn. All of them keep the token's color and width:

- **Swatches** push the ring 4px further out, so it sits outside the chosen ring instead of on top of it.
- **The slider** draws the ring on its thumb, because the focused element is the thumb's hidden input.
- **The editor canvas** is scaled, so its rings divide by the canvas scale to stay 2 app pixels wide. On a Tile the ring sits inside the Tile edge. The selection ring sits outside, so a selected Tile with focus shows both.

## Components

Each part below is in the reference prototype; [Reference prototype](#reference-prototype) lists the screenshot for each state.

### Nav cluster

- Fixed at the bottom left, 16px from both edges. Order: Home, Layout, Integrations, Settings.
- A puck of `--surface-raised` with radius 12, 4px padding, 2px gaps, a 1px `--line` ring and `--shadow-float`.
- Buttons are 36 × 36, radius 8, with an 18px Lucide icon at stroke 1.9 in `--ink-2`. Hover: `--fill-hover` and `--ink`. Current: `--fill-active` and `--ink`, with `aria-current="page"`. Settings shows as current while the Settings dialog is open.
- A tooltip above each button names it after a 500ms delay. Later tooltips open instantly while the pointer stays in the cluster.

### Update icon

A separate puck 8px right of the nav cluster, so nothing it does moves the nav icons. It isn't rendered when no update exists.

| State | Icon | Label |
| --- | --- | --- |
| Available | Refresh icon with an 8px `--signal-led` dot | Update available |
| Downloading | 22px progress ring: track `--line-strong`, fill `--accent`, stroke 2.5. No dot | Downloading… {pct}% |
| Ready | Refresh icon with the dot | Ready to restart |

- Closed, it shows a 44 × 44 icon. It opens to show the label on mouse hover and on keyboard focus, and closes on leave and blur. Touch never opens it; a tap goes straight to the dialog.
- It's always laid out at full width and `clip-path: inset()` reveals it, so opening it costs no layout. The clipped part takes no pointer hits. Open, the clip reaches past the edge far enough to show the focus ring.
- The button's `aria-label` is the label. The visible label is `aria-hidden`.
- Clicking it opens the Update dialog at the current step.

### Update dialog

440 wide, 24px padding, `--radius-sheet`, `--surface-raised`, a `--line` ring and `--shadow-dialog` over the scrim. The title is nameplate 18/620.

1. **Offer.** The version as the title, the current version under it, then a "What's new" heading over a change list in a `--surface-inset` group. Buttons: Later (secondary) and Update (primary). Nothing downloads before Update.
2. **Downloading.** A 6px progress bar (`--surface-inset` track with a `--line` ring, `--accent` fill drawn with `scaleX`) and a 12px tabular row with megabytes and percent. Hide closes the dialog; the download continues and the Update icon shows its ring.
3. **Ready.** Titled **Ready to restart**. The body names the version and says what restarting does. Buttons: Later and Restart (primary).

Opened from the keyboard, focus lands on the first control. Opened with the pointer, focus goes to the dialog itself, so no ring appears. Escape closes it and focus returns to the Update icon. Each dialog has at most one primary button.

### Settings dialog

600 wide. A head row with the nameplate title and a 32px close icon button. Sections have a 14/620 heading over a group of rows in `--surface-inset` with a `--line` ring. A row is at least 56 tall: title 14/520 and an optional 12px hint in `--ink-2` on the left, the control on the right. Rows inside a group are split by a `--line` hairline.

Controls in rows: a switch, a segmented control (the theme: System, Light, Dark) or a secondary button. Settings for one Device aren't here; they're in the Device panel.

### Device panel

A popover anchored to the Device's name in the editor header: below it, aligned to its start, 8px away. The name keeps its place, so the panel reads as the name opening up.

- 320 wide, 16px padding, `--radius-group`, `--surface-raised`, a `--line` ring and `--shadow-dialog`. Title 14/620.
- Fields, top to bottom: Name (input), Orientation (segmented), Brightness (slider, with the value tabular at the label's right), Photo frame (a switch; when on, Start after (select) and Folder, the path in one ellipsized line with Choose folder on its right in the desktop app only), then a row with the Firmware version and its status on the left and Restart on the right.
- The trigger is the nameplate name with a chevron. It shows `--fill-active` while the panel is open, and the chevron turns 180°.

### Home

Home has no headline. The Devices are the content, centered on the mat.

- **One Device:** drawn 560px wide, name at 24 nameplate.
- **Two or three:** 312px each, in one row with 56px gaps.
- **More than three:** the row wraps. Not prototyped yet.
- **Caption under each Device**, aligned to its left edge: the name in nameplate 18/620, cut with an ellipsis, then a row with the status light and its words. A Device that isn't connected adds when it was last seen. A Firmware update adds the amber badge.
- **The Device is the button.** Clicking it opens its Layout. On a fine pointer it lifts 2px with `--shadow-device-lifted`.
- **Connected:** the lit screen and, in dark mode, the light spill on the mat.
- **Not connected:** the saved Page stays visible through the offline preview tokens, with no spill. The status light is `--led-off`, and both the status and last-seen words stay.
- **No Devices:** a switched-off Device silhouette, 360px at 55% opacity, then a nameplate title, a 16px body in `--ink-2` that asks for a USB data cable, a 14px line about New boards, and a secondary **Connection help** button. No promise about how long detection takes. Max width 440.

### Layout editor

A grid of the canvas and a 340px inspector rail on the right edge.

**Header,** 64 tall over the canvas: the Device panel trigger, the connection status, then on the right the save state (12px `--ink-2` with a check) and **Add Tile**, a secondary button with a plus icon.

**Canvas.** The Device, centered, drawn between 420 and 680 wide to fit, with no light spill. Under it, 32px down, the Page tabs: 34 tall, `--ink-2`; the current tab is `--ink` with an 18 × 4 `--accent` pill 6px below, the same shape as the Device's pager. **+ Page** follows in `--ink-2`; it's an enabled action, so it never uses the disabled color.

On the canvas, the editor adds marks the Device never draws, in screen pixels:

- **Selected Tile:** `--screen-tile-raised` with a 5px `--screen-accent` ring 6px outside it.
- **Empty cell:** a 3px dashed `--screen-pager-idle` outline at `--radius-tile` with a 44px plus in `--screen-ink-2`. Clicking one opens Add Tile for that cell.
- **Add Tile target:** the cell gets a 5px solid `--screen-accent` outline, a `--screen-tile` fill and an accent plus.

**Inspector.** `--surface` with a 1px `--line` left edge, 20px padding and 20px between fields. The head has a 44px Tile thumbnail in screen colors, the Tile's name at 16/620 and a 12px line saying what it is and where. The footer stays pinned to the bottom while the fields scroll: Duplicate on the left, Clear Tile (quiet danger) on the right.

| Action Tile | Live tile |
| --- | --- |
| Label | Label |
| Icon: the icon, its name, Change | Integration, a select led by the Integration's icon |
| Background: Default, the six Tile colors and a custom color picker | Value |
| Size: 1 × 1, 2 × 1, 2 × 2 | Display: Graph and value, or Value only (graph Tiles) |
| When tapped | History (graph Tiles) |
| When held. "Nothing" is a real choice and uses `--ink-2` | Accent: the four Live tile accents |
| | Size. Sizes the Tile can't draw are disabled, such as 1 × 1 for a graph |

**Add Tile** replaces the inspector while it's open, so the Page and the target cell stay in view. A popover from the header button would cover the canvas.

- The head says where the Tile goes: the row, column and Page. On a full Page it says the Tile starts a new Page.
- **Action:** the action kinds in a two-column grid. Each is a bordered button with a 32px icon tile in screen colors, a 12/620 name and a 12px `--ink-2` line.
- **Preset tiles:** grouped under each connected Integration, drawn as Preset tile previews.
- Cancel sits in the pinned footer, and Escape also cancels. Picking a choice places the Tile and selects it, so the inspector shows its fields.
- While it's open, the Add Tile button shows `--fill-active` and `aria-expanded="true"`.

### Integrations

A page title in nameplate 24 with a 14px `--ink-2` line under it, then one list sheet: `--surface`, `--radius-sheet` and a `--line` ring. It's a list, not a card grid.

Each row is a grid of a 40px glyph, the name (16/620) with one line about it in `--ink-2`, the Integration's Preset tiles drawn as 84 × 56 previews in screen colors, and its state on the right. The state is a status light with words, or a **Set up** or **Create** button. The Custom Integration's line says what the user gets, not how it works; URLs, JSON and commands belong to its setup flow.

### Buttons

| Kind | Look |
| --- | --- |
| Secondary, the default | 34 tall, 14px padding, `--surface-raised`, a 1px `--line-strong` border, `--ink` at 14/520. Hover mixes in 5% ink |
| Primary | `--button-primary-bg` and `--button-primary-ink`: ink, not blue. One per dialog |
| Quiet danger | No border or fill, `--danger-ink` |
| Small | 28 tall, 10px padding, 12px text |
| Icon | 32 × 32, no border or fill, `--ink-2`; hover `--fill-hover` and `--ink` |
| Open toggle | `--fill-active` while `aria-expanded="true"` |
| Disabled | No border, `--fill-disabled`, `--ink-disabled`, `cursor: not-allowed`, no press |

Pressing a button scales it to `--press-scale`.

### Fields

- **Label:** 12/620 `--ink-2`, 8px above the control.
- **Input and select:** 36 tall, radius 8, a 1px `--line-control` edge on `--surface-raised`. A select is a button showing the value and an up-down chevron in `--ink-2`, with an optional leading icon.
- **Segmented control:** a `--fill-hover` track with a `--line` ring and 2px padding. Segments are 28 tall at 12/520 in `--ink-2`. The chosen one is `--surface-raised` with `--ink` and a hairline ring. A disabled segment uses `--ink-disabled`.
- **Switch:** 40 × 24. Off is `--line-control`, on is `--live-ink`, and the 20px white thumb slides across.
- **Slider:** a 6px `--surface-inset` track with a `--line` ring, an `--accent` fill and an 18px white thumb ringed in `--line-control`.
- **Swatches:** 28px circles. The chosen one shows a check, a 2px `--surface` gap and a 2px `--ink` ring. The check is white on Tile colors and `--screen-bg` on the light accents.

### Selection and focus

| | In the app | On the Device and canvas |
| --- | --- | --- |
| Current or chosen | `--fill-active`, a raised segment, or an `--ink` ring | `--screen-accent` ring, current pager pill |
| Keyboard focus | The focus ring | The focus ring, inside the Tile |
| Hover | `--fill-hover` | Empty cells brighten to `--screen-ink-2` |

Popovers and dialogs follow one rule. Opened from the keyboard, focus lands on the first control. Opened with the pointer, focus goes to the popup itself. Closing returns focus to the trigger.

### Status lights, badges and tooltips

- **Status light:** an 8px dot before its words. Connected is `--live-led` with a 3px halo of the same color at 18%. Not connected is `--led-off`.
- **Badge:** a 24 tall chip in `--signal-tint` with `--signal-ink` text at 12/620 and a 14px icon. Used for a Firmware update.
- **Tooltip:** `--button-primary-bg` with `--button-primary-ink`, 12/520, radius 7, 5 × 9 padding, 8px from its trigger. A tooltip never repeats a visible label.

## Motion

Motion gives feedback and shows where things come from. It never decorates. Animate only `transform`, `opacity` and `clip-path`, plus color on hover. Never animate `width`, `height`, `top`, `left`, margins or blur.

| What | Property | Duration | Easing |
| --- | --- | --- | --- |
| Press on buttons, nav buttons, Add Tile choices | `scale` to `--press-scale` (0.97) | `--duration-press` 120ms | `--ease-out` |
| Press on a Device on Home | `scale` to 0.985 | 120ms | `--ease-out` |
| Hover color | background, color | `--duration-hover` 150ms | `ease` |
| Device lift on Home (fine pointer only) | `translateY(-2px)` and shadow | 150ms | `--ease-out` |
| Tooltip in | opacity, scale 0.96 to 1 from its trigger | `--duration-tooltip` 140ms | `--ease-out` |
| Device panel in | opacity, scale 0.97 to 1 from the trigger | `--duration-popover` 180ms | `--ease-out` |
| Dialog in | opacity, scale 0.97 to 1 from the center; the scrim fades | `--duration-dialog` 220ms | `--ease-out` |
| Any exit | the reverse of its enter | `--duration-exit` 140ms | `--ease-out` |
| Update icon opens | `clip-path` | `--duration-expand` 280ms | `--ease-out` |
| Update icon closes | `clip-path` | `--duration-collapse` 200ms | `--ease-out` |
| Update label in | opacity, after `--delay-label` 60ms | `--duration-label` 160ms | `--ease-out` |
| Update label out | opacity | 140ms | `--ease-out` |
| Switch thumb | `translate` | 180ms | `--ease-out` |
| Device panel chevron | `rotate(180deg)` | 180ms | `--ease-out` |
| Download progress | `scaleX` | 120ms | `linear` |
| Theme change | background, color | `--duration-theme` 240ms | `ease` |

`--ease-out` is `cubic-bezier(0.23, 1, 0.32, 1)`. `--ease-in-out` (`cubic-bezier(0.77, 0, 0.175, 1)`) is for things that move on screen, and `--ease-drawer` (`cubic-bezier(0.32, 0.72, 0, 1)`) is for sheets, if one is ever added.

**What never animates:** changing Page, changing section from the nav, selecting a Tile, swapping the inspector between action, Live tile and Add Tile, and anything you do with the keyboard. Keyboard focus opens the Update icon on the next frame.

**Interruptible.** Use CSS transitions, not keyframes, for anything a pointer can reverse. A transition turns back from wherever it is: leaving the Update icon halfway through opening closes it from that width.

**Reduced motion.** Under `prefers-reduced-motion: reduce`, the tokens remove movement and keep fades: `--press-scale` becomes 1, popovers and dialogs fade in 120ms and 140ms without scaling, the Device lift is off, and every Update icon duration and delay becomes 0ms, so it jumps straight to its end state.

## Device translation

The Firmware draws its screens with LVGL at 1024 × 600 in RGB565. To match the app, it takes the screen colors, the geometry and the three Archivo instances below. Nothing else on this page reaches the Device.

### Colors in RGB565

Every screen token is RGB565-exact: red and blue are multiples of 8 and green a multiple of 4, or the channel is 255. The `UI_RGB565` macro in `firmware/components/ui/ui_theme.h` keeps every bit, so the panel shows the token itself. Keep it that way when a color changes; the contrast script fails any lossy screen token.

| Token | Hex | RGB565 | Firmware |
| --- | --- | --- | --- |
| `--screen-bg` | `#081018` | `0x0883` | `UI_RGB565(8, 16, 24)` |
| `--screen-tile` | `#182030` | `0x1906` | `UI_RGB565(24, 32, 48)` |
| `--screen-tile-raised` | `#202c40` | `0x2168` | `UI_RGB565(32, 44, 64)` |
| `--screen-track` | `#303848` | `0x31C9` | `UI_RGB565(48, 56, 72)` |
| `--screen-pager-idle` | `#606470` | `0x632E` | `UI_RGB565(96, 100, 112)` |
| `--screen-outline` | `#282c30` | `0x2966` | `UI_RGB565(40, 44, 48)` |
| `--screen-ink` | `#f0f0f8` | `0xF79F` | `UI_RGB565(240, 240, 248)` |
| `--screen-ink-2` | `#9098b0` | `0x94D6` | `UI_RGB565(144, 152, 176)` |
| `--screen-accent` | `#58a8ff` | `0x5D5F` | `UI_RGB565(88, 168, 255)` |
| `--screen-live` | `#40d890` | `0x46D2` | `UI_RGB565(64, 216, 144)` |
| `--screen-signal` | `#ffb020` | `0xFD84` | `UI_RGB565(255, 176, 32)` |
| `--screen-danger` | `#ff5860` | `0xFACC` | `UI_RGB565(255, 88, 96)` |
| `--tile-color-violet` | `#483cc0` | `0x49F8` | `UI_RGB565(72, 60, 192)` |
| `--tile-color-green` | `#107050` | `0x138A` | `UI_RGB565(16, 112, 80)` |
| `--tile-color-blue` | `#205cb8` | `0x22F7` | `UI_RGB565(32, 92, 184)` |
| `--tile-color-red` | `#b02830` | `0xB146` | `UI_RGB565(176, 40, 48)` |
| `--tile-color-orange` | `#a05810` | `0xA2C2` | `UI_RGB565(160, 88, 16)` |
| `--tile-color-slate` | `#606470` | `0x632E` | `UI_RGB565(96, 100, 112)` |
| `--tile-accent-backlight` | `#58a8ff` | `0x5D5F` | `UI_RGB565(88, 168, 255)` |
| `--tile-accent-violet` | `#a08cff` | `0xA47F` | `UI_RGB565(160, 140, 255)` |
| `--tile-accent-coral` | `#ff8868` | `0xFC4D` | `UI_RGB565(255, 136, 104)` |
| `--tile-accent-mint` | `#60d4c0` | `0x66B8` | `UI_RGB565(96, 212, 192)` |

Two screen colors are mixes rather than tokens, because they follow the user's accent:

- **Graph area:** the accent mixed 14% into `--screen-tile`, which is `lv_color_mix(accent, tile, 36)`.
- **Toggled-on Tile:** a 4px inset ring of `--screen-ink`.

### Geometry

All values are screen pixels at 1024 × 600, landscape.

| Token | Value | What |
| --- | --- | --- |
| `--screen-pad-x` | 28 | Left and right margin of the Tile grid |
| `--screen-pad-top` | 76 | Top of the Tile grid, under the Page title |
| `--screen-pad-bottom` | 56 | Bottom margin, over the pager |
| `--screen-gap` | 16 | Between Tiles |
| `--radius-tile` | 18 | Tile corners |
| `--screen-title-x`, `--screen-title-y` | 36, 22 | Page title position |
| `--screen-pager-dot` | 12 | Pager dot, `--screen-pager-idle` |
| `--screen-pager-current` | 36 | Current Page pill width, `--screen-accent` |
| `--screen-pager-bottom` | 18 | Pager distance from the bottom edge |

Graph lines are 4px in the accent. A second series is 3px in `--screen-ink-2`. Bars are 8px on `--screen-track` with round ends; the volume bar is 28px with radius 10. A Tile that needs newer Firmware has no fill, a 3px `--screen-outline` ring, its label in `--screen-ink-2` and "Needs Firmware {x.y}" in `--screen-signal`.

### Fonts on the Device

The Firmware renders three static instances of Archivo. They're cut from the variable font, which has a width axis from 62 to 125 and a weight axis from 100 to 900:

| Instance | Weight | Width | Sizes (px) | Used for |
| --- | --- | --- | --- | --- |
| Archivo Medium | 520 | 100 | 22, 26, 44 | Second lines, Tile labels, key legends |
| Archivo Semibold | 620 | 100 | 32, 40 | Live tile values and now-playing titles, the one big number on a Live tile |
| Archivo Nameplate | 620 | 118 | 26 | Page title |

The app draws its Device previews with the same values: `--screen-font`, `--screen-weight-medium`, `--screen-weight-semibold`, `--screen-stretch-plate` and the `--screen-text-*` sizes, which are `title` 26, `label` 26, `sub` 22, `value` 32, `big` 40 and `legend` 44.

That makes six LVGL fonts, each at 4 bits per pixel. The glyph set for all but the legend size:

- U+0020 to U+007E, Basic Latin
- U+00A0 to U+00FF, Latin-1 Supplement
- U+2013 and U+2014, the dashes; U+2018, U+2019, U+201C and U+201D, the curly quotes; U+2022, the bullet; U+2026, the ellipsis that in-progress states use; U+2212, the minus sign

The 44px legend font needs Basic Latin only. Icons aren't in the font: Tile icons are Assets the Firmware stores and draws (ADR 0004).

To build an instance, cut it from the variable font with fontTools, then convert it with `lv_font_conv`:

```sh
fonttools varLib.instancer "Archivo[wdth,wght].ttf" wght=520 wdth=100 -o Archivo-Medium.ttf
lv_font_conv --font Archivo-Medium.ttf --size 26 --bpp 4 --format lvgl \
  -r 0x20-0x7E -r 0xA0-0xFF -r 0x2013-0x2014 -r 0x2018-0x2019 -r 0x201C-0x201D \
  -r 0x2022 -r 0x2026 -r 0x2212 -o ui_font_medium_26.c
```

### Desktop only

The Firmware never draws these. They exist only in the app's picture of a Device or in the app's chrome:

- The felt grain on the mat
- The screen's light spilling onto the mat
- The glass reflection across the screen
- The bezel, its edge highlight and every shadow
- Blur of any kind
- The offline preview's dimming and grey
- The editor's marks: selection ring, focus ring, empty-cell outlines and the Add Tile target
- The album-art gradient in the prototype, which stands in for a real cover image

LVGL 9.6 can't dither a gradient, so the Device's own fills stay flat colors.

## Contrast

`contrast.mjs` in the reference prototype measures WCAG 2.2 contrast for every text and UI-part pair in both themes. It runs against `tokens.css`, so a token change shows up on the next run. Text pairs need 4.5:1. Large text and UI parts, including the focus ring, need 3:1.

It also measures:

- text and the focus ring over the felt grain's darkest or lightest pixel
- the offline preview, after dimming and grey
- every Device pair after `UI_RGB565`
- that both dark blocks match
- that every screen token is RGB565-exact

Three kinds of pair are reported but not gated, as WCAG allows: disabled text, status lights (always next to words) and button borders (the label identifies the button).

## Fonts and licenses

- **Archivo** is by the Archivo Project Authors (Omnibus-Type) under the [SIL Open Font License 1.1](https://github.com/Omnibus-Type/Archivo/blob/master/OFL.txt). It declares no Reserved Font Name, so converted instances can keep the name. The app bundles it from `@fontsource-variable/archivo`, whose package license is `OFL-1.1`. Ship the OFL text next to the font files in the app, and with the converted fonts in the Firmware source. Bundling the files is #73's job.
- **Lucide**, the app's interface icons, is ISC-licensed through `lucide-react`.
- Icons for the icon picker follow [`docs/research/icon-sources.md`](research/icon-sources.md).

## Reference prototype

A static React prototype shows every state on this page. It's local only, under the git-ignored `.verify/69/final/`:

| Path | What |
| --- | --- |
| `src/` | The prototype. Every state has a URL hash, such as `#editor-live` |
| `tokens.css` | The same file as `app/ui/src/styles/tokens.css` |
| `contrast.mjs` | The contrast script; `contrast-output.txt` is its last run |
| `motion-check.mjs` | Checks that the Update icon animates by clip only, reverses mid-flight and jumps under reduced motion |
| `shoot.mjs` | Takes every screenshot below and fails if a focus shot has no visible ring |
| `screens/` | The screenshots, light and dark, at 1180 × 760 |

| Screens | Files in `screens/`, each with `-light` and `-dark` |
| --- | --- |
| Home with 0, 1 and 3 Devices, one not connected | `home-empty`, `home-single`, `home-three` |
| Editor, action Tile and Live tile selected | `editor-action`, `editor-live` |
| Add Tile | `editor-add` |
| Device panel | `editor-devicepanel` |
| Every Update state | `update-hidden`, `update-available`, `update-hovered`, `update-dialog`, `update-downloading`, `update-downloadingdock`, `update-ready`, `update-readydock` |
| Keyboard focus | `settings-focus`, `focus-nav`, `focus-update`, `focus-devicename`, `focus-pages`, `focus-inspector`, `focus-dialog` |
| Integrations | `integrations` |

To run it from `.verify/69/final/`:

```sh
npm install
npx vite build && npx vite preview --port 4189 --strictPort
node shoot.mjs http://localhost:4189/
node motion-check.mjs http://localhost:4189/
node contrast.mjs
```
