#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "td_link.h"

/* Everything below runs on the td_link task only. */

/* Where a frame came from and where its reply goes: USB-Serial/JTAG, or the authenticated Wi-Fi session. */
typedef enum {
    TD_LINK_USB,
    TD_LINK_NET,
} td_link_transport_t;

/* The transport of the frame being handled. */
td_link_transport_t td_link_origin(void);

/* Sends one frame on a transport: SLIP under the stdout lock on USB, a sealed record on the session. Counts a USB
 * frame in frames_tx. */
bool td_link_send_on(td_link_transport_t transport, uint8_t type, uint8_t seq, uint8_t flags, const uint8_t *payload,
                     size_t len);

/* td_link_send_on the transport of the frame being handled. */
bool td_link_send(uint8_t type, uint8_t seq, uint8_t flags, const uint8_t *payload, size_t len);

/* True when the app said `hello` on either transport and that link is still live. */
bool td_link_online_any(void);

/* Counts a frame the link could not use in `dropped`. */
void td_link_count_drop(void);

/* Turns an ok reply into a refusal with this message. */
void td_link_fail(cJSON *reply, const char *error);

/* Chunk and ack frames, and the periodic work of a transfer: the chunks of a get that are due, and the abort of a
 * transfer that stopped moving. */
void td_link_files_chunk(uint8_t seq, const uint8_t *payload, size_t len);
void td_link_files_ack(const uint8_t *payload, size_t len);
void td_link_files_service(uint32_t now_ms);
/* A transport went away: a transfer it started is aborted, as if it had stopped moving. */
void td_link_files_closed(td_link_transport_t transport);

/* Message handlers. Each fills the reply, or calls td_link_fail. */
void td_link_layout_put(const cJSON *request, cJSON *reply);
void td_link_layout_get(const cJSON *request, cJSON *reply);
void td_link_icon_put(const cJSON *request, cJSON *reply);
void td_link_icon_list(const cJSON *request, cJSON *reply);
void td_link_icon_rm(const cJSON *request, cJSON *reply);
void td_link_settings_get(const cJSON *request, cJSON *reply);
void td_link_settings_put(const cJSON *request, cJSON *reply);
void td_link_store_stat(const cJSON *request, cJSON *reply);
void td_link_xfer_end(const cJSON *request, cJSON *reply);
void td_link_update_begin(const cJSON *request, cJSON *reply);
void td_link_update_end(const cJSON *request, cJSON *reply);

/* The `layout state` fields, from the layout on screen. */
void td_link_layout_fields(cJSON *reply);
