#include "td_netlink.h"
#include <string.h>

/* The largest multiple of 10^6 below 2^32, so code draws stay uniform. */
#define CODE_DRAW_LIMIT 4294000000u

const uint8_t td_netlink_busy_item[5] = {0x00, 0x03, TD_NETLINK_ERROR, TD_NETLINK_VERSION, TD_NETLINK_BUSY};

static uint32_t now_ms(const td_netlink_deck_t *deck)
{
    return deck->clock(deck->ctx);
}

static uint32_t since(const td_netlink_deck_t *deck, uint32_t start)
{
    return now_ms(deck) - start;
}

static void put_item(td_netlink_out_t *out, uint8_t kind, const td_netlink_part_t *parts, size_t count)
{
    size_t at = 4;
    for (size_t i = 0; i < count; i++) {
        memcpy(out->item + at, parts[i].data, parts[i].len);
        at += parts[i].len;
    }
    out->item[0] = (uint8_t)((at - 2) >> 8);
    out->item[1] = (uint8_t)(at - 2);
    out->item[2] = kind;
    out->item[3] = TD_NETLINK_VERSION;
    out->len = (uint16_t)at;
}

static void window_end(td_netlink_deck_t *deck, td_netlink_reason_t why)
{
    td_netlink_window_t *w = &deck->window;
    if (!w->open) {
        return;
    }
    td_netlink_wipe(w->code, sizeof w->code);
    td_netlink_wipe(w->window_id, sizeof w->window_id);
    w->open = false;
    w->bound = false;
    w->failures = 0;
    w->ended = why;
    w->generation++;
}

static void window_expire(td_netlink_deck_t *deck)
{
    if (deck->window.open && since(deck, deck->window.opened_ms) >= TD_NETLINK_WINDOW_MS) {
        window_end(deck, TD_NETLINK_EXPIRED);
    }
}

static int find_app(const td_netlink_apps_t *apps, const uint8_t app_id[TD_NETLINK_APP_ID_LEN])
{
    for (int i = 0; i < apps->count; i++) {
        if (memcmp(apps->apps[i].app_id, app_id, TD_NETLINK_APP_ID_LEN) == 0) {
            return i;
        }
    }
    return -1;
}

void td_netlink_deck_init(td_netlink_deck_t *deck, const uint8_t deck_id[TD_NETLINK_DECK_ID_LEN], const td_netlink_apps_t *stored,
                      td_netlink_rng_t rng, td_netlink_save_t save, td_netlink_clock_t clock, void *ctx)
{
    memset(deck, 0, sizeof *deck);
    memcpy(deck->deck_id, deck_id, TD_NETLINK_DECK_ID_LEN);
    if (stored && stored->count <= TD_NETLINK_APPS_MAX) {
        deck->apps = *stored;
    }
    deck->rng = rng;
    deck->save = save;
    deck->clock = clock;
    deck->ctx = ctx;
}

td_netlink_reason_t td_netlink_window_open(td_netlink_deck_t *deck)
{
    td_netlink_window_t *w = &deck->window;
    window_expire(deck);
    if (w->open) {
        return TD_NETLINK_BUSY;
    }
    if (deck->apps.count >= TD_NETLINK_APPS_MAX) {
        return TD_NETLINK_FULL;
    }
    uint32_t r;
    do {
        uint8_t raw[4];
        deck->rng(deck->ctx, raw, sizeof raw);
        r = (uint32_t)raw[0] << 24 | (uint32_t)raw[1] << 16 | (uint32_t)raw[2] << 8 | raw[3];
        td_netlink_wipe(raw, sizeof raw);
    } while (r >= CODE_DRAW_LIMIT);
    r %= 1000000u;
    for (int i = TD_NETLINK_CODE_LEN - 1; i >= 0; i--) {
        w->code[i] = (char)('0' + r % 10);
        r /= 10;
    }
    deck->rng(deck->ctx, w->window_id, sizeof w->window_id);
    w->opened_ms = now_ms(deck);
    w->open = true;
    w->bound = false;
    w->failures = 0;
    w->ended = TD_NETLINK_REASON_NONE;
    w->generation++;
    return TD_NETLINK_REASON_NONE;
}

