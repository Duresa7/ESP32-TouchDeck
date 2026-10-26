#include "td_link_priv.h"
#include <math.h>
#include <stdio.h>
#include <string.h>
#include "esp_heap_caps.h"
#include "esp_timer.h"
#include "power.h"
#include "td_frame.h"
#include "td_layout.h"
#include "td_store.h"
#include "td_xfer.h"
#include "ui.h"

#define LAYOUT_DIR "/storage/layouts"
#define ICON_DIR "/storage/icons"
#define ICON_ID_LEN 16
#define ICON_FILE_BYTES 12300
#define ICON_PAGE 32
#define GET_IDLE_MS 5000

typedef enum {
    KIND_NONE,
    KIND_LAYOUT_PUT,
    KIND_ICON_PUT,
    KIND_GET,
} kind_t;

/* One transfer at a time. A put holds the whole file in PSRAM and writes it with td_store_write_atomic at xfer.end,
 * so a cut transfer leaves nothing on the volume. A get reads the file once and sends it from memory. */
static struct {
    kind_t kind;
    td_xfer_t x;
    uint8_t *buf;
    size_t fill;
    char path[TD_STORE_PATH_MAX];
    char orientation[10];
    char icon_id[ICON_ID_LEN + 1];
    uint32_t last_ms;
    uint8_t last_id;
    uint8_t tx_seq;
} s;

static void close_xfer(void)
{
    heap_caps_free(s.buf);
    s.buf = NULL;
    s.fill = 0;
    s.kind = KIND_NONE;
}

