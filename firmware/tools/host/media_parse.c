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

#include <stdio.h>
#include "key_sink_link.h"
#include "td_frame.h"
#include "td_link.h"
#define TD_LINK_EVENTS_PER_TURN 16
#define pdTRUE 1
#define pdFALSE 0
typedef td_press_t press_t;
static press_t queue_buf[16];
static size_t queue_head, queue_tail, queue_len;
static int s_presses = 1;
static uint32_t s_press_seq, sent_seq, live_gen, tap_gen, emitted_count, last_emitted_seq;
static bool selected, online_flag;
static bool td_link_v2_selected(void) { return selected; }
uint32_t ui_live_generation(void) { return live_gen; }
uint32_t ui_live_tap_generation(void) { return tap_gen; }
td_link_state_t td_link_state(void) { return (td_link_state_t){ .online = online_flag }; }
static int64_t esp_timer_get_time(void) { return 0; }
#define __atomic_add_fetch(p, n, order) (*(p) += (n))
static void xQueueReset(int queue) { (void)queue; queue_head = queue_tail = queue_len = 0; }
static void xQueueSend(int queue, const press_t *press, int wait)
{
    (void)queue; (void)wait;
    sent_seq = press->seq;
    if (queue_len < 16) { queue_buf[queue_tail] = *press; queue_tail = (queue_tail + 1) % 16; queue_len++; }
}
static int xQueueReceive(int queue, press_t *out, int wait)
{
    (void)queue; (void)wait;
    if (queue_len == 0) return pdFALSE;
    *out = queue_buf[queue_head];
    queue_head = (queue_head + 1) % 16;
    queue_len--;
    return pdTRUE;
}
void key_sink_link_discard(void) {}
size_t key_sink_link_next(char *out, size_t cap) { (void)out; (void)cap; return 0; }
bool td_link_send(uint8_t type, uint8_t seq, uint8_t flags, const uint8_t *payload, size_t len)
{
    (void)type; (void)seq; (void)flags; (void)len;
    emitted_count++;
    const char *p = strstr((const char *)payload, "\"seq\":");
    if (p) last_emitted_seq = (uint32_t)strtoul(p + 6, NULL, 10);
    return true;
}
#include "press_part.c"
int host_press(void)
{
    xQueueReset(s_presses);
    s_press_seq = sent_seq = emitted_count = last_emitted_seq = 0;
    live_gen = tap_gen = 1;
    online_flag = true;
    selected = false;
    td_link_press_part("p", "t", "widget", "mute");
    selected = true;
    td_link_press_part("p", "t", "widget", "invalid!");
    td_link_press_part("p", "t", "widget", "");
    td_link_press_part("p", "t", "widget", "mute");
    if (sent_seq != 1) return 1;
    send_events();
    if (emitted_count != 1 || last_emitted_seq != 1) return 2;

    /* Queued press and in-flight part tap across a session reset must not reach the new session. */
    td_link_press("p", "t", "widget");
    tap_gen = 1;
    live_gen = 2;
    xQueueReset(s_presses);
    td_link_press_part("p", "t", "widget", "mute");
    send_events();
    if (emitted_count != 1) return 3;

    tap_gen = 2;
    td_link_press_part("p", "t", "widget", "mute");
    send_events();
    return (emitted_count == 2 && last_emitted_seq == 4) ? 0 : 4;
}
