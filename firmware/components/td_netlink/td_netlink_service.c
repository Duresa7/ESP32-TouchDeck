#include "td_netlink_service.h"
#include "td_netlink_priv.h"
#include "td_netlink.h"
#include "td_netlink_nvs.h"
#include "ui.h"
#include <errno.h>
#include <fcntl.h>
#include <stdatomic.h>
#include <string.h>
#include "esp_heap_caps.h"
#include "esp_log.h"
#include "esp_mac.h"
#include "esp_netif.h"
#include "esp_random.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/idf_additions.h"
#include "freertos/message_buffer.h"
#include "freertos/semphr.h"
#include "freertos/task.h"
#include "lwip/sockets.h"

static const char *TAG = "netlink";

/* The stack stays internal: pairing commits to NVS from this task, and a flash write asserts on a PSRAM stack. */
#define TASK_STACK 6144
#define TASK_PRIORITY 2
#define TASK_CORE 1
#define POLL_MS 10
#define WRITE_MS 1000
#define REFUSE_MS 100
#define DRAIN_MS 100
#define OUT_RECORDS 4
#define SEND_WAIT_MS 100
#define COMMAND_WAIT_MS 2000
#define FORGET_CLOSE_MS 1000

typedef enum {
    CMD_NONE,
    CMD_PAIR,
    CMD_FORGET,
    CMD_FORGET_FROM_SESSION,
} cmd_t;

typedef struct {
    bool listening;
    bool session;
    uint8_t app_id[TD_NETLINK_APP_ID_LEN];
    char name[TD_NETLINK_NAME_MAX + 1];
    uint32_t since_ms;
    uint8_t paired;
    bool pairing;
} snapshot_t;

/* Everything the net task owns, in PSRAM, so only its stack and the SRP arithmetic use internal RAM. */
typedef struct {
    td_netlink_deck_t deck;
    td_netlink_conn_t conn;
    td_netlink_out_t item;
    uint8_t head[2];
    bool in_body;
    size_t want;
    size_t got;
    uint8_t body[TD_NETLINK_SEALED_MAX];
    uint8_t plain[TD_NETLINK_PLAIN_MAX];
    uint8_t wire[TD_NETLINK_WIRE_MAX];
} net_t;

static td_netlink_link_t s_link;
static MessageBufferHandle_t s_out;
static SemaphoreHandle_t s_cmd_lock;
static SemaphoreHandle_t s_cmd_done;
static _Atomic cmd_t s_cmd;
static td_netlink_reason_t s_cmd_result;
static portMUX_TYPE s_lock = portMUX_INITIALIZER_UNLOCKED;
static snapshot_t s_snap;
static atomic_bool s_ready;
static atomic_bool s_cancel;
static atomic_bool s_hello;

/* Net task only. */
static net_t *n;
static int s_listen = -1;
static int s_client = -1;
static bool s_shown;
static uint32_t s_shown_generation;
static bool s_close_after_write;
static uint32_t s_close_by_ms;

static uint32_t now_ms(void)
{
    return (uint32_t)(esp_timer_get_time() / 1000);
}

static uint32_t clock_ms(void *ctx)
{
    (void)ctx;
    return now_ms();
}

static void fill_random(void *ctx, uint8_t *out, size_t len)
{
    (void)ctx;
    esp_fill_random(out, len);
}

static bool save_apps(void *ctx, const td_netlink_apps_t *apps)
{
    (void)ctx;
    return td_netlink_nvs_save(TD_NETLINK_NVS_NAMESPACE, apps);
}

static bool write_all(int fd, const uint8_t *buf, size_t len, uint32_t timeout_ms)
{
    const int64_t until = esp_timer_get_time() + (int64_t)timeout_ms * 1000;
    size_t done = 0;
    while (done < len) {
        ssize_t sent = send(fd, buf + done, len - done, 0);
        if (sent > 0) {
            done += (size_t)sent;
            continue;
        }
        if (sent < 0 && errno != EAGAIN && errno != EWOULDBLOCK) {
            return false;
        }
        const int64_t left = until - esp_timer_get_time();
        if (left <= 0) {
            return false;
        }
        fd_set w;
        FD_ZERO(&w);
        FD_SET(fd, &w);
        struct timeval tv = { .tv_sec = 0, .tv_usec = left < 100000 ? (long)left : 100000 };
        select(fd + 1, NULL, &w, NULL, &tv);
    }
    return true;
}

