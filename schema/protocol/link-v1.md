# TouchDeck app link v1

The app link shares the debug console's USB-Serial/JTAG byte stream. Text commands
and their `@td` replies remain available between binary frames. The device uses
VID `303a` and PID `1001`. Opening the port does not reset it.

## Frame layout

A frame begins and ends with `0xC0`. Each frame has both delimiters, including
when two frames are adjacent. Within the body, `0xC0` becomes `0xDB 0xDC` and
`0xDB` becomes `0xDB 0xDD`. No other escape is valid. `0xC0` is not a valid UTF-8
byte, so the receiver recognizes frames without a text-mode command. A literal
`0xDB` outside a frame remains text.

The decoded body has these fields in order.

| Offset | Field | Encoding |
|---|---|---|
| 0 | `ver` | u8, always 1 |
| 1 | `type` | u8, message 1, chunk 2, ack 3 |
| 2 | `seq` | u8, copied from the request into its response |
| 3 | `flags` | u8, request 0, response 1, event 2 |
| 4 | `len` | u16le, decoded payload length, 0 to 1024 |
| 6 | `payload` | Exactly `len` bytes |
| 6 + `len` | `crc` | u16le, CRC-16/CCITT-FALSE of bytes 0 through the payload |

CRC-16/CCITT-FALSE uses polynomial `0x1021`, initial value `0xFFFF`, no
reflection, and no final XOR. Its check value for ASCII `123456789` is `0x29B1`.

A maximum decoded body is 1032 bytes. The receiver unescapes incrementally into
a 1100-byte buffer. The encoder reserves up to 2066 wire bytes, including both
delimiters, so a payload made entirely of `0xC0` and `0xDB` fits. A bad escape,
oversized body, invalid header, unsupported version, length mismatch, or CRC
mismatch increments `crc_errors` and produces no reply. The receiver discards
a malformed SLIP frame through its closing delimiter, then resumes text input.

Only flags 0, 1, and 2 are valid. An event has `seq` 0. Requests may use any u8
sequence, though clients normally cycle through 1 to 255. A client keeps a
sequence unique among outstanding requests and matches replies by sequence.

## Payloads

| Type | Payload |
|---|---|
| Message, 1 | One UTF-8 JSON object with a string `m` field |
| Chunk, 2 | `xfer` u8, `idx` u16le, then 0 to 1021 data bytes |
| Ack, 3 | `xfer` u8, then `next` u16le, exactly 3 bytes |

A maximum chunk has 1021 data bytes and a 1024-byte payload. The codec validates
chunk and ack lengths. M2-01 dispatches only request messages. Chunk transfers,
acks, and events have wire formats reserved for later milestones.

Transfers use a window of four unacknowledged chunks. `idx` starts at zero, and
`next` acknowledges every index below it and names the next contiguous index
the receiver needs. An ack identifies the transfer by `xfer`. Its frame sequence
echoes the request that caused it. A sender retransmits unacknowledged chunks
after 200 ms, with at most five transmissions per chunk, including the first.
A receiver must accept a duplicate chunk without applying its data twice.

## Messages

A response is a type 1 frame with flags 1 and the request's sequence. It carries
the same `m`, a Boolean `ok`, and the fields below. All times are milliseconds
since boot. Counters reset at boot.

| Request | Successful response fields |
|---|---|
| `{"m":"hello"}` | `product:"touchdeck"`, `proto:1`, `elf_sha256`, `version`, `running`, `sinks:["log"]`, `recovery:false`, `layout:"builtin"` |
| `{"m":"ping","n":N}` | `t` and `n`, with the supplied `n` echoed |
| `{"m":"link.state"}` | `online`, `frames_rx`, `frames_tx`, `crc_errors`, `dropped`, `last_rx_ms` |

`elf_sha256` is the running app descriptor's 64-character lowercase hex hash.
`version` is its build version. `running` is the OTA partition label, normally
`ota_0` at this milestone. `ping` without `n` returns `n:null`.

An unknown `m` returns `ok:false` and `error:"unknown message"`. A payload that
cannot be parsed as one object with a string `m` produces no response and
increments `dropped`. An unsupported incoming frame type or flags, a full frame
queue, or a failed response also increments `dropped`.

`frames_rx` counts valid frames before dispatch, including frames later dropped.
`frames_tx` counts complete response writes. A `link.state` reply therefore
includes its request in `frames_rx` and excludes itself from `frames_tx`.
`last_rx_ms` records the most recent valid frame's arrival time. `online` becomes
true on `hello` and expires after 5000 ms without a valid frame. After expiry,
another `hello` establishes the connection again.

The text command `link state` returns the same state fields in the console's
usual `{"cmd":"link","ok":true,...}` envelope.

## Concurrency and discovery

The RX task splits text and frame bodies before text NUL mapping or overflow
handling. The console consumes text at priority 2. The `td_link` task consumes
the frame stream at priority 3 with a 6144-byte stack. Each channel preserves
its own input order. There is no cross-channel reply-order guarantee. For
example, a ping may answer before an earlier long console line is rejected.
Asynchronous console commands such as `perf fps` retain their existing delayed
reply behavior.

Frames, console replies, prompts, and echo use `td_console_write` or its line
variant under the stdout lock. ESP-IDF log formatting uses that same lock.
A complete frame cannot interleave with a text write or log line. A console
stream can contain frames between its writes, so clients demultiplex bytes
before splitting the text into lines.

Discovery probes every serial port with VID `303a` and PID `1001`. A candidate
is a deck only if it answers `hello` within 500 ms with product `touchdeck` and
protocol 1. The client opens without toggling DTR or RTS. Windows opens COM3
exclusively, so mixed traffic uses one port handle and one receive decoder.

GPIO19, GPIO20, the USB-Serial/JTAG controller, its clock, and its power remain
untouched. This milestone adds no light sleep, deep sleep, eFuse write, or
configuration that disables USB-Serial/JTAG. Unattended recovery through the
ROM's reset into download mode depends on those invariants.

## Golden frames

[golden-frames.json](golden-frames.json) supplies hex wire bytes and decoded
fields to the firmware tests, Python selftest, and the later Rust codec. A
fixture's `valid` field describes framing, not message dispatch. For example,
an empty message payload is a valid frame but not a dispatchable JSON object.

The fixtures include every escape, a maximum chunk with 1021 data bytes, a
maximum payload made entirely of `0xC0` and `0xDB`, a chunk with no data, and
invalid CRC, version, length, and escape cases. Tests compare exact bytes in
both codec directions and preserve frame state across fragmented reads.
