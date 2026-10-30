#include "td_link.h"
#include "td_link_priv.h"
#include "key_sink_link.h"
#include "td_frame.h"
#include "td_v2_json.h"
#include "td_rx_sync.h"
#include <stdio.h>
#include <string.h>
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/queue.h"
#include "freertos/stream_buffer.h"
#include "freertos/task.h"

#define TD_LINK_STREAM_BYTES 8192
#define TD_LINK_IDLE_MS 5000
#define TD_LINK_POLL_MS 10
#define TD_LINK_PRESS_QUEUE 16
#define TD_LINK_EVENTS_PER_TURN 16

_Static_assert(TD_RX_FRAME_MAX >= TD_FRAME_BODY_MAX, "receive buffer must hold a decoded maximum frame");

static StreamBufferHandle_t s_frames;
static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;
static td_link_state_t s_state;
static uint8_t s_rx_packet[2 + TD_FRAME_BODY_MAX];
static QueueHandle_t s_presses;
static uint32_t s_press_seq;

typedef struct {
    uint32_t seq;
    uint32_t t;
    char page[TD_LINK_ID_MAX];
    char tile[TD_LINK_ID_MAX];
    char kind[12];
} press_t;

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

void td_link_count_drop(void)
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
        td_link_count_drop();
        return;
    }
    s_rx_packet[0] = len & 0xff;
    s_rx_packet[1] = len >> 8;
    memcpy(s_rx_packet + 2, body, len);
    size_t sent = xStreamBufferSend(s_frames, s_rx_packet, len + 2, 0);
    configASSERT(sent == len + 2);
}

static bool read_bytes(uint8_t *buf, size_t len, TickType_t first_wait)
{
    size_t used = 0;
    while (used < len) {
        size_t got = xStreamBufferReceive(s_frames, buf + used, len - used, used == 0 ? first_wait : portMAX_DELAY);
        if (got == 0) {
            return false;
        }
        used += got;
    }
    return true;
}

bool td_link_send(uint8_t type, uint8_t seq, uint8_t flags, const uint8_t *payload, size_t len)
{
    static uint8_t wire[TD_FRAME_WIRE_MAX];
    td_frame_t frame = { .type = type, .seq = seq, .flags = flags, .len = (uint16_t)len, .payload = payload };
    size_t wire_len = td_frame_encode(&frame, wire, sizeof wire);
    if (!wire_len || !td_console_write((const char *)wire, wire_len)) {
        td_link_count_drop();
        return false;
    }
    portENTER_CRITICAL(&s_lock);
    s_state.frames_tx++;
    portEXIT_CRITICAL(&s_lock);
    return true;
}

void td_link_fail(cJSON *reply, const char *error)
{
    cJSON_ReplaceItemInObjectCaseSensitive(reply, "ok", cJSON_CreateBool(false));
    cJSON_DeleteItemFromObjectCaseSensitive(reply, "error");
    cJSON_AddStringToObject(reply, "error", error);
}

static void handle_message(const td_frame_t *frame)
{
    static char payload[TD_FRAME_PAYLOAD_MAX + 1];
    memcpy(payload, frame->payload, frame->len);
    payload[frame->len] = '\0';
    const char *end = NULL;
    cJSON *request = cJSON_ParseWithLengthOpts(payload, frame->len + 1, &end, true);
    const char *raw_message = NULL;
    size_t message_len = 0;
    cJSON *recovered = NULL;
    if (td_v2_json_message(payload, frame->len, &raw_message, &message_len)) {
        recovered = cJSON_ParseWithLength(raw_message, message_len);
    }
    const char *message = cJSON_GetStringValue(recovered);
    bool strict = message && (!strncmp(message, "v2.", 3) || !strcmp(message, "hello"));
    cJSON *reply;
    if (strict && !td_v2_json_valid_live(payload, frame->len, !strcmp(message, "v2.live.set"))) {
        reply = cJSON_CreateObject();
        char echo[65];
        snprintf(echo, sizeof echo, "%.64s", message);
        cJSON_AddStringToObject(reply, "m", echo);
        cJSON_AddBoolToObject(reply, "ok", false);
        cJSON_AddStringToObject(reply, "reason", "args");
        cJSON_AddStringToObject(reply, "error", "Invalid request fields");
    } else if (!cJSON_IsObject(request) || end != payload + frame->len) {
        cJSON_Delete(recovered);
        cJSON_Delete(request);
        td_link_count_drop();
        return;
    } else {
        reply = td_link_dispatch(request);
    }
    cJSON_Delete(recovered);
    cJSON_Delete(request);
    bool printed = reply && cJSON_PrintPreallocated(reply, payload, sizeof payload, false);
    cJSON_Delete(reply);
    if (!printed) {
        td_link_count_drop();
        return;
    }
    td_link_send(TD_FRAME_MESSAGE, frame->seq, TD_FRAME_RESPONSE, (const uint8_t *)payload, strlen(payload));
}

