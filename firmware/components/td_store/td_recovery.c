#include "td_recovery.h"
#include <stdatomic.h>
#include "esp_log.h"
#include "esp_system.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "td_recovery_priv.h"
#include "td_store.h"

#define ARM_TASK_STACK 4096
#define ARM_TASK_PRIORITY 5
#define RELEASE_POLL_MS 10

static const char *TAG = "recovery";

static atomic_bool s_mode;
static atomic_bool s_arming;
static atomic_bool s_released;
static _Atomic(td_recovery_info_fn_t) s_info_fn;

bool td_recovery_boot(void)
{
    td_store_stat_t st;
    if (td_store_stat(TD_RECOVERY_MARKER_PATH, &st) != ESP_OK) {
        return false;
    }
    const esp_err_t err = td_store_remove(TD_RECOVERY_MARKER_PATH);
    if (err != ESP_OK) {
        ESP_LOGW(TAG, "marker not deleted: %s", esp_err_to_name(err));
    }
    atomic_store(&s_mode, true);
    ESP_LOGI(TAG, "mode on, files kept");
    return true;
}

bool td_recovery_mode(void)
{
    return atomic_load(&s_mode);
}

void td_recovery_leave(const char *why)
{
    if (atomic_exchange(&s_mode, false)) {
        ESP_LOGI(TAG, "mode off: %s", why);
    }
}

esp_err_t td_recovery_mark(void)
{
    static const char mark[] = "1";
    return td_store_write_atomic(TD_RECOVERY_MARKER_PATH, mark, sizeof mark - 1);
}

static void arm_task(void *arg)
{
    (void)arg;
    const esp_err_t err = td_recovery_mark();
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "marker not written, the deck restarts into its saved state: %s", esp_err_to_name(err));
    }
    while (!atomic_load(&s_released)) {
        vTaskDelay(pdMS_TO_TICKS(RELEASE_POLL_MS));
    }
    vTaskDelay(pdMS_TO_TICKS(TD_RECOVERY_RESTART_DELAY_MS));
    ESP_LOGI(TAG, "restarting");
    esp_restart();
}

void td_recovery_armed(void)
{
    ESP_LOGI(TAG, "armed");
    atomic_store(&s_released, false);
    if (atomic_exchange(&s_arming, true)) {
        return;
    }
    if (xTaskCreate(arm_task, "td_recovery", ARM_TASK_STACK, NULL, ARM_TASK_PRIORITY, NULL) != pdPASS) {
        atomic_store(&s_arming, false);
        ESP_LOGE(TAG, "arm task not started, the deck stays up");
    }
}

void td_recovery_released(void)
{
    atomic_store(&s_released, true);
}

void td_recovery_set_info_fn(td_recovery_info_fn_t fn)
{
    atomic_store(&s_info_fn, fn);
}

void td_recovery_get_info(td_recovery_info_t *out)
{
    *out = (td_recovery_info_t){ .watcher = "IDLE" };
    const td_recovery_info_fn_t fn = atomic_load(&s_info_fn);
    if (fn != NULL) {
        fn(out);
    }
}
