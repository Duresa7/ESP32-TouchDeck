#include "touch.h"
#include "touch_hold.h"
#include "touch_priv.h"
#include "touch_rate.h"
#include <stdatomic.h>
#include "board.h"
#include "board_pins.h"
#include "ch422g.h"
#include "driver/gpio.h"
#include "driver/i2c_master.h"
#include "esp_check.h"
#include "esp_lcd_io_i2c.h"
#include "esp_lcd_panel_io.h"
#include "esp_lcd_touch.h"
#include "esp_lcd_touch_gt911.h"
#include "esp_log.h"
#include "esp_lv_adapter.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "lvgl.h"

#define GT911_STATUS_REG 0x814E
#define GT911_STATUS_REPORT_READY 0x80

#define I2C_TIMEOUT_MS 50
#define PROBE_ATTEMPTS 5
#define PROBE_RETRY_MS 50
#define RESET_HOLD_MS 10
#define ADDRESS_LATCH_MS 50
#define CONFIG_LOAD_MS 100
#define LOCK_TIMEOUT_MS 3000
#define INJECT_POLL_MS 1

static const char *TAG = "touch";

static struct {
    atomic_bool ready;
    atomic_bool log;
    atomic_uint reads;
    uint8_t address;
    esp_lcd_panel_io_handle_t io;
    esp_lcd_touch_handle_t tp;
} s;

/* What the console task and the LVGL task both touch. Every access takes shared.lock. */
static struct {
    portMUX_TYPE lock;
    td_touch_held_t held;
    td_touch_queue_t queue;
    td_touch_rate_t rate;
    td_touch_sample_t last;
    int64_t drained_us;
    uint32_t polls;
    uint32_t frames;
    uint32_t pressed_frames;
    uint32_t errors;
    uint32_t injected;
    uint32_t rejected;
} shared = { .lock = portMUX_INITIALIZER_UNLOCKED };

static uint16_t clamp(uint16_t v, uint16_t size)
{
    return v < size ? v : size - 1;
}

/* One status read tells a fresh report from a poll that found nothing new. The driver would read the same byte
 * and clear it, so a poll with no report skips the driver and costs one transfer. */
static td_touch_poll_t poll_controller(td_touch_sample_t *report)
{
    uint8_t status = 0;
    if (esp_lcd_panel_io_rx_param(s.io, GT911_STATUS_REG, &status, 1) != ESP_OK) {
        return TD_TOUCH_POLL_ERROR;
    }
    if ((status & GT911_STATUS_REPORT_READY) == 0) {
        return TD_TOUCH_POLL_IDLE;
    }

    esp_lcd_touch_point_data_t point;
    uint8_t count = 0;
    if (esp_lcd_touch_read_data(s.tp) != ESP_OK || esp_lcd_touch_get_data(s.tp, &point, &count, 1) != ESP_OK) {
        return TD_TOUCH_POLL_ERROR;
    }
    *report = (td_touch_sample_t){ .x = clamp(point.x, TD_LCD_H_RES), .y = clamp(point.y, TD_LCD_V_RES),
                                   .pressed = count > 0 };
    return TD_TOUCH_POLL_REPORT;
}

static void read_cb(lv_indev_t *indev, lv_indev_data_t *data)
{
    (void)indev;
    atomic_fetch_add(&s.reads, 1);

    td_touch_sample_t report = { 0 };
    const td_touch_poll_t poll = poll_controller(&report);
    const int64_t now = esp_timer_get_time();

    bool injected;
    td_touch_sample_t out;
    td_touch_sample_t held;
    portENTER_CRITICAL(&shared.lock);
    td_touch_held_poll(&shared.held, poll, report);
    shared.polls++;
    if (poll == TD_TOUCH_POLL_REPORT) {
        shared.frames++;
        shared.pressed_frames += report.pressed;
        td_touch_rate_frame(&shared.rate, report.pressed, now);
    } else if (poll == TD_TOUCH_POLL_ERROR) {
        shared.errors++;
    }
    held = shared.held.sample;
    out = td_touch_next(&shared.queue, held, &injected);
    if (injected) {
        shared.injected++;
        if (td_touch_queue_count(&shared.queue) == 0) {
            shared.drained_us = now;
        }
    }
    shared.last = out;
    const bool release_follows = injected && out.pressed && td_touch_release_follows(&shared.queue);
    portEXIT_CRITICAL(&shared.lock);

    if (poll == TD_TOUCH_POLL_REPORT && atomic_load(&s.log)) {
        ESP_LOGI(TAG, "x=%u y=%u pressed=%d", held.x, held.y, held.pressed);
    }

    data->point.x = out.x;
    data->point.y = out.y;
    data->state = out.pressed ? LV_INDEV_STATE_PRESSED : LV_INDEV_STATE_RELEASED;
    data->continue_reading = release_follows;
}

