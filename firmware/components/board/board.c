#include "board.h"
#include "board_pins.h"
#include "ch422g.h"
#include "driver/gpio.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "freertos/task.h"

#define I2C_XFER_TIMEOUT_MS 50

static const char *TAG = "board";

typedef struct {
    i2c_master_bus_handle_t bus;
    i2c_master_dev_handle_t set_dev;
    i2c_master_dev_handle_t io_dev;
    bool done;
    esp_err_t result;
} board_state_t;

static board_state_t s_board;

static esp_err_t chip_write(void *ctx, uint8_t addr, uint8_t value)
{
    const board_state_t *board = ctx;
    i2c_master_dev_handle_t dev;
    switch (addr) {
    case TD_CH422G_ADDR_SET:
        dev = board->set_dev;
        break;
    case TD_CH422G_ADDR_IO:
        dev = board->io_dev;
        break;
    default:
        return ESP_ERR_INVALID_ARG;
    }
    return i2c_master_transmit(dev, &value, 1, I2C_XFER_TIMEOUT_MS);
}

static void chip_delay_ms(void *ctx, uint32_t ms)
{
    (void)ctx;
    vTaskDelay(pdMS_TO_TICKS(ms));
}

static esp_err_t chip_hold_touch_int_low(void *ctx)
{
    (void)ctx;
    const gpio_config_t cfg = {
        .pin_bit_mask = 1ULL << TD_TOUCH_PIN_INT,
        .mode = GPIO_MODE_OUTPUT,
        .pull_up_en = GPIO_PULLUP_DISABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = GPIO_INTR_DISABLE,
    };
    esp_err_t err = gpio_config(&cfg);
    if (err != ESP_OK) {
        return err;
    }
    return gpio_set_level(TD_TOUCH_PIN_INT, 0);
}

static esp_err_t add_device(uint8_t addr, i2c_master_dev_handle_t *out)
{
    const i2c_device_config_t cfg = {
        .dev_addr_length = I2C_ADDR_BIT_LEN_7,
        .device_address = addr,
        .scl_speed_hz = TD_I2C_HZ,
    };
    esp_err_t err = i2c_master_bus_add_device(s_board.bus, &cfg, out);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "adding i2c device 0x%02x failed: %s", addr, esp_err_to_name(err));
    }
    return err;
}

static esp_err_t bring_up(void)
{
    const i2c_master_bus_config_t bus_cfg = {
        .i2c_port = TD_I2C_PORT,
        .sda_io_num = TD_I2C_PIN_SDA,
        .scl_io_num = TD_I2C_PIN_SCL,
        .clk_source = I2C_CLK_SRC_DEFAULT,
        .glitch_ignore_cnt = 7,
        .flags.enable_internal_pullup = true,
    };
    i2c_master_bus_handle_t bus = NULL;
    esp_err_t err = i2c_new_master_bus(&bus_cfg, &bus);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "creating the i2c bus failed: %s", esp_err_to_name(err));
        return err;
    }
    s_board.bus = bus;

    err = add_device(TD_CH422G_ADDR_SET, &s_board.set_dev);
    if (err != ESP_OK) {
        return err;
    }
    err = add_device(TD_CH422G_ADDR_IO, &s_board.io_dev);
    if (err != ESP_OK) {
        return err;
    }

    const ch422g_platform_t platform = {
        .write = chip_write,
        .delay_ms = chip_delay_ms,
        .hold_touch_int_low = chip_hold_touch_int_low,
        .ctx = &s_board,
    };
    err = ch422g_start(&platform);
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "starting the CH422G failed: %s", esp_err_to_name(err));
        return err;
    }

    ESP_LOGI(TAG, "i2c bus and CH422G ready");
    return ESP_OK;
}

/* Created on first use. The critical section keeps two first callers from creating it twice. */
static SemaphoreHandle_t init_lock(void)
{
    static StaticSemaphore_t storage;
    static SemaphoreHandle_t lock;
    static portMUX_TYPE guard = portMUX_INITIALIZER_UNLOCKED;

    portENTER_CRITICAL(&guard);
    if (lock == NULL) {
        lock = xSemaphoreCreateMutexStatic(&storage);
    }
    portEXIT_CRITICAL(&guard);
    return lock;
}

esp_err_t board_init(void)
{
    SemaphoreHandle_t lock = init_lock();
    xSemaphoreTake(lock, portMAX_DELAY);
    if (!s_board.done) {
        s_board.result = bring_up();
        s_board.done = true;
    }
    esp_err_t result = s_board.result;
    xSemaphoreGive(lock);
    return result;
}

i2c_master_bus_handle_t board_i2c_bus(void)
{
    return s_board.bus;
}
