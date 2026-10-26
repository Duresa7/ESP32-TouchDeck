#include "td_update.h"
#include <string.h>
#include "esp_ota_ops.h"
#include "esp_partition.h"
#include "td_console.h"

static const char *img_state_name(esp_ota_img_states_t state)
{
    switch (state) {
    case ESP_OTA_IMG_NEW:
        return "NEW";
    case ESP_OTA_IMG_PENDING_VERIFY:
        return "PENDING_VERIFY";
    case ESP_OTA_IMG_VALID:
        return "VALID";
    case ESP_OTA_IMG_INVALID:
        return "INVALID";
    case ESP_OTA_IMG_ABORTED:
        return "ABORTED";
    case ESP_OTA_IMG_UNDEFINED:
        return "UNDEFINED";
    }
    return "UNKNOWN";
}

/* The record otadata keeps for a slot. NONE when it keeps none, as for a slot no update has ever written. */
static const char *slot_state(esp_partition_subtype_t subtype)
{
    const esp_partition_t *slot = esp_partition_find_first(ESP_PARTITION_TYPE_APP, subtype, NULL);
    esp_ota_img_states_t state;
    if (slot == NULL || esp_ota_get_state_partition(slot, &state) != ESP_OK) {
        return "NONE";
    }
    return img_state_name(state);
}

static const char *label_or_unknown(const esp_partition_t *partition)
{
    return partition != NULL ? partition->label : "unknown";
}

static void write_reply(td_reply_t *reply, char *buf)
{
    td_reply_end(reply);
    td_console_write_line(buf);
}

static int fail(const char *message)
{
    char buf[TD_REPLY_MAX];
    td_reply_error(buf, sizeof buf, "update", message);
    td_console_write_line(buf);
    return 1;
}

static int op_state(void)
{
    char buf[TD_REPLY_MAX];
    td_update_view_t view;
    td_update_view(&view);
    td_reply_t reply;
    td_reply_begin(&reply, buf, sizeof buf, "update", true);
    td_reply_str(&reply, "op", "state");
    td_reply_str(&reply, "running", label_or_unknown(esp_ota_get_running_partition()));
    td_reply_str(&reply, "boot", label_or_unknown(esp_ota_get_boot_partition()));
    td_reply_str(&reply, "ota_0", slot_state(ESP_PARTITION_SUBTYPE_APP_OTA_0));
    td_reply_str(&reply, "ota_1", slot_state(ESP_PARTITION_SUBTYPE_APP_OTA_1));
    td_reply_bool(&reply, "confirmed", td_update_confirmed());
    td_reply_str(&reply, "state", td_update_phase_name(view.phase));
    if (view.phase == TD_UPDATE_FAILED) {
        td_reply_str(&reply, "reason", td_update_reason_name(view.reason));
    }
    td_reply_int(&reply, "bytes", view.bytes);
    td_reply_int(&reply, "written", view.written);
    write_reply(&reply, buf);
    return 0;
}

static int op_confirm(void)
{
    if (!td_update_confirm_early()) {
        return fail("could not confirm");
    }
    char buf[TD_REPLY_MAX];
    td_reply_t reply;
    td_reply_begin(&reply, buf, sizeof buf, "update", true);
    td_reply_str(&reply, "op", "confirm");
    td_reply_str(&reply, "running", label_or_unknown(esp_ota_get_running_partition()));
    td_reply_bool(&reply, "confirmed", td_update_confirmed());
    write_reply(&reply, buf);
    return 0;
}

/* The reply goes out first, because the restart that follows a good rollback never returns. */
static int op_invalidate(void)
{
    if (!esp_ota_check_rollback_is_possible()) {
        return fail("rollback is not possible, the other slot holds no valid image");
    }
    char buf[TD_REPLY_MAX];
    td_reply_t reply;
    td_reply_begin(&reply, buf, sizeof buf, "update", true);
    td_reply_str(&reply, "op", "invalidate");
    td_reply_str(&reply, "running", label_or_unknown(esp_ota_get_running_partition()));
    write_reply(&reply, buf);
    const esp_err_t err = esp_ota_mark_app_invalid_rollback_and_reboot();
    return err == ESP_OK ? 0 : fail(esp_err_to_name(err));
}

static int cmd_update(int argc, char **argv)
{
    if (argc == 2 && strcmp(argv[1], "state") == 0) {
        return op_state();
    }
    if (argc == 2 && strcmp(argv[1], "confirm") == 0) {
        return op_confirm();
    }
    if (argc == 2 && strcmp(argv[1], "invalidate") == 0) {
        return op_invalidate();
    }
    return fail("usage: update state | confirm | invalidate");
}

const esp_console_cmd_t *td_update_console_commands(size_t *count)
{
    static const esp_console_cmd_t commands[] = {
        { .command = "update", .help = "report the firmware slots and update state", .func = cmd_update },
    };
    *count = sizeof commands / sizeof commands[0];
    return commands;
}
