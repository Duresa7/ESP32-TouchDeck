#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "td_link.h"

/* Everything below runs on the td_link task only. */

/* Encodes and writes one frame under the stdout lock. Counts it in frames_tx. */
bool td_link_send(uint8_t type, uint8_t seq, uint8_t flags, const uint8_t *payload, size_t len);

/* Counts a frame the link could not use in `dropped`. */
void td_link_count_drop(void);

/* Turns an ok reply into a refusal with this message. */
void td_link_fail(cJSON *reply, const char *error);

/* A v2 refusal: td_link_fail plus the stable `reason`. */
void td_link_v2_fail(cJSON *reply, const char *reason, const char *error);

/* True when the request has `m`, no other field outside `allowed`, and no key twice. */
bool td_link_v2_fields(const cJSON *request, const char *const *allowed, size_t count);

/* Chunk and ack frames, and the periodic work of a transfer: the chunks of a get that are due, and the abort of a
 * transfer that stopped moving. */
void td_link_files_chunk(uint8_t seq, const uint8_t *payload, size_t len);
void td_link_files_ack(const uint8_t *payload, size_t len);
void td_link_files_service(uint32_t now_ms);

/* A protocol selection, v1 or v2, discards an open v2 upload. */
void td_link_files_select(void);

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
void td_link_asset_stat(const cJSON *request, cJSON *reply);
void td_link_asset_list(const cJSON *request, cJSON *reply);
void td_link_asset_put(const cJSON *request, cJSON *reply);
void td_link_asset_end(const cJSON *request, cJSON *reply);
void td_link_asset_delete(const cJSON *request, cJSON *reply);

/* The `layout state` fields, from the layout on screen. */
void td_link_layout_fields(cJSON *reply);
