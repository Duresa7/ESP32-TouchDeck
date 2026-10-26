#pragma once

#include "td_net_crypto.h"
#include "td_net_record.h"
#include "td_net_srp.h"
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define TD_NET_PORT 47820
#define TD_NET_VERSION 1
#define TD_NET_ITEM_MAX 512
#define TD_NET_WIRE_MAX (2 + TD_NET_SEALED_MAX)
#define TD_NET_APPS_MAX 4
#define TD_NET_NAME_MAX 64
#define TD_NET_DECK_ID_LEN 6
#define TD_NET_APP_ID_LEN 16
#define TD_NET_WINDOW_MS 120000u
#define TD_NET_HANDSHAKE_MS 10000u
#define TD_NET_ONLINE_MS 5000u

typedef enum {
    TD_NET_PAIR_REQUEST = 0x10,
    TD_NET_PAIR_OFFER = 0x11,
    TD_NET_PAIR_START = 0x12,
    TD_NET_PAIR_CHALLENGE = 0x13,
    TD_NET_PAIR_PROOF = 0x14,
    TD_NET_PAIR_ACCEPT = 0x15,
    TD_NET_PAIR_COMMIT = 0x16,
    TD_NET_PAIR_STORED = 0x17,
    TD_NET_SESSION_OPEN = 0x20,
    TD_NET_SESSION_CHALLENGE = 0x21,
    TD_NET_SESSION_PROOF = 0x22,
    TD_NET_SESSION_READY = 0x23,
    TD_NET_ERROR = 0x7f,
} td_net_kind_t;

typedef enum {
    TD_NET_REASON_NONE = 0,
    TD_NET_BUSY = 1,
    TD_NET_BAD_VERSION = 2,
    TD_NET_ARGS = 3,
    TD_NET_FULL = 4,
    TD_NET_AUTH = 5,
    TD_NET_EXPIRED = 6,
    TD_NET_CANCELLED = 7,
    TD_NET_STORAGE = 8,
} td_net_reason_t;

typedef struct {
    uint8_t app_id[TD_NET_APP_ID_LEN];
    uint8_t name_len;
    char name[TD_NET_NAME_MAX];
    uint8_t ltk[TD_NET_KEY_LEN];
} td_net_app_t;

typedef struct {
    uint8_t count;
    td_net_app_t apps[TD_NET_APPS_MAX];
} td_net_apps_t;

/* Persists the whole table atomically. Returns true once it is durable. */
typedef bool (*td_net_save_t)(void *ctx, const td_net_apps_t *apps);
typedef void (*td_net_rng_t)(void *ctx, uint8_t *out, size_t len);
typedef uint32_t (*td_net_clock_t)(void *ctx);

typedef struct {
    bool open;
    bool bound;
    uint8_t failures;
    char code[TD_NET_CODE_LEN];
    uint8_t window_id[16];
    uint32_t opened_ms;
    /* Why the last window ended, for the connection that still waits on it. */
    td_net_reason_t ended;
    uint32_t generation;
} td_net_window_t;

typedef struct {
    uint8_t deck_id[TD_NET_DECK_ID_LEN];
    td_net_apps_t apps;
    td_net_window_t window;
    td_net_rng_t rng;
    td_net_save_t save;
    td_net_clock_t clock;
    void *ctx;
} td_net_deck_t;

typedef enum {
    TD_NET_CONN_FIRST,
    TD_NET_CONN_PAIR_CODE,
    TD_NET_CONN_PAIR_PROOF,
    TD_NET_CONN_PAIR_COMMIT,
    TD_NET_CONN_SESSION_PROOF,
    TD_NET_CONN_READY,
    TD_NET_CONN_CLOSED,
} td_net_conn_state_t;