void td_netlink_window_cancel(td_netlink_deck_t *deck)
{
    window_end(deck, TD_NETLINK_CANCELLED);
}

bool td_netlink_deck_tick(td_netlink_deck_t *deck)
{
    window_expire(deck);
    return deck->window.open;
}

bool td_netlink_forget(td_netlink_deck_t *deck)
{
    window_end(deck, TD_NETLINK_CANCELLED);
    const td_netlink_apps_t empty = {0};
    bool ok = deck->save(deck->ctx, &empty);
    td_netlink_wipe(&deck->apps, sizeof deck->apps);
    return ok;
}

void td_netlink_conn_init(td_netlink_conn_t *conn, td_netlink_deck_t *deck)
{
    memset(conn, 0, sizeof *conn);
    conn->state = TD_NETLINK_CONN_FIRST;
    conn->accepted_ms = now_ms(deck);
}

void td_netlink_conn_limits(const td_netlink_conn_t *conn, uint16_t *min, uint16_t *max)
{
    if (conn->state == TD_NETLINK_CONN_READY) {
        *min = TD_NETLINK_SEALED_MIN;
        *max = TD_NETLINK_SEALED_MAX;
    } else {
        *min = 2;
        *max = TD_NETLINK_ITEM_MAX;
    }
}

static bool owns_window(const td_netlink_conn_t *conn, const td_netlink_deck_t *deck)
{
    bool pairing = conn->state >= TD_NETLINK_CONN_PAIR_CODE && conn->state <= TD_NETLINK_CONN_PAIR_COMMIT;
    return pairing && deck->window.open && deck->window.generation == conn->hs.pair.window_generation;
}

static void conn_end(td_netlink_conn_t *conn, td_netlink_deck_t *deck)
{
    if (owns_window(conn, deck)) {
        window_end(deck, TD_NETLINK_REASON_NONE);
    }
    td_netlink_dir_close(&conn->rx);
    td_netlink_dir_close(&conn->tx);
    td_netlink_wipe(&conn->hs, sizeof conn->hs);
    conn->state = TD_NETLINK_CONN_CLOSED;
}

static td_netlink_action_t fail(td_netlink_conn_t *conn, td_netlink_deck_t *deck, td_netlink_out_t *out, td_netlink_reason_t reason)
{
    const uint8_t byte = (uint8_t)reason;
    const td_netlink_part_t fields[] = {{&byte, 1}};
    put_item(out, TD_NETLINK_ERROR, fields, 1);
    conn_end(conn, deck);
    return TD_NETLINK_SEND_CLOSE;
}

static td_netlink_action_t deadline(td_netlink_conn_t *conn, td_netlink_deck_t *deck, td_netlink_out_t *out)
{
    switch (conn->state) {
    case TD_NETLINK_CONN_FIRST:
    case TD_NETLINK_CONN_SESSION_PROOF:
        if (since(deck, conn->accepted_ms) >= TD_NETLINK_HANDSHAKE_MS) {
            return fail(conn, deck, out, TD_NETLINK_EXPIRED);
        }
        return TD_NETLINK_NOTHING;
    case TD_NETLINK_CONN_PAIR_CODE:
    case TD_NETLINK_CONN_PAIR_PROOF:
    case TD_NETLINK_CONN_PAIR_COMMIT:
        window_expire(deck);
        if (!owns_window(conn, deck)) {
            td_netlink_reason_t why = deck->window.ended == TD_NETLINK_CANCELLED ? TD_NETLINK_CANCELLED : TD_NETLINK_EXPIRED;
            return fail(conn, deck, out, why);
        }
        if (conn->state != TD_NETLINK_CONN_PAIR_CODE && since(deck, conn->hs.pair.attempt_ms) >= TD_NETLINK_HANDSHAKE_MS) {
            return fail(conn, deck, out, TD_NETLINK_EXPIRED);
        }
        return TD_NETLINK_NOTHING;
    case TD_NETLINK_CONN_READY: {
        uint32_t last = conn->hello_seen ? conn->last_rx_ms : conn->ready_ms;
        if (since(deck, last) >= TD_NETLINK_ONLINE_MS) {
            conn_end(conn, deck);
            return TD_NETLINK_CLOSE;
        }
        return TD_NETLINK_NOTHING;
    }
    case TD_NETLINK_CONN_CLOSED:
        break;
    }
    return TD_NETLINK_NOTHING;
}

