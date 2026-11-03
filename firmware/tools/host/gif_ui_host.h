#pragma once
#include "td_layout.h"
#include "td_asset.h"
#include "ui_gif_player.h"
#include "esp_heap_caps.h"
#include <stdatomic.h>
#ifdef _WIN32
#include <windows.h>
typedef SRWLOCK portMUX_TYPE;
#define portMUX_INITIALIZER_UNLOCKED SRWLOCK_INIT
#else
#include <pthread.h>
typedef pthread_mutex_t portMUX_TYPE;
#define portMUX_INITIALIZER_UNLOCKED PTHREAD_MUTEX_INITIALIZER
#endif
void host_lock(portMUX_TYPE *lock);
void host_unlock(portMUX_TYPE *lock);
#define portENTER_CRITICAL host_lock
#define portEXIT_CRITICAL host_unlock
#define CONFIG_TD_GIF_CAPS 1
#define ESP_OK 0
#define pdPASS 1
#define pdTRUE 1
#define portMAX_DELAY 0xffffffffu
#define pdMS_TO_TICKS(ms) ((TickType_t)(ms))
#define LV_IMAGE_HEADER_MAGIC 0x19
#define LV_COLOR_FORMAT_RGB565A8 1
typedef void *TaskHandle_t;
typedef unsigned TickType_t;
typedef struct {
    struct { unsigned magic, cf, w, h, stride; } header;
    size_t data_size;
    const uint8_t *data;
} lv_image_dsc_t;
typedef struct { _Atomic(const uint8_t *) pixels; } lv_obj_t;
typedef struct lv_timer lv_timer_t;
typedef struct { unsigned state; bool idle_face; } power_info_t;
#define POWER_OFF 1
int64_t esp_timer_get_time(void);
void vTaskDelay(TickType_t ticks);
unsigned ulTaskNotifyTake(int clear, TickType_t timeout);
void xTaskNotifyGive(TaskHandle_t task);
int xTaskCreatePinnedToCore(void (*fn)(void *), const char *name, unsigned stack, void *ctx,
                          unsigned priority, TaskHandle_t *task, unsigned core);
int esp_lv_adapter_lock(int timeout_ms);
void esp_lv_adapter_unlock(void);
lv_obj_t *lv_image_create(lv_obj_t *parent);
void lv_image_set_src(lv_obj_t *obj, const lv_image_dsc_t *src);
void lv_image_cache_drop(const void *src);
void lv_image_header_cache_drop(const void *src);
void lv_obj_center(lv_obj_t *obj);
void lv_obj_set_clickable(lv_obj_t *obj, bool value);
lv_timer_t *lv_timer_create(void (*callback)(lv_timer_t *), unsigned period, void *ctx);
int power_get_info(power_info_t *info, int64_t now);
void ui_gif_prepare(const td_layout_t *layout);
void ui_gif_discard(void);
void ui_gif_begin(void);
bool ui_gif_build(lv_obj_t *obj, const ui_tile_t *tile, uint8_t page);
void ui_gif_resume(void);
void ui_gif_page(uint8_t page, bool sliding);
void ui_gif_set_dimmed(bool dimmed);
bool ui_layout_reload(bool use_builtin, td_layout_error_t *err);
td_layout_t *host_stored_layout(void);
void host_load_locked(const td_layout_t *layout);
