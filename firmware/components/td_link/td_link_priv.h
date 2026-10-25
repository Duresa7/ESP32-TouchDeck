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

/* Chunk and ack frames, and the periodic work of a transfer: the chunks of a get that are due, and the abort of a
 * transfer that stopped moving. */
void td_link_files_chunk(uint8_t seq, const uint8_t *payload, size_t len);
void td_link_files_ack(const uint8_t *payload, size_t len);
void td_link_files_service(uint32_t now_ms);

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

/* The `layout state` fields, from the layout on screen. */
void td_link_layout_fields(cJSON *reply);