static bool valid_name(const uint8_t *s, size_t n)
{
    size_t i = 0;
    while (i < n) {
        uint32_t c = s[i];
        size_t extra;
        uint32_t min;
        if (c < 0x80) {
            extra = 0;
            min = 0;
        } else if ((c & 0xe0) == 0xc0) {
            extra = 1;
            min = 0x80;
            c &= 0x1f;
        } else if ((c & 0xf0) == 0xe0) {
            extra = 2;
            min = 0x800;
            c &= 0x0f;
        } else if ((c & 0xf8) == 0xf0) {
            extra = 3;
            min = 0x10000;
            c &= 0x07;
        } else {
            return false;
        }
        if (n - i <= extra) {
            return false;
        }
        for (size_t k = 1; k <= extra; k++) {
            if ((s[i + k] & 0xc0) != 0x80) {
                return false;
            }
            c = c << 6 | (s[i + k] & 0x3f);
        }
        bool control = c < 0x20 || (c >= 0x7f && c <= 0x9f);
        if (c < min || c > 0x10ffff || (c >= 0xd800 && c <= 0xdfff) || control) {
            return false;
        }
        i += extra + 1;
    }
    return true;
}

static void hex(char *out, const uint8_t *in, size_t len)
{
    static const char digits[] = "0123456789abcdef";
    for (size_t i = 0; i < len; i++) {
        out[2 * i] = digits[in[i] >> 4];
        out[2 * i + 1] = digits[in[i] & 15];
    }
}

static td_netlink_action_t pair_request(td_netlink_conn_t *conn, td_netlink_deck_t *deck, const uint8_t *body, size_t len,
                                    td_netlink_out_t *out)
{
    const uint8_t *fields = body + 2;
    size_t n = len - 2;
    if (n < TD_NETLINK_APP_ID_LEN + 2 || n > TD_NETLINK_APP_ID_LEN + 1 + TD_NETLINK_NAME_MAX
            || fields[TD_NETLINK_APP_ID_LEN] != n - TD_NETLINK_APP_ID_LEN - 1
            || !valid_name(fields + TD_NETLINK_APP_ID_LEN + 1, n - TD_NETLINK_APP_ID_LEN - 1)) {
        return fail(conn, deck, out, TD_NETLINK_ARGS);
    }
    if (deck->apps.count >= TD_NETLINK_APPS_MAX || find_app(&deck->apps, fields) >= 0) {
        return fail(conn, deck, out, TD_NETLINK_FULL);
    }
    td_netlink_window_t *w = &deck->window;
    window_expire(deck);
    if (w->open && w->bound) {
        return fail(conn, deck, out, TD_NETLINK_BUSY);
    }
    if (!w->open) {
        td_netlink_reason_t refused = td_netlink_window_open(deck);
        if (refused != TD_NETLINK_REASON_NONE) {
            return fail(conn, deck, out, refused);
        }
    }
    w->bound = true;
    td_netlink_pairing_t *p = &conn->hs.pair;
    p->window_generation = w->generation;
    memcpy(conn->app_id, fields, TD_NETLINK_APP_ID_LEN);
    conn->name_len = fields[TD_NETLINK_APP_ID_LEN];
    memcpy(conn->name, fields + TD_NETLINK_APP_ID_LEN + 1, conn->name_len);
    p->request[0] = (uint8_t)(len >> 8);
    p->request[1] = (uint8_t)len;
    memcpy(p->request + 2, body, len);
    p->request_len = (uint16_t)(len + 2);

    uint32_t remaining = TD_NETLINK_WINDOW_MS - since(deck, w->opened_ms);
    const uint8_t left[4] = {
        (uint8_t)(remaining >> 24), (uint8_t)(remaining >> 16), (uint8_t)(remaining >> 8), (uint8_t)remaining,
    };
    const td_netlink_part_t offer[] = {
        {deck->deck_id, TD_NETLINK_DECK_ID_LEN}, {w->window_id, sizeof w->window_id}, {left, 4},
    };
    put_item(out, TD_NETLINK_PAIR_OFFER, offer, 3);
    memcpy(p->offer, out->item, out->len);
    conn->state = TD_NETLINK_CONN_PAIR_CODE;
    return TD_NETLINK_SEND;
}

