# TouchDeck network link v1

This contract carries [link-v1](link-v1.md) over TCP port 47820. The pairing
suite is SRP-6a, RFC 5054's 3072-bit group with generator 5 and SHA-512.
Session authentication uses HMAC-SHA256 and HKDF-SHA256. Encryption uses
AES-256-GCM with a 16-byte tag. There is one suite and no negotiation.

Normative words MUST, MUST NOT, and SHOULD describe implementation requirements.
All test secrets in `net-v1-vectors.json` are public synthetic fixtures. They
MUST NOT be used by a device or app outside the reference tests.

## Discovery and ownership

The app discovers `_touchdeck._tcp` or accepts a typed IP address. The service
port is 47820. TXT fields are `id=<12 lowercase hex station MAC>`, `proto=1`,
`name=<hostname>`, and `paired=<decimal count>`. Discovery is untrusted.
The app pins the six-byte deck id when pairing and checks it on reconnect.
An address change does not change the pairing identity.

The deck admits one TCP owner, including a connection waiting for code entry.
It sends the `busy` handshake error to a second connection, then closes that
connection. The write is bounded to 100 ms and allocates no PAKE context.
USB remains independent. Replies return on the request's transport. Events go
to every online transport. One app uses one transport per deck and prefers USB
when USB answers `hello`. Switching transport closes the old session, releases
held app keys, and starts a new `hello`. Transfer state belongs to its initiating
transport and is aborted if that transport closes. A competing transfer follows
link-v1's existing replacement rule, without redirecting replies or chunks.

No text console, log output, SLIP delimiter, or SLIP escaping runs over TCP.

## Byte conventions and framing

`||` concatenates byte strings. `u16be` and `u64be` are unsigned big-endian
integers. `H` is SHA-256, `H512` is SHA-512, and `MAC(k,m)` is HMAC-SHA256.
Literal strings below are ASCII, with no terminator unless `\0` is written.
`hex` uses lowercase ASCII. Random values come from a cryptographic RNG.
Each random value is generated independently. Comparisons of proofs and tags
MUST be constant-time. Secrets MUST be erased when their state ends.

Every TCP item has `u16be(len) || body`, where `len` excludes the length prefix.
TCP reads may split or combine items. Receivers read exactly the announced size
and reject a length outside the bounds for their current state before allocation.
There are two phases, which MUST NOT be detected by sniffing bytes.

* Before session authentication, `body = kind:u8 || version:u8 || fields`.
  Version is 1. These bounded handshake items are public, except the encrypted
  key envelope in `pair.accept`. There are no application frames in this phase.
* After `session.ready`, `body = ciphertext || tag`. Every application record
  is AEAD-sealed. Its plaintext is exactly one complete decoded link-v1 body,
  including the six-byte header, payload, and two-byte CRC. Payload length
  remains little-endian. Plaintext length is 8 to 1032, sealed length 24 to 1048.

Handshake items have at most 512 body bytes. Unknown kinds, versions, trailing
bytes, unexpected state transitions, malformed UTF-8, and wrong field lengths
close the connection. Handshake errors use kind `0x7f`, version 1, and a one-byte
reason. They are unauthenticated hints and never establish identity or cause
stored credentials to be deleted.

| Reason byte | Name | Meaning |
|---|---|---|
| 1 | `busy` | Another TCP owner or pairing window owns the deck |
| 2 | `version` | Unsupported protocol version |
| 3 | `args` | Invalid handshake fields or state |
| 4 | `full` | Four app entries exist, or this app id already exists |
| 5 | `auth` | Authentication failed, including unknown app id |
| 6 | `expired` | Deadline or pairing window expired |
| 7 | `cancelled` | The user cancelled pairing |
| 8 | `storage` | Pairing could not be committed |

For example, `busy` is hex `00037f0101`. Errors normally close the connection.
Only the first two `auth` errors after a pairing proof return to code entry.
An established session closes silently on any cryptographic or framing error.
No plaintext error is accepted once session authentication finishes.

## Pairing window and identity

The first item is either `pair.request` or `session.open`. `pair.request` asks
the deck to display a fresh six-digit decimal code, zero-padded to six ASCII
digits, full screen with a Cancel button. Sample uniformly from 0 to 999999
using rejection sampling. The window lasts 120000 ms from display, on a
monotonic clock. The code never leaves the display or PAKE computation.

