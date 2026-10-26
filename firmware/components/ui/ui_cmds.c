#include "ui.h"
#include "ui_priv.h"
#include <stdlib.h>
#include <string.h>
#include "board_pins.h"
#include "display.h"
#include "esp_lv_adapter.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "td_console.h"
#include "touch.h"
#include "ui_theme.h"

#define LOCK_TIMEOUT_MS 3000
#define HOLD_MAX_MS 2000
#define HOLD_REGISTER_POLL_MS 5
#define HOLD_REGISTER_MAX_MS 150
#define HOLD_SETTLE_SLACK_MS 40

static void send(td_reply_t *r)
{
    td_reply_end(r);
    td_console_write_line(r->buf);
}

static void send_error(const char *message)
{
    char buf[TD_REPLY_MAX];
    td_reply_error(buf, sizeof buf, "ui", message);
    td_console_write_line(buf);
}

/* strtoul accepts a sign and leading space, so the first character must be a digit. A number too large for
 * unsigned long saturates, and the caller's range check refuses it. */
static bool parse_number(const char *word, unsigned long max, unsigned long *out)
{
    if (word[0] < '0' || word[0] > '9') {
        return false;
    }
    char *end = NULL;
    const unsigned long value = strtoul(word, &end, 10);
    if (*end != '\0' || value > max) {
        return false;
    }
    *out = value;
    return true;
}

static void cmd_state(void)
{
    ui_state_t st;
    ui_get_state(&st);

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "ui", true);
    td_reply_int(&r, "page", st.page);
    td_reply_int(&r, "pages", st.pages);
    td_reply_int(&r, "tile_count", st.tile_count);
    td_reply_str(&r, "title", st.title);
    td_reply_str(&r, "pressed_tile", st.pressed_tile);
    td_reply_int(&r, "taps", st.taps);
    td_reply_bool(&r, "animating", st.animating);
    td_reply_int(&r, "transitions", st.transitions);
    td_reply_int(&r, "transition_ms", st.transition_ms);
    td_reply_int(&r, "transition_frames", st.transition_frames);
    td_reply_int(&r, "last_ms", st.last_ms);
    td_reply_int(&r, "last_frames", st.last_frames);
    td_reply_int(&r, "dot_x", ui_dot_x(0, st.pages));
    td_reply_int(&r, "dot_pitch", ui_theme.dot_pitch);
    td_reply_int(&r, "dot_y", ui_theme.dot_y);
    td_reply_int(&r, "dot_active", ui_theme.color.dot_active);
    td_reply_int(&r, "dot_idle", ui_theme.color.dot_idle);
    td_reply_int(&r, "glow", ui_theme.color.glow);
    send(&r);
}

static void cmd_goto(int argc, char **argv)
{
    unsigned long page = 0;
    if (argc != 3 || !parse_number(argv[2], 255, &page)) {
        send_error("usage: ui goto <page>");
        return;
    }
    if (esp_lv_adapter_lock(LOCK_TIMEOUT_MS) != ESP_OK) {
        send_error("display busy");
        return;
    }
    const bool exists = ui_goto((uint8_t)page);
    esp_lv_adapter_unlock();
    if (!exists) {
        send_error("out of range");
        return;
    }

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "ui", true);
    td_reply_str(&r, "op", "goto");
    td_reply_int(&r, "page", page);
    send(&r);
}

static void cmd_tile(int argc, char **argv)
{
    if (argc != 3) {
        send_error("usage: ui tile <id>");
        return;
    }
    ui_tile_info_t tile;
    if (!ui_tile_info(argv[2], &tile)) {
        send_error("no such tile");
        return;
    }
    const ui_rect_t rect = tile.rect;

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "ui", true);
    td_reply_str(&r, "op", "tile");
    td_reply_str(&r, "id", argv[2]);
    td_reply_str(&r, "name", tile.label);
    td_reply_int(&r, "page", tile.page);
    td_reply_int(&r, "x", rect.x);
    td_reply_int(&r, "y", rect.y);
    td_reply_int(&r, "w", rect.w);
    td_reply_int(&r, "h", rect.h);
    td_reply_int(&r, "cx", rect.x + rect.w / 2);
    td_reply_int(&r, "cy", rect.y + rect.h / 2);
    td_reply_str(&r, "action", tile.action);
    td_reply_str(&r, "icon", tile.icon);
    send(&r);
}