static td_netlink_action_t pair_start(td_netlink_conn_t *conn, td_netlink_deck_t *deck, const uint8_t *body, size_t len,
                                  td_netlink_out_t *out)
{
    if (len != 2 + TD_NETLINK_SRP_LEN + 32) {
        return fail(conn, deck, out, TD_NETLINK_ARGS);
    }
    td_netlink_pairing_t *p = &conn->hs.pair;
    p->attempt_ms = now_ms(deck);
    uint8_t salt[TD_NETLINK_SALT_LEN], b[TD_NETLINK_EXP_LEN], nonce[32];
    deck->rng(deck->ctx, salt, sizeof salt);
    deck->rng(deck->ctx, b, sizeof b);
    deck->rng(deck->ctx, nonce, sizeof nonce);

    char identity[10 + 2 * TD_NETLINK_DECK_ID_LEN + 1 + 2 * TD_NETLINK_APP_ID_LEN];
    memcpy(identity, "touchdeck:", 10);
    hex(identity + 10, deck->deck_id, TD_NETLINK_DECK_ID_LEN);
    identity[10 + 2 * TD_NETLINK_DECK_ID_LEN] = ':';
    hex(identity + 11 + 2 * TD_NETLINK_DECK_ID_LEN, conn->app_id, TD_NETLINK_APP_ID_LEN);

    uint8_t B[TD_NETLINK_SRP_LEN], key[TD_NETLINK_H512_LEN];
    const td_netlink_srp_in_t in = {
        .identity = (const uint8_t *)identity, .identity_len = sizeof identity,
        .code = deck->window.code, .salt = salt, .b = b, .A = body + 2,
    };
    td_netlink_srp_result_t srp = td_netlink_srp_server(&in, B, key);
    td_netlink_wipe(b, sizeof b);
    if (srp != TD_NETLINK_SRP_OK) {
        return fail(conn, deck, out, TD_NETLINK_ARGS);
    }
    const td_netlink_part_t challenge[] = {{salt, sizeof salt}, {B, sizeof B}, {nonce, sizeof nonce}};
    put_item(out, TD_NETLINK_PAIR_CHALLENGE, challenge, 3);

    static const char label[] = "touchdeck/net-v1/pair";
    const uint8_t start_len[2] = {(uint8_t)(len >> 8), (uint8_t)len};
    const td_netlink_part_t transcript[] = {
        {label, sizeof label}, {p->request, p->request_len}, {p->offer, sizeof p->offer},
        {start_len, 2}, {body, len}, {out->item, out->len},
    };
    uint8_t prk[TD_NETLINK_H_LEN], client_key[TD_NETLINK_KEY_LEN];
    const td_netlink_part_t t[] = {{p->transcript, TD_NETLINK_H_LEN}};
    bool ok = td_netlink_sha256(transcript, 6, p->transcript)
              && td_netlink_hmac(p->transcript, TD_NETLINK_H_LEN, (const td_netlink_part_t[]) {{key, sizeof key}}, 1, prk)
              && td_netlink_derive(prk, "pair/client", client_key) && td_netlink_derive(prk, "pair/deck", p->deck_key)
              && td_netlink_derive(prk, "pair/wrap", p->wrap_key) && td_netlink_derive(prk, "pair/commit", p->commit_key)
              && td_netlink_hmac(client_key, TD_NETLINK_KEY_LEN, t, 1, p->client_proof);
    td_netlink_wipe(key, sizeof key);
    td_netlink_wipe(prk, sizeof prk);
    td_netlink_wipe(client_key, sizeof client_key);
    if (!ok) {
        return fail(conn, deck, out, TD_NETLINK_ARGS);
    }
    conn->state = TD_NETLINK_CONN_PAIR_PROOF;
    td_netlink_action_t late = deadline(conn, deck, out);
    return late == TD_NETLINK_NOTHING ? TD_NETLINK_SEND : late;
}

