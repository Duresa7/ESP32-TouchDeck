# Open TouchDeck network link v2

net-v2 carries link frames between a Device and the Companion over two
**Carriers** other than USB: TCP over the Device's Wi-Fi station interface, and
a BLE GATT service. Both Carriers move the same items in the same format. On
top of them the Companion runs the same link session as over USB, including
[link-v2](link-v2.md) protocol selection.

net-v2 replaces [net-v1](net-v1.md), which never shipped in Firmware. It keeps
net-v1's SRP-6a pairing with a six-digit code, the long-term keys, the busy
rule and the Device identity. It changes three things:

* Every session adds an ephemeral X25519 exchange, so a stolen long-term key
  doesn't decrypt recorded sessions ([ADR 0009](../../docs/adr/0009-net-sessions-add-x25519-for-forward-secrecy.md)).
* Records carry link frames whose application protocol is chosen by link-v2's
  hello negotiation, so link-v2 is no longer USB-only.
* One item format rides on TCP and on BLE GATT, with fragmentation rules that
  work at the smallest ATT MTU.

The suite is fixed, with no negotiation: SRP-6a over RFC 5054's 3072-bit group
with generator 5 and SHA-512 for pairing; X25519 (RFC 7748) for the session
exchange; HMAC-SHA256 and HKDF-SHA256 for proofs and keys; AES-256-GCM with a
16-byte tag for records.

MUST, MUST NOT and SHOULD state requirements. All test values in
[net-v2-vectors.json](net-v2-vectors.json) are public synthetic fixtures. They
MUST NOT be used by a Device or a Companion outside the reference tests.

## Carriers and discovery

The Device's only radios are 2.4 GHz Wi-Fi and BLE. It has no Ethernet, so the
TCP Carrier always rides on Wi-Fi. A test harness may run the Firmware's network
service over an emulated network interface; that is a test stand-in, not a
Carrier.

A **carrier connection** is one TCP connection, or one BLE connection from the
moment the Companion has enabled notifications on both `tx` and `flow` (see
below). Each carrier connection carries at most one pairing exchange or one
session. Ending the session ends the carrier connection: TCP closes, and on BLE
either side disconnects. A new session always needs a new carrier connection.

**TCP.** The Device listens on TCP port 47820 on its station interface. The
Companion discovers it with DNS-SD over multicast DNS (RFC 6762 and RFC 6763),
service type `_touchdeck._tcp`, or connects to an address the person typed.
A typed address is an IPv4 address, an IPv6 address or a host name, with an
optional port that defaults to 47820. Discovery uses only standard DNS-SD and
no OS-specific service, so it works the same on Windows and Linux. Networks
that block multicast need the typed address. The TXT record has these keys:

| Key | Value |
|---|---|
| `id` | Device id, 12 lowercase hex digits |
| `proto` | `2` |
| `name` | The Device name from link-v2, at most 48 UTF-8 bytes |
| `paired` | Number of stored Pairings, decimal |

Both sides SHOULD disable Nagle's algorithm (`TCP_NODELAY`).

**BLE.** The Device is a GATT server and the Companion is the client. The
service and its characteristics use these 128-bit UUIDs:

| Name | UUID | Properties | Direction |
|---|---|---|---|
| service | `6198f058-afef-4469-b47f-07315672e2dd` | primary service | |
| `rx` | `6198f059-afef-4469-b47f-07315672e2dd` | Write Without Response | Companion to Device |
| `tx` | `6198f05a-afef-4469-b47f-07315672e2dd` | Notify | Device to Companion |
| `flow` | `6198f05b-afef-4469-b47f-07315672e2dd` | Notify | Device to Companion |

The Device advertises the service UUID in its advertising data while it can
accept a carrier connection. Its scan response carries service data for the
same UUID: `proto:u8` (2), `device_id[6]`, `paired:u8`. The Companion matches
Devices by `device_id`, never by BLE address, because a Device MAY use a random
address. Both Windows (WinRT) and Linux (BlueZ, for example through btleplug)
report service data per UUID.

The link service needs no BLE pairing, bonding or link-layer encryption, and
the Companion MUST NOT start BLE pairing for it. net-v2 authenticates and
encrypts everything above GATT. Keeping the OS out of the trust decision means
no OS-level bond that reconnects on its own at boot, and the same Pairing works
on Wi-Fi and BLE. Bonds that the Device's Bluetooth keyboard role makes with
its hosts are separate and outside this document.

