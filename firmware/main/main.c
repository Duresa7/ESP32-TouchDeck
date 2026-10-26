#include <stdio.h>
#include <inttypes.h>
#include "sdkconfig.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "cJSON.h"
#include "esp_chip_info.h"
#include "esp_heap_caps.h"
#include "esp_flash.h"
#include "esp_ota_ops.h"
#include "esp_psram.h"
#include "esp_system.h"
#include "esp_log.h"
#include "esp_timer.h"
#include "td_console.h"
#include "td_link.h"
#include "display.h"
#include "touch.h"
#include "ui.h"
#include "key_sink_log.h"
#include "keys.h"
#include "power.h"
#include "td_store.h"

static const char *TAG = "touchdeck";
static const char *SETTINGS_TAG = "settings";

static key_sink_log_t s_key_log;

static void register_commands(const char *owner, const esp_console_cmd_t *cmds, size_t count)
{
    esp_err_t err = td_console_register(cmds, count);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "%s commands not registered: %s", owner, esp_err_to_name(err));
    }
}

/* A layout of up to 32 KB parses into thousands of small cJSON nodes. Below 16 KB malloc prefers internal RAM, which
 * the display and LVGL need, so cJSON takes PSRAM first and internal RAM only when PSRAM is full. */
static void *cjson_malloc(size_t size)
{
    return heap_caps_malloc_prefer(size, 2, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT, MALLOC_CAP_8BIT);
}

void app_main(void)
{
    cJSON_InitHooks(&(cJSON_Hooks){ .malloc_fn = cjson_malloc, .free_fn = heap_caps_free });

    esp_chip_info_t chip_info;
    esp_chip_info(&chip_info);

    uint32_t flash_size = 0;
    esp_flash_get_size(NULL, &flash_size);

    ESP_LOGI(TAG, "TouchDeck firmware starting");
    ESP_LOGI(TAG, "Chip: %s, %d core(s), revision v%d.%d",
             CONFIG_IDF_TARGET, chip_info.cores,
             chip_info.revision / 100, chip_info.revision % 100);
    ESP_LOGI(TAG, "Flash: %" PRIu32 " MB", flash_size / (1024 * 1024));
    // The display frame buffers will need PSRAM, so confirm it came up.
    ESP_LOGI(TAG, "PSRAM: %u KB", (unsigned)(esp_psram_get_size() / 1024));

    const esp_partition_t *running = esp_ota_get_running_partition();
    if (running) {
        ESP_LOGI(TAG, "Running partition %s at 0x%" PRIx32,
                 running->label, running->address);
    } else {
        ESP_LOGW(TAG, "Running partition not found in the partition table");
    }

    /* td_console_start seals the command list, so every component hands over its commands first. */
    size_t display_cmd_count;
    const esp_console_cmd_t *display_cmds = display_console_commands(&display_cmd_count);
    register_commands("Display", display_cmds, display_cmd_count);
    size_t ui_cmd_count;
    const esp_console_cmd_t *ui_cmds = ui_console_commands(&ui_cmd_count);
    register_commands("UI", ui_cmds, ui_cmd_count);
    size_t touch_cmd_count;
    const esp_console_cmd_t *touch_cmds = touch_console_commands(&touch_cmd_count);
    register_commands("Touch", touch_cmds, touch_cmd_count);
    size_t keys_cmd_count;
    const esp_console_cmd_t *keys_cmds = keys_console_commands(&s_key_log, &keys_cmd_count);
    register_commands("Keys", keys_cmds, keys_cmd_count);
    size_t power_cmd_count;
    const esp_console_cmd_t *power_cmds = power_console_commands(&power_cmd_count);
    register_commands("Power", power_cmds, power_cmd_count);
    size_t link_cmd_count;
    const esp_console_cmd_t *link_cmds = td_link_console_commands(&link_cmd_count);
    register_commands("Link", link_cmds, link_cmd_count);
    size_t store_cmd_count;
    const esp_console_cmd_t *store_cmds = td_store_console_commands(&store_cmd_count);
    register_commands("Store", store_cmds, store_cmd_count);

    /* The volume mounts before the console starts, and the settings load before the display does. Orientation is read
     * and reported here and applied when the deck builds in the orientation milestone. */
    td_settings_t settings;
    td_settings_defaults(&settings);
    esp_err_t store_err = td_store_mount(NULL);
    if (store_err == ESP_OK) {
        const esp_err_t settings_err = td_store_settings_load(&settings);
        if (settings_err == ESP_ERR_INVALID_RESPONSE) {
            ESP_LOGW(SETTINGS_TAG, "settings.json is not usable, using the defaults for what it does not give");
        }
    } else {
        ESP_LOGE(TAG, "Store failed to mount, the settings are the defaults: %s", esp_err_to_name(store_err));
    }
    ESP_LOGI(SETTINGS_TAG, "idle_timeout_s=%u orientation=%s", (unsigned)settings.idle_timeout_s,
             td_settings_orientation_name(settings.orientation));

    /* The engine exists before the console can run a keys command. Its only sink here is the console key log.
     * The USB HID sink plugs in behind the same key_sink_t in the post-product update. */
    key_sink_log_start(&s_key_log);
    esp_err_t keys_err = keys_start(key_sink_log_sink(&s_key_log));
    if (keys_err != ESP_OK) {
        ESP_LOGE(TAG, "Keys failed to start: %s", esp_err_to_name(keys_err));
    }

    esp_err_t link_err = td_link_start();
    if (link_err != ESP_OK) {
        ESP_LOGE(TAG, "Link failed to start: %s", esp_err_to_name(link_err));
    }

    esp_err_t console_err = td_console_start();
    if (console_err != ESP_OK) {
        ESP_LOGE(TAG, "Console failed to start: %s", esp_err_to_name(console_err));
    }

    ui_layout_boot(td_settings_orientation_name(settings.orientation));
    esp_err_t display_err = display_start(ui_build);
    if (display_err != ESP_OK) {
        ESP_LOGE(TAG, "Display failed to start: %s", esp_err_to_name(display_err));
    }

    /* The gate needs the CH422G that display_start brought up, and it must exist before touch reads start. */
    esp_err_t power_err = power_start(esp_timer_get_time());
    if (power_err != ESP_OK) {
        ESP_LOGE(TAG, "Power failed to start, the screen stays on: %s", esp_err_to_name(power_err));
    } else {
        power_set_idle_timeout_s(settings.idle_timeout_s, esp_timer_get_time());
    }

    esp_err_t touch_err = touch_start();
    if (touch_err != ESP_OK) {
        ESP_LOGE(TAG, "Touch failed to start: %s", esp_err_to_name(touch_err));
    }

    int ticks = 0;
    while (true) {
        ESP_LOGI(TAG, "alive %d, free heap %" PRIu32 " bytes",
                 ticks++, esp_get_free_heap_size());
        vTaskDelay(pdMS_TO_TICKS(2000));
    }
}
