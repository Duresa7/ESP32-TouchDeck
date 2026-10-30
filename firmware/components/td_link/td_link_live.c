#include <math.h>
#include <string.h>
#include "td_link_priv.h"
#include "ui.h"

static const char *const k_fields[] = { "layout", "page", "tile", "stream", "n", "ttl_ms", "type", "value" };
static const char *const k_types[] = { "number", "series", "text", "image" };

static bool whole(const cJSON *item, double max)
{
    return cJSON_IsNumber(item) && floor(item->valuedouble) == item->valuedouble && item->valuedouble >= 0 &&
           item->valuedouble <= max;
}

static int type_of(const cJSON *item)
{
    for (int i = 0; cJSON_IsString(item) && i < (int)(sizeof k_types / sizeof k_types[0]); i++) {
        if (strcmp(item->valuestring, k_types[i]) == 0) {
            return i;
        }
    }
    return -1;
}

/* Reads the value of a number or a series into values. A text or image value is only checked for its JSON type: no
 * stream this Firmware draws takes one, so the type check refuses it. False for a value of the wrong shape. */
static bool read_value(const cJSON *value, ui_stream_type_t type, double values[UI_LIVE_SERIES_MAX + 1], size_t *count)
{
    *count = 0;
    switch (type) {
    case UI_STREAM_NUMBER:
        values[0] = cJSON_IsNumber(value) ? value->valuedouble : 0;
        *count = 1;
        return cJSON_IsNumber(value);
    case UI_STREAM_SERIES: {
        if (!cJSON_IsArray(value)) {
            return false;
        }
        const cJSON *item = NULL;
        cJSON_ArrayForEach(item, value)
        {
            if (!cJSON_IsNumber(item)) {
                return false;
            }
            if (*count <= UI_LIVE_SERIES_MAX) {
                values[(*count)++] = item->valuedouble;
            }
        }
        return true;
    }
    case UI_STREAM_TEXT:
        return cJSON_IsString(value);
    case UI_STREAM_IMAGE:
        return cJSON_IsNull(value) || cJSON_IsString(value);
    }
    return false;
}

void td_link_live_set(const cJSON *request, cJSON *reply)
{
    const cJSON *layout = cJSON_GetObjectItemCaseSensitive(request, "layout");
    const cJSON *page = cJSON_GetObjectItemCaseSensitive(request, "page");
    const cJSON *tile = cJSON_GetObjectItemCaseSensitive(request, "tile");
    const cJSON *stream = cJSON_GetObjectItemCaseSensitive(request, "stream");
    const cJSON *n = cJSON_GetObjectItemCaseSensitive(request, "n");
    const cJSON *ttl = cJSON_GetObjectItemCaseSensitive(request, "ttl_ms");
    const int type = type_of(cJSON_GetObjectItemCaseSensitive(request, "type"));
    double values[UI_LIVE_SERIES_MAX + 1];
    ui_live_set_t set = { 0 };
    if (!td_link_v2_fields(request, k_fields, sizeof k_fields / sizeof k_fields[0]) || !cJSON_IsString(layout) ||
        !cJSON_IsString(page) || !cJSON_IsString(tile) || !cJSON_IsString(stream) || !whole(n, 4294967295.0) ||
        !whole(ttl, 4294967295.0) || type < 0 ||
        !read_value(cJSON_GetObjectItemCaseSensitive(request, "value"), (ui_stream_type_t)type, values, &set.count)) {
        td_link_v2_fail(reply, "args", "Invalid live value");
        return;
    }
    set.layout = layout->valuestring;
    set.page = page->valuestring;
    set.tile = tile->valuestring;
    set.stream = stream->valuestring;
    set.n = (uint32_t)n->valuedouble;
    set.ttl_ms = (uint32_t)ttl->valuedouble;
    set.type = (ui_stream_type_t)type;
    set.values = values;
    uint32_t retry_ms = 0;
    const ui_live_result_t result = ui_live_update(&set, &retry_ms);
    if (result != UI_LIVE_OK) {
        td_link_v2_fail(reply, ui_live_reason(result), "Request refused");
        if (result == UI_LIVE_RATE) {
            cJSON_AddNumberToObject(reply, "retry_ms", retry_ms);
        }
        return;
    }
    cJSON_AddNumberToObject(reply, "n", set.n);
}
