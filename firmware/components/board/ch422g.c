#include "ch422g.h"
#include <stddef.h>
#include <sys/lock.h>
#include "board_pins.h"

#define CH422G_SYS_IO_OUTPUT_ENABLE 0x01

#define IO_LCD_ON (TD_CH422G_IO_DI1 | TD_CH422G_IO_LCD_RST | TD_CH422G_IO_DISP)
#define IO_TOUCH_RELEASED (IO_LCD_ON | TD_CH422G_IO_TP_RST)
#define IO_RUNNING (TD_CH422G_IO_SD_CS | TD_CH422G_IO_LCD_RST | TD_CH422G_IO_DISP | TD_CH422G_IO_TP_RST)

_Static_assert(IO_LCD_ON == 0x2C, "waveshare demo: LCD out of reset, backlight on, touch held in reset");
_Static_assert(IO_TOUCH_RELEASED == 0x2E, "waveshare demo: touch reset released");
_Static_assert(IO_RUNNING == 0x1E, "waveshare demo: final running state");

#define TRY(call)                      \
    do {                               \
        const esp_err_t err_ = (call); \
        if (err_ != ESP_OK) {          \
            return err_;               \
        }                              \
    } while (0)

static struct {
    ch422g_platform_t platform;
    uint8_t output;
    bool started;
} s_chip;

/* The setters read-modify-write the cached byte from several tasks, so every access takes this lock. */
static _lock_t s_lock;

static esp_err_t write_io(uint8_t value)
{
    const esp_err_t err = s_chip.platform.write(s_chip.platform.ctx, TD_CH422G_ADDR_IO, value);
    if (err == ESP_OK) {
        s_chip.output = value;
    }
    return err;
}

/* Waveshare's demo order. GPIO4 goes low before touch reset releases, so the GT911 latches address 0x5D. */
static esp_err_t run_start_sequence(void)
{
    const ch422g_platform_t *p = &s_chip.platform;
    TRY(p->write(p->ctx, TD_CH422G_ADDR_SET, CH422G_SYS_IO_OUTPUT_ENABLE));
    TRY(write_io(IO_LCD_ON));
    p->delay_ms(p->ctx, 100);
    TRY(p->hold_touch_int_low(p->ctx));
    p->delay_ms(p->ctx, 100);
    TRY(write_io(IO_TOUCH_RELEASED));
    p->delay_ms(p->ctx, 200);
    return write_io(IO_RUNNING);
}

esp_err_t ch422g_start(const ch422g_platform_t *platform)
{
    if (platform == NULL || platform->write == NULL || platform->delay_ms == NULL ||
        platform->hold_touch_int_low == NULL) {
        return ESP_ERR_INVALID_ARG;
    }

    _lock_acquire(&s_lock);
    s_chip.platform = *platform;
    s_chip.output = 0;
    const esp_err_t err = run_start_sequence();
    s_chip.started = (err == ESP_OK);
    _lock_release(&s_lock);
    return err;
}

static esp_err_t update_output(uint8_t mask, bool set)
{
    _lock_acquire(&s_lock);
    esp_err_t err = ESP_ERR_INVALID_STATE;
    if (s_chip.started) {
        err = write_io((uint8_t)((s_chip.output & ~mask) | (set ? mask : 0)));
    }
    _lock_release(&s_lock);
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
