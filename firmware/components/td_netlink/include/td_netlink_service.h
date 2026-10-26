#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "cJSON.h"
#include "td_console.h"

/* How the net task hands the app link its traffic: one authenticated link-v1 body from the session, and the end of
 * that session. Both run on the net task and must not block. */
typedef struct {
    void (*frame)(const uint8_t *body, size_t len);
    void (*closed)(void);
} td_netlink_link_t;

/* The app link registers itself before td_netlink_start. */
void td_netlink_attach(const td_netlink_link_t *link);

/* Loads the stored pairings, opens the TCP listener on 47820, and starts the net task. Call it once the network
 * interface exists. It never waits for a client. */
esp_err_t td_netlink_start(void);

/* Queues one link-v1 body for the session, from the link task. False when no session is ready or the queue stayed
 * full for 100 ms. */
bool td_netlink_send(const uint8_t *body, size_t len);

/* The link saw `hello` on the session, so the session now stays up while records keep arriving. */
void td_netlink_hello(void);

/* The `net.state` fields: listening, port, session, paired, and pairing. */
void td_netlink_state_fields(cJSON *reply);

/* Cancels a pairing window, erases every pairing, and drops the session. A request that came over the session keeps
 * it open for its reply, the next record the link sends, and closes it after that. False when the erase failed. */
bool td_netlink_forget_all(bool from_session);

/* How many apps are paired, for the mDNS TXT record. */
unsigned td_netlink_paired(void);

const esp_console_cmd_t *td_netlink_console_commands(size_t *count);
