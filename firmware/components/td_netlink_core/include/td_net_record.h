#pragma once

#include "td_net_crypto.h"
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define TD_NET_PLAIN_MIN 8
#define TD_NET_PLAIN_MAX 1032
#define TD_NET_SEALED_MIN (TD_NET_PLAIN_MIN + TD_NET_TAG_LEN)
#define TD_NET_SEALED_MAX (TD_NET_PLAIN_MAX + TD_NET_TAG_LEN)
#define TD_NET_COUNTER_LIMIT (UINT64_C(1) << 32)

typedef enum {
    TD_NET_C2D = 0,
    TD_NET_D2C = 1,
} td_net_direction_t;

/* One direction of a session. A failure closes it for good and erases the key. */
typedef struct {
    uint8_t key[TD_NET_KEY_LEN];
    uint64_t counter;
    uint8_t direction;
    bool closed;
} td_net_dir_t;

void td_net_dir_init(td_net_dir_t *dir, td_net_direction_t direction, const uint8_t key[TD_NET_KEY_LEN]);
void td_net_dir_close(td_net_dir_t *dir);
/* body is one complete link-v1 body. wire receives u16be(len) || sealed, 2 + len + 16 bytes. */
bool td_net_record_seal(td_net_dir_t *dir, const uint8_t *body, size_t len, uint8_t *wire);
/* sealed is the record after its length prefix. body receives sealed_len - 16 bytes,
 * exposed only after the tag and the whole link-v1 body check out. */
bool td_net_record_open(td_net_dir_t *dir, const uint8_t *sealed, size_t sealed_len, uint8_t *body);