`app_id` is a random 16-byte identifier, stable for this app installation.
`app_name` is 1 to 64 bytes of valid UTF-8, with no NUL or Unicode control
characters. `name_len` counts bytes. The name identifies the entry to a user
and grants no authority. `deck_id` is the six-byte station MAC in network order.
`window_id` is a fresh random 16-byte value. A new request cannot replace or
extend an existing window. Three failed client proofs close the window and
connection. Another window requires a new request and a new code.

Cancel, socket closure, successful commit, or expiry destroys the window,
code, salt, verifier, private exponents, and temporary keys. A failed proof
destroys that attempt's SRP context but preserves the same window and failure
count. Each retry uses fresh exponents, salt, and nonces. An invalid public
value or malformed handshake closes the connection and window outright.

`net pair` opens an unbound window for local testing. Its first `pair.request`
binds the existing window to that socket and receives the remaining time.
Neither this command nor any state reply prints the code. The pairing display
is the permitted way to obtain a real code for manual verification.

## Pairing exchange

`E(kind,fields)` means the complete encoded handshake item, including its
length, kind, and version. Fields below are concatenated in table order.
Numbers in brackets are byte lengths, not lengths to send.

| Kind | Name | Sender | Fields |
|---|---|---|---|
| `0x10` | `pair.request` | app | `app_id[16], name_len:u8, app_name[name_len]` |
| `0x11` | `pair.offer` | deck | `deck_id[6], window_id[16], remaining_ms:u32be` |
| `0x12` | `pair.start` | app | `A[384], client_nonce[32]` |
| `0x13` | `pair.challenge` | deck | `salt[16], B[384], deck_nonce[32]` |
| `0x14` | `pair.proof` | app | `client_proof[32]` |
| `0x15` | `pair.accept` | deck | `deck_proof[32], wrapped_key[48]` |
| `0x16` | `pair.commit` | app | `commit_proof[32]` |
| `0x17` | `pair.stored` | deck | `stored_proof[32]` |

The deck waits for code entry after `pair.offer`. The app sends `pair.start`
only after the user enters the code. A pairing attempt has a 10000 ms absolute
deadline starting with the first byte of `pair.start`. It includes all crypto,
proofs, persistence, and `pair.stored`. The 120000 ms window deadline still
applies. Waiting for human input is the sole exception to the 10000 ms handshake
deadline. The initial request must finish within 10000 ms of TCP acceptance.
Partial reads never extend a deadline. The deck checks deadlines on every
transition, including before NVS commit.

### SRP arithmetic

`N` is exactly the 3072-bit prime in RFC 5054 Appendix A, group 5. `g = 5`.
No group is supplied by a peer. `PAD(n)` is the unsigned 384-byte big-endian
encoding. `MIN(n)` is its shortest nonempty unsigned big-endian encoding.
`INT(bytes)` reads an unsigned big-endian integer.

```text
I = "touchdeck:" || hex(deck_id) || ":" || hex(app_id)
P = the six ASCII code digits
a, b = independent random nonzero 32-byte unsigned integers
k = INT(H512(PAD(N) || PAD(g)))
x = INT(H512(salt || H512(I || ":" || P)))
v = g^x mod N
A = g^a mod N
B = (k*v + g^b) mod N
u = INT(H512(PAD(A) || PAD(B)))
S_app = (B - k*g^x mod N)^(a + u*x) mod N
S_deck = (A * v^u mod N)^b mod N
K_srp = H512(MIN(S))
```

The subtraction is modulo N. The receiver MUST require `0 < A < N` or
`0 < B < N`, as applicable, and `u != 0`. It MUST abort on `S = 0` or on a
zero client subtraction base. Public values on the wire are always padded,
even when their most significant byte is zero. Salts retain all 16 bytes,
including leading zero bytes. No verifier or password hash is sent.

### Transcript, proofs, and key delivery

`request`, `offer`, `start`, and `challenge` below are the actual complete
`E` bytes. A retry retains the original request and offer and replaces start
and challenge. The transcript binds roles, version, group selection through
the fixed protocol, both identities, the app name, window, and randomness.