/* Shuts the write side and reads whatever still arrives for up to 100 ms, so the peer reads our last item before the
 * close, instead of losing it to a reset. */
static void close_orderly(int fd)
{
    shutdown(fd, SHUT_WR);
    const int64_t until = esp_timer_get_time() + DRAIN_MS * 1000;
    uint8_t sink[64];
    for (;;) {
        const int64_t left = until - esp_timer_get_time();
        if (left <= 0) {
            break;
        }
        fd_set r;
        FD_ZERO(&r);
        FD_SET(fd, &r);
        struct timeval tv = { .tv_sec = 0, .tv_usec = (long)left };
        if (select(fd + 1, &r, NULL, NULL, &tv) <= 0 || recv(fd, sink, sizeof sink, 0) <= 0) {
            break;
        }
    }
    close(fd);
}

static void drop_client(bool orderly)
{
    if (s_client < 0) {
        return;
    }
    if (orderly) {
        close_orderly(s_client);
    } else {
        close(s_client);
    }
    s_client = -1;
    td_netlink_conn_close(&n->conn, &n->deck);
    n->in_body = false;
    n->got = 0;
    s_close_after_write = false;
    atomic_store(&s_hello, false);
    while (xMessageBufferReceive(s_out, n->plain, sizeof n->plain, 0) > 0) {
    }
    td_netlink_wipe(n->plain, sizeof n->plain);
    if (atomic_exchange(&s_ready, false)) {
        ESP_LOGI(TAG, "session closed");
        if (s_link.closed) {
            s_link.closed();
        }
    }
}

static void act(td_netlink_action_t action)
{
    switch (action) {
    case TD_NETLINK_NOTHING:
        break;
    case TD_NETLINK_SEND:
        if (!write_all(s_client, n->item.item, n->item.len, WRITE_MS)) {
            drop_client(false);
        } else if (n->conn.state == TD_NETLINK_CONN_READY && !atomic_load(&s_ready)) {
            atomic_store(&s_ready, true);
            ESP_LOGI(TAG, "session ready");
        }
        break;
    case TD_NETLINK_SEND_CLOSE:
        write_all(s_client, n->item.item, n->item.len, WRITE_MS);
        drop_client(true);
        break;
    case TD_NETLINK_CLOSE:
        drop_client(false);
        break;
    }
    td_netlink_wipe(&n->item, sizeof n->item);
}

static void accept_client(void)
{
    const int fd = accept(s_listen, NULL, NULL);
    if (fd < 0) {
        return;
    }
    fcntl(fd, F_SETFL, fcntl(fd, F_GETFL, 0) | O_NONBLOCK);
    if (s_client >= 0) {
        write_all(fd, td_netlink_busy_item, sizeof td_netlink_busy_item, REFUSE_MS);
        close_orderly(fd);
        ESP_LOGI(TAG, "refused a second connection");
        return;
    }
    const int one = 1;
    setsockopt(fd, IPPROTO_TCP, TCP_NODELAY, &one, sizeof one);
    s_client = fd;
    n->in_body = false;
    n->got = 0;
    td_netlink_conn_init(&n->conn, &n->deck);
}

/* Reads what the socket holds, one length prefix and one body at a time. A bad length is the core's to refuse
 * before any body is read. */