Advertised data and TXT records are untrusted hints. The Companion pins
`device_id` when it pairs and checks it in every session. An address change
doesn't change the Pairing.

## Device identity

`device_id` is the six-byte factory base MAC from eFuse, in byte order. It is
the link-v2 `id` (12 lowercase hex digits) and the USB serial number. It is not
the BLE address, the Wi-Fi interface address if that ever differs, or any
randomized address. The same Device has the same `device_id` on every Carrier,
so the Companion keeps one Device record and changes only its active Carrier.

A selected link-v2 hello over a net-v2 session MUST carry an `id` equal to the
session's `device_id`. The Companion closes the session otherwise. The id is
identity, not authentication; the session handshake is the authentication.

## Ownership and the busy rule

The Device admits one net-v2 **owner** across both network Carriers: a carrier
connection that starts while there is no owner, until it closes. A connection
waiting for code entry is an owner. While an owner exists, any other carrier
connection, on TCP or on BLE, receives the `busy` error as soon as it starts
and then closes. The Device writes that error within 100 ms and allocates no
SRP or X25519 state for it.

USB wins. While the USB link is online (a valid `hello` within the last
5000 ms, as in link-v1), the Device:

* answers `session.open` with `busy`,
* closes an established net-v2 session when USB comes online,
* still allows pairing, because pairing carries no link frames. A person can
  set up Wi-Fi and pair while the Device is plugged in.

USB itself is never refused. Replies return on the Carrier that carried the
request. Events go to the online Carrier. Because a session and an online USB
link never overlap, there is at most one. A link-v2 transfer belongs to the
Carrier that started it and is aborted when that Carrier closes. link-v2
protocol selection is per session: a new net-v2 session starts at protocol 1,
independent of USB.

A Companion uses one Carrier per Device and prefers USB when USB answers
`hello`. Switching Carrier closes the old session, releases held keys, and
starts a new `hello` on the new Carrier.

## Items

`||` concatenates byte strings. `u16be` and `u64be` are unsigned big-endian
integers. `H` is SHA-256, `H512` is SHA-512 and `MAC(k,m)` is HMAC-SHA256.
Literal strings are ASCII with no terminator unless `\0` is written. `hex` is
lowercase. Random values come from a cryptographic RNG and are each generated
independently. Comparisons of proofs and tags MUST be constant-time. Secrets
MUST be erased when their state ends.

Each direction of a carrier connection is a byte stream of **items**:

```text
item = u16be(len) || body
```

`len` excludes its own two bytes. A receiver reads exactly `len` bytes, and
rejects a length outside the bounds of its current state before it allocates.
There are two phases, and receivers MUST NOT tell them apart by sniffing bytes.

* Before `session.ready`, `body = kind:u8 || version:u8 || fields`. `version`
  is 2. Handshake bodies are at most 512 bytes. No link frame travels in this
  phase.
* After `session.ready`, `body = ciphertext || tag`, an AEAD record. Its
  plaintext is exactly one complete link frame body: the six-byte header,
  payload and two-byte CRC, as in link-v1, with frame version 1 and a
  little-endian payload length. Plaintext is 8 to 1032 bytes, so `len` is 24
  to 1048 and an item is at most 1050 bytes.

No text console, log output, SLIP delimiter or SLIP escaping runs on a network
Carrier. The CRC stays inside the record so every Carrier hands the link
session the same frame bodies.

### TCP

Items follow each other on the TCP stream. TCP reads may split or combine
items.

### BLE fragments

On BLE the Companion-to-Device stream is the sequence of `rx` writes, and the
Device-to-Companion stream is the sequence of `tx` notifications. Each write or
notification is one **fragment**:

* A fragment holds 1 to `F` bytes of exactly one item. An item's bytes go out
  in order, as `ceil(len_item / F)` fragments. Every fragment but the item's
  last has exactly `F` bytes. The next item starts a new fragment.
* `F = ATT_MTU - 3`. A sender uses the negotiated ATT_MTU only when its BLE
  stack reports it. Otherwise it uses the ATT minimum, 23, so `F = 20`. The
  Device always knows its ATT_MTU. A Companion on WinRT can read it from the
  GATT session; one on BlueZ through btleplug may not be told, and then sends
  20-byte fragments. Neither side requires a larger MTU.
