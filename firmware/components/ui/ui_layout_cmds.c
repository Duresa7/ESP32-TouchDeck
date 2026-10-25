#include "ui_priv.h"
#include <stdio.h>
#include <string.h>
#include "display.h"
#include "esp_lv_adapter.h"
#include "td_console.h"

#define LOCK_TIMEOUT_MS 3000

static void send(td_reply_t *r)
{
    td_reply_end(r);
    td_console_write_line(r->buf);
}

static void send_error(const char *message)
{
    char buf[TD_REPLY_MAX];
    td_reply_error(buf, sizeof buf, "layout", message);
    td_console_write_line(buf);
}

/* The screen's event callbacks and LVGL's timers, so a lane can see that a rebuild leaves both counts flat. -1 when
 * the display stayed busy. */
static void count_lvgl(int *screen_events, int *timers)
{
    *screen_events = -1;
    *timers = -1;
    if (esp_lv_adapter_lock(LOCK_TIMEOUT_MS) != ESP_OK) {
        return;
    }
    *screen_events = (int)lv_obj_get_event_count(lv_screen_active());
    int n = 0;
    for (lv_timer_t *t = lv_timer_get_next(NULL); t != NULL; t = lv_timer_get_next(t)) {
        n++;
    }
    *timers = n;
    esp_lv_adapter_unlock();
}

static void cmd_state(void)
{
    ui_layout_info_t info;
    ui_layout_info(&info);
    int screen_events = 0;
    int timers = 0;
    count_lvgl(&screen_events, &timers);

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "layout", true);
    td_reply_str(&r, "op", "state");
    td_reply_str(&r, "source", info.from_storage ? "storage" : "builtin");
    td_reply_str(&r, "name", info.name);
    td_reply_str(&r, "orientation", info.orientation);
    td_reply_int(&r, "pages", info.pages);
    td_reply_int(&r, "tiles", info.tiles);
    td_reply_int(&r, "icons_loaded", info.icons_loaded);
    td_reply_int(&r, "icons_missing", info.icons_missing);
    td_reply_int(&r, "bytes", info.bytes);
    td_reply_int(&r, "load_ms", info.load_ms);
    td_reply_int(&r, "screen_events", screen_events);
    td_reply_int(&r, "timers", timers);
    td_reply_str(&r, "error", info.error);
    send(&r);
}

static void cmd_load(const char *op, bool use_builtin)
{
    td_layout_error_t err;
    if (!ui_layout_reload(use_builtin, &err)) {
        char message[sizeof err.key + sizeof err.message + 2];
        snprintf(message, sizeof message, "%s: %s", err.key, err.message);
        send_error(message);
        return;
    }
    ui_layout_info_t info;
    ui_layout_info(&info);

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "layout", true);
    td_reply_str(&r, "op", op);
    td_reply_str(&r, "source", info.from_storage ? "storage" : "builtin");
    td_reply_str(&r, "name", info.name);
    td_reply_int(&r, "pages", info.pages);
    td_reply_int(&r, "tiles", info.tiles);
    td_reply_int(&r, "icons_loaded", info.icons_loaded);
    td_reply_int(&r, "icons_missing", info.icons_missing);
    td_reply_int(&r, "load_ms", info.load_ms);
    send(&r);
}

int ui_layout_cmd(int argc, char **argv)
{
    const char *sub = argc > 1 ? argv[1] : "";
    const bool state = strcmp(sub, "state") == 0 && argc == 2;
    const bool reload = strcmp(sub, "reload") == 0 && argc == 2;
    const bool use = strcmp(sub, "use") == 0 && argc == 3 &&
                     (strcmp(argv[2], "builtin") == 0 || strcmp(argv[2], "storage") == 0);
    if (!state && !reload && !use) {
        send_error("usage: layout state | reload | use builtin|storage");
        return 0;
    }
    if (!display_ready() || !ui_ready()) {
        send_error("display not ready");
        return 0;
    }
    if (state) {
        cmd_state();
    } else if (reload) {
        cmd_load("reload", false);
    } else {
        cmd_load("use", strcmp(argv[2], "builtin") == 0);
    }
    return 0;
}