static bool open_xfer(kind_t kind, uint32_t bytes)
{
    close_xfer();
    s.buf = heap_caps_malloc(bytes, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    if (s.buf == NULL) {
        s.buf = heap_caps_malloc(bytes, MALLOC_CAP_8BIT);
    }
    if (s.buf == NULL) {
        return false;
    }
    s.kind = kind;
    s.last_id = s.last_id % 255 + 1;
    s.last_ms = (uint32_t)(esp_timer_get_time() / 1000);
    return true;
}

static bool put_sink(void *ctx, const uint8_t *data, size_t len)
{
    (void)ctx;
    memcpy(s.buf + s.fill, data, len);
    s.fill += len;
    return true;
}

static bool u32_arg(const cJSON *request, const char *name, uint32_t *out)
{
    const cJSON *item = cJSON_GetObjectItemCaseSensitive(request, name);
    if (!cJSON_IsNumber(item) || item->valuedouble < 0 || item->valuedouble > 4294967295.0
        || item->valuedouble != floor(item->valuedouble)) {
        return false;
    }
    *out = (uint32_t)item->valuedouble;
    return true;
}

static const char *orientation_arg(const cJSON *request)
{
    const cJSON *item = cJSON_GetObjectItemCaseSensitive(request, "orientation");
    if (cJSON_IsString(item)) {
        if (strcmp(item->valuestring, "landscape") == 0) {
            return "landscape";
        }
        if (strcmp(item->valuestring, "portrait") == 0) {
            return "portrait";
        }
    }
    return NULL;
}

static bool icon_id_valid(const char *id)
{
    if (strlen(id) != ICON_ID_LEN) {
        return false;
    }
    for (size_t i = 0; i < ICON_ID_LEN; i++) {
        if (!((id[i] >= '0' && id[i] <= '9') || (id[i] >= 'a' && id[i] <= 'f'))) {
            return false;
        }
    }
    return true;
}

static const char *icon_id_arg(const cJSON *request)
{
    const cJSON *item = cJSON_GetObjectItemCaseSensitive(request, "id");
    return cJSON_IsString(item) && icon_id_valid(item->valuestring) ? item->valuestring : NULL;
}

void td_link_layout_fields(cJSON *reply)
{
    ui_layout_info_t info;
    ui_layout_info(&info);
    cJSON_AddStringToObject(reply, "source", info.from_storage ? "storage" : "builtin");
    cJSON_AddStringToObject(reply, "name", info.name);
    cJSON_AddStringToObject(reply, "orientation", info.orientation);
    cJSON_AddNumberToObject(reply, "pages", info.pages);
    cJSON_AddNumberToObject(reply, "tiles", info.tiles);
    cJSON_AddNumberToObject(reply, "icons_loaded", info.icons_loaded);
    cJSON_AddNumberToObject(reply, "icons_missing", info.icons_missing);
    cJSON_AddNumberToObject(reply, "bytes", info.bytes);
    cJSON_AddNumberToObject(reply, "load_ms", info.load_ms);
    cJSON_AddStringToObject(reply, "error", info.error);
}

void td_link_layout_put(const cJSON *request, cJSON *reply)
{
    const char *orientation = orientation_arg(request);
    uint32_t bytes;
    uint32_t crc32;
    if (orientation == NULL) {
        td_link_fail(reply, "orientation must be landscape or portrait");
    } else if (!u32_arg(request, "bytes", &bytes) || !u32_arg(request, "crc32", &crc32)) {
        td_link_fail(reply, "layout.put needs bytes and crc32");
    } else if (bytes == 0 || bytes > TD_LAYOUT_MAX_BYTES) {
        td_link_fail(reply, "layout must be 1 to 32768 bytes");
    } else if (!td_store_mounted()) {
        td_link_fail(reply, "store not mounted");
    } else if (!open_xfer(KIND_LAYOUT_PUT, bytes)) {
        td_link_fail(reply, "no memory");
    } else {
        td_xfer_init(&s.x, s.last_id, false, bytes, crc32, put_sink, NULL);
        snprintf(s.orientation, sizeof s.orientation, "%s", orientation);
        td_layout_path(orientation, s.path, sizeof s.path);
        cJSON_AddNumberToObject(reply, "xfer", s.x.id);
    }
}

void td_link_icon_put(const cJSON *request, cJSON *reply)
{
    const char *id = icon_id_arg(request);
    uint32_t bytes;
    uint32_t crc32;
    if (id == NULL) {
        td_link_fail(reply, "id must be 16 lowercase hex digits");
    } else if (!u32_arg(request, "bytes", &bytes) || !u32_arg(request, "crc32", &crc32)) {
        td_link_fail(reply, "icon.put needs bytes and crc32");
    } else if (bytes != ICON_FILE_BYTES) {
        td_link_fail(reply, "an icon is 12300 bytes");
    } else if (!td_store_mounted()) {
        td_link_fail(reply, "store not mounted");
    } else if (!open_xfer(KIND_ICON_PUT, bytes)) {
        td_link_fail(reply, "no memory");
    } else {
        td_xfer_init(&s.x, s.last_id, false, bytes, crc32, put_sink, NULL);
        snprintf(s.icon_id, sizeof s.icon_id, "%s", id);
        snprintf(s.path, sizeof s.path, ICON_DIR "/%s.bin", id);
        cJSON_AddNumberToObject(reply, "xfer", s.x.id);
    }
}

void td_link_layout_get(const cJSON *request, cJSON *reply)
{
    const char *orientation = orientation_arg(request);
    if (orientation == NULL) {
        td_link_fail(reply, "orientation must be landscape or portrait");
        return;
    }
    char path[TD_STORE_PATH_MAX];
    td_layout_path(orientation, path, sizeof path);
    td_store_stat_t stat;
    char message[64];
    if (td_store_stat(path, &stat) != ESP_OK || stat.is_dir) {
        snprintf(message, sizeof message, "no stored layout for %s", orientation);
        td_link_fail(reply, message);
        return;
    }
    if (stat.size == 0 || stat.size > TD_LAYOUT_MAX_BYTES) {
        td_link_fail(reply, "the stored layout is not a valid size");
        return;
    }
    if (!open_xfer(KIND_GET, stat.size)) {
        td_link_fail(reply, "no memory");
        return;
    }
    size_t len = 0;
    if (td_store_read(path, s.buf, stat.size, &len) != ESP_OK || len != stat.size) {
        close_xfer();
        td_link_fail(reply, "read failed");
        return;
    }
    const uint32_t crc32 = td_xfer_crc32(s.buf, len);
    td_xfer_init(&s.x, s.last_id, true, (uint32_t)len, crc32, NULL, NULL);
    cJSON_AddNumberToObject(reply, "xfer", s.x.id);
    cJSON_AddNumberToObject(reply, "bytes", (double)len);
    cJSON_AddNumberToObject(reply, "crc32", (double)crc32);
}

static bool le16_is(const uint8_t *p, unsigned value)
{
    return (unsigned)(p[0] | (p[1] << 8)) == value;
}

static void finish_put(cJSON *reply)
{
    char message[sizeof(td_layout_error_t) + 4];
    if (s.x.received != s.x.bytes) {
        snprintf(message, sizeof message, "received %u of %u bytes", (unsigned)s.x.received, (unsigned)s.x.bytes);
        td_link_fail(reply, message);
        return;
    }
    if (!td_xfer_complete(&s.x)) {
        td_link_fail(reply, "crc32 mismatch");
        return;
    }
    if (s.kind == KIND_ICON_PUT) {
        const uint8_t *h = s.buf;
        if (memcmp(h, "TDI1", 4) != 0 || !le16_is(h + 4, 64) || !le16_is(h + 6, 64) || !le16_is(h + 8, 1)
            || !le16_is(h + 10, 0)) {
            td_link_fail(reply, "bad icon header");
            return;
        }
        if (td_store_write_atomic(s.path, s.buf, s.fill) != ESP_OK) {
            td_link_fail(reply, "write failed");
            return;
        }
        cJSON_AddStringToObject(reply, "id", s.icon_id);
        return;
    }
    /* A candidate layout is validated before it replaces anything, so a bad file never costs the good one. */
    td_layout_t *parsed = NULL;
    td_layout_error_t err;
    if (!td_layout_parse((const char *)s.buf, s.fill, &parsed, &err)) {
        snprintf(message, sizeof message, "%s: %s", err.key, err.message);
        td_link_fail(reply, message);
        return;
    }
    td_layout_free(parsed);
    if (td_store_write_atomic(s.path, s.buf, s.fill) != ESP_OK) {
        td_link_fail(reply, "write failed");
        return;
    }
    const bool shown = strcmp(s.orientation, ui_layout_slot()) == 0;
    close_xfer();
    if (shown && ui_ready() && !ui_layout_reload(false, &err)) {
        snprintf(message, sizeof message, "%s: %s", err.key, err.message);
        td_link_fail(reply, message);
        return;
    }
    td_link_layout_fields(reply);
}

void td_link_xfer_end(const cJSON *request, cJSON *reply)
{
    uint32_t id;
    if (!u32_arg(request, "xfer", &id)) {
        td_link_fail(reply, "xfer.end needs xfer");
    } else if (s.kind == KIND_NONE || id != s.x.id) {
        td_link_fail(reply, "no transfer with that xfer");
    } else if (s.kind != KIND_GET) {
        finish_put(reply);
    }
    close_xfer();
}

static void send_chunk(uint16_t idx)
{
    static uint8_t payload[TD_FRAME_PAYLOAD_MAX];
    const size_t start = (size_t)idx * TD_FRAME_CHUNK_DATA_MAX;
    size_t len = s.x.bytes - start;
    if (len > TD_FRAME_CHUNK_DATA_MAX) {
        len = TD_FRAME_CHUNK_DATA_MAX;
    }
    payload[0] = s.x.id;
    payload[1] = idx & 0xff;
    payload[2] = idx >> 8;
    memcpy(payload + TD_FRAME_CHUNK_HEADER, s.buf + start, len);
    s.tx_seq = s.tx_seq % 255 + 1;
    td_link_send(TD_FRAME_CHUNK, s.tx_seq, 0, payload, TD_FRAME_CHUNK_HEADER + len);
}

void td_link_files_chunk(uint8_t seq, const uint8_t *payload, size_t len)
{
    if (len < TD_FRAME_CHUNK_HEADER || (s.kind != KIND_LAYOUT_PUT && s.kind != KIND_ICON_PUT) || payload[0] != s.x.id) {
        td_link_count_drop();
        return;
    }
    const uint16_t idx = (uint16_t)(payload[1] | (payload[2] << 8));
    /* A duplicate, a gap and a chunk past the window all get the same answer: the next index the deck wants. */
    td_xfer_receive(&s.x, idx, payload + TD_FRAME_CHUNK_HEADER, len - TD_FRAME_CHUNK_HEADER);
    s.last_ms = (uint32_t)(esp_timer_get_time() / 1000);
    const uint8_t ack[3] = { s.x.id, s.x.next & 0xff, s.x.next >> 8 };
    td_link_send(TD_FRAME_ACK, seq, TD_FRAME_RESPONSE, ack, sizeof ack);
}

void td_link_files_ack(const uint8_t *payload, size_t len)
{
    if (len != 3 || s.kind != KIND_GET || payload[0] != s.x.id) {
        td_link_count_drop();
        return;
    }
    td_xfer_ack(&s.x, (uint16_t)(payload[1] | (payload[2] << 8)));
    s.last_ms = (uint32_t)(esp_timer_get_time() / 1000);
}

void td_link_files_service(uint32_t now_ms)
{
    if (s.kind == KIND_NONE) {
        return;
    }
    if (s.kind == KIND_GET) {
        int idx;
        for (int guard = 0; guard < 2 * TD_XFER_WINDOW && (idx = td_xfer_due(&s.x, now_ms)) >= 0; guard++) {
            send_chunk((uint16_t)idx);
        }
        if (s.x.aborted || now_ms - s.last_ms > GET_IDLE_MS) {
            close_xfer();
        }
    } else if (now_ms - s.last_ms > TD_XFER_IDLE_MS) {
        close_xfer();
    }
}

typedef struct {
    uint32_t skip;
    uint32_t seen;
    uint32_t count;
    bool more;
    cJSON *ids;
} icon_list_t;

static bool icon_name_valid(const char *name)
{
    char id[ICON_ID_LEN + 1];
    if (strlen(name) != ICON_ID_LEN + 4 || strcmp(name + ICON_ID_LEN, ".bin") != 0) {
        return false;
    }
    memcpy(id, name, ICON_ID_LEN);
    id[ICON_ID_LEN] = '\0';
    return icon_id_valid(id);
}

static bool icon_list_cb(void *ctx, const char *name, uint32_t size, bool is_dir)
{
    icon_list_t *list = ctx;
    (void)size;
    if (is_dir || !icon_name_valid(name) || list->seen++ < list->skip) {
        return true;
    }
    if (list->count == ICON_PAGE) {
        list->more = true;
        return false;
    }
    char id[ICON_ID_LEN + 1];
    memcpy(id, name, ICON_ID_LEN);
    id[ICON_ID_LEN] = '\0';
    cJSON_AddItemToArray(list->ids, cJSON_CreateString(id));
    list->count++;
    return true;
}

void td_link_icon_list(const cJSON *request, cJSON *reply)
{
    const cJSON *cursor = cJSON_GetObjectItemCaseSensitive(request, "cursor");
    icon_list_t list = { .ids = cJSON_CreateArray() };
    if (list.ids == NULL) {
        td_link_fail(reply, "no memory");
        return;
    }
    if (cJSON_IsNumber(cursor) && cursor->valuedouble >= 0 && cursor->valuedouble < 1e9) {
        list.skip = (uint32_t)cursor->valuedouble;
    }
    td_store_list(ICON_DIR, icon_list_cb, &list);
    cJSON_AddItemToObject(reply, "ids", list.ids);
    if (list.more) {
        cJSON_AddNumberToObject(reply, "next", list.skip + list.count);
    } else {
        cJSON_AddNullToObject(reply, "next");
    }
}

void td_link_icon_rm(const cJSON *request, cJSON *reply)
{
    const char *id = icon_id_arg(request);
    if (id == NULL) {
        td_link_fail(reply, "id must be 16 lowercase hex digits");
        return;
    }
    char path[TD_STORE_PATH_MAX];
    snprintf(path, sizeof path, ICON_DIR "/%s.bin", id);
    esp_err_t err = td_store_remove(path);
    if (err == ESP_ERR_NOT_FOUND) {
        td_link_fail(reply, "no such icon");
    } else if (err != ESP_OK) {
        td_link_fail(reply, "remove failed");
    }
}

static void settings_fields(cJSON *reply, const td_settings_t *settings)
{
    cJSON_AddNumberToObject(reply, "idle_timeout_s", settings->idle_timeout_s);
    cJSON_AddStringToObject(reply, "orientation", td_settings_orientation_name(settings->orientation));
}

void td_link_settings_get(const cJSON *request, cJSON *reply)
{
    (void)request;
    td_settings_t settings;
    td_store_settings_load(&settings);
    settings_fields(reply, &settings);
}

void td_link_settings_put(const cJSON *request, cJSON *reply)
{
    td_settings_t before;
    td_store_settings_load(&before);
    td_settings_t next = before;
    const cJSON *idle = cJSON_GetObjectItemCaseSensitive(request, "idle_timeout_s");
    const cJSON *orientation = cJSON_GetObjectItemCaseSensitive(request, "orientation");
    if (idle != NULL) {
        uint32_t seconds;
        if (!u32_arg(request, "idle_timeout_s", &seconds) || seconds < TD_SETTINGS_IDLE_MIN_S
            || seconds > TD_SETTINGS_IDLE_MAX_S) {
            td_link_fail(reply, "idle_timeout_s must be a whole number from 1 to 86400");
            return;
        }
        next.idle_timeout_s = seconds;
    }
    if (orientation != NULL
        && (!cJSON_IsString(orientation) || !td_settings_orientation_parse(orientation->valuestring, &next.orientation))) {
        td_link_fail(reply, "orientation must be landscape or portrait");
        return;
    }
    if (td_store_settings_save(&next) != ESP_OK) {
        td_link_fail(reply, "settings not saved");
        return;
    }
    power_set_idle_timeout_s(next.idle_timeout_s, esp_timer_get_time());
    settings_fields(reply, &next);
    cJSON_AddBoolToObject(reply, "reboot_required", next.orientation != before.orientation);
}

static bool count_cb(void *ctx, const char *name, uint32_t size, bool is_dir)
{
    (void)name;
    (void)size;
    if (!is_dir) {
        ++*(uint32_t *)ctx;
    }
    return true;
}

void td_link_store_stat(const cJSON *request, cJSON *reply)
{
    (void)request;
    td_store_info_t info;
    if (td_store_info(&info) != ESP_OK) {
        td_link_fail(reply, "store not mounted");
        return;
    }
    uint32_t layouts = 0;
    uint32_t icons = 0;
    td_store_list(LAYOUT_DIR, count_cb, &layouts);
    td_store_list(ICON_DIR, count_cb, &icons);
    cJSON_AddNumberToObject(reply, "total_bytes", info.total_bytes);
    cJSON_AddNumberToObject(reply, "used_bytes", info.used_bytes);
    cJSON_AddNumberToObject(reply, "layouts", layouts);
    cJSON_AddNumberToObject(reply, "icons", icons);
}