static const char *hold_error(esp_err_t err)
{
    switch (err) {
    case ESP_ERR_INVALID_STATE:
        return "touch not ready";
    case ESP_ERR_NOT_ALLOWED:
        return "finger down";
    case ESP_ERR_NOT_FINISHED:
        return "hold queued";
    case ESP_ERR_NO_MEM:
        return "queue full";
    default:
        return "hold refused";
    }
}

static void cmd_hold(int argc, char **argv)
{
    unsigned long v[3] = { 0 };
    static const unsigned long max[3] = { TD_LCD_H_RES - 1, TD_LCD_V_RES - 1, HOLD_MAX_MS };
    bool ok = argc == 5;
    for (int i = 0; ok && i < 3; i++) {
        ok = parse_number(argv[2 + i], max[i], &v[i]);
    }
    if (!ok || v[2] == 0) {
        send_error("usage: ui hold <x 0-1023> <y 0-599> <ms 1-2000>");
        return;
    }
    const esp_err_t err = touch_inject_hold((uint16_t)v[0], (uint16_t)v[1], (uint32_t)v[2]);
    if (err != ESP_OK) {
        send_error(hold_error(err));
        return;
    }

    /* The console runs one command at a time, so a screenshot sent behind this line is taken after the pressed look
     * has finished its transition. The reply names the tile that the press registered on. A hold shorter than the
     * wait can end before that, and then the tile is the empty string. */
    ui_state_t st;
    ui_get_state(&st);
    for (int waited = 0; st.pressed_tile[0] == '\0' && waited < HOLD_REGISTER_MAX_MS; waited += HOLD_REGISTER_POLL_MS) {
        vTaskDelay(pdMS_TO_TICKS(HOLD_REGISTER_POLL_MS));
        ui_get_state(&st);
    }
    const char *pressed = st.pressed_tile;
    if (pressed[0] != '\0') {
        vTaskDelay(pdMS_TO_TICKS(ui_theme.anim_press_ms + HOLD_SETTLE_SLACK_MS));
    }

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "ui", true);
    td_reply_str(&r, "op", "hold");
    td_reply_int(&r, "x", v[0]);
    td_reply_int(&r, "y", v[1]);
    td_reply_int(&r, "ms", v[2]);
    td_reply_str(&r, "pressed_tile", pressed);
    send(&r);
}

static int cmd_ui(int argc, char **argv)
{
    const char *sub = argc > 1 ? argv[1] : "";
    const bool state = strcmp(sub, "state") == 0 && argc == 2;
    const bool known = state || strcmp(sub, "goto") == 0 || strcmp(sub, "tile") == 0 || strcmp(sub, "hold") == 0;
    if (!known) {
        send_error("usage: ui state | goto <page> | tile <id> | hold <x> <y> <ms>");
        return 0;
    }
    if (!display_ready() || !ui_ready()) {
        send_error("display not ready");
        return 0;
    }

    if (state) {
        cmd_state();
    } else if (strcmp(sub, "goto") == 0) {
        cmd_goto(argc, argv);
    } else if (strcmp(sub, "tile") == 0) {
        cmd_tile(argc, argv);
    } else {
        cmd_hold(argc, argv);
    }
    return 0;
}

const esp_console_cmd_t *ui_console_commands(size_t *count)
{
    static const esp_console_cmd_t cmds[] = {
        { .command = "ui", .help = "report the deck's state, go to a page, find a tile, or hold a press on the glass",
          .hint = "state | goto <page> | tile <id> | hold <x> <y> <ms>", .func = cmd_ui },
        { .command = "layout", .help = "report the loaded layout, reload it from storage, or switch to the built-in one",
          .hint = "state | reload | use builtin|storage", .func = ui_layout_cmd },
    };
    *count = sizeof cmds / sizeof cmds[0];
    return cmds;
}
