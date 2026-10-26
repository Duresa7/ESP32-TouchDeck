#include "td_settings.h"
#include <stdio.h>
#include <string.h>
#include "cJSON.h"

#define STORE_ROOT "/storage"
#define PATH_MAX_LEN 95
#define SEGMENT_MAX_LEN 32

void td_settings_defaults(td_settings_t *out)
{
    out->idle_timeout_s = TD_SETTINGS_IDLE_DEFAULT_S;
    out->orientation = TD_ORIENTATION_LANDSCAPE;
}

const char *td_settings_orientation_name(td_orientation_t orientation)
{
    return orientation == TD_ORIENTATION_PORTRAIT ? "portrait" : "landscape";
}

bool td_settings_orientation_parse(const char *name, td_orientation_t *out)
{
    if (name == NULL) {
        return false;
    }
    if (strcmp(name, "landscape") == 0) {
        *out = TD_ORIENTATION_LANDSCAPE;
        return true;
    }
    if (strcmp(name, "portrait") == 0) {
        *out = TD_ORIENTATION_PORTRAIT;
        return true;
    }
    return false;
}

static bool idle_in_range(double value)
{
    if (!(value >= TD_SETTINGS_IDLE_MIN_S && value <= TD_SETTINGS_IDLE_MAX_S)) {
        return false;
    }
    return (double)(uint32_t)value == value;
}

bool td_settings_parse(const char *json, size_t len, td_settings_t *out)
{
    td_settings_defaults(out);
    if (json == NULL) {
        return false;
    }
    cJSON *root = cJSON_ParseWithLength(json, len);
    if (root == NULL) {
        return false;
    }
    if (!cJSON_IsObject(root)) {
        cJSON_Delete(root);
        return false;
    }

    const cJSON *idle = cJSON_GetObjectItemCaseSensitive(root, "idle_timeout_s");
    if (cJSON_IsNumber(idle) && idle_in_range(idle->valuedouble)) {
        out->idle_timeout_s = (uint32_t)idle->valuedouble;
    }
    const cJSON *orientation = cJSON_GetObjectItemCaseSensitive(root, "orientation");
    if (cJSON_IsString(orientation)) {
        td_settings_orientation_parse(orientation->valuestring, &out->orientation);
    }

    cJSON_Delete(root);
    return true;
}

size_t td_settings_format(const td_settings_t *settings, char *buf, size_t cap)
{
    if (settings->idle_timeout_s < TD_SETTINGS_IDLE_MIN_S || settings->idle_timeout_s > TD_SETTINGS_IDLE_MAX_S ||
        (settings->orientation != TD_ORIENTATION_LANDSCAPE && settings->orientation != TD_ORIENTATION_PORTRAIT)) {
        return 0;
    }
    const int n = snprintf(buf, cap, "{\"idle_timeout_s\":%u,\"orientation\":\"%s\"}", (unsigned)settings->idle_timeout_s,
                           td_settings_orientation_name(settings->orientation));
    return n > 0 && (size_t)n < cap ? (size_t)n : 0;
}

static bool segment_char_ok(char c)
{
    return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '.' || c == '_' ||
           c == '-';
}

bool td_store_path_valid(const char *path, bool allow_root)
{
    const size_t root_len = sizeof(STORE_ROOT) - 1;
    if (path == NULL || strncmp(path, STORE_ROOT, root_len) != 0) {
        return false;
    }
    const size_t len = strlen(path);
    if (len > PATH_MAX_LEN) {
        return false;
    }
    if (len == root_len) {
        return allow_root;
    }
    if (path[root_len] != '/') {
        return false;
    }

    size_t segment = 0;
    for (const char *p = path + root_len + 1;; p++) {
        if (*p == '/' || *p == '\0') {
            if (segment == 0 || segment > SEGMENT_MAX_LEN) {
                return false;
            }
            const char *start = p - segment;
            if ((segment == 1 && start[0] == '.') || (segment == 2 && start[0] == '.' && start[1] == '.')) {
                return false;
            }
            segment = 0;
            if (*p == '\0') {
                return true;
            }
        } else if (segment_char_ok(*p)) {
            segment++;
        } else {
            return false;
        }
    }
}

size_t td_store_b64_encode(const uint8_t *in, size_t len, char *out, size_t cap)
{
    static const char alphabet[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    if (in == NULL || out == NULL || len == 0 || cap < 4 * ((len + 2) / 3) + 1) {
        return 0;
    }
    size_t n = 0;
    for (size_t i = 0; i < len; i += 3) {
        const size_t left = len - i;
        const uint32_t group = (uint32_t)in[i] << 16 | (left > 1 ? (uint32_t)in[i + 1] << 8 : 0) |
                               (left > 2 ? (uint32_t)in[i + 2] : 0);
        out[n++] = alphabet[group >> 18];
        out[n++] = alphabet[(group >> 12) & 63];
        out[n++] = left > 1 ? alphabet[(group >> 6) & 63] : '=';
        out[n++] = left > 2 ? alphabet[group & 63] : '=';
    }
    out[n] = '\0';
    return n;
}

static int b64_value(char c)
{
    if (c >= 'A' && c <= 'Z') {
        return c - 'A';
    }
    if (c >= 'a' && c <= 'z') {
        return c - 'a' + 26;
    }
    if (c >= '0' && c <= '9') {
        return c - '0' + 52;
    }
    if (c == '+') {
        return 62;
    }
    if (c == '/') {
        return 63;
    }
    return -1;
}

bool td_store_b64_decode(const char *in, size_t in_len, uint8_t *out, size_t out_cap, size_t *out_len)
{
    if (in == NULL || out == NULL || out_len == NULL || in_len == 0 || in_len % 4 != 0) {
        return false;
    }
    size_t n = 0;
    for (size_t i = 0; i < in_len; i += 4) {
        const bool last = i + 4 == in_len;
        int v[4];
        size_t pad = 0;
        for (size_t j = 0; j < 4; j++) {
            const char c = in[i + j];
            if (c == '=') {
                if (!last || j < 2) {
                    return false;
                }
                pad++;
                v[j] = 0;
            } else if (pad > 0 || (v[j] = b64_value(c)) < 0) {
                return false;
            }
        }
        const size_t take = 3 - pad;
        const uint32_t group = (uint32_t)v[0] << 18 | (uint32_t)v[1] << 12 | (uint32_t)v[2] << 6 | (uint32_t)v[3];
        if (pad == 1 && (group & 0xFF) != 0) {
            return false;
        }
        if (pad == 2 && (group & 0xFFFF) != 0) {
            return false;
        }
        if (n + take > out_cap) {
            return false;
        }
        out[n++] = (uint8_t)(group >> 16);
        if (take > 1) {
            out[n++] = (uint8_t)(group >> 8);
        }
        if (take > 2) {
            out[n++] = (uint8_t)group;
        }
    }
    *out_len = n;
    return true;
}
