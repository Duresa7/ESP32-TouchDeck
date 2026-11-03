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
const char *ui_live_reason(ui_live_result_t result) { (void)result; return "args"; }
ui_live_result_t ui_live_update(const ui_live_set_t *set, uint32_t *retry)
{
    (void)retry;
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