* Because `F` is at least 20, an item's first fragment always holds its whole
  length prefix.
* A receiver accepts any fragment of 1 to `ATT_MTU - 3` bytes (514 at most).
  It closes the carrier connection on an empty fragment, a first fragment
  shorter than 2 bytes, a fragment that runs past the end of its item, or a
  length outside the current state's bounds.
* Prepared (long) writes and writes with response to `rx` are refused with an
  ATT error, and the Device disconnects.

A 1050-byte item is 53 fragments at `F = 20` and 5 at `F = 244`. The
`ble_fragments` in the vectors show both for one record each way.

**Flow control.** Writes without response have no acknowledgement, so the
Device grants credit on `flow`. The value of each `flow` notification is
`u64be(limit)`: the total number of `rx` bytes the Companion may have written
since the carrier connection started. The limit never decreases.

1. The Companion connects, optionally exchanges MTU, and enables notifications
   on `flow` and on `tx`, in either order. The carrier connection starts when
   both are enabled.
2. The Device then notifies `flow` with its initial limit, at least 1050 so one
   maximum item fits. 2100 is RECOMMENDED. The Companion writes nothing to
   `rx` before that notification.
3. The Companion keeps `bytes_written <= limit`. The Device raises the limit as
   it frees receive buffer space, in steps it chooses.
4. A write beyond the limit, a write before the carrier connection starts, or
   notifications disabled on `tx` or `flow` ends the carrier connection.

`tx` notifications have no credit. The Device waits for its stack's buffers
and never drops a notification. Lost, duplicated or reordered fragments fail
the record's length, tag or counter check, so the session closes rather than
accepting corrupt data.

**Connection parameters.** The Device SHOULD request a connection interval of
15 to 30 ms, peripheral latency 0, and a supervision timeout of 4000 ms. A
Companion SHOULD write several fragments per connection event when its stack
allows. At one 20-byte fragment per 30 ms event, the slowest case these rules
allow, a 1050-byte item takes about 1.6 s, within the 5000 ms link expiry.
These are estimates, not measurements. BLE throughput is far below USB, so the
Companion lowers Asset rate, photo interval and live update rate as link-v2's
throughput section describes.

## Handshake errors

Errors are handshake items with kind `0x7f`, version 2 and a one-byte reason.
They are unauthenticated hints. They never establish identity or cause a stored
Pairing to be deleted. Unknown kinds, versions, trailing bytes, unexpected
state transitions, malformed UTF-8 and wrong field lengths close the carrier
connection. A Device that receives an item with version 1 sends `version`.

| Reason | Name | Meaning |
|---|---|---|
| 1 | `busy` | Another owner holds the Device, or USB is online and this is `session.open` |
| 2 | `version` | Unsupported item version |
| 3 | `args` | Invalid handshake fields or state |
| 4 | `full` | Four Pairings exist, or this Companion id already exists |
| 5 | `auth` | Authentication failed, including an unknown Companion id |
| 6 | `expired` | A deadline or the pairing window expired |
| 7 | `cancelled` | The person cancelled pairing on the Device |
| 8 | `storage` | The Pairing could not be committed |

For example, `busy` is hex `00037f0201`. Errors close the carrier connection,
except that the first two `auth` errors after a pairing proof return to code
entry. After a terminal error, TCP shuts down its write side and drains input
for at most 100 ms before closing; BLE waits at most 100 ms after the
notification before disconnecting. An established session closes silently on
any cryptographic or framing error. Once `session.ready` is sent, no plaintext
error is valid.

## Pairing

Pairing is net-v1's, with item version 2 and labels under `touchdeck/net-v2/`.
It is the same on both Carriers, and one Pairing serves both. Field names
below follow the glossary; the wire bytes are the same as net-v1's except the
version byte and the label prefix.

The first item on a carrier connection is `pair.request` or `session.open`.
`pair.request` asks the Device to show a fresh six-digit decimal code,
zero-padded, full screen with a Cancel button. The code is sampled uniformly
from 0 to 999999 by rejection sampling. The pairing window lasts 120000 ms on a
monotonic clock. The code never leaves the display or the SRP computation.

