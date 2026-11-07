#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "esp_err.h"
#include "td_frame.h"

#ifdef __cplusplus
extern "C" {
#endif

/*
 * A carrier moves whole link frames between the Device and the Companion, each its own way. USB moves them as SLIP
 * over the USB-Serial/JTAG console, and td_link_start adds it first.
 *
 * Online state is kept per carrier: a carrier goes online when a `hello` arrives on it, and offline when nothing has
 * arrived on it for 5 seconds. The link sends every frame, replies and events alike, through one active carrier: the
 * first carrier added that is online, or, while none is, the one the last handled frame arrived on. So USB wins
 * whenever the Companion talks over it.
 */
typedef struct td_link_carrier td_link_carrier_t;

struct td_link_carrier {
    const char *name;
    /* Sends one frame, on the td_link task. Returns false when the frame did not go out whole. A carrier with state
     * of its own embeds this struct and finds its state from the pointer. */
    bool (*send)(const td_link_carrier_t *carrier, const td_frame_t *frame);
};

#define TD_LINK_CARRIERS_MAX 3

/*
 * Adds a carrier after td_link_start, behind the carriers already added. The carrier must stay valid for good.
 * Returns ESP_ERR_INVALID_STATE before td_link_start or when it was added already, and ESP_ERR_NO_MEM when
 * TD_LINK_CARRIERS_MAX are in use.
 */
esp_err_t td_link_carrier_add(const td_link_carrier_t *carrier);

/*
 * Hands the link one frame body, SLIP already removed, that arrived on `carrier`. It has the td_rx_frame_fn shape,
 * with the carrier as its context. Any task, without blocking for long. A NULL body counts a malformed frame.
 */
void td_link_carrier_receive(const uint8_t *body, size_t len, void *carrier);

#ifdef __cplusplus
}
#endif