static void attempt_wipe(td_netlink_pairing_t *p)
{
    td_netlink_wipe(p->transcript, sizeof p->transcript);
    td_netlink_wipe(p->client_proof, sizeof p->client_proof);
    td_netlink_wipe(p->deck_key, sizeof p->deck_key);
    td_netlink_wipe(p->wrap_key, sizeof p->wrap_key);
    td_netlink_wipe(p->commit_key, sizeof p->commit_key);
    td_netlink_wipe(p->ltk, sizeof p->ltk);
}

static td_netlink_action_t pair_proof(td_netlink_conn_t *conn, td_netlink_deck_t *deck, const uint8_t *body, size_t len,
                                  td_netlink_out_t *out)
{
    if (len != 2 + TD_NETLINK_H_LEN) {
        return fail(conn, deck, out, TD_NETLINK_ARGS);
    }
    td_netlink_pairing_t *p = &conn->hs.pair;
    if (!td_netlink_equal(body + 2, p->client_proof, TD_NETLINK_H_LEN)) {
        attempt_wipe(p);
        if (++deck->window.failures >= 3) {
            return fail(conn, deck, out, TD_NETLINK_AUTH);
        }
        const uint8_t reason = TD_NETLINK_AUTH;
        put_item(out, TD_NETLINK_ERROR, (const td_netlink_part_t[]) {{&reason, 1}}, 1);
        conn->state = TD_NETLINK_CONN_PAIR_CODE;
        return TD_NETLINK_SEND;
    }
    deck->rng(deck->ctx, p->ltk, sizeof p->ltk);
    uint8_t deck_proof[TD_NETLINK_H_LEN], wrapped[TD_NETLINK_KEY_LEN + TD_NETLINK_TAG_LEN], aad[3 * TD_NETLINK_H_LEN];
    const td_netlink_part_t signed_part[] = {{p->transcript, TD_NETLINK_H_LEN}, {p->client_proof, TD_NETLINK_H_LEN}};
    const uint8_t zero_nonce[TD_NETLINK_NONCE_LEN] = {0};
    bool ok = td_netlink_hmac(p->deck_key, TD_NETLINK_KEY_LEN, signed_part, 2, deck_proof);
    memcpy(aad, p->transcript, TD_NETLINK_H_LEN);
    memcpy(aad + TD_NETLINK_H_LEN, p->client_proof, TD_NETLINK_H_LEN);
    memcpy(aad + 2 * TD_NETLINK_H_LEN, deck_proof, TD_NETLINK_H_LEN);
    ok = ok && td_netlink_seal(p->wrap_key, zero_nonce, aad, sizeof aad, p->ltk, TD_NETLINK_KEY_LEN, wrapped);
    td_netlink_wipe(p->deck_key, sizeof p->deck_key);
    td_netlink_wipe(p->wrap_key, sizeof p->wrap_key);
    if (!ok) {
        return fail(conn, deck, out, TD_NETLINK_ARGS);
    }
    const td_netlink_part_t accept[] = {{deck_proof, sizeof deck_proof}, {wrapped, sizeof wrapped}};
    put_item(out, TD_NETLINK_PAIR_ACCEPT, accept, 2);
    conn->state = TD_NETLINK_CONN_PAIR_COMMIT;
    return TD_NETLINK_SEND;
}

