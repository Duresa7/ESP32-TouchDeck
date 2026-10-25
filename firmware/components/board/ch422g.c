#include "ch422g.h"
#include <stddef.h>
#include "board_pins.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"

#define CH422G_SYS_IO_OUTPUT_ENABLE 0x01

#define IO_LCD_ON (TD_CH422G_IO_DI1 | TD_CH422G_IO_LCD_RST | TD_CH422G_IO_DISP)
#define IO_TOUCH_RELEASED (IO_LCD_ON | TD_CH422G_IO_TP_RST)
#define IO_RUNNING (TD_CH422G_IO_SD_CS | TD_CH422G_IO_LCD_RST | TD_CH422G_IO_DISP | TD_CH422G_IO_TP_RST)

_Static_assert(IO_LCD_ON == 0x2C, "waveshare demo: LCD out of reset, backlight on, touch held in reset");
_Static_assert(IO_TOUCH_RELEASED == 0x2E, "waveshare demo: touch reset released");
_Static_assert(IO_RUNNING == 0x1E, "waveshare demo: final running state");

typedef enum {
    STEP_WRITE_SET,
    STEP_WRITE_IO,
    STEP_DELAY,
    STEP_HOLD_TOUCH_INT_LOW,
} step_kind_t;

typedef struct {
    step_kind_t kind;
    uint32_t arg;
} step_t;

static const step_t k_start_steps[] = {
    {STEP_WRITE_SET, CH422G_SYS_IO_OUTPUT_ENABLE},
    {STEP_WRITE_IO, IO_LCD_ON},
    {STEP_DELAY, 100},
    {STEP_HOLD_TOUCH_INT_LOW, 0},
    {STEP_DELAY, 100},
    {STEP_WRITE_IO, IO_TOUCH_RELEASED},
    {STEP_DELAY, 200},
    {STEP_WRITE_IO, IO_RUNNING},
};

static struct {
    ch422g_platform_t platform;
    uint8_t output;
    bool started;
} s_chip;

/* The setters read-modify-write the cached byte from several tasks, so every access takes this lock.
 * It is created on first use, and the critical section keeps two first users from creating it twice. */
static SemaphoreHandle_t chip_lock(void)
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

static esp_err_t write_io(uint8_t value)
{
    esp_err_t err = s_chip.platform.write(s_chip.platform.ctx, TD_CH422G_ADDR_IO, value);
    if (err == ESP_OK) {
        s_chip.output = value;
    }
    return err;
}

static esp_err_t run_step(const step_t *step)
{
    const ch422g_platform_t *p = &s_chip.platform;
    switch (step->kind) {
    case STEP_WRITE_SET:
        return p->write(p->ctx, TD_CH422G_ADDR_SET, (uint8_t)step->arg);
    case STEP_WRITE_IO:
        return write_io((uint8_t)step->arg);
    case STEP_DELAY:
        p->delay_ms(p->ctx, step->arg);
        return ESP_OK;
    case STEP_HOLD_TOUCH_INT_LOW:
        return p->hold_touch_int_low(p->ctx);
    }
    return ESP_ERR_INVALID_ARG;
}

esp_err_t ch422g_start(const ch422g_platform_t *platform)
{
    if (platform == NULL || platform->write == NULL || platform->delay_ms == NULL ||
        platform->hold_touch_int_low == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    SemaphoreHandle_t lock = chip_lock();
    xSemaphoreTake(lock, portMAX_DELAY);

    s_chip.started = false;
    s_chip.platform = *platform;
    s_chip.output = 0;

    esp_err_t err = ESP_OK;
    for (size_t i = 0; i < sizeof k_start_steps / sizeof k_start_steps[0] && err == ESP_OK; i++) {
        err = run_step(&k_start_steps[i]);
    }
    s_chip.started = (err == ESP_OK);

    xSemaphoreGive(lock);
    return err;
}

static esp_err_t update_output(uint8_t mask, bool set)
{
    SemaphoreHandle_t lock = chip_lock();
    xSemaphoreTake(lock, portMAX_DELAY);

    esp_err_t err = ESP_ERR_INVALID_STATE;
    if (s_chip.started) {
        err = write_io((uint8_t)((s_chip.output & ~mask) | (set ? mask : 0)));
    }

    xSemaphoreGive(lock);
    return err;
}

esp_err_t ch422g_set_backlight(bool on)
{
    return update_output(TD_CH422G_IO_DISP, on);
}

esp_err_t ch422g_lcd_reset(bool asserted)
{
    return update_output(TD_CH422G_IO_LCD_RST, !asserted);
}

esp_err_t ch422g_touch_reset(bool asserted)
{
    return update_output(TD_CH422G_IO_TP_RST, !asserted);
}

uint8_t ch422g_output(void)
{
    return s_chip.output;
}