static void read_client(void)
{
    while (s_client >= 0) {
        uint8_t *dst = n->in_body ? n->body + n->got : n->head + n->got;
        const size_t want = (n->in_body ? n->want : sizeof n->head) - n->got;
        const ssize_t got = recv(s_client, dst, want, 0);
        if (got < 0 && (errno == EAGAIN || errno == EWOULDBLOCK)) {
            return;
        }
        if (got <= 0) {
            drop_client(false);
            return;
        }
        n->got += (size_t)got;
        if (!n->in_body) {
            if (n->got < sizeof n->head) {
                continue;
            }
            const uint16_t len = (uint16_t)(n->head[0] << 8 | n->head[1]);
            uint16_t min, max;
            td_netlink_conn_limits(&n->conn, &min, &max);
            n->got = 0;
            if (len < min || len > max) {
                if (n->conn.state == TD_NETLINK_CONN_READY) {
                    drop_client(false);
                } else {
                    act(td_netlink_conn_item(&n->conn, &n->deck, n->body, len, &n->item));
                }
                return;
            }
            n->in_body = true;
            n->want = len;
            continue;
        }
        if (n->got < n->want) {
            continue;
        }
        const size_t len = n->want;
        n->in_body = false;
        n->got = 0;
        if (n->conn.state != TD_NETLINK_CONN_READY) {
            act(td_netlink_conn_item(&n->conn, &n->deck, n->body, len, &n->item));
        } else if (!td_netlink_conn_record(&n->conn, &n->deck, n->body, len, n->plain)) {
            drop_client(false);
        } else if (s_link.frame) {
            s_link.frame(n->plain, len - TD_NETLINK_TAG_LEN);
        }
    }
}

/* Seals and writes what the link queued. A forget that came over the session closes it after its reply. */
static void write_records(void)
{
    size_t len;
    while ((len = xMessageBufferReceive(s_out, n->plain, sizeof n->plain, 0)) > 0) {
        if (s_client < 0 || n->conn.state != TD_NETLINK_CONN_READY) {
            continue;
        }
        if (!td_netlink_conn_send(&n->conn, n->plain, len, n->wire)
                || !write_all(s_client, n->wire, 2 + len + TD_NETLINK_TAG_LEN, WRITE_MS)) {
            drop_client(false);
        } else if (s_close_after_write) {
            drop_client(true);
        }
    }
}

static void publish(void)
{
    snapshot_t snap = {
        .listening = s_listen >= 0,
        .paired = n->deck.apps.count,
        .pairing = s_shown,
    };
    if (atomic_load(&s_ready) && !s_close_after_write) {
        snap.session = true;
        memcpy(snap.app_id, n->conn.app_id, sizeof snap.app_id);
        memcpy(snap.name, n->conn.name, n->conn.name_len);
        snap.since_ms = n->conn.ready_ms;
    }
    portENTER_CRITICAL(&s_lock);
    s_snap = snap;
    portEXIT_CRITICAL(&s_lock);
}

static void run_command(void)
{
    const cmd_t cmd = atomic_exchange(&s_cmd, CMD_NONE);
    if (cmd == CMD_NONE) {
        return;
    }
    td_netlink_reason_t result = TD_NETLINK_REASON_NONE;
    if (cmd == CMD_PAIR) {
        result = td_netlink_window_open(&n->deck);
    } else {
        result = td_netlink_forget(&n->deck) ? TD_NETLINK_REASON_NONE : TD_NETLINK_STORAGE;
        ESP_LOGI(TAG, "pairings erased%s", result == TD_NETLINK_REASON_NONE ? "" : ", but the erase failed");
        const td_netlink_conn_state_t state = n->conn.state;
        const bool pairing = state >= TD_NETLINK_CONN_PAIR_CODE && state <= TD_NETLINK_CONN_PAIR_COMMIT;
        if (cmd == CMD_FORGET_FROM_SESSION && atomic_load(&s_ready)) {
            s_close_after_write = true;
            s_close_by_ms = now_ms() + FORGET_CLOSE_MS;
        } else if (s_client >= 0 && !pairing) {
            drop_client(false);
        }
    }
    s_cmd_result = result;
    publish();
    xSemaphoreGive(s_cmd_done);
}

static void on_cancel(void)
{
    atomic_store(&s_cancel, true);
}