static bool commit_mac(const td_netlink_pairing_t *p, const char *word, uint8_t out[TD_NETLINK_H_LEN])
{
    uint8_t digest[TD_NETLINK_H_LEN];
    bool ok = td_netlink_sha256((const td_netlink_part_t[]) {{p->ltk, TD_NETLINK_KEY_LEN}}, 1, digest);
    const td_netlink_part_t parts[] = {{word, 6}, {p->transcript, TD_NETLINK_H_LEN}, {digest, sizeof digest}};
    ok = ok && td_netlink_hmac(p->commit_key, TD_NETLINK_KEY_LEN, parts, 3, out);
    td_netlink_wipe(digest, sizeof digest);
    return ok;
}

static td_netlink_action_t pair_commit(td_netlink_conn_t *conn, td_netlink_deck_t *deck, const uint8_t *body, size_t len,
                                   td_netlink_out_t *out)
{
    if (len != 2 + TD_NETLINK_H_LEN) {
        return fail(conn, deck, out, TD_NETLINK_ARGS);
    }
    td_netlink_pairing_t *p = &conn->hs.pair;
    uint8_t expected[TD_NETLINK_H_LEN];
    if (!commit_mac(p, "commit", expected) || !td_netlink_equal(body + 2, expected, TD_NETLINK_H_LEN)) {
        return fail(conn, deck, out, TD_NETLINK_AUTH);
    }
    td_netlink_action_t late = deadline(conn, deck, out);
    if (late != TD_NETLINK_NOTHING) {
        return late;
    }
    if (deck->apps.count >= TD_NETLINK_APPS_MAX || find_app(&deck->apps, conn->app_id) >= 0) {
        return fail(conn, deck, out, TD_NETLINK_FULL);
    }
    td_netlink_apps_t next = deck->apps;
    td_netlink_app_t *app = &next.apps[next.count++];
    memset(app, 0, sizeof *app);
    memcpy(app->app_id, conn->app_id, TD_NETLINK_APP_ID_LEN);
    app->name_len = conn->name_len;
    memcpy(app->name, conn->name, conn->name_len);
    memcpy(app->ltk, p->ltk, TD_NETLINK_KEY_LEN);
    bool stored = deck->save(deck->ctx, &next);
    if (stored) {
        deck->apps = next;
    }
    td_netlink_wipe(&next, sizeof next);
    uint8_t proof[TD_NETLINK_H_LEN];
    if (!stored || !commit_mac(p, "stored", proof)) {
        return fail(conn, deck, out, TD_NETLINK_STORAGE);
    }
    put_item(out, TD_NETLINK_PAIR_STORED, (const td_netlink_part_t[]) {{proof, sizeof proof}}, 1);
    conn_end(conn, deck);
    return TD_NETLINK_SEND_CLOSE;
}