`companion_id` is a random 16-byte id, stable for one Companion installation.
`companion_name` is 1 to 64 bytes of valid UTF-8 with no NUL or Unicode control
characters; `name_len` counts bytes. The name labels the Pairing for a person
and grants no authority. `window_id` is a fresh random 16-byte value. A new
request can't replace or extend an open window. Three failed client proofs
close the window and the carrier connection.

Cancel, carrier close, commit or expiry destroys the window, code, salt,
verifier, private exponents and temporary keys. A failed proof destroys that
attempt's SRP state but keeps the window and its failure count. Each retry
uses fresh exponents, salt and nonces. An invalid public value or malformed
item closes the carrier connection and the window.

| Kind | Name | Sender | Fields |
|---|---|---|---|
| `0x10` | `pair.request` | Companion | `companion_id[16], name_len:u8, companion_name[name_len]` |
| `0x11` | `pair.offer` | Device | `device_id[6], window_id[16], remaining_ms:u32be` |
| `0x12` | `pair.start` | Companion | `A[384], client_nonce[32]` |
| `0x13` | `pair.challenge` | Device | `salt[16], B[384], device_nonce[32]` |
| `0x14` | `pair.proof` | Companion | `client_proof[32]` |
| `0x15` | `pair.accept` | Device | `device_proof[32], wrapped_key[48]` |
| `0x16` | `pair.commit` | Companion | `commit_proof[32]` |
| `0x17` | `pair.stored` | Device | `stored_proof[32]` |

`E(kind, fields)` is a complete item, prefix included. Numbers in brackets are
byte lengths. The Device waits for code entry after `pair.offer`, and the
Companion sends `pair.start` only after the person enters the code. A pairing
attempt has a 10000 ms deadline from the first byte of `pair.start`, covering
all crypto, proofs, persistence and `pair.stored`. The first item must arrive
within 10000 ms of the carrier connection starting. Waiting for the person is
the only wait outside these deadlines. Partial reads never extend a deadline.

### SRP arithmetic

Unchanged from net-v1. `N` is the 3072-bit prime in RFC 5054 Appendix A and
`g = 5`. `PAD(n)` is the 384-byte big-endian encoding and `MIN(n)` the
shortest nonempty one. `INT(bytes)` reads an unsigned big-endian integer.

```text
I = "touchdeck:" || hex(device_id) || ":" || hex(companion_id)
P = the six ASCII code digits
a, b = independent random nonzero 32-byte unsigned integers
k = INT(H512(PAD(N) || PAD(g)))
x = INT(H512(salt || H512(I || ":" || P)))
v = g^x mod N
A = g^a mod N
B = (k*v + g^b) mod N
u = INT(H512(PAD(A) || PAD(B)))
S_companion = (B - k*g^x mod N)^(a + u*x) mod N
S_device = (A * v^u mod N)^b mod N
K_srp = H512(MIN(S))
```

The receiver MUST require `0 < A < N` or `0 < B < N`, and `u != 0`, and MUST
abort on `S = 0` or a zero client subtraction base. Public values are always
padded to 384 bytes. Salts keep all 16 bytes, including leading zeros. No
verifier or password hash is sent. net-v1's leading-zero vectors and its notes
on ESP-IDF `esp_srp` and RustCrypto `srp` padding apply unchanged.

### Transcript, proofs and key delivery

`request`, `offer`, `start` and `challenge` are the complete items. A retry
keeps the original request and offer and replaces start and challenge.

```text
T_pair = H("touchdeck/net-v2/pair\0" || request || offer || start || challenge)
PRK_pair = HKDF-Extract-SHA256(salt = T_pair, IKM = K_srp)
derive(PRK, label) = HKDF-Expand-SHA256(PRK, "touchdeck/net-v2/" || label, 32)
kc = derive(PRK_pair, "pair/client")
kd = derive(PRK_pair, "pair/deck")
kw = derive(PRK_pair, "pair/wrap")
ka = derive(PRK_pair, "pair/commit")
client_proof = MAC(kc, T_pair)
device_proof = MAC(kd, T_pair || client_proof)
commit_proof = MAC(ka, "commit" || T_pair || H(LTK))
stored_proof = MAC(ka, "stored" || T_pair || H(LTK))
```

The labels keep net-v1's words (`deck`, `client`) so the old code's
derivation carries over; only the prefix changes.

