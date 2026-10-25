#include "display_priv.h"
#include "board_pins.h"
#include "esp_lcd_panel_rgb.h"

/* Espressif's LCD FAQ says to raise this from 10 to 12 lines if the panel drifts. */
#define BOUNCE_BUFFER_LINES 10
#define DMA_BURST_BYTES 64

esp_err_t panel_rgb_create(size_t num_fbs, esp_lcd_panel_handle_t *out)
{
    const esp_lcd_rgb_panel_config_t config = {
        .clk_src = LCD_CLK_SRC_DEFAULT,
        .timings = {
            .pclk_hz = TD_LCD_PCLK_HZ,
            .h_res = TD_LCD_H_RES,
            .v_res = TD_LCD_V_RES,
            .hsync_pulse_width = TD_LCD_HSYNC_PULSE_WIDTH,
            .hsync_back_porch = TD_LCD_HSYNC_BACK_PORCH,
            .hsync_front_porch = TD_LCD_HSYNC_FRONT_PORCH,
            .vsync_pulse_width = TD_LCD_VSYNC_PULSE_WIDTH,
            .vsync_back_porch = TD_LCD_VSYNC_BACK_PORCH,
            .vsync_front_porch = TD_LCD_VSYNC_FRONT_PORCH,
            .flags.pclk_active_neg = TD_LCD_PCLK_ACTIVE_NEG,
        },
        .data_width = TD_LCD_DATA_WIDTH,
        .in_color_format = LCD_COLOR_FMT_RGB565,
        .num_fbs = num_fbs,
        .bounce_buffer_size_px = TD_LCD_H_RES * BOUNCE_BUFFER_LINES,
        .dma_burst_size = DMA_BURST_BYTES,
        .hsync_gpio_num = TD_LCD_PIN_HSYNC,
        .vsync_gpio_num = TD_LCD_PIN_VSYNC,
        .de_gpio_num = TD_LCD_PIN_DE,
        .pclk_gpio_num = TD_LCD_PIN_PCLK,
        .disp_gpio_num = TD_LCD_PIN_DISP_EN,
        .data_gpio_nums = TD_LCD_DATA_PINS,
        .flags.fb_in_psram = 1,
    };
    return esp_lcd_new_rgb_panel(&config, out);
}
