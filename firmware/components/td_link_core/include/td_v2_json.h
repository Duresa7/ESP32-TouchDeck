#pragma once
#include <stdbool.h>
#include <stddef.h>
bool td_v2_json_valid(const char *json, size_t len);
bool td_v2_json_message(const char *json, size_t len, const char **message, size_t *message_len);
