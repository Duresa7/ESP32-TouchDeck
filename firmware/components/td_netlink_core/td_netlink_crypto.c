#include "td_netlink_crypto.h"
#include "psa/crypto.h"
#include <string.h>

static bool hash(psa_algorithm_t alg, const td_netlink_part_t *parts, size_t count, uint8_t *out, size_t out_len)
{
    psa_hash_operation_t op = PSA_HASH_OPERATION_INIT;
    if (psa_hash_setup(&op, alg) != PSA_SUCCESS) {
        return false;
    }
    for (size_t i = 0; i < count; i++) {
        if (psa_hash_update(&op, parts[i].data, parts[i].len) != PSA_SUCCESS) {
            psa_hash_abort(&op);
            return false;
        }
    }
    size_t len = 0;
    if (psa_hash_finish(&op, out, out_len, &len) != PSA_SUCCESS) {
        psa_hash_abort(&op);
        return false;
    }
    return len == out_len;
}

bool td_netlink_sha256(const td_netlink_part_t *parts, size_t count, uint8_t out[TD_NETLINK_H_LEN])
{
    return hash(PSA_ALG_SHA_256, parts, count, out, TD_NETLINK_H_LEN);
}

bool td_netlink_sha512(const td_netlink_part_t *parts, size_t count, uint8_t out[TD_NETLINK_H512_LEN])
{
    return hash(PSA_ALG_SHA_512, parts, count, out, TD_NETLINK_H512_LEN);
}

static bool import_key(psa_key_type_t type, psa_key_usage_t usage, psa_algorithm_t alg,
                       const uint8_t *key, size_t len, psa_key_id_t *id)
{
    psa_key_attributes_t attr = PSA_KEY_ATTRIBUTES_INIT;
    psa_set_key_type(&attr, type);
    psa_set_key_bits(&attr, len * 8);
    psa_set_key_usage_flags(&attr, usage);
    psa_set_key_algorithm(&attr, alg);
    psa_status_t status = psa_import_key(&attr, key, len, id);
    psa_reset_key_attributes(&attr);
    return status == PSA_SUCCESS;
}

bool td_netlink_hmac(const uint8_t *key, size_t key_len, const td_netlink_part_t *parts, size_t count,
                 uint8_t out[TD_NETLINK_H_LEN])
{
    const psa_algorithm_t alg = PSA_ALG_HMAC(PSA_ALG_SHA_256);
    psa_key_id_t id;
    if (!import_key(PSA_KEY_TYPE_HMAC, PSA_KEY_USAGE_SIGN_MESSAGE, alg, key, key_len, &id)) {
        return false;
    }
    psa_mac_operation_t op = PSA_MAC_OPERATION_INIT;
    bool ok = psa_mac_sign_setup(&op, id, alg) == PSA_SUCCESS;
    for (size_t i = 0; ok && i < count; i++) {
        ok = psa_mac_update(&op, parts[i].data, parts[i].len) == PSA_SUCCESS;
    }
    size_t len = 0;
    ok = ok && psa_mac_sign_finish(&op, out, TD_NETLINK_H_LEN, &len) == PSA_SUCCESS && len == TD_NETLINK_H_LEN;
    if (!ok) {
        psa_mac_abort(&op);
    }
    psa_destroy_key(id);
    return ok;
}

bool td_netlink_derive(const uint8_t prk[TD_NETLINK_H_LEN], const char *label, uint8_t out[TD_NETLINK_KEY_LEN])
{
    static const char prefix[] = "touchdeck/net-v1/";
    static const uint8_t block = 1;
    const td_netlink_part_t info[] = {
        {prefix, sizeof prefix - 1}, {label, strlen(label)}, {&block, 1},
    };
    return td_netlink_hmac(prk, TD_NETLINK_H_LEN, info, 3, out);
}

static bool aead(bool seal, const uint8_t key[TD_NETLINK_KEY_LEN], const uint8_t nonce[TD_NETLINK_NONCE_LEN],
                 const uint8_t *aad, size_t aad_len, const uint8_t *in, size_t len, uint8_t *out)
{
    psa_key_id_t id;
    if (!import_key(PSA_KEY_TYPE_AES, seal ? PSA_KEY_USAGE_ENCRYPT : PSA_KEY_USAGE_DECRYPT,
                    PSA_ALG_GCM, key, TD_NETLINK_KEY_LEN, &id)) {
        return false;
    }
    size_t out_len = 0;
    psa_status_t status;
    if (seal) {
        status = psa_aead_encrypt(id, PSA_ALG_GCM, nonce, TD_NETLINK_NONCE_LEN, aad, aad_len,
                                  in, len, out, len + TD_NETLINK_TAG_LEN, &out_len);
    } else {
        status = len < TD_NETLINK_TAG_LEN ? PSA_ERROR_INVALID_SIGNATURE
                 : psa_aead_decrypt(id, PSA_ALG_GCM, nonce, TD_NETLINK_NONCE_LEN, aad, aad_len,
                                    in, len, out, len - TD_NETLINK_TAG_LEN, &out_len);
    }
    psa_destroy_key(id);
    return status == PSA_SUCCESS && out_len == (seal ? len + TD_NETLINK_TAG_LEN : len - TD_NETLINK_TAG_LEN);
}

bool td_netlink_seal(const uint8_t key[TD_NETLINK_KEY_LEN], const uint8_t nonce[TD_NETLINK_NONCE_LEN],
                 const uint8_t *aad, size_t aad_len, const uint8_t *in, size_t len, uint8_t *out)
{
    return aead(true, key, nonce, aad, aad_len, in, len, out);
}

bool td_netlink_open(const uint8_t key[TD_NETLINK_KEY_LEN], const uint8_t nonce[TD_NETLINK_NONCE_LEN],
                 const uint8_t *aad, size_t aad_len, const uint8_t *in, size_t len, uint8_t *out)
{
    return aead(false, key, nonce, aad, aad_len, in, len, out);
}

bool td_netlink_equal(const uint8_t *a, const uint8_t *b, size_t len)
{
    uint8_t diff = 0;
    for (size_t i = 0; i < len; i++) {
        diff |= a[i] ^ b[i];
    }
    return diff == 0;
}

void td_netlink_wipe(void *p, size_t len)
{
    volatile uint8_t *bytes = p;
    while (len--) {
        *bytes++ = 0;
    }
}