/* Waits up to `wait` ticks for one frame, and handles it. */
static void handle_frame(TickType_t wait)
{
    static uint8_t body[TD_FRAME_BODY_MAX];
    uint8_t header[2];
    if (!read_bytes(header, sizeof header, wait)) {
        return;
    }
    size_t len = header[0] | ((size_t)header[1] << 8);
    read_bytes(body, len, portMAX_DELAY);
    td_frame_t frame;
    if (td_frame_decode(body, len, &frame) != TD_FRAME_OK) {
        td_link_count_drop();
        return;
    }
    if (frame.type == TD_FRAME_MESSAGE && frame.flags == 0) {
        handle_message(&frame);
    } else if (frame.type == TD_FRAME_CHUNK && frame.flags == 0) {
        td_link_files_chunk(frame.seq, frame.payload, frame.len);
    } else if (frame.type == TD_FRAME_ACK && frame.flags == TD_FRAME_RESPONSE) {
        td_link_files_ack(frame.payload, frame.len);
    } else {
        td_link_count_drop();
    }
}

void td_link_press(const char *page, const char *tile, const char *kind)
{
    if (!s_presses) {
        return;
    }
    press_t press = { .seq = __atomic_add_fetch(&s_press_seq, 1, __ATOMIC_RELAXED),
                      .t = (uint32_t)(esp_timer_get_time() / 1000) };
    snprintf(press.page, sizeof press.page, "%s", page);
    snprintf(press.tile, sizeof press.tile, "%s", tile);
    snprintf(press.kind, sizeof press.kind, "%s", kind);
    xQueueSend(s_presses, &press, 0);
}

/* Events go out only while the app is connected. Otherwise they are thrown away, and their seq leaves a gap. */
static void send_events(void)
{
    static char json[KEY_SINK_LINK_EVENT_MAX];
    const bool online = td_link_state().online;
    press_t press;
    for (int turn = 0; turn < TD_LINK_EVENTS_PER_TURN && xQueueReceive(s_presses, &press, 0) == pdTRUE; turn++) {
        if (online) {
            int len = snprintf(json, sizeof json,
                               "{\"m\":\"press\",\"page\":\"%s\",\"tile\":\"%s\",\"kind\":\"%s\",\"seq\":%u,\"t\":%u}",
                               press.page, press.tile, press.kind, (unsigned)press.seq, (unsigned)press.t);
            td_link_send(TD_FRAME_MESSAGE, 0, TD_FRAME_EVENT, (const uint8_t *)json, (size_t)len);
        }
    }
    if (!online) {
        key_sink_link_discard();
        return;
    }
    for (int turn = 0; turn < TD_LINK_EVENTS_PER_TURN; turn++) {
        size_t len = key_sink_link_next(json, sizeof json);
        if (len == 0) {
            break;
        }
        td_link_send(TD_FRAME_MESSAGE, 0, TD_FRAME_EVENT, (const uint8_t *)json, len);
    }
}

static void link_task(void *arg)
{
    (void)arg;
    for (;;) {
        handle_frame(pdMS_TO_TICKS(TD_LINK_POLL_MS));
        td_link_files_service((uint32_t)(esp_timer_get_time() / 1000));
        send_events();
    }
}

esp_err_t td_link_start(void)
{
    if (s_frames) {
        return ESP_ERR_INVALID_STATE;
    }
    s_frames = xStreamBufferCreate(TD_LINK_STREAM_BYTES, 1);
    s_presses = xQueueCreate(TD_LINK_PRESS_QUEUE, sizeof(press_t));
    if (!s_frames || !s_presses) {
        if (s_frames) {
            vStreamBufferDelete(s_frames);
            s_frames = NULL;
        }
        if (s_presses) {
            vQueueDelete(s_presses);
            s_presses = NULL;
        }
        return ESP_ERR_NO_MEM;
    }
    if (xTaskCreate(link_task, "td_link", 8192, NULL, 3, NULL) != pdPASS) {
        vStreamBufferDelete(s_frames);
        s_frames = NULL;
        vQueueDelete(s_presses);
        s_presses = NULL;
        return ESP_ERR_NO_MEM;
    }
    td_console_set_frame_handler(receive_frame, NULL);
    return ESP_OK;
}
