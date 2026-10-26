#include "td_net_record.h"
#include "td_frame.h"
#include <string.h>

void td_net_dir_init(td_net_dir_t *dir, td_net_direction_t direction, const uint8_t key[TD_NET_KEY_LEN])
{
    memcpy(dir->key, key, TD_NET_KEY_LEN);
    dir->counter = 0;
    dir->direction = (uint8_t)direction;
    dir->closed = false;
}

void td_net_dir_close(td_net_dir_t *dir)
{
    td_net_wipe(dir->key, sizeof dir->key);
    dir->closed = true;
}

static void parameters(const td_net_dir_t *dir, size_t sealed_len, uint8_t nonce[TD_NET_NONCE_LEN], uint8_t aad[15])
{
    memset(nonce, 0, TD_NET_NONCE_LEN);
    memcpy(aad, "TDN1", 4);
    aad[4] = dir->direction;
    for (int i = 0; i < 8; i++) {
        uint8_t byte = (uint8_t)(dir->counter >> (56 - 8 * i));
        nonce[4 + i] = byte;
        aad[5 + i] = byte;
    }
    aad[13] = (uint8_t)(sealed_len >> 8);
    aad[14] = (uint8_t)sealed_len;
}

static bool valid_body(const uint8_t *body, size_t len)
{
    td_frame_t frame;
    return td_frame_decode(body, len, &frame) == TD_FRAME_OK;
}

bool td_net_record_seal(td_net_dir_t *dir, const uint8_t *body, size_t len, uint8_t *wire)
{
    if (dir->closed) {
        return false;
    }
    if (dir->counter >= TD_NET_COUNTER_LIMIT || len < TD_NET_PLAIN_MIN || len > TD_NET_PLAIN_MAX
            || !valid_body(body, len)) {
        td_net_dir_close(dir);
        return false;
    }
    size_t sealed_len = len + TD_NET_TAG_LEN;
    uint8_t nonce[TD_NET_NONCE_LEN], aad[15];
    parameters(dir, sealed_len, nonce, aad);
    wire[0] = (uint8_t)(sealed_len >> 8);
    wire[1] = (uint8_t)sealed_len;
    if (!td_net_seal(dir->key, nonce, aad, sizeof aad, body, len, wire + 2)) {
        td_net_dir_close(dir);
        return false;
    }
    dir->counter++;
    return true;
}

bool td_net_record_open(td_net_dir_t *dir, const uint8_t *sealed, size_t sealed_len, uint8_t *body)
{
    if (dir->closed) {
        return false;
    }
    size_t len = sealed_len - TD_NET_TAG_LEN;
    uint8_t nonce[TD_NET_NONCE_LEN], aad[15];
    if (dir->counter >= TD_NET_COUNTER_LIMIT || sealed_len < TD_NET_SEALED_MIN || sealed_len > TD_NET_SEALED_MAX) {
        td_net_dir_close(dir);
        return false;
    }
    parameters(dir, sealed_len, nonce, aad);
    if (!td_net_open(dir->key, nonce, aad, sizeof aad, sealed, sealed_len, body) || !valid_body(body, len)) {
        td_net_wipe(body, len);
        td_net_dir_close(dir);
        return false;
    }
    dir->counter++;
    return true;
}