The Device verifies `client_proof` before it sends its proof or creates the
long-term key. It generates a fresh random 32-byte `LTK` and seals it with
AES-256-GCM under `kw`, a nonce of twelve zero bytes, and associated data
`T_pair || client_proof || device_proof`. `wrapped_key` is ciphertext then
tag. The zero nonce is used once with this attempt's unique key. The Companion
verifies `device_proof` before it opens the envelope.

The Companion writes a pending credential to the OS credential store before
`pair.commit`. After checking `commit_proof`, the Device atomically stores
`{companion_id, companion_name, LTK}` in NVS and sends `pair.stored`. The
Companion marks the Pairing confirmed only after checking `stored_proof`. The
Device then ends the carrier connection. A fresh carrier connection opens a
session with the LTK.

If `pair.stored` is lost, the Companion keeps the pending key and tries a
session. Success confirms it. Failure leaves it pending until the person
retries or forgets it. While USB is online the session gets `busy`, so the
Companion confirms later. An uncommitted key on the Device is discarded on
disconnect. The Device refuses an existing `companion_id` and never overwrites
or evicts a Pairing. It stores at most four, and refuses `pair.request` with
`full` before it shows a code.

## Session setup

Every session, including the first after pairing, runs this exchange. Its
deadline is 10000 ms from the carrier connection starting. Each side makes a
fresh X25519 key pair for this session only: `e_c` and `E_c = X25519(e_c, 9)`
for the Companion, `e_d` and `E_d` for the Device.

| Kind | Name | Sender | Fields |
|---|---|---|---|
| `0x20` | `session.open` | Companion | `device_id[6], companion_id[16], client_share[32]` |
| `0x21` | `session.challenge` | Device | `device_share[32], device_proof[32]` |
| `0x22` | `session.proof` | Companion | `client_proof[32]` |
| `0x23` | `session.ready` | Device | `ready_proof[32]` |

`client_share` is `E_c` and `device_share` is `E_d`, each the 32-byte
little-endian u-coordinate from RFC 7748. The fresh shares replace net-v1's
session nonces.

```text
T_session = H("touchdeck/net-v2/session\0" || device_id || companion_id || client_share || device_share)
Z = X25519(e_c, device_share) = X25519(e_d, client_share)
PRK_session = HKDF-Extract-SHA256(salt = T_session, IKM = LTK || Z)
kd = derive(PRK_session, "session/deck")
kc = derive(PRK_session, "session/client")
kr = derive(PRK_session, "session/ready")
key_c2d = derive(PRK_session, "session/c2d")
key_d2c = derive(PRK_session, "session/d2c")
device_proof = MAC(kd, T_session)
client_proof = MAC(kc, T_session || device_proof)
ready_proof = MAC(kr, T_session || client_proof)
```

The Device checks `device_id` against its own and looks up `companion_id`. An
unknown id gets `auth` before any X25519 work. Both sides MUST reject an
all-zero `Z` (RFC 7748 section 6.1), which a low-order share produces, by
closing the carrier connection. The Companion checks `device_proof` before it
sends its proof. The Device checks `client_proof` before `session.ready`. The
Companion checks `ready_proof` before it sends a record. A failed check closes
the carrier connection and erases the ephemeral keys, without deleting the
Pairing. Each side erases its X25519 private key once it has `Z`, and erases
`Z` and `PRK_session` once the five keys are derived. A session shows in
`net.state` only after the Device sends `session.ready`.

What this gives:

* Both proofs need the LTK, so only a paired Companion and its Device can
  finish the exchange. The shares are inside the transcript, so an attacker
  can't swap them.
* Traffic keys depend on `Z`. Someone who records a session and later steals
  the LTK, from NVS or the OS credential store, still can't compute `Z` and
  can't decrypt it. That is forward secrecy, which net-v1 lacked. A stolen LTK
  still lets the thief open new sessions until the Pairing is removed.
* Pairing resists offline code guessing from a passive capture or a fake
  Device that lacks the code. An active impersonator can test one guess per
  exchange, three per window on the real Device. The Companion MUST ask the
  person again after a failed proof and MUST NOT retry a code by itself.

## Records

Each direction has its own counter, starting at zero. Direction 0 is Companion
to Device, 1 is Device to Companion. Counters are implicit; only the next
expected one is accepted.

