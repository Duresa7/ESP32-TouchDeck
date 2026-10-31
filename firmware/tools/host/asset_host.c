#include "td_asset.h"
#include "td_store.h"
#include <stdlib.h>
static uint32_t used;
static unsigned slices;
static unsigned interrupt_at;
bool td_store_mounted(void) { return true; }
void td_store_lock(void) {}
void td_store_unlock(void) {}
uint32_t td_store_budget_cost(uint32_t bytes) { return ((bytes + 4087) / 4088) * 4096 + 4096; }
esp_err_t td_store_budget(td_store_info_t *out)
{
    *out = (td_store_info_t){8 * 1024 * 1024, used};
    return ESP_OK;
}
void td_store_budget_adjust(int64_t delta) { used = (uint32_t)((int64_t)used + delta); }
void host_interrupt(unsigned at)
{
    slices = 0;
    interrupt_at = at;
}
void vTaskDelay(unsigned ticks)
{
    (void)ticks;
    if (interrupt_at && ++slices == interrupt_at) {
        exit(94);
    }
}