static const char *direction_name(lv_dir_t dir)
{
    switch (dir) {
    case LV_DIR_LEFT:
        return "left";
    case LV_DIR_RIGHT:
        return "right";
    case LV_DIR_TOP:
        return "up";
    case LV_DIR_BOTTOM:
        return "down";
    default:
        return "other";
    }
}

static void on_gesture(lv_event_t *e)
{
    if (atomic_load(&s.log)) {
        ESP_LOGI(TAG, "gesture %s", direction_name(lv_indev_get_gesture_dir(lv_event_get_target(e))));
    }
}

/* GPIO4 is both the GT911 interrupt line and its address select. Holding it low while TP_RST releases picks 0x5D.
 * It floats afterwards, because the controller drives that line itself once it runs and polled mode never uses it. */
static esp_err_t reset_to_address_5d(void)
{
    const gpio_config_t low = {
        .pin_bit_mask = 1ULL << TD_TOUCH_PIN_INT,
        .mode = GPIO_MODE_OUTPUT,
    };
    ESP_RETURN_ON_ERROR(gpio_config(&low), TAG, "int pin as output");
    ESP_RETURN_ON_ERROR(gpio_set_level(TD_TOUCH_PIN_INT, 0), TAG, "int pin low");
    ESP_RETURN_ON_ERROR(ch422g_touch_reset(true), TAG, "assert touch reset");
    vTaskDelay(pdMS_TO_TICKS(RESET_HOLD_MS));
    ESP_RETURN_ON_ERROR(ch422g_touch_reset(false), TAG, "release touch reset");
    vTaskDelay(pdMS_TO_TICKS(ADDRESS_LATCH_MS));

    const gpio_config_t floating = {
        .pin_bit_mask = 1ULL << TD_TOUCH_PIN_INT,
        .mode = GPIO_MODE_INPUT,
    };
    ESP_RETURN_ON_ERROR(gpio_config(&floating), TAG, "int pin as input");
    vTaskDelay(pdMS_TO_TICKS(CONFIG_LOAD_MS));
    return ESP_OK;
}

static esp_err_t find_address(i2c_master_bus_handle_t bus, uint8_t *address)
{
    static const uint8_t candidates[] = { ESP_LCD_TOUCH_IO_I2C_GT911_ADDRESS,
                                          ESP_LCD_TOUCH_IO_I2C_GT911_ADDRESS_BACKUP };
    for (int attempt = 0; attempt < PROBE_ATTEMPTS; attempt++) {
        for (size_t i = 0; i < sizeof candidates; i++) {
            if (i2c_master_probe(bus, candidates[i], I2C_TIMEOUT_MS) == ESP_OK) {
                *address = candidates[i];
                return ESP_OK;
            }
        }
        vTaskDelay(pdMS_TO_TICKS(PROBE_RETRY_MS));
    }
    return ESP_ERR_NOT_FOUND;
}

static esp_err_t start_driver(i2c_master_bus_handle_t bus, uint8_t address)
{
    esp_lcd_panel_io_i2c_config_t io_config = ESP_LCD_TOUCH_IO_I2C_GT911_CONFIG();
    io_config.dev_addr = address;
    io_config.scl_speed_hz = TD_I2C_HZ;
    io_config.transaction_timeout_ms = I2C_TIMEOUT_MS;
    ESP_RETURN_ON_ERROR(esp_lcd_new_panel_io_i2c(bus, &io_config, &s.io), TAG, "panel io");

    /* Given both pins the driver runs its own reset and address select, which would fight the CH422G reset. */
    const esp_lcd_touch_config_t config = {
        .x_max = TD_LCD_H_RES,
        .y_max = TD_LCD_V_RES,
        .rst_gpio_num = GPIO_NUM_NC,
        .int_gpio_num = GPIO_NUM_NC,
    };
    return esp_lcd_touch_new_i2c_gt911(s.io, &config, &s.tp);
}

