# Companion local API, v1

This is the contract between the Companion and its clients: the Electron window, a browser tab, and tests. Every Companion, Electron and UI ticket builds against it. When the code and this page disagree, one of them has a bug. The `api_contract_is_documented` test fails the build when an endpoint, event or error code is missing here.

Related decisions: [ADR 0001](adr/0001-rust-companion-behind-electron.md) (Rust Companion behind Electron, loopback only), [ADR 0005](adr/0005-updater-keeps-the-signing-key.md) (updates stay in Electron) and [ADR 0006](adr/0006-command-approval-on-the-pc.md) (command approvals happen on the PC). The words Device, New board, Firmware, Companion, Layout, Page, Tile, Live tile, Preset tile, Asset and Integration mean what [GLOSSARY.md](../GLOSSARY.md) says.

## At a glance

- JSON routes under `http://127.0.0.1:PORT/api/v1`, served by axum, bound to `127.0.0.1` only.
- One WebSocket at `/api/v1/events`. It opens with a `sync` snapshot of the shared state, then sends full-object deltas.
- The Companion serves the built UI from the same origin, so the window and a browser tab run the same page.
- Every call carries `Authorization: Bearer tds_…`, a session token. Session tokens are minted from a pairing token that rotates on every Companion start. There are no cookies and no CORS.
- Electron's main process checks an HMAC proof that the Companion knows the pairing token before it sends the token. Browsers pair with a one-time code.
- Devices are addressed by their USB-serial Device id. A New board keeps its id when it becomes a Device.
- Slow Device work is a Job, at most one per Device.
- Every error has one shape: `{"error": {"code", "message", "details"}}`.
- No session can approve a command. Approvals happen in a native dialog on the PC.
- Routes are declared once with a typed `Endpoint` trait. ts-rs generates the TypeScript client, and a test fails when the committed file is stale.
- `/api/v1` changes only by addition.

## Transport

| Part | Rule |
|---|---|
| Requests | `GET`, `POST`, `PUT`, `PATCH` and `DELETE` on `/api/v1/...`. JSON bodies use `application/json`. Image uploads use `image/*` |
| Responses | JSON, or raw bytes with their media type for Asset files. `204` for an empty success |
| Events | `GET /api/v1/events` upgrades to a WebSocket. The server sends events. The client sends only `subscribe` and `unsubscribe` |
| Slow work | `202 Accepted` with a `Job`, then `job.updated` events. `GET /jobs/{id}` recovers after a reload |
| UI | Every other path serves the embedded UI. Unknown non-API paths return `index.html`, so client routes work after a reload |

We compared three shapes. REST plus one WebSocket won because Firmware bundles, Asset uploads and icon bytes travel as plain request and response bodies, and every route can be tested with `curl` or `reqwest`.

- **JSON-RPC 2.0 over one WebSocket.** It orders replies and events on one stream. It loses on binary data (base64 adds a third, and a 6 MB frame stalls console and progress events), on reconnects (every in-flight call fails), on middleware (Host and Origin checks per message) and on testing. The `sync` snapshot and full-object Job updates give REST the ordering where it matters.
- **HTTP plus Server-Sent Events.** `EventSource` can't send an `Authorization` header, so the token would go in a cookie or the URL. It flows one way, so changing subscriptions needs extra routes.

## Security model

### Threat model

A valid session can push a Layout and read Integration status. Running a program also needs a person to answer a native dialog on the PC ([ADR 0006](adr/0006-command-approval-on-the-pc.md)). Reviews still grade every auth defect as severe: it gives an attacker the user's Devices, Layouts and keys typed by key Tiles.

| Actor | Reaches `127.0.0.1:PORT`? | Defense |
|---|---|---|
| A web page in the user's browser (cross-site fetch, form post, WebSocket) | Yes, through the browser | No ambient credential, an Origin check that also covers the WebSocket upgrade, no CORS, `Sec-Fetch-Site` |
| DNS rebinding (`evil.example` resolves to `127.0.0.1`) | Yes, with `Host: evil.example:PORT` | Host allowlist on every request, static files included |
| Another Windows user on the same PC | Yes, loopback is shared | A runtime file only the owner can read, a pairing token that rotates per start, the HMAC proof before Electron sends the token, a lockout on wrong pairing codes |
| A process running as the same user | Yes | Out of scope. It can read the user's files and keyring anyway |
| Another machine on the network | No | The Companion binds `127.0.0.1` only and asserts `local_addr().ip().is_loopback()` at start. No setting changes the bind address (ADR 0001) |

### Secrets

All secrets come from the OS random source (`getrandom`). Tokens are base64url without padding. The prefixes let the pre-push hook and secret scanners recognize them.

| Secret | Format | Entropy | Lifetime | Stored |
|---|---|---|---|---|
| Pairing token | `tdp_` + 43 chars | 256 bits | Until the next Companion start or Reset pairing | `runtime/companion.json`, readable only by the user |
| Window session token | `tds_` + 43 chars | 256 bits | Until its window closes or the Companion exits | Companion memory and Electron main memory only |
| Browser session token | `tds_` + 43 chars | 256 bits | 7 days after its last use | Browser `localStorage`. The Companion keeps only `SHA-256(token)` in `runtime/sessions.json` |
| Pairing code | 12 Crockford base32 chars shown as `XXXX-XXXX-XXXX` | 60 bits | 120 s, single use, at most 5 outstanding (the oldest is evicted) | Companion memory only |

- The Companion compares the pairing token with `subtle::ConstantTimeEq`.
- It looks up session tokens by `SHA-256(token)`. The hash input is secret, so lookup timing leaks nothing useful.
- Five wrong pairing codes in a row invalidate every outstanding code. That caps a local guesser at 5 tries against 60 bits per round. The tray says "Pairing was blocked after wrong codes. Start pairing again." An attacker can only make the user start again.
- Logs never contain tokens or codes. The `tracing` layer redacts `Authorization` and `Sec-WebSocket-Protocol`.

### Port and runtime file

**Port.** On first run the Companion picks a random free port in 49152 to 65535 and saves it in `companion-settings.json`, which isn't secret. Later starts reuse it, so browser bookmarks and `localStorage`, which is scoped to `127.0.0.1:PORT`, survive restarts. If the port is taken, the Companion picks and saves a new one, and browsers must pair again. The tray says so.

**Location.** `%LOCALAPPDATA%\io.github.duresa7.touchdeck\runtime\companion.json`, built from `dirs::data_local_dir()` and `td_core::app_dirs::APP_ID`.

- It's under Local, not Roaming, because the token belongs to this PC and must not roam with a domain profile.
- `TOUCHDECK_APP_DATA_DIR` moves it to `<dir>/runtime` for tests and scratch runs.
- On Linux source builds it's `$XDG_RUNTIME_DIR/touchdeck/companion.json`, in a `0700` directory, as a `0600` file opened with `O_NOFOLLOW`.

**Contents.**

```json
{ "version": 1, "api": ["v1"], "port": 51234, "token": "tdp_…", "pid": 4242,
  "build": "<git sha>", "started_at_ms": 1767225600000 }
```

**Writing it on Windows** (crate `windows-sys`):