```text
T_pair = H("touchdeck/net-v1/pair\0" || request || offer || start || challenge)
PRK_pair = HKDF-Extract-SHA256(salt=T_pair, IKM=K_srp)
derive(PRK, label) = HKDF-Expand-SHA256(PRK, "touchdeck/net-v1/" || label, 32)
kc = derive(PRK_pair, "pair/client")
kd = derive(PRK_pair, "pair/deck")
kw = derive(PRK_pair, "pair/wrap")
ka = derive(PRK_pair, "pair/commit")
client_proof = MAC(kc, T_pair)
deck_proof = MAC(kd, T_pair || client_proof)
```

The deck verifies the client proof before sending its proof or creating the
long-term key. It generates a fresh independent random 32-byte `LTK` and seals
it with AES-256-GCM under `kw`, nonce twelve zero bytes, and associated data
`T_pair || client_proof || deck_proof`. `wrapped_key` is ciphertext then tag.
The zero nonce is used exactly once with this attempt's unique wrapping key.
The app verifies the deck proof before opening the envelope. Nothing is stored
or trusted on a failed proof or tag. Built-in SRP proof formats are not used.

```text
commit_proof = MAC(ka, "commit" || T_pair || H(LTK))
stored_proof = MAC(ka, "stored" || T_pair || H(LTK))
```

The app writes a pending credential to its OS keyring before `pair.commit`.
After validating the commit proof, the deck atomically stores
`{app_id, app_name, LTK}` in NVS and sends `pair.stored`. The app marks its entry
confirmed only after verifying the stored proof. The deck closes the socket
after sending it. Pairing does not carry application records. A fresh TCP
connection authenticates with the LTK before any link messages are sent.

If the final acknowledgement is lost, the app retains the pending key and tries
session authentication. Success confirms it. Failure leaves the pending entry
for an explicit user retry or forget, because a network error cannot prove that
NVS failed. An uncommitted key on the deck is discarded on disconnect. The deck
rejects an existing app id and never silently overwrites or evicts a pairing.
There are at most four entries. Full storage refuses a request before showing
a code. A lost key therefore needs explicit forget and pairing again.

## Reconnecting session

Every connection, including the first after pairing, performs this exchange.
Its absolute deadline is 10000 ms from TCP acceptance. The app selects its
stored deck and app identities, and each side generates a fresh 32-byte nonce.

| Kind | Name | Sender | Fields |
|---|---|---|---|
| `0x20` | `session.open` | app | `deck_id[6], app_id[16], client_nonce[32]` |
| `0x21` | `session.challenge` | deck | `deck_nonce[32], deck_proof[32]` |
| `0x22` | `session.proof` | app | `client_proof[32]` |
| `0x23` | `session.ready` | deck | `ready_proof[32]` |

```text
T_session = H("touchdeck/net-v1/session\0" || deck_id || app_id || client_nonce || deck_nonce)
PRK_session = HKDF-Extract-SHA256(salt=T_session, IKM=LTK)
kd = derive(PRK_session, "session/deck")
kc = derive(PRK_session, "session/client")
kr = derive(PRK_session, "session/ready")
key_c2d = derive(PRK_session, "session/c2d")
key_d2c = derive(PRK_session, "session/d2c")
deck_proof = MAC(kd, T_session)
client_proof = MAC(kc, T_session || deck_proof)
ready_proof = MAC(kr, T_session || client_proof)
```

The deck checks its own id and looks up the app id. An unknown id and a bad
proof both produce `auth`, without identity details. The app verifies the
deck proof before sending its proof. The deck verifies that proof before
`session.ready`. The app verifies the ready proof before sending records.
`session.ready` is the last handshake item. A failed check closes the socket
and erases ephemeral keys, without deleting a stored pairing. A session is
present in `net.state` only after the deck sends `session.ready`.

The LTK challenge-response authenticates both sides and refreshes traffic keys.
It does not provide forward secrecy against later LTK compromise. Recorded
sessions become decryptable if that key is stolen. Pairing resists offline
code guessing from a passive capture or from a fake deck without the code.
An active impersonator can test a guess per PAKE exchange. Three failed proofs
bound guesses per window on the real deck. A fake deck can lie about its UI,
so the app MUST require another explicit user submission after a failed proof
and MUST NOT automatically retry a code against new challenges.

