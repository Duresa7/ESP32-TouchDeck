#include "td_link.h"
#include "td_frame.h"
#include "td_rx_sync.h"
#include <string.h>
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/stream_buffer.h"
#include "freertos/task.h"

#define TD_LINK_STREAM_BYTES 8192
#define TD_LINK_IDLE_MS 5000

static StreamBufferHandle_t s_frames;
static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;
static td_link_state_t s_state;
static uint8_t s_rx_packet[2 + TD_FRAME_BODY_MAX];

td_link_state_t td_link_state(void)
{
    int64_t now = esp_timer_get_time() / 1000;
    portENTER_CRITICAL(&s_lock);
    td_link_state_t state = s_state;
    portEXIT_CRITICAL(&s_lock);
    state.online = state.online && now - state.last_rx_ms < TD_LINK_IDLE_MS;
    return state;
}

void td_link_hello(void)
{
    portENTER_CRITICAL(&s_lock);
    s_state.online = true;
    portEXIT_CRITICAL(&s_lock);
}

static void drop_frame(void)
{
    portENTER_CRITICAL(&s_lock);
    s_state.dropped++;
    portEXIT_CRITICAL(&s_lock);
}

static void receive_frame(const uint8_t *body, size_t len, void *context)
{
    (void)context;
    td_frame_t frame;
    if (td_frame_decode(body, len, &frame) != TD_FRAME_OK) {
        portENTER_CRITICAL(&s_lock);
        s_state.crc_errors++;
        portEXIT_CRITICAL(&s_lock);
        return;
    }
    int64_t now = esp_timer_get_time() / 1000;
    portENTER_CRITICAL(&s_lock);
    if (now - s_state.last_rx_ms >= TD_LINK_IDLE_MS) {
        s_state.online = false;
    }
    s_state.last_rx_ms = now;
    s_state.frames_rx++;
    portEXIT_CRITICAL(&s_lock);

    if (xStreamBufferSpacesAvailable(s_frames) < len + 2) {
        drop_frame();
        return;
    }
    s_rx_packet[0] = len & 0xff;
    s_rx_packet[1] = len >> 8;
    memcpy(s_rx_packet + 2, body, len);
    /* Only td_rx writes this stream, so the space check reserves the whole record. */
    xStreamBufferSend(s_frames, s_rx_packet, len + 2, 0);
}

static void read_bytes(uint8_t *buf, size_t len)
{
    size_t used = 0;
    while (used < len) {
        used += xStreamBufferReceive(s_frames, buf + used, len - used, portMAX_DELAY);
    }
}

static void link_task(void *arg)
{
    (void)arg;
    static uint8_t body[TD_FRAME_BODY_MAX];
    static char payload[TD_FRAME_PAYLOAD_MAX + 1];
    static uint8_t wire[TD_FRAME_WIRE_MAX];
    for (;;) {
        uint8_t header[2];
        read_bytes(header, sizeof header);
        size_t len = header[0] | ((size_t)header[1] << 8);
        read_bytes(body, len);
        td_frame_t frame;
        if (td_frame_decode(body, len, &frame) != TD_FRAME_OK
            || frame.type != TD_FRAME_MESSAGE || frame.flags != 0) {
            drop_frame();
            continue;
        }
        memcpy(payload, frame.payload, frame.len);
        payload[frame.len] = '\0';
        const char *end = NULL;
        cJSON *request = cJSON_ParseWithLengthOpts(payload, frame.len + 1, &end, true);
        if (!cJSON_IsObject(request) || end != payload + frame.len) {
            cJSON_Delete(request);
            drop_frame();
            continue;
        }
        cJSON *reply = td_link_dispatch(request);
        cJSON_Delete(request);
        bool printed = reply && cJSON_PrintPreallocated(reply, payload, sizeof payload, false);
        cJSON_Delete(reply);
        if (!printed) {
            drop_frame();
            continue;
        }
        td_frame_t response = {
            .type = TD_FRAME_MESSAGE, .seq = frame.seq, .flags = TD_FRAME_RESPONSE,
            .len = (uint16_t)strlen(payload), .payload = (const uint8_t *)payload,
        };
        size_t wire_len = td_frame_encode(&response, wire, sizeof wire);
        if (!wire_len || !td_console_write((const char *)wire, wire_len)) {
            drop_frame();
            continue;
        }
        portENTER_CRITICAL(&s_lock);
        s_state.frames_tx++;
        portEXIT_CRITICAL(&s_lock);
    }
}

esp_err_t td_link_start(void)
{
    if (s_frames) {
        return ESP_ERR_INVALID_STATE;
    }
    s_frames = xStreamBufferCreate(TD_LINK_STREAM_BYTES, 1);
    if (!s_frames) {
        return ESP_ERR_NO_MEM;
    }
    if (xTaskCreate(link_task, "td_link", 6144, NULL, 3, NULL) != pdPASS) {
        vStreamBufferDelete(s_frames);
        s_frames = NULL;
        return ESP_ERR_NO_MEM;
    }
    td_console_set_frame_handler(receive_frame, NULL);
    return ESP_OK;
}
