#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define TD_NET_H_LEN 32
#define TD_NET_H512_LEN 64
#define TD_NET_KEY_LEN 32
#define TD_NET_TAG_LEN 16
#define TD_NET_NONCE_LEN 12

typedef struct {
    const void *data;
    size_t len;
} td_net_part_t;

/* Each returns false when PSA reports any failure, with out left unspecified. */
bool td_net_sha256(const td_net_part_t *parts, size_t count, uint8_t out[TD_NET_H_LEN]);
bool td_net_sha512(const td_net_part_t *parts, size_t count, uint8_t out[TD_NET_H512_LEN]);
bool td_net_hmac(const uint8_t *key, size_t key_len, const td_net_part_t *parts, size_t count,
                 uint8_t out[TD_NET_H_LEN]);
/* HKDF-Expand-SHA256(prk, "touchdeck/net-v1/" || label, 32), one block. */
bool td_net_derive(const uint8_t prk[TD_NET_H_LEN], const char *label, uint8_t out[TD_NET_KEY_LEN]);
/* out receives len + 16 bytes, ciphertext then tag. */
bool td_net_seal(const uint8_t key[TD_NET_KEY_LEN], const uint8_t nonce[TD_NET_NONCE_LEN],
                 const uint8_t *aad, size_t aad_len, const uint8_t *in, size_t len, uint8_t *out);
/* in holds len bytes including the tag; out receives len - 16 bytes. */
bool td_net_open(const uint8_t key[TD_NET_KEY_LEN], const uint8_t nonce[TD_NET_NONCE_LEN],
                 const uint8_t *aad, size_t aad_len, const uint8_t *in, size_t len, uint8_t *out);
bool td_net_equal(const uint8_t *a, const uint8_t *b, size_t len);
void td_net_wipe(void *p, size_t len);
