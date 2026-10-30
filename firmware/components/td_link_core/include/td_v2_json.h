#pragma once
#include <stdbool.h>
#include <stddef.h>
bool td_v2_json_valid(const char *json, size_t len);
/* As td_v2_json_valid, but with `live` a number anywhere inside the top-level `value` member may have a fraction or an
 * exponent, as v2.live.set allows. */
bool td_v2_json_valid_live(const char *json, size_t len, bool live);
bool td_v2_json_message(const char *json, size_t len, const char **message, size_t *message_len);
