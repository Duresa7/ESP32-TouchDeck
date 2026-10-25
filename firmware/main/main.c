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

static const char *TAG = "touchdeck";

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
    esp_err_t register_err = td_console_register(display_cmds, display_cmd_count);
    if (register_err != ESP_OK) {
        ESP_LOGE(TAG, "Display commands not registered: %s", esp_err_to_name(register_err));
    }

    esp_err_t console_err = td_console_start();
    if (console_err != ESP_OK) {
        ESP_LOGE(TAG, "Console failed to start: %s", esp_err_to_name(console_err));
    }

    esp_err_t display_err = display_start();
    if (display_err != ESP_OK) {
        ESP_LOGE(TAG, "Display failed to start: %s", esp_err_to_name(display_err));
    }

    int ticks = 0;
    while (true) {
        ESP_LOGI(TAG, "alive %d, free heap %" PRIu32 " bytes",
                 ticks++, esp_get_free_heap_size());
        vTaskDelay(pdMS_TO_TICKS(2000));
    }
}