```text
nonce = 00000000 || u64be(counter)
len = plaintext.length + 16
AAD = "TDN2" || direction:u8 || u64be(counter) || u16be(len)
body = AES-256-GCM(key_direction, nonce, plaintext, AAD)
item = u16be(len) || body
```

The sender serializes writes per direction, increments its counter once per
sealed record, and never retries a partly written record on the same carrier
connection. A link chunk retransmission is a new record with a new counter.
The receiver authenticates before it exposes plaintext or refreshes liveness,
then checks the whole link frame body: CRC, frame version, header and exact
payload length. Any AEAD failure, replay, skipped or reordered record, invalid
body, end of stream mid-item, or impossible length closes the session. A valid
frame with an invalid message follows link-v1 dispatch. Nobody scans for a new
record after a framing error.

Counters MUST NOT wrap. Close before sending or receiving counter `2^32`.
Reconnect to continue. Counters and keys never outlive the carrier connection.

## Link protocol selection and liveness

After `session.ready`, the Companion runs link-v2's negotiation exactly as on
USB. It sends the plain `{"m":"hello"}`. A Device that implements net-v2 MUST
answer with `protocols` containing 2. The Companion then sends
`{"m":"hello","proto":2}`, and the reply selects protocol 2 for this session
and carries `id`, `name` and `caps`. All link-v2 rules apply unchanged:
re-selection, transfer aborts, live value resets, error reasons and limits.
The Companion's link session is the same code on every Carrier; only frame
delivery differs.

The Companion sends `hello` at once after `session.ready` and `ping` every
2000 ms, even with other traffic. The link is online on `hello` and expires
5000 ms after the last valid authenticated frame. On a network Carrier, expiry
closes the carrier connection rather than only resetting selection. Before the
first `hello`, the Device closes 5000 ms after `session.ready`. The Companion
closes after 5000 ms without a valid frame from the Device. Partial items and
invalid bytes refresh nothing. Closing the session releases the Companion's
held keys and aborts its transfers.

Messages that another contract limits to USB, such as Wi-Fi credentials, keep
that limit; their contract defines the refusal over a network Carrier.

## Timeouts and limits

| Item | Value |
|---|---|
| First handshake item after the carrier connection starts | 10000 ms |
| Session setup, from the carrier connection starting | 10000 ms |
| Pairing window, from the code appearing | 120000 ms |
| Pairing attempt, from the first byte of `pair.start` | 10000 ms |
| First `hello` after `session.ready` | 5000 ms |
| `ping` interval, link expiry | 2000 ms, 5000 ms |
| `busy` write, terminal drain | 100 ms each |
| Handshake body | 512 bytes |
| Record body (`len`) | 24 to 1048 bytes |
| Item on the wire | at most 1050 bytes |
| BLE fragment | 1 to `ATT_MTU - 3` bytes, 20 when the MTU is unknown |
| BLE initial credit | at least 1050 bytes, 2100 recommended |
| Records per direction per session | `2^32` |
| Stored Pairings | 4 |
| Failed proofs per pairing window | 3 |

## Storage, management and secrets

The Device stores Pairings in NVS, separate from Layouts and Wi-Fi
credentials. net-v1 never shipped, so there's nothing to migrate. The
Companion keeps LTKs in the OS credential store (Windows Credential Manager or
the Secret Service on Linux, for example through the `keyring` crate). It may
keep `device_id`, `companion_id` and the Device name in ordinary settings.
Forgetting a Device in the Companion deletes its local key and closes its
session; it doesn't revoke the Device's entry. `net.forget` on the Device does.

These link messages work over USB and over an authenticated net-v2 session.
They use the normal link response envelope. Their JSON names stay as in net-v1
([ADR 0003](../../docs/adr/0003-brand-changes-wire-names-stay.md)).

| Request | Successful response fields |
|---|---|
| `{"m":"net.state"}` | `listening` Boolean, `port:47820`, `ble` Boolean, `session` null or `{app_id,app_name,carrier,since_ms}`, `paired` 0 to 4, `pairing` Boolean |
| `{"m":"net.forget"}` | The same fields after clearing every Pairing |

