#include <stdlib.h>
#include <string.h>
#include "td_layout.h"

const char *host_layout(const char *json)
{
    static td_layout_error_t error;
    td_layout_t *layout = NULL;
    if (!td_layout_parse(json, strlen(json), &layout, &error)) return error.key;
    free(layout);
    return "ok";
}

#include "td_input.h"
int host_input(void)
{
    td_input_mailbox_t box = {0};
    td_input_event_t event;
    if (!td_input_put(&box, "p", "t", 0.1, false, 0)) return 1;
    if (!td_input_take(&box, 0, &event) || event.value != 0.1) return 2;
    td_input_put(&box, "p", "t", 0.2, false, 1);
    td_input_put(&box, "p", "t", 0.3, false, 2);
    if (td_input_take(&box, 49, &event)) return 3;
    if (!td_input_take(&box, 50, &event) || event.value != 0.3 || event.seq != 3) return 4;
    td_input_put(&box, "p", "t", 0.4, false, 51);
    td_input_put(&box, "p", "t", 0.5, true, 52);
    if (!td_input_take(&box, 52, &event) || !event.end || event.value != 0.5) return 5;
    td_input_put(&box, "p", "t", 0.6, false, 53);
    td_input_cancel(&box);
    if (td_input_take(&box, 100, &event)) return 6;
    if (td_input_put(&box, "p", "t", 1.1, false, 101)) return 7;
    if (td_input_put(&box, "p\"", "t", 0.1, false, 101)) return 8;
    td_input_put(&box, "p", "t", 0.8, true, 102);
    td_input_put(&box, "p", "other", 0.2, false, 103);
    if (!td_input_take(&box, 103, &event) || !event.end || strcmp(event.tile, "t")) return 9;
    return 0;
}

#include "cJSON.h"
#include "ui.h"
static char received[1025];
void td_link_live_set(const cJSON *request, cJSON *reply);
bool td_link_v2_fields(const cJSON *request, const char *const *allowed, size_t count)
{
    (void)request; (void)allowed; (void)count;
    return true;
}
void td_link_v2_fail(cJSON *reply, const char *reason, const char *error)
{
    (void)error;
    cJSON_AddStringToObject(reply, "reason", reason);
}
ui_live_result_t ui_live_update(const ui_live_set_t *set, uint32_t *retry)
{
    (void)retry;
    if (!strcmp(set->tile, "unknown") || (set->type == UI_STREAM_TEXT && strlen(set->text) > UI_LIVE_TEXT_MAX)) {
        const ui_stream_t stream = {.id = "s", .type = UI_STREAM_TEXT};
        const ui_tile_t tile = {.id = "t", .streams = &stream, .stream_count = 1};
        const ui_page_t page = {.id = "p", .tiles = &tile, .tile_count = 1};
        ui_live_table_t table = {0};
        ui_live_bind(&table, NULL, 0, "builtin", &page, 1);
        return ui_live_set(&table, set, 0, retry);
    }
    const char *value = set->type == UI_STREAM_TEXT ? set->text : set->image;
    if (value && !strcmp(value, "refuse-asset")) return UI_LIVE_ASSET;
    if (!value) value = "clear";
    size_t n = strlen(value);
    if (n >= sizeof received) return UI_LIVE_SIZE;
    memcpy(received, value, n + 1);
    return UI_LIVE_OK;
}
const char *host_live(const char *json)
{
    cJSON *request = cJSON_Parse(json), *reply = cJSON_CreateObject();
    td_link_live_set(request, reply);
    const char *reason = cJSON_GetStringValue(cJSON_GetObjectItemCaseSensitive(reply, "reason"));
    if (reason) strcpy(received, reason);
    cJSON_Delete(request);
    cJSON_Delete(reply);
    return received;
}

/* The press queue as td_link drives it: a press takes the next seq only once td_press_init accepts it, and goes out
 * only while the live-table generation is the one it was queued with. */
static uint32_t press_seq, emitted_count, last_emitted_seq;
static char emitted[256];

static bool queue_press(td_press_t *press, const char *part, bool selected, uint32_t generation)
{
    if (!td_press_init(press, "p", "t", "widget", part, selected, generation, 7)) return false;
    press->seq = ++press_seq;
    return true;
}

static void send_press(const td_press_t *press, bool selected, uint32_t generation)
{
    if (!td_press_ready(press, true, selected, generation)) return;
    if (td_press_json(press, emitted, sizeof emitted) == 0) return;
    emitted_count++;
    last_emitted_seq = press->seq;
}

int host_press(void)
{
    td_press_t press;
    press_seq = emitted_count = last_emitted_seq = 0;
    if (queue_press(&press, "mute", false, 1)) return 1;
    if (queue_press(&press, "invalid!", true, 1) || queue_press(&press, "", true, 1)) return 1;
    if (!queue_press(&press, "mute", true, 1) || press.seq != 1) return 1;
    send_press(&press, true, 1);
    if (emitted_count != 1 || last_emitted_seq != 1) return 2;
    if (strcmp(emitted, "{\"m\":\"press\",\"page\":\"p\",\"tile\":\"t\",\"kind\":\"widget\",\"seq\":1,\"t\":7,"
                       "\"part\":\"mute\"}")) return 2;

    td_press_t plain;
    if (!queue_press(&plain, NULL, true, 1)) return 3;
    send_press(&plain, true, 1);
    if (strcmp(emitted, "{\"m\":\"press\",\"page\":\"p\",\"tile\":\"t\",\"kind\":\"widget\",\"seq\":2,\"t\":7}")) return 3;

    /* A tap named in generation 1 that would go out after the table moved to generation 2. */
    if (!queue_press(&press, "mute", true, 1)) return 3;
    send_press(&press, true, 2);
    if (emitted_count != 2) return 3;

    if (!queue_press(&press, "mute", true, 2)) return 4;
    send_press(&press, true, 2);
    if (td_press_json(&press, emitted, 16) != 0) return 4;
    return (emitted_count == 3 && last_emitted_seq == 4) ? 0 : 4;
}