## Authenticated records and liveness

Each direction has an independent counter starting at zero. Direction byte
0 means app to deck, and 1 means deck to app. Counters are implicit and are
not transmitted. The next expected counter is the only accepted counter.

```text
nonce = 00000000 || u64be(counter)
len = plaintext.length + 16
AAD = "TDN1" || direction:u8 || u64be(counter) || u16be(len)
body = AES-256-GCM(key_direction, nonce, plaintext, AAD)
wire = u16be(len) || body
```

The sender serializes all writes in each direction. It increments its counter
once per complete sealed record, and never retries a partially written record
on the same socket. A partial write failure closes the socket. A link-v1
chunk retransmission is a newly sealed record with a new counter.
The receiver authenticates before exposing any plaintext or updating liveness.
It then validates the entire decoded link-v1 body, including CRC, version,
header, and exact payload length. Any AEAD failure, replay, skipped or reordered
record, invalid body, EOF mid-record, or impossible length closes the session.
A full authenticated frame with an invalid message payload follows link-v1's
dispatch rules. Neither endpoint scans for a new record after a framing error.

Counters MUST NOT wrap. Close before sending or receiving counter `2^32`.
This bounds each direction to `2^32` records per key. Reconnect with new nonces
to continue. Counters and session keys never persist across TCP connections.

The app sends `hello` immediately after session authentication and `ping` every
2000 ms, even when other traffic exists. The link becomes online on `hello` and
expires 5000 ms after the last valid authenticated frame, as in link-v1.
The TCP session closes on that expiry. Before the first `hello`, it closes
5000 ms after `session.ready`. The app also closes after 5000 ms without a valid
authenticated frame from the deck. Invalid or partial bytes do not refresh
either deadline. Events are emitted only to online links. USB online state is
separate, and the deck considers app-controlled tiles online if either link is
online. Closing Wi-Fi releases its app-side held keys and aborts its transfers.

## Storage, management, and secrets

The deck stores pairings in NVS, separately from layouts and Wi-Fi credentials.
The app stores LTKs in the OS keyring through the `keyring` crate. It may store
deck id, app id, and display name in ordinary settings. An app's forget action
deletes that deck's local keyring entry and closes its session. It does not
revoke the deck's NVS entry. Deck-wide `net forget` performs revocation.

These messages are available over USB and authenticated Wi-Fi. They use the
normal link-v1 response envelope with the same `m`, `ok`, and frame sequence.
No unauthenticated TCP caller can dispatch them.

| Request | Successful response fields |
|---|---|
| `{"m":"net.state"}` | `listening` Boolean, `port:47820`, `session` null or `{app_id,app_name,since_ms}`, `paired` 0 to 4, `pairing` Boolean |
| `{"m":"net.forget"}` | The same state fields after clearing all app entries |

`app_id` in JSON is 32 lowercase hex characters. `since_ms` is the deck's
monotonic boot time at session establishment. `pairing` is true only while the
code is displayed. `listening` reflects the TCP listener, independent of a live
session. A failure uses `ok:false`, an `error` sentence, and `reason:"storage"`.
`net.forget` cancels a window, erases all pairings durably, and drops the network
session. Over Wi-Fi it sends its final encrypted response before closing, with
`session:null` in the response. The write is best effort and bounded by 100 ms.
USB receives its response normally. Failure to erase still closes the network
session and reports failure, so a caller never mistakes failure for revocation.

Console `net state` and `net forget` return the same fields in the console
envelope. `net pair` opens the test window described above. All keep USB safe.
Passwords, codes, verifiers, private exponents, LTKs, and session keys MUST NOT
appear in logs, console replies, `hello`, screenshots other than the pairing
display, crash reports, or evidence. Normal state commands have no code hook.
Synthetic fixture values are explicitly public and are the sole test exception.

## Resource limits