`app_id` is `companion_id` as 32 lowercase hex characters and `app_name` is
`companion_name`. `carrier` is `"wifi"` or `"ble"`. `since_ms` is the
Device's monotonic time at `session.ready`. `listening` reflects the TCP
listener and `ble` whether the link service is advertising or connected.
`pairing` is true only while a code is shown. A failure is `ok:false` with an
`error` sentence and `reason:"storage"`. `net.forget` cancels a window, erases
every Pairing durably and ends the session. Over a network Carrier it sends
its final encrypted reply, with `session:null`, best effort within 100 ms,
then closes. A failed erase still ends the session and reports the failure.

Codes, verifiers, private exponents, X25519 private keys, `Z`, LTKs and
session keys MUST NOT appear in logs, console replies, `hello`, screenshots
other than the pairing display, crash reports or evidence. Synthetic fixture
values are the only exception.

## Resource limits

net-v1's budget applies to whichever network Carrier is active: 16 KiB steady
state for the network owner, up to 64 KiB during SRP, and at most 8 KiB of
either in internal DRAM. X25519 adds a few hundred bytes per session. There is
one SRP context, one 512-byte handshake buffer, one 1048-byte receive record
body, one 1048-byte transmit record body, and on BLE one reassembly buffer
sized to the credit the Device grants. lwIP, Wi-Fi and NimBLE buffers are
outside this budget and still count against the radio build's 24 KiB
internal-RAM floor. Crypto runs below UI priority, never in a UI, USB or BLE
host callback.

## Implementation notes

These were checked on 2026-11-06. They point at the primitives the earlier
`td_netlink_core` (Firmware) and `td-link` `net.rs` (Companion) code already
used: SRP-6a 3072/SHA-512, AES-256-GCM, HMAC and HKDF-SHA256 through Mbed TLS
PSA on the Device, and `srp`, `aes-gcm`, `hkdf`, `hmac` and `sha2` in Rust.

* **Device X25519.** The ESP-IDF 6.1 tree has
  `CONFIG_MBEDTLS_ECP_DP_CURVE25519_ENABLED`, default `y`, which its
  `esp_config.h` maps to `PSA_WANT_ECC_MONTGOMERY_255`. Use `psa_generate_key`
  with `PSA_KEY_TYPE_ECC_KEY_PAIR(PSA_ECC_FAMILY_MONTGOMERY)`, 255 bits, and
  `psa_raw_key_agreement(PSA_ALG_ECDH, ...)`. The radio build's sdkconfig must
  keep the option on.
* **Companion X25519.** `x25519-dalek` 2 with `EphemeralSecret` and
  `SharedSecret::was_contributory()` for the all-zero check.
* **Companion BLE.** One GATT client that runs on WinRT and BlueZ, for example
  `btleplug`. It writes `rx` without response and subscribes to `tx` and
  `flow`. It reads the ATT MTU only where the platform reports it.
* **SRP.** net-v1's notes on `esp_srp` (log-tag cap, salt generation, no
  double hashing of `K_srp`) and on RustCrypto `srp` 0.6 (unpadded `u`) apply
  as written.

## Reference and conformance

The Node reference is [net-ref/net-v2.mjs](net-ref/net-v2.mjs). It reuses the
net-v1 reference's SRP arithmetic and link frame checks, and uses only Node's
built-in crypto. [net-ref/vectors-v2.mjs](net-ref/vectors-v2.mjs) regenerates
the vectors from fixed public inputs; the test checks the committed file
without rewriting it. Run from the repository root with Node 22 or later:

```powershell
node --test schema/protocol/net-ref/net-v2.test.mjs
node schema/protocol/net-ref/vectors-v2.mjs --write
```

[net-v2-vectors.json](net-v2-vectors.json) holds:

* `pake` and `pairing`: SRP values, every pairing item, the transcript, keys,
  proofs and the wrapped LTK.
* `session`: both X25519 shares, `Z`, the transcript, keys, proofs and the
  four session items.
* `records`: the discovery and selected link-v2 hellos, two records each way,
  built from [golden-frames-v2.json](golden-frames-v2.json). The selected
  reply carries the fixture `device_id` as its `id`. The two selected records
  include their BLE fragments at ATT_MTU 23 and 247.
* `tampered_record`, which MUST close the receiver, and
  `non_contributory_share`, an all-zero share that MUST close the handshake.
* `errors.busy` and the `ble` UUIDs and recommended initial credit.

Implementations also test wrong codes, wrong ids, bad public values,
reflection, replay, reorder, truncation, counter exhaustion, maximum-size
items, fragment boundaries at ATT_MTU 23, and credit overrun.