/* Pairing state that outlives one item. Erased when the attempt or window ends. */
typedef struct {
    uint8_t request[2 + 2 + TD_NET_APP_ID_LEN + 1 + TD_NET_NAME_MAX];
    uint16_t request_len;
    uint8_t offer[2 + 2 + 26];
    uint32_t window_generation;
    uint32_t attempt_ms;
    uint8_t transcript[TD_NET_H_LEN];
    uint8_t client_proof[TD_NET_H_LEN];
    uint8_t deck_key[TD_NET_KEY_LEN];
    uint8_t wrap_key[TD_NET_KEY_LEN];
    uint8_t commit_key[TD_NET_KEY_LEN];
    uint8_t ltk[TD_NET_KEY_LEN];
} td_net_pairing_t;

typedef struct {
    uint8_t transcript[TD_NET_H_LEN];
    uint8_t deck_proof[TD_NET_H_LEN];
    uint8_t client_key[TD_NET_KEY_LEN];
    uint8_t ready_key[TD_NET_KEY_LEN];
    uint8_t c2d[TD_NET_KEY_LEN];
    uint8_t d2c[TD_NET_KEY_LEN];
} td_net_session_keys_t;

typedef struct {
    td_net_conn_state_t state;
    uint32_t accepted_ms;
    union {
        td_net_pairing_t pair;
        td_net_session_keys_t keys;
    } hs;
    uint8_t app_id[TD_NET_APP_ID_LEN];
    uint8_t name_len;
    char name[TD_NET_NAME_MAX];
    uint32_t ready_ms;
    uint32_t last_rx_ms;
    bool hello_seen;
    td_net_dir_t rx;
    td_net_dir_t tx;
} td_net_conn_t;

/* What the transport does with one step's output. */
typedef enum {
    TD_NET_NOTHING,
    TD_NET_SEND,
    /* Send out, then shut the write side and drain for at most 100 ms. */
    TD_NET_SEND_CLOSE,
    /* Close at once with nothing sent: a broken session or a silent handshake end. */
    TD_NET_CLOSE,
} td_net_action_t;

typedef struct {
    uint8_t item[2 + TD_NET_ITEM_MAX];
    uint16_t len;
} td_net_out_t;

void td_net_deck_init(td_net_deck_t *deck, const uint8_t deck_id[TD_NET_DECK_ID_LEN], const td_net_apps_t *stored,
                      td_net_rng_t rng, td_net_save_t save, td_net_clock_t clock, void *ctx);
/* `net pair`: an unbound window that the next pair.request binds. */
td_net_reason_t td_net_window_open(td_net_deck_t *deck);
/* The pairing screen's Cancel. A bound connection reports `cancelled` on its next tick. */
void td_net_window_cancel(td_net_deck_t *deck);
/* Ends an unbound window at expiry. Returns true while the code must stay on screen. */
bool td_net_deck_tick(td_net_deck_t *deck);
/* Cancels any window and erases every pairing. False when the store failed. */
bool td_net_forget(td_net_deck_t *deck);

void td_net_conn_init(td_net_conn_t *conn, td_net_deck_t *deck);
/* Bounds for the next item's u16be length, checked before the body is read. */
void td_net_conn_limits(const td_net_conn_t *conn, uint16_t *min, uint16_t *max);
/* One handshake item body (kind, version, fields) after its length prefix. */
td_net_action_t td_net_conn_item(td_net_conn_t *conn, td_net_deck_t *deck, const uint8_t *body, size_t len,
                                 td_net_out_t *out);
/* Deadlines, cancel, expiry, and session liveness. */
td_net_action_t td_net_conn_tick(td_net_conn_t *conn, td_net_deck_t *deck, td_net_out_t *out);
/* A session record after its length prefix. body receives len - 16 bytes. */
bool td_net_conn_record(td_net_conn_t *conn, td_net_deck_t *deck, const uint8_t *sealed, size_t len, uint8_t *body);
bool td_net_conn_send(td_net_conn_t *conn, const uint8_t *body, size_t len, uint8_t *wire);
/* The dispatcher saw `hello` on this session, so liveness now follows the last valid record. */
void td_net_conn_hello(td_net_conn_t *conn);
/* Socket gone: erase keys and destroy a window this connection owned. */
void td_net_conn_close(td_net_conn_t *conn, td_net_deck_t *deck);

/* The `busy` refusal for a second connection, a complete item. */
extern const uint8_t td_net_busy_item[5];
