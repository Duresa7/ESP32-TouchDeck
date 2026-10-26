#include "td_link.h"
#include "td_link_priv.h"
#include "key_sink_link.h"
#include "td_frame.h"
#include "td_netlink_service.h"
#include "td_rx_sync.h"
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/queue.h"
#include "freertos/semphr.h"
#include "freertos/stream_buffer.h"
#include "freertos/task.h"

#define TD_LINK_STREAM_BYTES 8192
#define TD_LINK_IDLE_MS 5000
#define TD_LINK_POLL_MS 10
#define TD_LINK_PRESS_QUEUE 16
#define TD_LINK_EVENTS_PER_TURN 16

_Static_assert(TD_RX_FRAME_MAX >= TD_FRAME_BODY_MAX, "receive buffer must hold a decoded maximum frame");

/* Each packet in s_frames is u16le(len), the transport, and one decoded body. USB's receive task and the net task
 * both write it, one whole packet at a time under s_frames_lock. */
#define PACKET_HEADER 3

static StreamBufferHandle_t s_frames;
static SemaphoreHandle_t s_frames_lock;
static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;
static td_link_state_t s_state;
static uint8_t s_rx_packet[PACKET_HEADER + TD_FRAME_BODY_MAX];
static td_link_transport_t s_origin;
static atomic_bool s_net_online;
static atomic_bool s_net_closed;
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
    if (s_origin == TD_LINK_NET) {
        atomic_store(&s_net_online, true);
        td_netlink_hello();
        return;
    }
    portENTER_CRITICAL(&s_lock);
    s_state.online = true;
    portEXIT_CRITICAL(&s_lock);
}

td_link_transport_t td_link_origin(void)
{
    return s_origin;
}

bool td_link_online_any(void)
{
    return td_link_state().online || atomic_load(&s_net_online);
}

static void enqueue(td_link_transport_t transport, const uint8_t *body, size_t len)
{
    xSemaphoreTake(s_frames_lock, portMAX_DELAY);
    if (xStreamBufferSpacesAvailable(s_frames) < len + PACKET_HEADER) {
        xSemaphoreGive(s_frames_lock);
        td_link_count_drop();
        return;
    }
    s_rx_packet[0] = len & 0xff;
    s_rx_packet[1] = len >> 8;
    s_rx_packet[2] = (uint8_t)transport;
    memcpy(s_rx_packet + PACKET_HEADER, body, len);
    size_t sent = xStreamBufferSend(s_frames, s_rx_packet, len + PACKET_HEADER, 0);
    xSemaphoreGive(s_frames_lock);
    configASSERT(sent == len + PACKET_HEADER);
}

/* The session's records arrive authenticated with their bodies checked, so they skip USB's counters. */
static void net_frame(const uint8_t *body, size_t len)
{
    enqueue(TD_LINK_NET, body, len);
}

static void net_closed(void)
{
    atomic_store(&s_net_online, false);
    atomic_store(&s_net_closed, true);
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
    enqueue(TD_LINK_USB, body, len);
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

bool td_link_send_on(td_link_transport_t transport, uint8_t type, uint8_t seq, uint8_t flags, const uint8_t *payload,
                     size_t len)
{
    static uint8_t wire[TD_FRAME_WIRE_MAX];
    td_frame_t frame = { .type = type, .seq = seq, .flags = flags, .len = (uint16_t)len, .payload = payload };
    if (transport == TD_LINK_NET) {
        size_t body_len = td_frame_encode_body(&frame, wire, sizeof wire);
        if (!body_len || !td_netlink_send(wire, body_len)) {
            td_link_count_drop();
            return false;
        }
        return true;
    }
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

bool td_link_send(uint8_t type, uint8_t seq, uint8_t flags, const uint8_t *payload, size_t len)
{
    return td_link_send_on(s_origin, type, seq, flags, payload, len);
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
    if (!cJSON_IsObject(request) || end != payload + frame->len) {
        cJSON_Delete(request);
        td_link_count_drop();
        return;
    }
    cJSON *reply = td_link_dispatch(request);
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
    uint8_t header[PACKET_HEADER];
    if (!read_bytes(header, sizeof header, wait)) {
        return;
    }
    size_t len = header[0] | ((size_t)header[1] << 8);
    s_origin = header[2] == TD_LINK_NET ? TD_LINK_NET : TD_LINK_USB;
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

/* Each event goes to every link the app is online on. With none online it is thrown away, and its seq leaves a gap. */
static void send_event(const bool online[2], const char *json, size_t len)
{
    for (int transport = TD_LINK_USB; transport <= TD_LINK_NET; transport++) {
        if (online[transport]) {
            td_link_send_on((td_link_transport_t)transport, TD_FRAME_MESSAGE, 0, TD_FRAME_EVENT, (const uint8_t *)json,
                            len);
        }
    }
}

static void send_events(void)
{
    static char json[KEY_SINK_LINK_EVENT_MAX];
    const bool online[2] = { td_link_state().online, atomic_load(&s_net_online) };
    press_t press;
    for (int turn = 0; turn < TD_LINK_EVENTS_PER_TURN && xQueueReceive(s_presses, &press, 0) == pdTRUE; turn++) {
        int len = snprintf(json, sizeof json,
                           "{\"m\":\"press\",\"page\":\"%s\",\"tile\":\"%s\",\"kind\":\"%s\",\"seq\":%u,\"t\":%u}",
                           press.page, press.tile, press.kind, (unsigned)press.seq, (unsigned)press.t);
        send_event(online, json, (size_t)len);
    }
    if (!online[TD_LINK_USB] && !online[TD_LINK_NET]) {
        key_sink_link_discard();
        return;
    }
    for (int turn = 0; turn < TD_LINK_EVENTS_PER_TURN; turn++) {
        size_t len = key_sink_link_next(json, sizeof json);
        if (len == 0) {
            break;
        }
        send_event(online, json, len);
    }
}

static void link_task(void *arg)
{
    (void)arg;
    for (;;) {
        handle_frame(pdMS_TO_TICKS(TD_LINK_POLL_MS));
        if (atomic_exchange(&s_net_closed, false)) {
            td_link_files_closed(TD_LINK_NET);
        }
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
    s_frames_lock = xSemaphoreCreateMutex();
    if (!s_frames || !s_presses || !s_frames_lock) {
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
    td_netlink_attach(&(const td_netlink_link_t){ .frame = net_frame, .closed = net_closed });
    return ESP_OK;
}
