# Network protocol reference

This reference implements the byte and crypto contract in
[net-v1.md](../net-v1.md). It needs Node 22 or later and no packages.
Its BigInt SRP code is a test oracle. Production code uses the documented
ESP-IDF and RustCrypto backends. JavaScript cannot guarantee secret erasure
or constant-time big-integer arithmetic.

Run the conformance tests from the repository root.

```powershell
node --test schema/protocol/net-ref/net.test.mjs
```

The test compares generated results with the committed JSON without rewriting
it. It also checks the existing link-v1 golden bodies, malicious public values,
wrong codes, transcript changes, authentication failures, record limits,
replay, reorder, truncation, and pairing deadlines.

`protocol.mjs` contains the SRP equations, typed handshake encodings, key
derivation, key wrapping, complete link body validation, directional record
states, and pairing-window state. `vectors.mjs` defines public deterministic
inputs and regenerates [net-v1-vectors.json](../net-v1-vectors.json).

```powershell
node schema/protocol/net-ref/vectors.mjs --write
```

Every secret in that JSON is a public fixture. Never use those values to pair
with a real deck. `S` uses its minimum unsigned big-endian representation.
`A` and `B` always use 384 bytes. All other binary JSON values are lowercase
hex. `counter` is a JSON integer. The code and app name are literal strings.
The two records in each direction contain the existing hello and ping golden
bodies. The tampered record changes the last tag bit of the first app record.

The leading-zero case catches the stable RustCrypto convenience-handshake
padding mismatch. The lower-level SRP arithmetic with explicitly padded u
is required for that version, as described in the protocol.

## TCP verification client

The same test command runs a localhost TCP peer and exercises pairing, hello,
ping, a layout transfer with acknowledgements, widget updates, an event,
keepalive, busy refusal, and reconnect. The peer exists only during the test.

`client.mjs` is also a verification client for the firmware owner. This command
pairs, checks hello and ping, reads the stored landscape layout, updates a
widget if a tile id is supplied, and reconnects without pairing again.

```powershell
node schema/protocol/net-ref/client.mjs <deck-ip> [widget-tile-id]
```

The code prompt hides input. No code or key is accepted as a command-line
argument or printed. The client keeps the credential in memory for this run.
Its pairing occupies a deck slot until `net forget`. A production app uses
the OS keyring. The reference CLI requires a person to read the pairing display.
That board check is separate from this PR's fully automated localhost test.

Automation can import `pairClient` and supply `getCode({signal})` and
`savePending(credential)` callbacks. `savePending` completes before the commit
message. The caller retains that pending credential if the final acknowledgement
is lost, and can pass it to `connectSession`. Neither callback may log secrets.
`connectSession` sends hello, then pings every two seconds until `close()`.
It exposes `request(message)`, `getLayout(orientation)`, and an `onEvent` callback.
An orderly `await session.wire.finish(deadline)` before reconnect avoids racing
the remote socket's close handling. `deadline` uses the exported monotonic `now()`.

The Node client is a verification tool with bounded requests and layout reads.
Its limits and volatile storage are intentional. M3-06 owns the production
transport, transfer engine, discovery, UI, and persistent keyring behavior.