static void show_window(void)
{
    const td_netlink_window_t *w = &n->deck.window;
    if (w->open && (!s_shown || s_shown_generation != w->generation)) {
        ui_pairing_show(w->code, on_cancel);
        s_shown = true;
        s_shown_generation = w->generation;
        ESP_LOGI(TAG, "pairing window open");
    } else if (!w->open && s_shown) {
        ui_pairing_hide();
        s_shown = false;
        ESP_LOGI(TAG, "pairing window closed");
    }
}

static void net_task(void *arg)
{
    (void)arg;
    for (;;) {
        write_records();
        run_command();
        if (atomic_exchange(&s_cancel, false)) {
            td_netlink_window_cancel(&n->deck);
        }
        if (atomic_exchange(&s_hello, false) && s_client >= 0) {
            td_netlink_conn_hello(&n->conn);
        }
        fd_set r;
        FD_ZERO(&r);
        FD_SET(s_listen, &r);
        int top = s_listen;
        if (s_client >= 0) {
            FD_SET(s_client, &r);
            top = s_client > top ? s_client : top;
        }
        struct timeval tv = { .tv_sec = 0, .tv_usec = POLL_MS * 1000 };
        if (select(top + 1, &r, NULL, NULL, &tv) > 0) {
            if (s_client >= 0 && FD_ISSET(s_client, &r)) {
                read_client();
            }
            if (FD_ISSET(s_listen, &r)) {
                accept_client();
            }
        }
        td_netlink_deck_tick(&n->deck);
        if (s_client >= 0) {
            act(td_netlink_conn_tick(&n->conn, &n->deck, &n->item));
        }
        if (s_close_after_write && (int32_t)(now_ms() - s_close_by_ms) >= 0) {
            drop_client(false);
        }
        show_window();
        publish();
    }
}

void td_netlink_attach(const td_netlink_link_t *link)
{
    s_link = *link;
}

static int open_listener(void)
{
    const int fd = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (fd < 0) {
        return -1;
    }
    const int one = 1;
    setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &one, sizeof one);
    struct sockaddr_in addr = {
        .sin_family = AF_INET,
        .sin_port = htons(TD_NETLINK_PORT),
        .sin_addr.s_addr = htonl(INADDR_ANY),
    };
    if (bind(fd, (struct sockaddr *)&addr, sizeof addr) != 0 || listen(fd, 1) != 0) {
        close(fd);
        return -1;
    }
    fcntl(fd, F_SETFL, fcntl(fd, F_GETFL, 0) | O_NONBLOCK);
    return fd;
}