The network owner has a 16 KiB steady-state allocation budget, including its
task stack, receive buffer, transmit buffer, queues, and crypto state. Pairing
may raise that total to 64 KiB temporarily for 3072-bit arithmetic. At most
8 KiB of either budget may consume internal DRAM. Buffers, suitable stacks,
and large MPI allocations use PSRAM where supported. These are implementation
ceilings, not measured hardware results. Existing shared lwIP, Wi-Fi, and link
transfer buffers are outside this incremental budget and still count against
the project's at-least-24-KiB free internal-DRAM gate.

There is one SRP context, one 512-byte maximum handshake body, one 1048-byte
receive body, and one 1048-byte transmit body. Hash transcripts incrementally.
Queues are bounded. Handshake state is freed before normal link traffic starts.
Crypto runs below UI priority and never in a UI or USB handler. A budget miss
must be reported before the firmware owner claims readiness.

## Implementation compatibility

The inspected ESP-IDF 6.1 checkout ships Mbed TLS 4.1.0 and protocomm's
`esp_srp` implementation. `ESP_NG_3072`, SHA-512, and MPI modular exponentiation
are present. `CONFIG_ESP_PROTOCOMM_SUPPORT_SECURITY_VERSION_2=y` builds that
code. The firmware also needs SHA-256, HMAC, HKDF, and AES-GCM through Mbed TLS's
PSA API. `PSA_ALG_HKDF(PSA_ALG_SHA_256)` and `PSA_ALG_GCM` are the corresponding
algorithms. A plain X25519 exchange with the code in a hash is not a PAKE.

Firmware can call `esp_srp_set_salt_verifier`,
`esp_srp_srv_pubkey_from_salt_verifier`, and `esp_srp_get_session_key`.
The last returns `H512(MIN(S))`, already the required `K_srp`. It MUST NOT be
hashed a second time. The wrapper pads outgoing B and validates incoming A.
Generate a fixed-length random salt and verifier explicitly before setting
them. The convenience salt generator converts a random MPI to minimum-length
bytes and therefore does not preserve leading zeros reliably. The wrapper
uses the proofs in this document, not `esp_srp_exchange_proofs`.

The `srp6a` ESP-IDF log tag MUST be capped below DEBUG before any SRP call.
The inspected source has debug statements for the password and intermediate
secrets. The cap must also hold when application-wide debug logging is enabled.
The firmware owner verifies zero secret exposure and checks allocation and RNG
return values. The public APIs do not promise constant-time MPI arithmetic.
This protocol addresses network attackers, not physical side-channel attacks.

RustCrypto maintains `srp` in [RustCrypto/PAKEs](https://github.com/RustCrypto/PAKEs).
The registry checked on 2026-10-26 lists stable 0.6.0 and 0.7.0-rc.3, updated
2026-04-03. Stable `srp = "0.6"` with `sha2 = "0.10"` and `num-bigint = "0.4"`
can use `SrpClient::<Sha512>::new(&G_3072)`, `compute_x`, and
`compute_premaster_secret`, with explicitly padded k and u from this document.
Its `process_reply` hashes unpadded A and B when computing u, so it MUST NOT be
used as-is. SHA-512 of the returned minimum-length S yields `K_srp`.
The newer implementation pads u, but its built-in proofs still are not this
protocol's proofs. `aes-gcm`, `hkdf`, `hmac`, and `sha2` supply the session suite.

SPAKE2 was considered. RustCrypto supplies it, but the inspected ESP-IDF tree
does not supply the same SPAKE2 construction. Selecting SRP reuses the shipped
deck implementation and avoids a new curve or PAKE port. The cost is larger
pairing arithmetic and the explicit encoding adapter described above.

## Reference and conformance

The Node reference and one-command test live in `net-ref/`. They use only
Node's built-in crypto. Run from the repository root with Node 22 or later.

```powershell
node --test schema/protocol/net-ref/net.test.mjs
```

`net-v1-vectors.json` fixes public test randomness and supplies the SRP
intermediates, exact handshake bytes, transcript hashes, HKDF outputs, proofs,
wrapped LTK, two application records each way, and a tampered record that MUST
close the receiver. Implementations also test leading-zero encodings, bad
public values, wrong codes, wrong identities, reflection, replay, reorder,
truncation, counter exhaustion, and maximum-size frames. The reference's
big-integer arithmetic is for verification, not a production crypto backend.
