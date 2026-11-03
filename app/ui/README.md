# Open TouchDeck UI

The React UI the Companion serves. It talks to the Companion only through the generated client in
`src/companion/api.gen.ts`.

Run these from `app/`:

| Command | What it does |
| --- | --- |
| `pnpm dev:ui` | The UI on `http://127.0.0.1:5174/`, against the Companion running on this PC. Start the Companion first, for example with `cargo run -p td-companion --features test-hooks -- --mock-devices 1`. |
| `pnpm dev:ui:demo` | The UI against a fake Companion with three demo Devices. No Companion needed. Add `?update=available` for an app update to offer; `none`, `no-notes`, `disabled`, `offline` and `failed` give the updater's other answers. Two browsers start paired; `?browsers=0` starts with none. `?data=worst` swaps in the worst-case data from `src/companion/fake-worst.ts`: names at their limits, every script, huge counts and 10 Devices. `?devices=` sets how many Devices there are, 0 to 5 on demo data and 0 to 10 on the worst case, and `?offline=all` unplugs them all. A switch at the bottom center flips between these; `?toggle=0` hides it. |
| `pnpm build:ui` | The production build in `ui/dist`, which the Companion embeds. |
| `pnpm test:ui` | Vitest. `pnpm test` runs it too. |
| `pnpm e2e:ui` | Playwright against `dev:ui:demo`. `e2e/break-ui.spec.ts` checks every screen on the worst-case data at 800 × 600, 1180 × 760 and 3840 × 2160 in both themes. Set `TD_BREAK_SURVEY` to a folder to record each screen's screenshot and spills instead of failing. |
| `pnpm e2e:ui:companion` | One Playwright check against the real Companion with one mock Device. It builds and starts the Companion itself, with its own data folder under `target/`. |

In `pnpm dev:ui`, Vite proxies `/api`, the events WebSocket included, to the Companion, and rewrites
`Host` and `Origin` to `127.0.0.1:PORT`. It finds the port and pairing token in the Companion's
`companion.json`, and plays the part Electron main plays later: it checks the Companion's proof and
mints a window session for the page. Only the dev page itself can use the proxy or get a session. See
[`dev/companion.ts`](dev/companion.ts).