esp_err_t td_netlink_start(void)
{
    if (n) {
        return ESP_ERR_INVALID_STATE;
    }
    n = heap_caps_calloc(1, sizeof *n, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    s_out = xMessageBufferCreateWithCaps(OUT_RECORDS * (TD_NETLINK_PLAIN_MAX + sizeof(size_t)), MALLOC_CAP_SPIRAM);
    s_cmd_lock = xSemaphoreCreateMutex();
    s_cmd_done = xSemaphoreCreateBinary();
    if (!n || !s_out || !s_cmd_lock || !s_cmd_done) {
        return ESP_ERR_NO_MEM;
    }
    /* Wi-Fi brings the stack up first when it starts. Without it the listener still opens, on no interface. */
    esp_err_t err = esp_netif_init();
    if (err != ESP_OK && err != ESP_ERR_INVALID_STATE) {
        return err;
    }
    uint8_t deck_id[TD_NETLINK_DECK_ID_LEN];
    err = esp_read_mac(deck_id, ESP_MAC_WIFI_STA);
    if (err != ESP_OK) {
        return err;
    }
    td_netlink_apps_t stored;
    if (!td_netlink_nvs_load(TD_NETLINK_NVS_NAMESPACE, &stored)) {
        ESP_LOGW(TAG, "stored pairings unreadable, starting with none");
    }
    td_netlink_deck_init(&n->deck, deck_id, &stored, fill_random, save_apps, clock_ms, NULL);
    td_netlink_wipe(&stored, sizeof stored);
    s_listen = open_listener();
    if (s_listen < 0) {
        ESP_LOGE(TAG, "listener on %d failed: errno %d", TD_NETLINK_PORT, errno);
    }
    publish();
    if (s_listen < 0) {
        return ESP_FAIL;
    }
    if (xTaskCreatePinnedToCore(net_task, "td_netlink", TASK_STACK, NULL, TASK_PRIORITY, NULL, TASK_CORE) != pdPASS) {
        close(s_listen);
        s_listen = -1;
        publish();
        return ESP_ERR_NO_MEM;
    }
    ESP_LOGI(TAG, "listening on %d, %u paired", TD_NETLINK_PORT, (unsigned)n->deck.apps.count);
    return ESP_OK;
}

bool td_netlink_send(const uint8_t *body, size_t len)
{
    if (!s_out || !atomic_load(&s_ready) || len > TD_NETLINK_PLAIN_MAX) {
        return false;
    }
    return xMessageBufferSend(s_out, body, len, pdMS_TO_TICKS(SEND_WAIT_MS)) == len;
}

void td_netlink_hello(void)
{
    atomic_store(&s_hello, true);
}

static snapshot_t snapshot(void)
{
    portENTER_CRITICAL(&s_lock);
    const snapshot_t snap = s_snap;
    portEXIT_CRITICAL(&s_lock);
    return snap;
}

void td_netlink_state_fields(cJSON *reply)
{
    const snapshot_t snap = snapshot();
    cJSON_AddBoolToObject(reply, "listening", snap.listening);
    cJSON_AddNumberToObject(reply, "port", TD_NETLINK_PORT);
    if (snap.session) {
        static const char digits[] = "0123456789abcdef";
        char app_id[2 * TD_NETLINK_APP_ID_LEN + 1];
        for (size_t i = 0; i < TD_NETLINK_APP_ID_LEN; i++) {
            app_id[2 * i] = digits[snap.app_id[i] >> 4];
            app_id[2 * i + 1] = digits[snap.app_id[i] & 15];
        }
        app_id[sizeof app_id - 1] = '\0';
        cJSON *session = cJSON_AddObjectToObject(reply, "session");
        cJSON_AddStringToObject(session, "app_id", app_id);
        cJSON_AddStringToObject(session, "app_name", snap.name);
        cJSON_AddNumberToObject(session, "since_ms", snap.since_ms);
    } else {
        cJSON_AddNullToObject(reply, "session");
    }
    cJSON_AddNumberToObject(reply, "paired", snap.paired);
    cJSON_AddBoolToObject(reply, "pairing", snap.pairing);
}

unsigned td_netlink_paired(void)
{
    return snapshot().paired;
}

/* Hands the net task one command and waits for it. Callers take turns on the lock. */
static bool command(cmd_t cmd, td_netlink_reason_t *result)
{
    if (!n || s_listen < 0 || xSemaphoreTake(s_cmd_lock, pdMS_TO_TICKS(COMMAND_WAIT_MS)) != pdTRUE) {
        return false;
    }
    xSemaphoreTake(s_cmd_done, 0);
    atomic_store(&s_cmd, cmd);
    const bool done = xSemaphoreTake(s_cmd_done, pdMS_TO_TICKS(COMMAND_WAIT_MS)) == pdTRUE;
    if (!done) {
        atomic_store(&s_cmd, CMD_NONE);
    }
    *result = s_cmd_result;
    xSemaphoreGive(s_cmd_lock);
    return done;
}

bool td_netlink_forget_all(bool from_session)
{
    td_netlink_reason_t result;
    return command(from_session ? CMD_FORGET_FROM_SESSION : CMD_FORGET, &result) && result == TD_NETLINK_REASON_NONE;
}

const char *td_netlink_pair_window(void)
{
    td_netlink_reason_t result;
    if (!command(CMD_PAIR, &result)) {
        return "net not ready";
    }
    switch (result) {
    case TD_NETLINK_REASON_NONE:
        return NULL;
    case TD_NETLINK_FULL:
        return "four apps are paired; run net forget first";
    default:
        return "a pairing window is already open";
    }
}
