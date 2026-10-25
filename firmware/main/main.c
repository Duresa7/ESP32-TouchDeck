#include <stdio.h>
#include <inttypes.h>
#include "sdkconfig.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "esp_chip_info.h"
#include "esp_flash.h"
#include "esp_ota_ops.h"
#include "esp_psram.h"
#include "esp_system.h"
#include "esp_log.h"
#include "td_console.h"
#include "display.h"
#include "touch.h"
#include "key_sink_log.h"
#include "keys.h"

static const char *TAG = "touchdeck";

static key_sink_log_t s_key_log;

static void register_commands(const char *owner, const esp_console_cmd_t *cmds, size_t count)
{
    esp_err_t err = td_console_register(cmds, count);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "%s commands not registered: %s", owner, esp_err_to_name(err));
    }
}

void app_main(void)
{
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
    size_t touch_cmd_count;
    const esp_console_cmd_t *touch_cmds = touch_console_commands(&touch_cmd_count);
    register_commands("Touch", touch_cmds, touch_cmd_count);
    size_t keys_cmd_count;
    const esp_console_cmd_t *keys_cmds = keys_console_commands(&s_key_log, &keys_cmd_count);
    register_commands("Keys", keys_cmds, keys_cmd_count);

    /* The engine exists before the console can run a keys command. Its only sink here is the console key log.
     * The USB HID sink plugs in behind the same key_sink_t in the post-product update. */
    key_sink_log_start(&s_key_log);
    esp_err_t keys_err = keys_start(key_sink_log_sink(&s_key_log));
    if (keys_err != ESP_OK) {
        ESP_LOGE(TAG, "Keys failed to start: %s", esp_err_to_name(keys_err));
    }

    esp_err_t console_err = td_console_start();
    if (console_err != ESP_OK) {
        ESP_LOGE(TAG, "Console failed to start: %s", esp_err_to_name(console_err));
    }

    esp_err_t display_err = display_start();
    if (display_err != ESP_OK) {
        ESP_LOGE(TAG, "Display failed to start: %s", esp_err_to_name(display_err));
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
