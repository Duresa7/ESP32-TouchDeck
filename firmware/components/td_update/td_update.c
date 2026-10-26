#include "td_update.h"
#include <string.h>
#include "esp_app_desc.h"
#include "esp_log.h"
#include "esp_ota_ops.h"
#include "esp_system.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define RESTART_DELAY_MS 250
#define CONFIRM_POLL_MS 250
#define CONFIRM_RETRY_MS 2000
#define CONFIRM_ATTEMPTS 5

static const char *TAG = "update";

/* Only the link task changes the machine, so it needs no lock. The console reads the small `view` copy, which
 * s_view_lock guards. */
static td_update_state_t s_u;
static const esp_partition_t *s_target;
static td_update_view_t s_view;
static portMUX_TYPE s_view_lock = portMUX_INITIALIZER_UNLOCKED;
static volatile bool s_confirmed;
static td_update_ready_fn s_first_frame;
static td_update_ready_fn s_layout_loaded;
static esp_timer_handle_t s_restart_timer;

static void publish(void)
{
    portENTER_CRITICAL(&s_view_lock);
    s_view = (td_update_view_t){ .phase = s_u.phase, .reason = s_u.reason, .bytes = s_u.bytes, .written = s_u.written };
    portEXIT_CRITICAL(&s_view_lock);
}

void td_update_view(td_update_view_t *out)
{
    portENTER_CRITICAL(&s_view_lock);
    *out = s_view;
    portEXIT_CRITICAL(&s_view_lock);
}

bool td_update_confirmed(void)
{
    return s_confirmed;
}

static const char *running_label(void)
{
    const esp_partition_t *running = esp_ota_get_running_partition();
    return running != NULL ? running->label : "unknown";
}

/* True while the bootloader waits for this image to confirm itself. A slot with no record, such as one flashed over
 * USB, or any other state is not waiting. */
static bool pending_verify(void)
{
    const esp_partition_t *running = esp_ota_get_running_partition();
    esp_ota_img_states_t state;
    return running != NULL && esp_ota_get_state_partition(running, &state) == ESP_OK
           && state == ESP_OTA_IMG_PENDING_VERIFY;
}

/* Confirms the running image. False when the bootloader's record would not take the mark. */
bool td_update_confirm_early(void)
{
    if (!pending_verify()) {
        s_confirmed = true;
        return true;
    }
    const esp_err_t err = esp_ota_mark_app_valid_cancel_rollback();
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "could not confirm %s: %s", running_label(), esp_err_to_name(err));
        return false;
    }
    s_confirmed = true;
    ESP_LOGI(TAG, "confirmed %s", running_label());
    return true;
}

static void confirm_task(void *arg)
{
    (void)arg;
    int attempts = 0;
    while (!s_confirmed) {
        vTaskDelay(pdMS_TO_TICKS(CONFIRM_POLL_MS));
        const td_update_confirm_inputs_t inputs = {
            .first_frame = s_first_frame != NULL && s_first_frame(),
            .layout_loaded = s_layout_loaded != NULL && s_layout_loaded(),
            .uptime_ms = (uint32_t)(esp_timer_get_time() / 1000),
        };
        if (!td_update_confirm_ready(&inputs) || s_confirmed) {
            continue;
        }
        if (!td_update_confirm_early() && ++attempts >= CONFIRM_ATTEMPTS) {
            break;
        }
        vTaskDelay(pdMS_TO_TICKS(CONFIRM_RETRY_MS));
    }
    vTaskDelete(NULL);
}

esp_err_t td_update_start(td_update_ready_fn first_frame, td_update_ready_fn layout_loaded)
{
    s_first_frame = first_frame;
    s_layout_loaded = layout_loaded;
    td_update_state_init(&s_u);
    publish();

    if (!pending_verify()) {
        s_confirmed = true;
        ESP_LOGI(TAG, "running %s, nothing to confirm", running_label());
        return ESP_OK;
    }
    s_confirmed = false;
    ESP_LOGI(TAG, "running %s, awaiting confirm", running_label());
    if (xTaskCreate(confirm_task, "td_confirm", 3072, NULL, 1, NULL) != pdPASS) {
        /* Without the watch the image never confirms, and the next reset rolls it back. That is the safe side. */
        ESP_LOGE(TAG, "confirm task did not start");
        return ESP_ERR_NO_MEM;
    }
    return ESP_OK;
}

/* Fails the update and gives the flash handle back, when there is one to give back. */
static void fail(td_update_reason_t reason, uint32_t handle)
{
    td_update_state_fail(&s_u, reason);
    if (handle != 0) {
        esp_ota_abort(handle);
    }
    s_target = NULL;
    publish();
}

