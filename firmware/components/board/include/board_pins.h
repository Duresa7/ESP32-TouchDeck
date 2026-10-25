#pragma once

#include <stdint.h>
#include "hal/gpio_types.h"

#define TD_LCD_H_RES 1024
#define TD_LCD_V_RES 600
#define TD_LCD_PCLK_HZ 21000000
#define TD_LCD_PCLK_ACTIVE_NEG 1

#define TD_LCD_HSYNC_BACK_PORCH 145
#define TD_LCD_HSYNC_FRONT_PORCH 170
#define TD_LCD_HSYNC_PULSE_WIDTH 30
#define TD_LCD_VSYNC_BACK_PORCH 23
#define TD_LCD_VSYNC_FRONT_PORCH 12
#define TD_LCD_VSYNC_PULSE_WIDTH 2

#define TD_LCD_PIN_VSYNC GPIO_NUM_3
#define TD_LCD_PIN_HSYNC GPIO_NUM_46
#define TD_LCD_PIN_DE GPIO_NUM_5
#define TD_LCD_PIN_PCLK GPIO_NUM_7
#define TD_LCD_PIN_DISP_EN GPIO_NUM_NC

/* D0..D15 in order, as an initializer list for a gpio_num_t array. */
#define TD_LCD_DATA_PINS                                                                   \
    {                                                                                      \
        GPIO_NUM_14, GPIO_NUM_38, GPIO_NUM_18, GPIO_NUM_17, GPIO_NUM_10, GPIO_NUM_39,      \
        GPIO_NUM_0, GPIO_NUM_45, GPIO_NUM_48, GPIO_NUM_47, GPIO_NUM_21, GPIO_NUM_1,        \
        GPIO_NUM_2, GPIO_NUM_42, GPIO_NUM_41, GPIO_NUM_40                                  \
    }
#define TD_LCD_DATA_WIDTH 16

#define TD_LCD_FRAME_BYTES (TD_LCD_H_RES * TD_LCD_V_RES * 2)

#define TD_I2C_PORT 0
#define TD_I2C_PIN_SDA GPIO_NUM_8
#define TD_I2C_PIN_SCL GPIO_NUM_9
#define TD_I2C_HZ 400000

/* Doubles as the GT911 address-select line, which must be low when TP_RST releases. */
#define TD_TOUCH_PIN_INT GPIO_NUM_4

#define TD_CH422G_ADDR_SET 0x24
#define TD_CH422G_ADDR_IO 0x38

typedef enum {
    TD_CH422G_IO_DI0 = 1u << 0,
    TD_CH422G_IO_TP_RST = 1u << 1,
    TD_CH422G_IO_DISP = 1u << 2,
    TD_CH422G_IO_LCD_RST = 1u << 3,
    TD_CH422G_IO_SD_CS = 1u << 4,
    TD_CH422G_IO_DI1 = 1u << 5,
} td_ch422g_io_t;

_Static_assert(TD_LCD_FRAME_BYTES == 1228800, "one RGB565 frame is 1228800 bytes");
_Static_assert((TD_CH422G_IO_DI0 | TD_CH422G_IO_TP_RST | TD_CH422G_IO_DISP | TD_CH422G_IO_LCD_RST |
                TD_CH422G_IO_SD_CS | TD_CH422G_IO_DI1) == 0x3F,
               "the six CH422G output bits are distinct and fill bits 0 to 5");