1. Read the user's SID from the process token (`OpenProcessToken`, `GetTokenInformation(TokenUser)`).
2. Create `runtime\` if it's missing. If `runtime\` is a reparse point (a junction or symlink), refuse to start. Apply the protected DACL `D:P(A;OICI;FA;;;<SID>)` with owner `<SID>` through `SetNamedSecurityInfoW` and `PROTECTED_DACL_SECURITY_INFORMATION`. Nothing inherits from `%LOCALAPPDATA%`. There's no Administrators or SYSTEM entry, because admins can take ownership anyway and backup tools use the backup privilege.
3. Create `runtime\.companion.json.<pid>.tmp` with `CreateFileW(CREATE_NEW)` and `SECURITY_ATTRIBUTES` from `O:<SID>D:P(A;;FA;;;<SID>)`. The file is private from its first byte.
4. Write it, call `FlushFileBuffers`, then `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`. A rename on one volume keeps the file's own security descriptor.
5. Read the final file's security with `GetNamedSecurityInfoW`. The DACL must be protected, hold exactly one allow entry for the user's SID, and the owner must be the user. Otherwise the Companion deletes the file, refuses to serve the API, and shows a tray error.
6. Write the file only after the listener is bound. Delete it on clean shutdown. `runtime\sessions.json` is written the same way.
7. Every 60 s, check that the file still exists with the same DACL. If it's gone or changed, rewrite it with the same token.

**Single instance.** A named mutex `Local\touchdeck-companion`. `Local\` is per sign-in session, so two signed-in users get two Companions, each with its own port and file.

### Request rules

Tower layers apply these to every request, including static UI files and the WebSocket upgrade, before routing. The decision is one pure function, `admit(&RequestMeta, port) -> Result<(), SecurityError>`, with unit tests for each row.

| # | Check | Passes | Fails with |
|---|---|---|---|
| 1 | `Host` | Exactly `127.0.0.1:PORT` or `localhost:PORT`. A missing `Host` fails | 421 `security.host` |
| 2 | `Origin`, if present | Exactly `http://127.0.0.1:PORT` or `http://localhost:PORT`. `null` fails | 403 `security.origin` |
| 3 | `Sec-Fetch-Site`, if present, on `/api/*` | `same-origin` or `none` | 403 `security.origin` |
| 4 | `Sec-Fetch-Site`, if present, on static `GET`s | Anything with `Sec-Fetch-Mode: navigate`, otherwise `same-origin` or `none` | 403 `security.origin` |
| 5 | Auth on `/api/v1/*`, except the public routes below | A valid `Authorization: Bearer tds_…`, or the WebSocket subprotocol | 401 `auth.missing`, `auth.invalid`, `auth.revoked` or `auth.expired` |
| 6 | Body size, set per endpoint | JSON 1 MiB by default. Image uploads 16 MiB | 413 `request.too_large` |
| 7 | `Content-Type` | `application/json` for JSON bodies, `image/*` for uploads | 415 `request.media_type` |

- **Public routes.** `GET /system/ping` returns `{api: ["v1"]}`. `POST /auth/proof` returns the HMAC proof. `POST /auth/sessions` needs the pairing token. `POST /auth/pair` needs a pairing code. Features can't add public routes, because `Endpoint` has no auth switch. The auth module mounts these four itself, before the auth layer.
- **CORS.** The Companion never sends `Access-Control-*` headers. A preflight `OPTIONS` carries a foreign `Origin` and fails rule 2.
- **CSRF.** No credential is ambient, so a forged request carries no token. Requiring `application/json` also blocks plain cross-site form posts before auth runs.
- Requests without `Origin`, such as Electron main, tests and scripts, pass rules 2 to 4 and still need auth.

### Response headers

Every response, API and static, carries:

```
Content-Security-Policy: default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';
  img-src 'self' blob: data:; connect-src 'self' ws://127.0.0.1:PORT ws://localhost:PORT;
  font-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'
X-Frame-Options: DENY
X-Content-Type-Options: nosniff
Referrer-Policy: no-referrer
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Resource-Policy: same-origin
Cache-Control: no-store
```

Hashed static files override `Cache-Control` with `public, max-age=31536000, immutable`. `index.html` keeps `no-store`. The UI makes no third-party requests: the Companion fetches and caches icon catalogs, so `connect-src` and `img-src` stay on `'self'`. The UI never uses `dangerouslySetInnerHTML`.

### How the Electron window gets its session

1. Electron main is single-instance. It reads `companion.json`. If the file is missing or the proof in step 2 fails, it starts the Companion and waits up to 10 s for a file with a newer `started_at_ms`.
2. **It checks the Companion before sending the token.** It sends `POST /api/v1/auth/proof` with `{nonce}`, 32 random bytes, and receives `{proof}`, which is `base64url(HMAC-SHA256(pairing_token, "touchdeck-companion-proof\0" || nonce))`. Main recomputes the HMAC with Node's `crypto` and compares with `timingSafeEqual`. A process squatting on the port can't answer, so it never receives the pairing token and the window never loads its page.
3. **It mints a session.** `POST /api/v1/auth/sessions` with `Authorization: Bearer tdp_…` and `{kind: "window"}` returns `{id, token}`.
4. `BrowserWindow` loads `http://127.0.0.1:PORT/` with `contextIsolation: true`, `sandbox: true` and `nodeIntegration: false`. `will-navigate` and `setWindowOpenHandler` keep it on the Companion origin. Other URLs open in the default browser.
5. The preload exposes exactly `window.openTouchDeck = { version: 1, session(): Promise<string | null>, onSessionChanged(cb), updates: { check, download, install } }`. `ipcMain.handle('session')` returns the token only when `event.senderFrame` is the top frame and its origin is the Companion origin. The renderer never sees the pairing token.
6. On a 401 with an `auth.*` code, or WebSocket close 4401, the client asks main for the session again. Main re-reads the file, checks the proof, mints a session, fires `onSessionChanged`, and the client retries once.
7. When the window closes, main sends `DELETE /auth/sessions/self`.

### How a browser pairs

The user starts pairing from **Open in browser** in the tray or in the window. The Companion mints a pairing code and does three things with it:

1. It opens `http://127.0.0.1:PORT/pair#code=XXXX-XXXX-XXXX` in the default browser. The code is in the fragment, which browsers never send to a server or in a `Referer`.
2. It shows the code in the tray's pairing notification. If the window started pairing, the window shows it too, with a **Copy link** button for a browser that isn't the default.
3. If a browser drops the fragment when the link is opened, the Companion opens `/pair?code=…` instead. The query form is allowed because the code is single-use, lasts 120 s and never leaves loopback. The page removes it with `history.replaceState(null, '', '/pair')` before doing anything else.

The `/pair` page reads the code from the fragment, the query, or a text field where the user types it. Input ignores case, spaces and dashes, and reads `O` as `0` and `I` or `L` as `1`. The page sends `POST /api/v1/auth/pair` with `{code, label}`, where `label` comes from the user agent (for example "Edge on Windows"), and receives `{id, token}`. It stores the token in `localStorage` as `touchdeck.session`, then goes to `/`.

- A used, expired or wrong code returns 401 `auth.code_invalid`. The page says "This pairing code has expired or is wrong. Choose Open in browser from the tray icon again."
- After the lockout it returns 429 `auth.code_locked`.
- A browser that opens the Companion without a session gets 401 on `GET /api/v1/system` and shows the `/pair` page with the text field.
- `POST /auth/pair-codes` needs a window session. A browser session gets 403 `auth.forbidden`, so a stolen browser token can't mint more sessions that would outlive its revoke.

### Session lifecycle

| | Window session | Browser session |
|---|---|---|
| Minted by | Electron main, with the pairing token | `POST /auth/pair`, with a pairing code |
| Stored | Memory only, in the Companion and Electron main | Browser `localStorage`. The Companion stores only its SHA-256 hash |
| Ends when | Its window closes (`DELETE /auth/sessions/self`), the Companion exits, or no WebSocket from it has been open for 60 s | 7 days pass without a request, or it's revoked |
| Can mint pairing codes | Yes | No |

- `last_used_ms` is written to disk at most once an hour.
- **Settings → Sessions** lists every session (`GET /auth/sessions`) with its kind, label, created time and last use. The caller's own row is marked. Each row has **Revoke** (`DELETE /auth/sessions/{id}`). **Sign out all browsers** is `DELETE /auth/sessions?kind=browser`. **Reset pairing** is `POST /auth/reset`.
- Any session may revoke any session or reset pairing, because both only take access away.

### Rotation and revoke

| Trigger | Pairing token | Window sessions | Browser sessions | What clients see |
|---|---|---|---|---|
| Companion start | New | All gone | Kept | The window mints a new session silently. Browsers keep working |
| **Reset pairing** in Settings or the tray | New. The file is rewritten atomically and the old token is rejected at once | Revoked | Revoked | Every WebSocket gets `auth.revoked`, then close 4401. In-flight calls get 401 `auth.revoked`. The window mints a new session silently. Browsers show "This browser was signed out. Pair it again from the tray icon." |
| Revoke one session | Unchanged | That one, if it's a window session | That one, if it's a browser session | That client gets 4401 and the sign-out message. A revoked window mints again through Electron main |
| 7 days idle | Unchanged | Not affected | That one expires | 401 `auth.expired`, then the `/pair` page |
| `companion.json` deleted or changed while running | Unchanged | Not affected | Not affected | Nothing. The 60 s check rewrites it |