static esp_err_t register_pointer_device(void)
{
    ESP_RETURN_ON_ERROR(esp_lv_adapter_lock(LOCK_TIMEOUT_MS), TAG, "adapter lock");
    esp_err_t err = ESP_ERR_INVALID_STATE;
    lv_display_t *disp = lv_display_get_default();
    if (disp != NULL) {
        lv_indev_t *indev = lv_indev_create();
        err = indev != NULL ? ESP_OK : ESP_ERR_NO_MEM;
        if (indev != NULL) {
            lv_indev_set_type(indev, LV_INDEV_TYPE_POINTER);
            lv_indev_set_read_cb(indev, read_cb);
            lv_indev_set_display(indev, disp);
            lv_timer_set_period(lv_indev_get_read_timer(indev), TD_TOUCH_STEP_MS);
            lv_indev_add_event_cb(indev, on_gesture, LV_EVENT_GESTURE, NULL);
        }
    }
    esp_lv_adapter_unlock();
    return err;
}

esp_err_t touch_start(void)
{
    if (atomic_load(&s.ready)) {
        return ESP_OK;
    }
    ESP_RETURN_ON_ERROR(board_init(), TAG, "board");
    i2c_master_bus_handle_t bus = board_i2c_bus();
    ESP_RETURN_ON_FALSE(bus != NULL, ESP_ERR_INVALID_STATE, TAG, "no i2c bus");

    ESP_RETURN_ON_ERROR(reset_to_address_5d(), TAG, "reset");
    esp_err_t err = find_address(bus, &s.address);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "GT911 did not answer at 0x%02X or 0x%02X", ESP_LCD_TOUCH_IO_I2C_GT911_ADDRESS,
                 ESP_LCD_TOUCH_IO_I2C_GT911_ADDRESS_BACKUP);
        return err;
    }
    ESP_LOGI(TAG, "GT911 answered at 0x%02X", s.address);

    ESP_RETURN_ON_ERROR(start_driver(bus, s.address), TAG, "driver");
    ESP_RETURN_ON_ERROR(register_pointer_device(), TAG, "pointer device");
    atomic_store(&s.ready, true);
    return ESP_OK;
}

bool touch_ready(void)
{
    return atomic_load(&s.ready);
}

esp_err_t touch_inject(const td_touch_sample_t *samples, size_t n, uint32_t timeout_ms, uint32_t *deliver_us)
{
    if (n == 0) {
        return ESP_ERR_INVALID_ARG;
    }
    const int64_t start = esp_timer_get_time();
    esp_err_t err = ESP_OK;
    portENTER_CRITICAL(&shared.lock);
    if (shared.held.sample.pressed) {
        err = ESP_ERR_INVALID_STATE;
    } else if (!td_touch_queue_push(&shared.queue, samples, n)) {
        shared.rejected++;
        err = ESP_ERR_NO_MEM;
    } else {
        shared.drained_us = 0;
    }
    portEXIT_CRITICAL(&shared.lock);
    if (err != ESP_OK) {
        return err;
    }

    /* The last pop happens inside a read, before LVGL acts on it, so the effect has landed only once a later read
     * has begun. */
    bool drained = false;
    unsigned landed_after = 0;
    while (esp_timer_get_time() - start < (int64_t)timeout_ms * 1000) {
        portENTER_CRITICAL(&shared.lock);
        const size_t waiting = td_touch_queue_count(&shared.queue);
        const int64_t drained_us = shared.drained_us;
        portEXIT_CRITICAL(&shared.lock);

        if (waiting == 0) {
            const unsigned reads = atomic_load(&s.reads);
            if (!drained) {
                drained = true;
                landed_after = reads + 1;
            } else if (reads >= landed_after) {
                *deliver_us = (uint32_t)(drained_us - start);
                return ESP_OK;
            }
        }
        vTaskDelay(pdMS_TO_TICKS(INJECT_POLL_MS));
    }
    return ESP_ERR_TIMEOUT;
}

void touch_get_stats(touch_stats_t *out)
{
    portENTER_CRITICAL(&shared.lock);
    *out = (touch_stats_t){
        .address = s.address,
        .polls = shared.polls,
        .frames = shared.frames,
        .pressed_frames = shared.pressed_frames,
        .errors = shared.errors,
        .injected = shared.injected,
        .rejected = shared.rejected,
        .queued = (uint32_t)td_touch_queue_count(&shared.queue),
        .last = shared.last,
        .contact_frames = shared.rate.frames,
        .contact_ms = td_touch_rate_span_ms(&shared.rate),
        .contact_hz = td_touch_rate_hz(&shared.rate),
    };
    portEXIT_CRITICAL(&shared.lock);
}

void touch_set_log(bool on)
{
    atomic_store(&s.log, on);
}
