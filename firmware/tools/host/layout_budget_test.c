#include "td_layout.h"
#include "td_asset.h"
#include "ui_gif_player.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

#define CONFIG_TD_GIF_CAPS 1
#define ESP_OK 0
typedef int esp_err_t;
typedef struct { bool failed; const char *reason; } cJSON;
enum { KIND_ICON_PUT = 1 };
static struct {
    struct { unsigned received, bytes; } x;
    unsigned kind, fill;
    uint8_t *buf;
    const char *path, *orientation, *icon_id;
} s;
static ui_tile_t tiles[5];
static ui_page_t pages[2];
static td_layout_t layout;
static unsigned committed;
static unsigned candidate;
static void td_link_fail(cJSON *reply, const char *message) { (void)message; reply->failed = true; }
static bool td_xfer_complete(const void *x) { (void)x; return true; }
static bool le16_is(const uint8_t *p, unsigned value) { return (unsigned)(p[0] | p[1] << 8) == value; }
static esp_err_t td_store_write_atomic(const char *path, const uint8_t *data, unsigned size)
{
    (void)path; (void)data; (void)size; committed = candidate; return ESP_OK;
}
static void cJSON_AddStringToObject(cJSON *reply, const char *name, const char *value)
{
    if (!strcmp(name, "reason")) reply->reason = value;
}
bool td_layout_parse(const char *data, size_t bytes, td_layout_t **out, td_layout_error_t *error)
{
    (void)data; (void)bytes; (void)error; *out = &layout; return true;
}
void td_layout_free(td_layout_t *p) { (void)p; }
static bool td_link_v2_selected(void) { return true; }
static void td_store_lock(void) {}
static void td_store_unlock(void) {}
bool td_asset_present(const char *id, uint32_t *bytes, td_asset_format_t *format)
{
    (void)id; (void)bytes; *format = TD_ASSET_FMT_GIF; return true;
}
bool td_asset_gif_canvas(const char *id, uint16_t *width, uint16_t *height)
{
    (void)id; *width = 128; *height = 128; return true;
}
static const char *ui_layout_slot(void) { return "portrait"; }
static void close_xfer(void) {}
static void ui_leave_recovery(const char *why) { (void)why; }
static bool ui_ready(void) { return false; }
static bool ui_layout_reload(bool builtin, td_layout_error_t *err) { (void)builtin; (void)err; return true; }
static void td_link_layout_fields(cJSON *reply) { (void)reply; }
#include "finish_put.c"

int main(void)
{
    for (unsigned i = 0; i < 5; i++) tiles[i] = (ui_tile_t){.render = UI_RENDER_GIF, .icon = "sha256:reused"};
    pages[0] = (ui_page_t){.tiles = tiles, .tile_count = 4};
    pages[1] = (ui_page_t){.tiles = tiles, .tile_count = 4};
    layout = (td_layout_t){.schema = 2, .pages = pages, .page_count = 2};
    uint8_t data[12] = {0};
    s.buf = data; s.fill = sizeof data; s.x.received = s.x.bytes = sizeof data;
    s.orientation = "landscape";
    candidate = 1;
    cJSON reply = {0};
    finish_put(&reply);
    assert(!reply.failed && committed == 1);
    candidate = 2;
    pages[1].tile_count = 5;
    reply = (cJSON){0};
    finish_put(&reply);
    assert(reply.failed && reply.reason && !strcmp(reply.reason, "size") && committed == 1);
    pages[1].tile_count = 4;
    reply = (cJSON){0};
    finish_put(&reply);
    assert(!reply.failed && committed == 2);
    puts("Layout commit counts reused Assets per Tile, resets each Page, refuses size and retains stored Layout: OK");
    return 0;
}
