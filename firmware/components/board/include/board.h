#pragma once

#include "driver/i2c_master.h"
#include "esp_err.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Idempotent and safe from several tasks. The first call creates the I2C bus and starts the CH422G;
 * every later call returns that first result. */
esp_err_t board_init(void);

/* The one I2C bus, NULL until it has been created. Other devices add themselves to this bus. */
i2c_master_bus_handle_t board_i2c_bus(void);

#ifdef __cplusplus
}
#endif