### WebSocket auth

Browsers can't set headers on a WebSocket. The client opens `new WebSocket(url, ['touchdeck.v1', 'touchdeck.bearer.' + token])`. The server:

- requires a valid `Origin` whenever one is present (rule 2),
- reads the token from the `touchdeck.bearer.*` entry,
- selects only `touchdeck.v1` in its reply, so the token is never echoed.

No token ever goes in a query string. Close codes:

| Code | Meaning |
|---|---|
| 4401 | The session is invalid, expired or revoked |
| 4403 | The `Origin` isn't allowed |
| 4008 | The client is too slow (see Backpressure) |
| 1001 | The Companion is shutting down |

### Command approvals happen on the PC

[ADR 0006](adr/0006-command-approval-on-the-pc.md) records the decision. The contract:

- **What needs approval.** Every action that starts a process: a command, a program launch, and opening anything other than an `http`, `https` or `mailto` URL. The custom command Integration (#101) follows the same rule. An approval is keyed by a hash of the action kind, program, arguments, working folder and shell.
- **The dialog.** The Companion's `ApprovalPrompter` shows a native task dialog it owns. It names the Device, the Page and the Tile, and shows the full program, arguments, working folder and shell. The buttons are **Run once**, **Always allow** and **Deny**. Deny is the default. One dialog shows at a time. Others wait in order.
- **Repeats.** Pressing the same Tile again while its request is pending adds no second request.
- **Expiry.** A request nobody answers expires after 5 minutes and is logged as `expired`.
- **Foreground.** Windows may keep a tray program's dialog from taking focus, so the tray also shows a notification. `POST /approvals/{id}/show` shows the dialog again.
- **What the API offers.** `GET /approvals` lists pending requests, and they're also in `sync`. `approval.requested` and `approval.resolved` events let every open client show and clear "Waiting for approval on this PC". `GET /actions/allowed` lists always-allowed actions. `DELETE /actions/allowed/{hash}` removes one. No route answers a request or adds an approval.
- **Tests.** Under `test-hooks`, a `FakePrompter` replaces the dialog and answers from a script set with `POST /test/approvals/script`. The test `release_router_answers_no_approvals` builds the release router and asserts it has no `/test/` route and no route that changes an approval except `DELETE /actions/allowed/{hash}`.

### Firmware files come from the PC

A session can install only two kinds of Firmware bundle: the one bundled with the Companion, or a `.tdfw` file the user picks in a native file dialog that the Companion opens. No route accepts bundle bytes.

- `POST /firmware/bundles/pick` opens the dialog on the PC, filtered to `*.tdfw`. It returns `{bundle: BundleInfo | null}`, where `null` means the user cancelled. The Companion parses and checks the file with `Bundle::parse` and keeps it for 1 hour under a `BundleId`.
- A second pick while a dialog is open gets 409 `system.dialog_open`.
- Firmware routes take `{bundle: "bundled" | BundleId}`.

Image uploads for Assets stay on the API (`POST /uploads`). The Companion decodes and re-renders every image to the Device's Tile size, and nothing it uploads is ever run.

## Devices and New boards

### Identity

- **`DeviceId`** is the USB serial number from `serialport::UsbPortInfo.serial_number`, which is the chip's MAC on the ESP32-S3 USB-Serial/JTAG, lowercased with separators removed, such as `a0b1c2d3e4f5`.
  - It's an opaque string matching `^[0-9a-z]{1,32}$`. The `DevicePath` extractor checks it once, and handlers receive a typed `DeviceId`.
  - Protocol v2 `hello` also reports the id, and the Companion compares the two. If they disagree, the USB serial wins and the Companion logs a warning. v1 Firmware has no id in `hello`, and the USB serial still identifies it.
  - If a driver reports no serial number, the id falls back to `port-<sanitized port name>`, and the summary sets `stable_id: false`.
  - Mock Devices use `mock0001`, `mock0002` and so on.
- **The port** (`COM7`) is for diagnostics only. It appears in `DeviceSummary.link.port` and never in a route. Every command that took a `port` today takes a `DeviceId` instead.

### Collections

| Collection | Members | Lifetime |
|---|---|---|
| `/devices` | Every known Device: connected, offline, or needing setup | Saved in `devices.json` with its id, name and last Layout summary. `DELETE /devices/{id}` forgets one |
| `/new-boards` | Ports with the Espressif USB VID and PID whose serial isn't a known Device and that don't answer as Open TouchDeck | Present only while plugged in |
| `/ports` | Raw probe results for troubleshooting | Live |

- **A New board becomes a Device under the same id.** After a `firmware.install` Job succeeds, the events are `new-board.removed {id}`, then `device.added {id}` with `setup: "needs-setup"`. Naming it and choosing a starting Layout then use the ordinary Device routes.
- **A known Device that stops answering stays a Device.** If a port's serial matches a known Device that won't say `hello`, its `link.state` is `unresponsive` and the UI offers `POST /devices/{id}/firmware/repair`. It never becomes a New board, so its name and Layout stay attached.
- A port held by another program is a New board with `probe: "busy"` when its serial is unknown, and the Device with `link.state: "busy"` when it's known.

### Link states

`LinkState` replaces today's `DeckInfo.online` and the `link-closed` event.

```rust
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum LinkState {
    Connected { port: String, since_ms: u64, protocol: u32 },
    Connecting { port: String },
    Offline,                                    // known, not plugged in
    Lost { reason: LostReason, detail: String, at_ms: u64 },  // dropped on its own, retrying
    Disconnected,                               // the user chose Disconnect, no auto-connect until replug or Connect
    Busy { port: String, detail: String },      // another program holds the port
    Unresponsive { port: String },              // known serial, no hello
    Updating { job: JobId },                    // a Firmware Job owns the port
}
pub enum LostReason { Unplugged, Timeout, IoError, Closed }
```

`Lost` and `Disconnected` are separate variants, so the UI can't confuse "Connection lost" with "Device not connected".

```rust
pub struct DeviceSummary {
    pub id: DeviceId, pub rev: u64,
    pub name: Option<String>,                  // None until named
    pub setup: SetupState,                     // NeedsSetup | Ready
    pub link: LinkState,
    pub firmware: Option<FirmwareInfo>,        // {version, protocol, build, recovery, update_available: Option<String>}
    pub capabilities: Option<Capabilities>,    // v2 hello: screen, storage_free, tile_kinds. None on v1
    pub orientation: Option<Orientation>,
    pub layout: Option<LayoutSummary>,         // what the Device shows
    pub key_mirror: KeyMirrorState,            // {enabled, reason}
    pub stable_id: bool,
}
pub struct NewBoard { pub id: DeviceId, pub port: String, pub probe: BoardProbe, pub seen_ms: u64 }
pub enum BoardProbe { Silent, Busy { detail: String }, OtherFirmware { product: String, detail: String } }
```

### Rules for several Devices

- Each Device has its own link session, Layout, key mirror, action resolution and Job slot. Its actor thread owns them, and the API reads `DeviceSummary` from that owner.
- A Job on one Device never blocks another Device.
- Approvals and action-log entries carry `device`.
- Integration values go to every connected Device whose Layout binds them. Integration status reports delivery per Device.

## Event stream

### Frames

```jsonc
{ "type": "hello", "seq": 0, "api": "v1", "build": "<sha>", "session": { "id": "…", "kind": "window" } }
{ "type": "sync",  "seq": 1, "data": { "devices": [], "new_boards": [], "approvals": [], "jobs": [], "integrations": [] } }
{ "type": "device.updated", "seq": 2, "device": "a0b1c2d3e4f5", "data": { /* full DeviceSummary */ } }
```

- `seq` counts up per connection, for debugging and gap detection.
- Feature snapshots fill the `sync` keys. On every reconnect the client replaces its state from `sync`. There's no replay buffer and no client-side merge.
- State events carry the full object (`DeviceSummary`, `Job`, `ApprovalRequest`), not a patch. Each object has a `rev` counter, and the client ignores an older `rev`. Applying an event twice or out of order is harmless.
- If `hello.build` differs from the build the page loaded with, the page reloads.
- Clients ignore event types they don't know.

### Topics

The client sends `{"op": "subscribe", "topic": "console", "device": "a0b1c2d3e4f5"}` or the same with `"op": "unsubscribe"`.

| Topic | Default | Carries |
|---|---|---|
| `state` | Always on | `device.added`, `device.updated`, `device.removed`, `new-board.added`, `new-board.removed`, `job.updated`, `approval.requested`, `approval.resolved`, `action.logged`, `integration.status`, `auth.revoked`, `companion.stopping` |
| `console` | Opt-in, per Device | `device.console` |
| `key-mirror` | Opt-in, per Device | `device.key-mirror` |
| `device-events` | Opt-in, per Device | `device.event`, the raw Device event, for diagnostics |

### Event catalogue

| Type | Topic | Payload |
|---|---|---|
| `hello` | (first frame) | `{api, build, session: {id, kind}}` |
| `sync` | (second frame) | `SyncData`: one key per feature snapshot |
| `device.added` | `state` | `DeviceSummary` |
| `device.updated` | `state` | `DeviceSummary` |
| `device.removed` | `state` | `{id}` |
| `new-board.added` | `state` | `NewBoard` |
| `new-board.removed` | `state` | `{id}` |
| `job.updated` | `state` | `Job` |
| `approval.requested` | `state` | `ApprovalRequest` |
| `approval.resolved` | `state` | `{id, answer: "once" \| "always" \| "deny" \| "expired"}` |
| `action.logged` | `state` | `ActionLog` |
| `integration.status` | `state` | `IntegrationSummary` |
| `auth.revoked` | `state` | `{}`, followed by close 4401 |
| `companion.stopping` | `state` | `{reason}`, followed by close 1001 |
| `device.console` | `console` | `{device, line}` |
| `device.key-mirror` | `key-mirror` | `{device, seq, report, outcome, gap, errors, arrived_ms}` |
| `device.event` | `device-events` | `{device, event}` |
| `lagged` | (any opt-in topic) | `{topic, device, dropped}` |

### Backpressure

Each client has a queue of 1024 frames.

- Opt-in topics drop frames on overflow and send `lagged`.
- If a `state` frame would overflow, the server closes with 4008. The client reconnects and gets a fresh `sync`, so state stays correct and a slow tab never stalls a Device.

The server pings every 20 s and drops a client after 2 missed pongs.

## Jobs

**Rule.** An operation that transfers files to a Device, flashes it, or can take longer than about 2 s is a Job. Restart and settings aren't Jobs: they're one request and reply on the link.

```rust
pub struct Job {
    pub id: JobId,                 // "job_" + 16 random chars
    pub kind: &'static str,        // see the table below
    pub scope: JobScope,           // Device(DeviceId) | NewBoard(DeviceId) | Companion
    pub state: JobState,           // queued | running | succeeded | failed | cancelled
    pub progress: Value,           // typed per kind in TypeScript
    pub result: Option<Value>,     // typed per kind
    pub error: Option<ErrorBody>,  // the same shape as an HTTP error
    pub cancellable: bool,         // whether the current phase allows DELETE
    pub created_ms: u64, pub finished_ms: Option<u64>, pub rev: u64,
}
```

| Kind | Progress | Result | Cancellable |
|---|---|---|---|
| `firmware.update`, `firmware.repair`, `firmware.install` | `{phase: "checking" \| "sending" \| "rebooting" \| "confirming" \| "recovering", pct?}` | `{message, version}` | While `checking` or `sending`, because the image goes to the inactive OTA slot |
| `layouts.push` | `{assets_sent, assets_total, bytes_sent, bytes_total}` | `PushReport {state, bytes, chunks, retransmits, elapsed_ms, assets: {sent, unavailable}}` | Before the Layout put starts |
| `assets.render` | `{frames_done, frames_total}` | `AssetRef {id, kind, width, height, bytes}` | Yes |
| `assets.prune` | `{removed, total}` | `{removed: AssetId[]}` | Yes |

- **One running Job per scope.** A second one gets 409 `device.busy` with `details.job` set to the running Job, so a double click can attach to it. There's no queue.
- `GET /jobs` lists running Jobs and the last 50 finished ones, kept for 10 minutes. `GET /jobs/{id}` reads one. `DELETE /jobs/{id}` cancels one, or returns 409 `job.not_cancellable`.
- In TypeScript, `companion.jobs.wait(job)` resolves with the typed `result` on `succeeded` and rejects with the `ApiError` on `failed`. It follows `job.updated` events and re-reads `GET /jobs/{id}` after a reconnect.
- **Crash halfway.** Jobs aren't saved. OTA rollback covers a Firmware Job lost mid-flash: the Device reconnects on its old or new build, and the UI shows its version. A lost Layout push leaves the Device's previous Layout, because the put is atomic on the Device (`xfer.end`).
- **Uploads aren't Jobs.** `POST /uploads` and `POST /firmware/bundles/pick` return an id in milliseconds, and Jobs then refer to that id.

## Routes

Paths are relative to `/api/v1`. Every request and response type is a Rust type with `#[derive(Serialize, Deserialize, TS)]`, generated into `api.gen.ts`. `→ ()` means `204 No Content`. `→ 202 Job` means the response is a `Job` and the work continues as `job.updated` events.

### Devices, ports and console

Feature modules `devices`, `ports` and `console`.

| Name | Method and path | Request → Response |
|---|---|---|
| `devices.list` | `GET /devices` | → `DeviceSummary[]` |
| `devices.get` | `GET /devices/{device}` | → `DeviceSummary` |
| `devices.connect` | `POST /devices/{device}/connect` | → `DeviceSummary`. Turns auto-connect back on after a Disconnect |
| `devices.disconnect` | `POST /devices/{device}/disconnect` | → `DeviceSummary` with `link.state: "disconnected"`. Releases the port until replug or Connect |
| `devices.forget` | `DELETE /devices/{device}` | → `()`. Only for a Device that isn't connected |
| `devices.restart` | `POST /devices/{device}/restart` | → `()`, then `device.updated` events |
| `devices.settings_get` | `GET /devices/{device}/settings` | → `DeviceSettings {idle_timeout_s, orientation, name?, brightness?}`. v2 fields are `null` on v1 Firmware |
| `devices.settings_patch` | `PATCH /devices/{device}/settings` | `DeviceSettingsPatch`, every field optional → `{settings, restart_required}`. A v2 field on v1 Firmware gives 409 `device.needs_firmware` |
| `new_boards.list` | `GET /new-boards` | → `NewBoard[]` |
| `ports.list` | `GET /ports` | → `PortProbe[]`, today's `Candidate` plus `usb_serial`. Never opens a port that a link holds |
| `ports.rescan` | `POST /ports/rescan` | → `()` |
| `console.send` | `POST /devices/{device}/console` | `{line}` → `()` |

### Layouts

Feature module `layouts`.

| Name | Method and path | Request → Response |
|---|---|---|
| `layouts.get` | `GET /devices/{device}/layouts/{orientation}` | → `{layout: Layout, revision}`. `revision` is the Device's CRC32 from `GetReport`, also sent as `ETag` |
| `layouts.push` | `PUT /devices/{device}/layouts/{orientation}` | `Layout` as typed JSON, optional `If-Match: <revision>` → 202 Job `layouts.push`. 422 `layout.orientation_mismatch` when the path and body disagree. 412 `layout.changed` when the revision moved |

The `Layout` body extractor runs `td_core::Layout` validation once. On failure it returns 422 `layout.invalid` with `details.issues: [{path, message}]`.

### Firmware

Feature module `firmware`.

| Name | Method and path | Request → Response |
|---|---|---|
| `firmware.bundled` | `GET /firmware/bundled` | → `{version, build}` of the build shipped with the Companion |
| `firmware.pick` | `POST /firmware/bundles/pick` | → `{bundle: BundleInfo {id, version, build, bytes} \| null}`. Opens a native file dialog on the PC |
| `firmware.update` | `POST /devices/{device}/firmware/update` | `{bundle: "bundled" \| BundleId}` → 202 Job `firmware.update` |
| `firmware.repair` | `POST /devices/{device}/firmware/repair` | `{bundle}` → 202 Job `firmware.repair`, through the ROM bootloader |
| `firmware.install` | `POST /new-boards/{device}/install` | `{bundle, overwrite_other_firmware: true}` → 202 Job `firmware.install`. The literal `true` is required, so another ESP32 project is never overwritten without the user's click. Anything else gives 422 `request.invalid` |

While a Firmware Job holds a Device, its `link.state` is `updating`.

### Assets

Feature module `assets`. Assets reach a Device only through `layouts.push`, which sends whatever the Layout uses and the Device lacks.

| Name | Method and path | Request → Response |
|---|---|---|
| `assets.builtin` | `GET /assets/builtin` | → `Builtin[] {name, label, tags, id}` |
| `assets.suggest` | `POST /assets/builtin/suggest` | `{action: TileAction}` → `Builtin \| null` |
| `assets.file` | `GET /assets/{asset}` | → the stored bytes with their media type: `application/vnd.touchdeck.tdi1`, `image/gif` or `image/png` |
| `assets.create` | `POST /assets` | `{source: {kind: "app", path} \| {kind: "upload", upload} \| {kind: "catalog", catalog, id}, fit}` → 202 Job `assets.render`. For `app`, the Companion reads only the icon resource of that program |
| `assets.upload` | `POST /uploads` | raw `image/*`, at most 16 MiB → `{id, media_type, bytes}`, kept for 1 hour |
| `assets.catalog_search` | `GET /assets/catalog/search?q=&catalog=` | → `CatalogHit[]`. The Companion fetches and caches catalogs |
| `assets.on_device` | `GET /devices/{device}/assets` | → `{ids: AssetId[], storage_free}` |
| `assets.prune` | `POST /devices/{device}/assets/prune` | `{also_keep: AssetId[]}` → 202 Job `assets.prune`. Removes Assets no stored Layout uses |

### Actions and approvals

Feature module `actions`.

| Name | Method and path | Request → Response |
|---|---|---|
| `approvals.list` | `GET /approvals` | → `ApprovalRequest[] {id, device, page, tile, hash, kind, program, args, cwd, shell, created_ms}` |
| `approvals.show` | `POST /approvals/{id}/show` | → `()`. Shows the native dialog again. 404 `approval.not_found` if it was answered or expired |
| `actions.log` | `GET /actions/log?device=` | → `ActionLog[] {id, device, at_ms, page, tile, kind, status, detail, pid, exit_code}`, the last 100 |
| `actions.allowed` | `GET /actions/allowed` | → `AllowedAction[] {hash, kind, program, args, cwd, shell, granted_ms}` |
| `actions.allowed_remove` | `DELETE /actions/allowed/{hash}` | → `()`. The next press asks again. 404 `action.not_found` |

### Integrations

Feature module `integrations`. CLIProxyAPI is the Integration with id `cliproxyapi`.

| Name | Method and path | Request → Response |
|---|---|---|
| `integrations.list` | `GET /integrations` | → `IntegrationSummary[] {id, name, icon, enabled, status, config_schema, presets}`. `status` reports delivery per Device as `{device, sent, error}` |
| `integrations.get` | `GET /integrations/{integration}` | → `{summary, config, secrets: {[field]: {set, hint?}}}` |
| `integrations.config_put` | `PUT /integrations/{integration}/config` | Config, checked against the Integration's schema → the same view as `integrations.get` |
| `integrations.secret_put` | `PUT /integrations/{integration}/secrets/{field}` | `{value}` → `{hint}`. Write-only. The value never comes back and is never in `config` |
| `integrations.secret_delete` | `DELETE /integrations/{integration}/secrets/{field}` | → `()` |
| `integrations.refresh` | `POST /integrations/{integration}/refresh` | → `()` |

Each Integration's Preset tiles are in `presets`. Live tiles get their values through the Device link, never through this API ([ADR 0004](adr/0004-firmware-draws-live-tiles.md)).

### System

Feature module `system`.

| Name | Method and path | Request → Response |
|---|---|---|
| `system.ping` | `GET /system/ping` | → `{api: ["v1"]}`. Public |
| `system.get` | `GET /system` | → `{version, build, api: "v1", mock, data_dir_override}` |
| `system.lock` | `POST /system/lock` | → `{lock}`. 409 `system.busy` while any Job runs. Once held, new Jobs fail with 503 `companion.installing` |
| `system.unlock` | `DELETE /system/lock/{lock}` | → `()` |
| `system.shutdown` | `POST /system/shutdown` | → `()`, then `companion.stopping`. Window sessions only |

App updates aren't API routes. They stay in Electron's preload as `openTouchDeck.updates.check()`, `download(onProgress)` and `install()` (ADR 0005). Before installing, Electron takes `system.lock`, then stops the Companion with `system.shutdown` and runs the installer.

### Auth

The auth module mounts these itself. Features can't import it.

| Name | Method and path | Request → Response |
|---|---|---|
| `auth.proof` | `POST /auth/proof` | `{nonce}` → `{proof}`. Public |
| `auth.session_create` | `POST /auth/sessions` | Bearer pairing token, `{kind: "window", label?}` → `{id, token}`. Public, but needs the pairing token |
| `auth.pair` | `POST /auth/pair` | `{code, label}` → `{id, token}`. Public, but needs a pairing code |
| `auth.pair_code` | `POST /auth/pair-codes` | → `{code, url, expires_ms}`. Window sessions only. Opens the browser and shows the code in the tray |
| `auth.sessions` | `GET /auth/sessions` | → `SessionInfo[] {id, kind, label, created_ms, last_used_ms, current}` |
| `auth.session_delete` | `DELETE /auth/sessions/{id}` | → `()`. `self` means the caller's own session |
| `auth.sessions_delete` | `DELETE /auth/sessions?kind=browser` | → `()` |
| `auth.reset` | `POST /auth/reset` | → `()`. Reset pairing |

### Test hooks

Feature module `test_hooks`, compiled only with the `test-hooks` Cargo feature and mounted only when the Companion runs on mock ports (`--mock-devices N`). Release builds don't contain it. Its types go into a separate `api.test.gen.ts`, so the product UI can't import them.

| Name | Method and path | Request → Response |
|---|---|---|
| `test.mock` | `POST /test/devices/{device}/mock` | `{action: "no-confirm" \| "rollback" \| "normal-update" \| "stop" \| "hang-up"}` → `()` |
| `test.press` | `POST /test/devices/{device}/press` | `{page, tile, kind}` → `()` |
| `test.sets` | `GET /test/devices/{device}/sets` | → `Value[]` |
| `test.record` | `GET /test/devices/{device}/record` | → `{received, lines, settings, reboots, assets, layouts}` |
| `test.emit` | `POST /test/devices/{device}/emit` | `{events, repeat?, every_ms?}` → `()` |
| `test.input_log` | `GET /test/input-log` | → `string[]` |
| `test.ports` | `POST /test/ports` | `{plug: {id, kind: "device" \| "device-v1" \| "silent" \| "other-firmware" \| "busy"}}` or `{unplug: id}` → `()` |
| `test.approvals_script` | `POST /test/approvals/script` | `{answers: ("once" \| "always" \| "deny" \| "never")[]}` → `()`. Sets what the `FakePrompter` answers next |
| `test.clock` | `POST /test/clock` | `{advance_ms}` → `()`. Moves the Companion's clock for expiry tests |

## Mapping from today's Tauri commands and events

Release builds of `app/src-tauri` register 32 commands. `--features e2e` adds 7, marked e2e. Each one maps to a route above, moves to Electron, or goes away.

| # | Today | Replacement | Notes |
|---|---|---|---|
| 1 | `app_update_check` | Electron preload `openTouchDeck.updates.check()` | ADR 0005 |
| 2 | `app_update_download` | Electron preload `openTouchDeck.updates.download(onProgress)` | The progress Channel becomes an IPC callback |
| 3 | `app_update_install` | Electron preload `openTouchDeck.updates.install()` | Takes `system.lock` first, which replaces `AppUpdateGate` |
| 4 | `e2e_app_update` (e2e) | Electron main's scripted updater, behind a test flag | App updates live in Electron |
| 5 | `update_firmware` | `firmware.update`, `firmware.repair` or `firmware.install`, with `firmware.pick` | Bytes no longer travel as `number[]`. `recover=true` on a known Device is `firmware.repair`, on an unknown board `firmware.install`. The global `updating` flag becomes the per-Device Job slot |
| 6 | `list_decks` | `devices.list`, `new_boards.list`, `ports.list` | Detection is automatic |
| 7 | `connect` | Automatic. `devices.connect` turns auto-connect back on | By `DeviceId`, not port |
| 8 | `disconnect` | `devices.disconnect` | Per Device |
| 9 | `deck_info` | `devices.get` | `online` becomes `link.state` |
| 10 | `console_send` | `console.send` | Per Device |
| 11 | `e2e_mock` (e2e) | `test.mock` | Per mock Device |
| 12 | `e2e_mock_press` (e2e) | `test.press` | Per mock Device |
| 13 | `e2e_mock_sets` (e2e) | `test.sets` | Per mock Device |
| 14 | `push_layout` | `layouts.push` | Typed `Layout` body instead of a string. Returns a Job whose result is `PushReport` |
| 15 | `remove_unused_icons` | `assets.prune` | Takes Asset ids to keep, not Layout texts |
| 16 | `builtin_icons` | `assets.builtin` | |
| 17 | `default_icon` | `assets.suggest` | |
| 18 | `icon_file` | `assets.file` | Raw bytes with a media type |
| 19 | `take_app_icon` | `assets.create` with `{source: {kind: "app", path}}` | A Job |
| 20 | `pull_layout` | `layouts.get` | Adds `revision` |
| 21 | `settings_get` | `devices.settings_get` | Per Device |
| 22 | `settings_put` | `devices.settings_patch` | `reboot_required` is now `restart_required` |
| 23 | `reboot_deck` | `devices.restart` | |
| 24 | `e2e_mock_record` (e2e) | `test.record` | `icons` is now `assets` |
| 25 | `mirror_state` | `DeviceSummary.key_mirror` | Per Device |
| 26 | `set_action_layout` | Removed | Presses always resolve against the Layout each Device reports. Importing a file happens in the UI: parse, then `layouts.push` |
| 27 | `action_layout` | `DeviceSummary.layout` | Per Device |
| 28 | `pending_approvals` | `approvals.list`, also in `sync` | Read-only. Entries carry `device` |
| 29 | `action_log` | `actions.log` | Entries carry `device` and an `id` |
| 30 | `approval_answer` | Removed. The native dialog on the PC answers | ADR 0006. `approvals.show` shows the dialog again. `actions.allowed_remove` removes an approval |
| 31 | `focus_window` | Removed from the API | The renderer calls `window.focus()`. If the key monitor needs the window in front, a preload call goes through security review |
| 32 | `e2e_emit` (e2e) | `test.emit` | Per mock Device |
| 33 | `e2e_input_log` (e2e) | `test.input_log` | |
| 34 | `sources_view` | `integrations.get` | `config_file` leaves the API |
| 35 | `sources_status` | `integrations.list` and the `integration.status` event | Delivery per Device |
| 36 | `sources_save` | `integrations.config_put` | Normalizing (trimming the URL, the 60 s floor) stays in the Integration |
| 37 | `sources_set_key` | `integrations.secret_put` | Write-only |
| 38 | `sources_clear_key` | `integrations.secret_delete` | |
| 39 | `sources_refresh` | `integrations.refresh` | |

| Today's event | Replacement | Topic |
|---|---|---|
| `console-line` | `device.console {device, line}` | `console` |
| `deck-event` | `device.event {device, event}` | `device-events` |
| `link-closed` | `device.updated` with `link.state` set to `lost` or `disconnected` | `state` |
| `key-mirror` | `device.key-mirror` | `key-mirror` |
| `action-layout` | `device.updated` with `layout` | `state` |
| `action-log` | `action.logged` | `state` |
| `approval-request` | `approval.requested`, plus `approval.resolved` so every client clears it | `state` |
| `firmware-update` | `job.updated` | `state` |
| `Channel<DownloadProgress>` in `app_update_download` | The Electron IPC callback passed to `updates.download(onProgress)` | none |

The UI's other Tauri calls go too. `getCurrentWindow().title()`, which showed the build SHA, becomes `system.get`, and the window title is the static "Open TouchDeck". `getVersion()` becomes `system.get`.

## Errors

### One shape

Handlers, extractors, middleware and `Job.error` all return this body and nothing else:

```json
{ "error": { "code": "layout.invalid", "message": "The Layout has 2 problems.",
             "details": { "issues": [ { "path": "pages[0].tiles[3].colSpan", "message": "exceeds the grid" } ] } } }
```

- `code` is stable, and the UI switches on it. Codes are `<namespace>.<snake_name>`.
- `message` is one plain sentence that uses the glossary's words. It's for logs, developer tools and the **Details** disclosure. User-facing text comes from the copy guide, keyed by `code`.
- `details` is optional and typed per code.
- An unknown code is a generic error to the client.

Each feature owns an error enum. The kernel takes the message from `Display` and the status from `ApiError::status()`.

```rust
#[derive(Debug, thiserror::Error, Serialize, TS)]
#[serde(tag = "code", content = "details")]
pub enum LayoutError {
    #[serde(rename = "layout.invalid")]
    #[error("The Layout has {} problems.", issues.len())]
    Invalid { issues: Vec<LayoutIssue> },
    #[serde(untagged)]
    #[error(transparent)]
    Common(#[from] CommonError),
}
```

The generated TypeScript has `type ApiErrorBody = LayoutError | DeviceError | … | CommonError`, each with a literal `code`, so `isApiError(e, 'layout.invalid')` narrows `e.details`. `ErrorCode` is the union of every code. That replaces today's string matching for "denied", "busy", "timed out" and "rolled back".

### Error code catalogue

Features add codes as they land. This list is the starting set.

| Code | HTTP | When |
|---|---|---|
| `security.host` | 421 | The `Host` isn't allowed |
| `security.origin` | 403 | The `Origin` or `Sec-Fetch-Site` isn't allowed |
| `auth.missing` | 401 | No token |
| `auth.invalid` | 401 | The token is unknown |
| `auth.revoked` | 401 | The session was revoked or pairing was reset |
| `auth.expired` | 401 | A browser session passed 7 days idle |
| `auth.forbidden` | 403 | The session kind can't call this route, such as a browser minting a pairing code |
| `auth.code_invalid` | 401 | The pairing code is wrong, used or expired |
| `auth.code_locked` | 429 | Too many wrong pairing codes. Start pairing again |
| `request.invalid` | 400 | Malformed JSON, path or query. `details: {part, reason}`. Wraps axum rejections |
| `request.too_large` | 413 | The body passed its limit |
| `request.media_type` | 415 | The `Content-Type` is wrong for this route |
| `route.not_found` | 404 | No route with that path |
| `route.method_not_allowed` | 405 | The path exists with another method |
| `device.not_found` | 404 | No Device with that id |
| `device.not_connected` | 409 | The Device is known but offline, disconnected, busy or unresponsive. `details.link` |
| `device.busy` | 409 | A Job holds the Device. `details.job` |
| `device.timeout` | 504 | The Device didn't reply in time |
| `device.link` | 502 | Link I/O failed or the Device refused. `details.reason` |
| `device.needs_firmware` | 409 | The feature needs newer Firmware. `details.version` |
| `new_board.not_found` | 404 | No New board with that id |
| `layout.invalid` | 422 | The Layout fails validation. `details.issues` |
| `layout.orientation_mismatch` | 422 | The path and body orientations differ |
| `layout.changed` | 412 | The Device's Layout changed since it was loaded |
| `layout.stored_invalid` | 502 | The Layout stored on the Device fails validation |
| `firmware.bundle_invalid` | 422 | The picked file isn't a valid bundle |
| `firmware.bundle_not_found` | 404 | The `BundleId` expired or never existed |
| `firmware.failed` | Job error | The update failed. `details.reason` |
| `firmware.rolled_back` | Job error | The Device rolled back to its previous Firmware |
| `firmware.confirm_timeout` | Job error | The new Firmware didn't confirm in time |
| `asset.not_found` | 404 | No Asset with that id |
| `asset.render_failed` | Job error | The image or icon couldn't be rendered |
| `upload.not_found` | 404 | The upload expired or never existed |
| `job.not_found` | 404 | No Job with that id |
| `job.not_cancellable` | 409 | The Job's current phase can't be cancelled |
| `approval.not_found` | 404 | The request was answered or expired |
| `action.not_found` | 404 | No always-allowed action with that hash |
| `integration.not_found` | 404 | No Integration with that id |
| `integration.config_invalid` | 422 | The config fails the Integration's schema. `details.issues` |
| `integration.credentials` | 500 | The keyring failed |
| `system.busy` | 409 | A Job runs, so the install lock is refused |
| `system.dialog_open` | 409 | A native file dialog is already open |
| `companion.installing` | 503 | The install lock is held |
| `internal` | 500 | A bug. The message is generic, and the detail goes only to the log |

## Types from Rust

### One declaration per route

Each route is one `impl Endpoint`. The trait fixes the name, method, path, path parameters, query, body, success type and error type in one place. The axum route, the typed Rust test client and the generated TypeScript client all read that declaration.

```rust
pub trait Endpoint: Sized + Send + Sync + 'static {
    const NAME: &'static str;                  // "layouts.push" → companion.layouts.push()
    const METHOD: Method;
    const PATH: &'static str;                  // relative to /api/v1, axum 0.8 `{param}` syntax
    const STATUS: StatusCode = StatusCode::OK; // 202 when Ok = JobRef
    const BODY_LIMIT: usize = 1 << 20;
    type Path: DeserializeOwned + TS + Send;   // () or a struct such as DevicePath { device: DeviceId }
    type Query: DeserializeOwned + TS + Send + Default;
    type Body: RequestBody;                    // Json<T> | Upload | NoBody
    type Ok: ResponseBody;                     // a JSON type | Bytes { media_type } | JobRef | ()
    type Error: ApiError;
    fn handle(ctx: Ctx, req: Req<Self>) -> impl Future<Output = Result<Self::Ok, Self::Error>> + Send;
}
pub struct Req<E: Endpoint> { pub path: E::Path, pub query: E::Query, pub body: E::Body, pub client: ClientInfo }
```

`ClientInfo` holds the session id and kind, for the audit log and for routes limited to window sessions.

### Generated TypeScript

ts-rs derives each type. The Companion's generator writes `<ui>/src/companion/api.gen.ts` with every type, an endpoint table, typed wrappers `companion.<feature>.<action>()`, the `EventMap`, the `JobMap`, the `ErrorCode` union and `SyncData`. The file starts with `// Generated from td-companion. Do not edit.` and uses LF endings. `pnpm gen:api` runs `cargo run -p td-companion --features test-hooks -- --emit-ts <path>`.

`td-core` gains an optional `ts` feature that adds `#[cfg_attr(feature = "ts", derive(TS))]` to `Layout`, `Hello`, `Settings`, `Orientation` and the other types the API carries.

### Freshness rule

- **`api_bindings_are_current`** renders `api.gen.ts` in memory and compares it byte for byte with the committed file. On a difference it fails with "run `pnpm gen:api`". It runs inside `cargo test --workspace`, which CI already runs.
- **`pnpm typecheck`**, part of `pnpm lint`, compiles the UI against the generated file, so a UI calling a removed or renamed route fails CI.
- **`api_contract_is_documented`** asserts that every endpoint `NAME`, event type and error code appears in this page.
- **`test_registry_is_consistent`** fails when an endpoint's `NAME` doesn't start with its feature's name, when two endpoints share a method and path, or when `Path`'s fields don't match the `{params}` in `PATH`.

Rejected options:

| Option | Why not |
|---|---|
| ts-rs alone | Types only. Paths, methods and request and response pairing stay hand-written in TypeScript and can drift |
| specta 2 | Still a release candidate. Its main draw, tauri-specta, leaves with Tauri |
| schemars plus `json-schema-to-typescript` | Two toolchains. Tagged enums become `oneOf`, which gives noisy TypeScript. Routes stay hand-written |
| utoipa plus `openapi-typescript` | Response types live in an attribute the compiler never checks against the handler, so the drift that matters most stays possible |
| aide | Pinned to an old schemars while the ecosystem has moved on. Pre-1.0 with a small maintainer base |

The trait and generator are about 300 lines we own. There's no OpenAPI document in v1. The registry knows every method, path and type, so it can emit one later if third-party clients appear.

## Feature modules

**The rule: a feature is one module that registers itself, and adding one edits no shared file.** A feature lives in `crates/td-companion/src/features/<name>.rs` or `<name>/`, and declares its endpoints, events, Job kinds, error enums and `sync` snapshot in one place:

```rust
pub fn feature() -> Feature {
    Feature::new("devices")
        .endpoint::<List>()
        .endpoint::<Get>()
        .endpoint::<Restart>()
        .event::<DeviceUpdated>()
        .snapshot("devices", |ctx| ctx.devices.summaries())
}
```

Two mechanisms can collect the features. #71 tries the first with a short spike and records its choice:

- **A build-script registry.** `build.rs` lists `src/features/*` and writes `$OUT_DIR/features.rs` with one `mod` per file and `pub fn all() -> Vec<Feature>`. `src/features.rs` holds only the `include!`. Keep it only if rust-analyzer resolves the generated modules: go to definition, completion and errors inside a feature file must all work.
- **Distributed registration with `inventory` (or `linkme`).** Each feature submits its `feature` function with `inventory::submit!`, and the kernel collects them with `inventory::iter`. Rust still needs a `mod` line per feature. If #71 can't generate those lines, they sit in `src/features/mod.rs`, one per line in sorted order, and that line is the only shared edit a feature makes.

Either way:

- An `Endpoint` that's declared but never passed to `.endpoint::<X>()` is a never-built struct, so `dead_code` warns, and the zero-warning build fails.
- `Ctx` is the one shared struct on purpose. It lists domain services (the Device manager, Integrations, the Asset store, actions, Jobs, events), not features, and changes only when a new service appears.
- `api` exports only `Endpoint`, `Req`, `Feature`, `Ctx`, `ApiError`, `CommonError`, `Event`, `JobKind`, `Jobs` and the body kinds. `api::auth` is private, so a feature can't reach tokens or declare a public route.
- Blocking `td-link` work runs on each Device's actor thread or in `spawn_blocking`. Handlers never block the runtime.

## HTTP framework

axum 0.8 on tokio. tokio, hyper, tower and tower-http are already in `Cargo.lock` through reqwest, and axum itself is there through the WebDriver plugin. The Companion adds axum's own small crates, `tungstenite` for the WebSocket, and the tower-http `limit` and `set-header` features. Removing Tauri drops much more than that. The scaffold ticket measures the release binary's growth and records it. The budget is 1.5 MB.

Rejected: actix-web (a second runtime layered on tokio), warp (filter types make per-feature routes and error mapping awkward), poem (a much smaller community, and its OpenAPI story uses the attribute-declared responses rejected above), tiny_http with tungstenite (a thread per connection, and hand-built upgrade, limits and middleware), and raw hyper (every layer above it would be ours).

**Embedded UI.** `build.rs` embeds `<ui>/dist` as an `include_bytes!` table. In development, the Vite dev server proxies `/api`, including the WebSocket, to the Companion and rewrites `Host` and `Origin` to the Companion's origin. The Companion has no dev-only origin exception that could ship.

## Test seam

`td_companion::testing::TestCompanion` is an in-process Companion, compiled under `cfg(test)` or the `test-hooks` feature. It binds `127.0.0.1:0` with a temporary data folder, in-memory keys, a fake input sink, a `FakePrompter` and `MockPorts`.

```rust
let td = TestCompanion::start().await;
let a = td.plug_device("mock0001").await;              // a mock Device with that USB serial and a v2 hello
let b = td.plug_device("mock0002").await;              // a second Device at the same time
td.plug_device_v1("mock0003").await;                   // v1 Firmware, no capabilities
td.plug_board("mock00ff", BoardKind::OtherFirmware).await;
td.unplug("mock0002").await;
td.prompter().answer_next(Answer::Once);
let job = td.call::<layouts::Push>(path(a, Landscape), layout("A")).await?;  // typed client from the Endpoint
let mut ev = td.events().await;                        // a WebSocket already past hello and sync
td.raw().get("/api/v1/devices").header("Host", "evil.example:1").send().await;  // for security tests
td.clock().advance(Duration::from_secs(121));
```

`MockPorts` implements the Device manager's `PortSource` trait, as the real port enumeration does. It replaces today's single `MOCK_PORT` and `TOUCHDECK_MOCK=1`. The Companion binary takes `--mock-devices N` under `test-hooks` for Playwright, and `TOUCHDECK_APP_DATA_DIR` for isolation.

Required integration tests in `crates/td-companion/tests/`:

1. **Two Devices.** Both connect within 2 s of `plug`. Different Layouts pushed to each stay separate. A press on one runs only its own action. A Firmware Job on one doesn't block a Layout push on the other. Unplugging one leaves the other connected.
2. **Hot-plug.** Unplug gives `link.state: lost` with `unplugged`. Replug gives `connected` within 2 s. After `devices.disconnect`, the Device stays disconnected until replug or `devices.connect`.
3. **New board.** Other Firmware gives `new-board.added`. `firmware.install` without `overwrite_other_firmware: true` gives 422. A successful install gives `new-board.removed`, then `device.added` with `setup: "needs-setup"` and the same id.
4. **Jobs.** A second POST gives 409 `device.busy` with the first Job's id. `jobs.wait` sees `succeeded` with a typed result. The mock's `rollback` mode gives `failed` with `firmware.rolled_back`.
5. **Approvals.** A command Tile press creates one `approval.requested` and runs nothing. A second press of the same Tile adds no request. The scripted `once` runs it and sends `approval.resolved`. After 5 minutes of test clock, an unanswered request resolves as `expired`. `actions.allowed_remove` makes the next press ask again. `release_router_answers_no_approvals` passes.
6. **Auth attacks.** Each request is rejected with the stated code and never reaches a handler:
   - no token, a wrong token, a revoked token, an expired browser session (7 days of test clock),
   - a foreign `Host`, a missing `Host`, and `localhost` against `127.0.0.1`,
   - a foreign `Origin` and `Origin: null`,
   - `Sec-Fetch-Site: cross-site`,
   - a WebSocket with a foreign `Origin`, and one without the bearer subprotocol,
   - a pairing code used twice, after 120 s, and the sixth try after 5 wrong codes,
   - a browser session calling `auth.pair_code` or `system.shutdown`,
   - a cross-site form post with `Content-Type: text/plain`,
   - an `OPTIONS` preflight, which gets no CORS headers,
   - Reset pairing, which closes open sockets with 4401 and rejects the old pairing token.
7. **Sessions.** A window session ends when its WebSocket has been closed for 60 s. Browser sessions survive a Companion restart. Window sessions don't.
8. **Runtime file**, Windows only (`#[cfg(windows)]`). After start the DACL is protected, has one entry, and the owner is the user. A junction planted at `runtime\` makes start fail. The file is gone after a clean shutdown.
9. **Contract.** `api_bindings_are_current`, `api_contract_is_documented` and `test_registry_is_consistent`.

Playwright then drives the Electron window and a browser page against `--mock-devices 2`. One spec pairs a browser through a code from `auth.pair_code`, once by link and once by typing it.

## Versioning

- **The path prefix is `/api/v1`.** The runtime file lists supported versions (`"api": ["v1"]`), and `GET /system/ping` returns them without auth.
- **The UI and API ship together.** The UI is embedded in the Companion binary, so the first-party UI always matches its API. Two boundaries remain:
  - A tab left open across an update sees a new `hello.build` and reloads.
  - Electron's preload contract has `openTouchDeck.version = 1`. Electron checks `GET /system/ping` and shows "Restart Open TouchDeck to finish updating" if it doesn't support the Companion's API version.
- **Inside v1, changes only add.** Allowed: new routes, new optional fields, new event types, new error codes and new Job kinds. Clients ignore unknown fields and event types, and treat an unknown error code as generic. Not allowed: renaming or removing a field, changing a type, or changing what a status code means. The generated file shows such a change in review as a removed or changed line.
- **A breaking change** adds `/api/v2` next to v1 for one release, but only if third-party clients exist. Otherwise it replaces v1 in the same release, because every first-party client ships with it.
- **The Firmware protocol version** (Companion to Device, Protocol v2) is separate and never appears in routes. The API shows it only as `DeviceSummary.firmware.protocol` and in `device.needs_firmware` errors.

## How to add an endpoint

1. Find the feature module under `crates/td-companion/src/features/`. If your route belongs to a new feature, add a new module there and register it the way the feature-module rule says.
2. Declare the request and response types with `#[derive(Serialize, Deserialize, TS)]`. Use glossary words for names and fields.
3. Write `struct X; impl Endpoint for X` with `NAME` set to `<feature>.<action>`, the method, the path under `/api/v1`, the types and the feature's error enum. Slow Device work returns `JobRef` with `STATUS = 202` and a registered `JobKind`.
4. Add `.endpoint::<X>()` to the feature's `feature()`. Add `.event::<E>()` for any new event.
5. Add new error variants to the feature's enum with a `code` and a status.
6. Run `pnpm gen:api` and commit the changed `api.gen.ts`.
7. Add the route, its events and its error codes to this page. Changes only add (see Versioning).
8. Write an integration test through `TestCompanion` that calls the route with `td.call::<X>()` and checks what a caller sees.
9. Run `cargo test --workspace` and `pnpm lint`. The freshness and contract tests tell you what you missed.

Never add a public route, a CORS header, a route that answers approvals, or a route that takes Firmware bytes. Each of those needs a new ADR and a security review.

## Risks

| Risk | Status |
|---|---|
| Any auth bug lets an attacker run programs as the user, because a session could push a command Tile and approve it | **Resolved** by ADR 0006. Approvals happen only in the native dialog on the PC. An auth bug still exposes Devices, Layouts, Integration settings and key Tiles |
| A pushed Layout can hold key Tiles that type text, such as a command line followed by Enter, into whatever window is focused. They don't need approval | **Accepted**, recorded here. Someone has to tap the Tile on the Device. #76 should consider approval for key Tiles that type long text |
| A bearer token in `localStorage` can be stolen through an XSS bug | **Reduced.** Browser sessions expire after 7 days idle, not 30. A strict CSP, no `dangerouslySetInnerHTML`, the session list with Revoke, and Reset pairing. A stolen browser session can't mint pairing codes, so revoking it ends the access |
| A raw Firmware upload could flash an attacker's build | **Resolved.** Only the bundled build or a file picked in the Companion's native dialog |
| Another Windows user on a shared PC squats on the port | **Reduced.** Rotation per start and the HMAC proof protect the window. A browser tab left on a dead port could send its session token to the squatter. Accepted: that token only works on the real Companion, which the squatter can reach anyway, so it gains that browser's session until the user signs it out |
| A local process guesses a typed pairing code | **Reduced.** 60-bit codes, 120 s, single use, and a lockout after 5 wrong tries |
| Browsers may drop the URL fragment when the Companion opens the link | **Resolved.** The query form is allowed, and the user can type the code |
| A tray program's dialog may not get the foreground on Windows | **Reduced.** The tray notification and `approvals.show`. The approval waits up to 5 minutes |
| `assets.create` with `kind: "app"` takes any path, so a session can learn whether a file exists | **Accepted.** The Companion returns only a rendered icon, never file contents |
| The build-script registry may confuse rust-analyzer | **Open**, #71 decides between it and `inventory` |
| `td-link`'s blocking `Session` under async handlers needs one actor thread per Device | **Open**, the Device manager ticket (#74) builds it |
| With Fast User Switching, two Companions try to open the same COM ports | **Open.** The second sees the port as `busy`. #74 decides whether to back off |
| A browser tab can't show the app update icon, because updates live in Electron | **Open.** The browser shows nothing for now |
| The key monitor's `focus_window` may need to come back | **Open.** If so, one preload call, through security review |
