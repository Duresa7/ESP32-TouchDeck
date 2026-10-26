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
