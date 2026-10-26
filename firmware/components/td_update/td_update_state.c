#include "td_update_state.h"
#include <string.h>

static const uint32_t K[64] = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
};

static uint32_t ror(uint32_t x, unsigned n)
{
    return (x >> n) | (x << (32 - n));
}

static void compress(td_sha256_t *s, const uint8_t *p)
{
    uint32_t w[64];
    for (int i = 0; i < 16; i++) {
        w[i] = (uint32_t)p[4 * i] << 24 | (uint32_t)p[4 * i + 1] << 16 | (uint32_t)p[4 * i + 2] << 8 | p[4 * i + 3];
    }
    for (int i = 16; i < 64; i++) {
        const uint32_t s0 = ror(w[i - 15], 7) ^ ror(w[i - 15], 18) ^ (w[i - 15] >> 3);
        const uint32_t s1 = ror(w[i - 2], 17) ^ ror(w[i - 2], 19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16] + s0 + w[i - 7] + s1;
    }
    uint32_t a = s->h[0], b = s->h[1], c = s->h[2], d = s->h[3], e = s->h[4], f = s->h[5], g = s->h[6], h = s->h[7];
    for (int i = 0; i < 64; i++) {
        const uint32_t t1 = h + (ror(e, 6) ^ ror(e, 11) ^ ror(e, 25)) + ((e & f) ^ (~e & g)) + K[i] + w[i];
        const uint32_t t2 = (ror(a, 2) ^ ror(a, 13) ^ ror(a, 22)) + ((a & b) ^ (a & c) ^ (b & c));
        h = g;
        g = f;
        f = e;
        e = d + t1;
        d = c;
        c = b;
        b = a;
        a = t1 + t2;
    }
    s->h[0] += a;
    s->h[1] += b;
    s->h[2] += c;
    s->h[3] += d;
    s->h[4] += e;
    s->h[5] += f;
    s->h[6] += g;
    s->h[7] += h;
}

void td_sha256_init(td_sha256_t *s)
{
    static const uint32_t init[8] = { 0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                                      0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19 };
    memcpy(s->h, init, sizeof init);
    s->total = 0;
    s->fill = 0;
}

void td_sha256_update(td_sha256_t *s, const uint8_t *data, size_t len)
{
    s->total += len;
    while (len > 0) {
        if (s->fill == 0 && len >= 64) {
            compress(s, data);
            data += 64;
            len -= 64;
            continue;
        }
        size_t take = 64 - s->fill;
        if (take > len) {
            take = len;
        }
        memcpy(s->block + s->fill, data, take);
        s->fill = (uint8_t)(s->fill + take);
        data += take;
        len -= take;
        if (s->fill == 64) {
            compress(s, s->block);
            s->fill = 0;
        }
    }
}

void td_sha256_final(td_sha256_t *s, uint8_t out[TD_SHA256_BYTES])
{
    const uint64_t bits = s->total * 8;
    const uint8_t one = 0x80;
    td_sha256_update(s, &one, 1);
    const uint8_t zero = 0;
    while (s->fill != 56) {
        td_sha256_update(s, &zero, 1);
    }
    uint8_t length[8];
    for (int i = 0; i < 8; i++) {
        length[i] = (uint8_t)(bits >> (56 - 8 * i));
    }
    td_sha256_update(s, length, sizeof length);
    for (int i = 0; i < 8; i++) {
        out[4 * i] = (uint8_t)(s->h[i] >> 24);
        out[4 * i + 1] = (uint8_t)(s->h[i] >> 16);
        out[4 * i + 2] = (uint8_t)(s->h[i] >> 8);
        out[4 * i + 3] = (uint8_t)s->h[i];
    }
}

static int hex_digit(char c)
{
    if (c >= '0' && c <= '9') {
        return c - '0';
    }
    if (c >= 'a' && c <= 'f') {
        return c - 'a' + 10;
    }
    if (c >= 'A' && c <= 'F') {
        return c - 'A' + 10;
    }
    return -1;
}

bool td_sha256_from_hex(const char *hex, uint8_t out[TD_SHA256_BYTES])
{
    if (hex == NULL || strlen(hex) != TD_SHA256_HEX) {
        return false;
    }
    for (int i = 0; i < TD_SHA256_BYTES; i++) {
        const int hi = hex_digit(hex[2 * i]);
        const int lo = hex_digit(hex[2 * i + 1]);
        if (hi < 0 || lo < 0) {
            return false;
        }
        out[i] = (uint8_t)(hi << 4 | lo);
    }
    return true;
}

void td_update_state_init(td_update_state_t *u)
{
    memset(u, 0, sizeof *u);
    u->phase = TD_UPDATE_IDLE;
}

td_update_reason_t td_update_state_begin(td_update_state_t *u, uint32_t bytes, const char *sha256_hex,
                                         uint32_t slot_size, bool running_confirmed)
{
    if (u->phase == TD_UPDATE_RECEIVING || u->phase == TD_UPDATE_VERIFYING || u->phase == TD_UPDATE_REBOOTING) {
        return TD_UPDATE_E_BUSY;
    }
    if (!running_confirmed) {
        return TD_UPDATE_E_UNCONFIRMED;
    }
    uint8_t expected[TD_SHA256_BYTES];
    if (bytes == 0 || bytes > slot_size) {
        return TD_UPDATE_E_SIZE;
    }
    if (!td_sha256_from_hex(sha256_hex, expected)) {
        return TD_UPDATE_E_SHA_FORMAT;
    }
    td_update_state_init(u);
    u->phase = TD_UPDATE_RECEIVING;
    u->bytes = bytes;
    memcpy(u->expected, expected, sizeof expected);
    td_sha256_init(&u->sha);
    return TD_UPDATE_OK;
}