static td_netlink_action_t session_open(td_netlink_conn_t *conn, td_netlink_deck_t *deck, const uint8_t *body, size_t len,
                                    td_netlink_out_t *out)
{
    if (len != 2 + TD_NETLINK_DECK_ID_LEN + TD_NETLINK_APP_ID_LEN + 32) {
        return fail(conn, deck, out, TD_NETLINK_ARGS);
    }
    const uint8_t *fields = body + 2;
    int index = find_app(&deck->apps, fields + TD_NETLINK_DECK_ID_LEN);
    if (memcmp(fields, deck->deck_id, TD_NETLINK_DECK_ID_LEN) != 0 || index < 0) {
        return fail(conn, deck, out, TD_NETLINK_AUTH);
    }
    const td_netlink_app_t *app = &deck->apps.apps[index];
    memcpy(conn->app_id, app->app_id, TD_NETLINK_APP_ID_LEN);
    conn->name_len = app->name_len;
    memcpy(conn->name, app->name, app->name_len);

    td_netlink_session_keys_t *k = &conn->hs.keys;
    uint8_t nonce[32], prk[TD_NETLINK_H_LEN], deck_key[TD_NETLINK_KEY_LEN];
    deck->rng(deck->ctx, nonce, sizeof nonce);
    static const char label[] = "touchdeck/net-v1/session";
    const td_netlink_part_t transcript[] = {{label, sizeof label}, {fields, len - 2}, {nonce, sizeof nonce}};
    const td_netlink_part_t t[] = {{k->transcript, TD_NETLINK_H_LEN}};
    bool ok = td_netlink_sha256(transcript, 3, k->transcript)
              && td_netlink_hmac(k->transcript, TD_NETLINK_H_LEN, (const td_netlink_part_t[]) {{app->ltk, TD_NETLINK_KEY_LEN}}, 1, prk)
              && td_netlink_derive(prk, "session/deck", deck_key) && td_netlink_derive(prk, "session/client", k->client_key)
              && td_netlink_derive(prk, "session/ready", k->ready_key) && td_netlink_derive(prk, "session/c2d", k->c2d)
              && td_netlink_derive(prk, "session/d2c", k->d2c)
              && td_netlink_hmac(deck_key, TD_NETLINK_KEY_LEN, t, 1, k->deck_proof);
    td_netlink_wipe(prk, sizeof prk);
    td_netlink_wipe(deck_key, sizeof deck_key);
    if (!ok) {
        return fail(conn, deck, out, TD_NETLINK_AUTH);
    }
    const td_netlink_part_t challenge[] = {{nonce, sizeof nonce}, {k->deck_proof, TD_NETLINK_H_LEN}};
    put_item(out, TD_NETLINK_SESSION_CHALLENGE, challenge, 2);
    conn->state = TD_NETLINK_CONN_SESSION_PROOF;
    return TD_NETLINK_SEND;
}

static td_netlink_action_t session_proof(td_netlink_conn_t *conn, td_netlink_deck_t *deck, const uint8_t *body, size_t len,
                                     td_netlink_out_t *out)
{
    if (len != 2 + TD_NETLINK_H_LEN) {
        return fail(conn, deck, out, TD_NETLINK_ARGS);
    }
    td_netlink_session_keys_t *k = &conn->hs.keys;
    uint8_t expected[TD_NETLINK_H_LEN], ready[TD_NETLINK_H_LEN];
    const td_netlink_part_t signed_part[] = {{k->transcript, TD_NETLINK_H_LEN}, {k->deck_proof, TD_NETLINK_H_LEN}};
    if (!td_netlink_hmac(k->client_key, TD_NETLINK_KEY_LEN, signed_part, 2, expected)
            || !td_netlink_equal(body + 2, expected, TD_NETLINK_H_LEN)) {
        return fail(conn, deck, out, TD_NETLINK_AUTH);
    }
    const td_netlink_part_t ready_part[] = {{k->transcript, TD_NETLINK_H_LEN}, {body + 2, TD_NETLINK_H_LEN}};
    if (!td_netlink_hmac(k->ready_key, TD_NETLINK_KEY_LEN, ready_part, 2, ready)) {
        return fail(conn, deck, out, TD_NETLINK_AUTH);
    }
    put_item(out, TD_NETLINK_SESSION_READY, (const td_netlink_part_t[]) {{ready, sizeof ready}}, 1);
    td_netlink_dir_init(&conn->rx, TD_NETLINK_C2D, k->c2d);
    td_netlink_dir_init(&conn->tx, TD_NETLINK_D2C, k->d2c);
    td_netlink_wipe(&conn->hs, sizeof conn->hs);
    conn->ready_ms = now_ms(deck);
    conn->last_rx_ms = conn->ready_ms;
    conn->hello_seen = false;
    conn->state = TD_NETLINK_CONN_READY;
    return TD_NETLINK_SEND;
}

