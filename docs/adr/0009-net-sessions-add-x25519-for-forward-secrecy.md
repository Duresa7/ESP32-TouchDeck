# Network sessions add an X25519 exchange for forward secrecy

net-v1 authenticated each Wi-Fi session with the Pairing's long-term key alone, so anyone who later stole that key from the Device's NVS or the Companion's credential store could decrypt every recorded session. net-v2 adds a fresh X25519 key pair on each side per session and feeds `LTK || Z` into HKDF, with both shares in the transcript that the LTK-keyed proofs cover. This follows TLS 1.3's PSK with (EC)DHE mode. The LTK still proves who is talking; `Z` makes the traffic keys unrecoverable once both ephemeral keys are erased.

## Considered options

- **Keep net-v1's LTK-only sessions.** No new primitive, but no forward secrecy, which the spec for Wi-Fi and BLE asked to close.
- **Run SRP again every session.** Reuses the shipped 3072-bit code, but needs the person's code each time and costs a 3072-bit exponentiation per connect.
- **Finite-field Diffie-Hellman over the SRP group.** Reuses the same MPI code, but its 384-byte shares are 19 fragments each at BLE's minimum 20-byte writes, and each side pays two 3072-bit exponentiations per session.
- **ECDH over P-256.** Standard and in Mbed TLS, but the ESP32-S3 has no ECC accelerator to favour it, its point validation is easier to get wrong, and X25519 is the simpler drop-in in RustCrypto and Node.
- **X25519.** 32-byte shares, constant-time by design, in the ESP-IDF 6.1 Mbed TLS build (`CONFIG_MBEDTLS_ECP_DP_CURVE25519_ENABLED`, default on), `x25519-dalek`, and Node's built-in crypto.

## Consequences

The Device's radio build must keep Curve25519 enabled in Mbed TLS. Both sides reject an all-zero shared secret. Pairing itself is unchanged: SRP already protects the LTK in transit, and a plain X25519 exchange with the code hashed in would not be a PAKE.