void td_update_state_started(td_update_state_t *u, uint32_t handle)
{
    if (u->phase == TD_UPDATE_RECEIVING) {
        u->handle = handle;
    }
}

td_update_reason_t td_update_state_chunk(td_update_state_t *u, const uint8_t *data, size_t len)
{
    if (u->phase != TD_UPDATE_RECEIVING) {
        return TD_UPDATE_E_STATE;
    }
    if (len > u->bytes - u->written) {
        td_update_state_fail(u, TD_UPDATE_E_RANGE);
        return TD_UPDATE_E_RANGE;
    }
    td_sha256_update(&u->sha, data, len);
    u->written += (uint32_t)len;
    return TD_UPDATE_OK;
}

td_update_reason_t td_update_state_finish(td_update_state_t *u)
{
    if (u->phase != TD_UPDATE_RECEIVING) {
        return TD_UPDATE_E_STATE;
    }
    if (u->written != u->bytes) {
        td_update_state_fail(u, TD_UPDATE_E_INCOMPLETE);
        return TD_UPDATE_E_INCOMPLETE;
    }
    uint8_t actual[TD_SHA256_BYTES];
    td_sha256_final(&u->sha, actual);
    if (memcmp(actual, u->expected, sizeof actual) != 0) {
        td_update_state_fail(u, TD_UPDATE_E_SHA);
        return TD_UPDATE_E_SHA;
    }
    u->phase = TD_UPDATE_VERIFYING;
    return TD_UPDATE_OK;
}

void td_update_state_rebooting(td_update_state_t *u)
{
    if (u->phase == TD_UPDATE_VERIFYING) {
        u->phase = TD_UPDATE_REBOOTING;
    }
}

void td_update_state_fail(td_update_state_t *u, td_update_reason_t reason)
{
    if (u->phase == TD_UPDATE_RECEIVING || u->phase == TD_UPDATE_VERIFYING) {
        u->phase = TD_UPDATE_FAILED;
        u->reason = reason;
        u->handle = 0;
    }
}

void td_update_state_abort(td_update_state_t *u)
{
    if (u->phase == TD_UPDATE_RECEIVING || u->phase == TD_UPDATE_VERIFYING) {
        u->phase = TD_UPDATE_IDLE;
        u->reason = TD_UPDATE_OK;
        u->handle = 0;
    }
}

const char *td_update_phase_name(td_update_phase_t phase)
{
    switch (phase) {
    case TD_UPDATE_IDLE:
        return "IDLE";
    case TD_UPDATE_RECEIVING:
        return "RECEIVING";
    case TD_UPDATE_VERIFYING:
        return "VERIFYING";
    case TD_UPDATE_REBOOTING:
        return "REBOOTING";
    case TD_UPDATE_FAILED:
        return "FAILED";
    }
    return "UNKNOWN";
}

const char *td_update_reason_name(td_update_reason_t reason)
{
    switch (reason) {
    case TD_UPDATE_OK:
        return "ok";
    case TD_UPDATE_E_BUSY:
        return "busy";
    case TD_UPDATE_E_STATE:
        return "state";
    case TD_UPDATE_E_SIZE:
        return "size";
    case TD_UPDATE_E_SHA_FORMAT:
        return "sha256-format";
    case TD_UPDATE_E_RANGE:
        return "range";
    case TD_UPDATE_E_INCOMPLETE:
        return "incomplete";
    case TD_UPDATE_E_SHA:
        return "sha256";
    case TD_UPDATE_E_FLASH:
        return "flash";
    case TD_UPDATE_E_UNCONFIRMED:
        return "unconfirmed";
    case TD_UPDATE_E_PROJECT:
        return "project";
    case TD_UPDATE_E_CUT:
        return "cut";
    }
    return "unknown";
}

const char *td_update_reason_text(td_update_reason_t reason)
{
    switch (reason) {
    case TD_UPDATE_OK:
        return "ok";
    case TD_UPDATE_E_BUSY:
        return "an update is already running";
    case TD_UPDATE_E_STATE:
        return "no update is receiving";
    case TD_UPDATE_E_SIZE:
        return "the image is empty or larger than the slot";
    case TD_UPDATE_E_SHA_FORMAT:
        return "sha256 must be 64 hex digits";
    case TD_UPDATE_E_RANGE:
        return "a chunk ran past the declared size";
    case TD_UPDATE_E_INCOMPLETE:
        return "the image ended before its declared size";
    case TD_UPDATE_E_SHA:
        return "sha256 mismatch";
    case TD_UPDATE_E_FLASH:
        return "the flash refused the image";
    case TD_UPDATE_E_UNCONFIRMED:
        return "the running image has not confirmed itself yet";
    case TD_UPDATE_E_PROJECT:
        return "the image belongs to another project";
    case TD_UPDATE_E_CUT:
        return "the transfer stopped";
    }
    return "unknown";
}

bool td_update_confirm_ready(const td_update_confirm_inputs_t *in)
{
    return in->first_frame && in->layout_loaded && in->uptime_ms >= TD_UPDATE_CONFIRM_UPTIME_MS;
}