td_netlink_action_t td_netlink_conn_item(td_netlink_conn_t *conn, td_netlink_deck_t *deck, const uint8_t *body, size_t len,
                                 td_netlink_out_t *out)
{
    out->len = 0;
    if (conn->state == TD_NETLINK_CONN_READY || conn->state == TD_NETLINK_CONN_CLOSED) {
        conn_end(conn, deck);
        return TD_NETLINK_CLOSE;
    }
    td_netlink_action_t late = deadline(conn, deck, out);
    if (late != TD_NETLINK_NOTHING) {
        return late;
    }
    if (len < 2 || len > TD_NETLINK_ITEM_MAX) {
        return fail(conn, deck, out, TD_NETLINK_ARGS);
    }
    if (body[1] != TD_NETLINK_VERSION) {
        return fail(conn, deck, out, TD_NETLINK_BAD_VERSION);
    }
    switch (conn->state) {
    case TD_NETLINK_CONN_FIRST:
        if (body[0] == TD_NETLINK_PAIR_REQUEST) {
            return pair_request(conn, deck, body, len, out);
        }
        if (body[0] == TD_NETLINK_SESSION_OPEN) {
            return session_open(conn, deck, body, len, out);
        }
        break;
    case TD_NETLINK_CONN_PAIR_CODE:
        if (body[0] == TD_NETLINK_PAIR_START) {
            return pair_start(conn, deck, body, len, out);
        }
        break;
    case TD_NETLINK_CONN_PAIR_PROOF:
        if (body[0] == TD_NETLINK_PAIR_PROOF) {
            return pair_proof(conn, deck, body, len, out);
        }
        break;
    case TD_NETLINK_CONN_PAIR_COMMIT:
        if (body[0] == TD_NETLINK_PAIR_COMMIT) {
            return pair_commit(conn, deck, body, len, out);
        }
        break;
    case TD_NETLINK_CONN_SESSION_PROOF:
        if (body[0] == TD_NETLINK_SESSION_PROOF) {
            return session_proof(conn, deck, body, len, out);
        }
        break;
    default:
        break;
    }
    return fail(conn, deck, out, TD_NETLINK_ARGS);
}

td_netlink_action_t td_netlink_conn_tick(td_netlink_conn_t *conn, td_netlink_deck_t *deck, td_netlink_out_t *out)
{
    out->len = 0;
    return deadline(conn, deck, out);
}

bool td_netlink_conn_record(td_netlink_conn_t *conn, td_netlink_deck_t *deck, const uint8_t *sealed, size_t len, uint8_t *body)
{
    if (conn->state != TD_NETLINK_CONN_READY || !td_netlink_record_open(&conn->rx, sealed, len, body)) {
        conn_end(conn, deck);
        return false;
    }
    conn->last_rx_ms = now_ms(deck);
    return true;
}

bool td_netlink_conn_send(td_netlink_conn_t *conn, const uint8_t *body, size_t len, uint8_t *wire)
{
    if (conn->state != TD_NETLINK_CONN_READY || !td_netlink_record_seal(&conn->tx, body, len, wire)) {
        td_netlink_dir_close(&conn->rx);
        td_netlink_dir_close(&conn->tx);
        conn->state = TD_NETLINK_CONN_CLOSED;
        return false;
    }
    return true;
}

void td_netlink_conn_hello(td_netlink_conn_t *conn)
{
    conn->hello_seen = true;
}

void td_netlink_conn_close(td_netlink_conn_t *conn, td_netlink_deck_t *deck)
{
    conn_end(conn, deck);
}