td_update_reason_t td_update_begin(uint32_t bytes, const char *sha256_hex, const char **slot)
{
    const esp_partition_t *target = esp_ota_get_next_update_partition(NULL);
    if (target == NULL) {
        return TD_UPDATE_E_FLASH;
    }
    const td_update_reason_t reason = td_update_state_begin(&s_u, bytes, sha256_hex, target->size, s_confirmed);
    if (reason != TD_UPDATE_OK) {
        return reason;
    }
    esp_ota_handle_t handle;
    const esp_err_t err = esp_ota_begin(target, OTA_WITH_SEQUENTIAL_WRITES, &handle);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "begin %s failed: %s", target->label, esp_err_to_name(err));
        fail(TD_UPDATE_E_FLASH, 0);
        return TD_UPDATE_E_FLASH;
    }
    td_update_state_started(&s_u, (uint32_t)handle);
    s_target = target;
    publish();
    ESP_LOGI(TAG, "receiving %u bytes into %s", (unsigned)bytes, target->label);
    *slot = target->label;
    return TD_UPDATE_OK;
}

bool td_update_write(const uint8_t *data, size_t len)
{
    const uint32_t handle = s_u.handle;
    const td_update_reason_t reason = td_update_state_chunk(&s_u, data, len);
    if (reason != TD_UPDATE_OK) {
        if (reason == TD_UPDATE_E_RANGE) {
            fail(reason, handle);
        }
        return false;
    }
    const esp_err_t err = esp_ota_write((esp_ota_handle_t)handle, data, len);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "write failed at %u: %s", (unsigned)s_u.written, esp_err_to_name(err));
        fail(TD_UPDATE_E_FLASH, handle);
        return false;
    }
    publish();
    return true;
}

static void restart_now(void *arg)
{
    (void)arg;
    esp_restart();
}

static void schedule_restart(void)
{
    const esp_timer_create_args_t args = { .callback = restart_now, .name = "td_update_restart" };
    if (esp_timer_create(&args, &s_restart_timer) != ESP_OK
        || esp_timer_start_once(s_restart_timer, (uint64_t)RESTART_DELAY_MS * 1000) != ESP_OK) {
        ESP_LOGE(TAG, "restart timer did not start, restarting now");
        esp_restart();
    }
}

td_update_reason_t td_update_end(const char **slot)
{
    if (s_u.phase == TD_UPDATE_FAILED) {
        return s_u.reason;
    }
    const uint32_t handle = s_u.handle;
    const td_update_reason_t reason = td_update_state_finish(&s_u);
    if (reason != TD_UPDATE_OK) {
        if (s_u.phase == TD_UPDATE_FAILED) {
            fail(reason, handle);
        }
        return reason;
    }
    publish();

    /* esp_ota_end checks the image's own header, segments and checksum. The handle is spent either way. */
    const esp_partition_t *target = s_target;
    esp_err_t err = esp_ota_end((esp_ota_handle_t)handle);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "image check failed: %s", esp_err_to_name(err));
        fail(TD_UPDATE_E_FLASH, 0);
        return TD_UPDATE_E_FLASH;
    }
    /* Another project's image would boot, never confirm, and leave the deck dead until a cable flash. */
    esp_app_desc_t desc;
    const esp_app_desc_t *mine = esp_app_get_description();
    if (esp_ota_get_partition_description(target, &desc) != ESP_OK
        || strncmp(desc.project_name, mine->project_name, sizeof desc.project_name) != 0) {
        ESP_LOGE(TAG, "image is for project \"%.31s\", not \"%.31s\"", desc.project_name, mine->project_name);
        fail(TD_UPDATE_E_PROJECT, 0);
        return TD_UPDATE_E_PROJECT;
    }
    err = esp_ota_set_boot_partition(target);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "set boot %s failed: %s", target->label, esp_err_to_name(err));
        fail(TD_UPDATE_E_FLASH, 0);
        return TD_UPDATE_E_FLASH;
    }
    td_update_state_rebooting(&s_u);
    publish();
    ESP_LOGI(TAG, "boot slot is %s, restarting in %d ms", target->label, RESTART_DELAY_MS);
    *slot = target->label;
    schedule_restart();
    return TD_UPDATE_OK;
}

void td_update_cancel(void)
{
    if (s_u.phase != TD_UPDATE_RECEIVING) {
        return;
    }
    const uint32_t handle = s_u.handle;
    td_update_state_abort(&s_u);
    if (handle != 0) {
        esp_ota_abort((esp_ota_handle_t)handle);
    }
    s_target = NULL;
    publish();
    ESP_LOGW(TAG, "transfer cut, update dropped");
}
