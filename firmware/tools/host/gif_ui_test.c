#include "gif_ui_host.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static portMUX_TYPE lv_lock = portMUX_INITIALIZER_UNLOCKED;
static atomic_uint allocations, frees, notifications;
static atomic_bool hold_worker, worker_held;
static _Thread_local bool is_worker;
static _Thread_local unsigned critical_depth;
static lv_obj_t images[64];
static unsigned image_count;
static void (*timer_callback)(lv_timer_t *);
static power_info_t power_state;
static void (*worker_fn)(void *);

void host_lock(portMUX_TYPE *lock)
{
#ifdef _WIN32
    AcquireSRWLockExclusive(lock);
#else
    pthread_mutex_lock(lock);
#endif
    critical_depth++;
}
void host_unlock(portMUX_TYPE *lock)
{
    critical_depth--;
#ifdef _WIN32
    ReleaseSRWLockExclusive(lock);
#else
    pthread_mutex_unlock(lock);
#endif
}
static void sleep_ms(unsigned ms)
{
#ifdef _WIN32
    Sleep(ms);
#else
    struct timespec duration = {ms / 1000, (long)(ms % 1000) * 1000000};
    nanosleep(&duration, NULL);
#endif
}
int64_t esp_timer_get_time(void)
{
#ifdef _WIN32
    return (int64_t)GetTickCount64() * 1000;
#else
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (int64_t)now.tv_sec * 1000000 + now.tv_nsec / 1000;
#endif
}
void *heap_caps_malloc(size_t bytes, unsigned caps)
{
    assert(!critical_depth && caps == (MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT));
    void *p = malloc(bytes);
    assert(p);
    atomic_fetch_add(&allocations, 1);
    return p;
}
void heap_caps_free(void *ptr)
{
    if (!ptr) return;
    assert(!critical_depth);
    for (unsigned i = 0; i < 64; i++) assert(atomic_load(&images[i].pixels) != ptr);
    atomic_fetch_add(&frees, 1);
    free(ptr);
}
void vTaskDelay(unsigned ticks)
{
    assert(!critical_depth);
    while (is_worker && atomic_load(&hold_worker)) {
        atomic_store(&worker_held, true);
        sleep_ms(1);
    }
    atomic_store(&worker_held, false);
    sleep_ms(ticks);
}
unsigned ulTaskNotifyTake(int clear, unsigned timeout)
{
    (void)clear; (void)timeout;
    while (!atomic_exchange(&notifications, 0)) sleep_ms(1);
    return 1;
}
void xTaskNotifyGive(TaskHandle_t task) { (void)task; atomic_fetch_add(&notifications, 1); }
#ifdef _WIN32
static DWORD WINAPI run_worker(void *ctx)
#else
static void *run_worker(void *ctx)
#endif
{
    is_worker = true;
    worker_fn(ctx);
    return 0;
}
int xTaskCreatePinnedToCore(void (*fn)(void *), const char *name, unsigned stack, void *ctx,
                          unsigned priority, TaskHandle_t *task, unsigned core)
{
    (void)name; (void)stack;
    assert(priority == 1 && core == 1);
    worker_fn = fn;
#ifdef _WIN32
    *task = CreateThread(NULL, 0, run_worker, ctx, 0, NULL);
    assert(*task);
#else
    static pthread_t thread;
    assert(pthread_create(&thread, NULL, run_worker, ctx) == 0);
    *task = &thread;
#endif
    return pdPASS;
}
int esp_lv_adapter_lock(unsigned timeout) { (void)timeout; host_lock(&lv_lock); return ESP_OK; }
void esp_lv_adapter_unlock(void) { host_unlock(&lv_lock); }
lv_obj_t *lv_image_create(lv_obj_t *parent) { (void)parent; assert(!is_worker); return &images[image_count++]; }
void lv_image_set_src(lv_obj_t *obj, const lv_image_dsc_t *src)
{
    assert(!is_worker);
    if (src) assert(src->header.w && src->header.h && src->data_size == src->header.w * src->header.h * 3);
    atomic_store(&obj->pixels, src ? src->data : NULL);
}
void lv_image_cache_drop(const void *src) { (void)src; assert(!is_worker); }
void lv_image_header_cache_drop(const void *src) { (void)src; assert(!is_worker); }
void lv_obj_center(lv_obj_t *obj) { (void)obj; assert(!is_worker); }
void lv_obj_set_clickable(lv_obj_t *obj, bool value) { (void)obj; (void)value; }
lv_timer_t *lv_timer_create(void (*cb)(lv_timer_t *), unsigned period, void *ctx)
{
    (void)period; (void)ctx; timer_callback = cb; return (lv_timer_t *)&timer_callback;
}
int power_get_info(power_info_t *info, int64_t now) { (void)now; *info = power_state; return ESP_OK; }
bool td_asset_id_valid(const char *id) { return id && strlen(id) == 64; }
static void pump(unsigned ms)
{
    for (unsigned n = 0; n < ms; n += 5) {
        esp_lv_adapter_lock(1);
        if (timer_callback) timer_callback(NULL);
        esp_lv_adapter_unlock();
        sleep_ms(5);
    }
}
static void wait_picture(unsigned image)
{
    for (unsigned n = 0; n < 1000 && !atomic_load(&images[image].pixels); n++) pump(5);
    assert(atomic_load(&images[image].pixels));
}
int main(void)
{
    ui_tile_t tile = {.render = UI_RENDER_GIF,
        .icon = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"};
    ui_page_t pages[2] = {{.tile_count = 1, .tiles = &tile}, {.tile_count = 1, .tiles = &tile}};
    td_layout_t layout = {.page_count = 2, .pages = pages};
    ui_gif_prepare(&layout);
    esp_lv_adapter_lock(1);
    ui_gif_begin();
    assert(ui_gif_build(NULL, &tile, 0));
    assert(ui_gif_build(NULL, &tile, 1));
    ui_gif_resume();
    esp_lv_adapter_unlock();
    wait_picture(0);
    const uint8_t *poster = atomic_load(&images[0].pixels);
    for (unsigned n = 0; n < 400 && atomic_load(&images[0].pixels) == poster; n++) pump(5);
    assert(atomic_load(&images[0].pixels) != poster);
    atomic_store(&hold_worker, true);
    for (unsigned n = 0; n < 1000 && !atomic_load(&worker_held); n++) sleep_ms(1);
    assert(atomic_load(&worker_held));
    esp_lv_adapter_lock(1);
    ui_gif_page(1, true);
    esp_lv_adapter_unlock();
    assert(atomic_load(&images[0].pixels) == poster);
    atomic_store(&hold_worker, false);
    wait_picture(1);
    const uint8_t *second_poster = atomic_load(&images[1].pixels);
    pump(150);
    assert(atomic_load(&images[1].pixels) == second_poster);
    esp_lv_adapter_lock(1);
    ui_gif_page(1, false);
    esp_lv_adapter_unlock();
    pump(150);
    for (unsigned mode = 0; mode < 3; mode++) {
        esp_lv_adapter_lock(1);
        if (mode == 0) power_state.idle_face = true;
        if (mode == 1) power_state.state = POWER_OFF;
        if (mode == 2) ui_gif_set_dimmed(true);
        esp_lv_adapter_unlock();
        pump(150);
        assert(atomic_load(&images[1].pixels) == second_poster);
        esp_lv_adapter_lock(1);
        power_state = (power_info_t){0};
        ui_gif_set_dimmed(false);
        esp_lv_adapter_unlock();
        pump(150);
    }
    assert(ui_gif_quiesce());
    assert(!atomic_load(&images[0].pixels) && !atomic_load(&images[1].pixels));
    assert(atomic_load(&allocations) == atomic_load(&frees));
    for (unsigned repeat = 0; repeat < 5; repeat++) {
        const unsigned index = image_count;
        ui_gif_prepare(&layout);
        esp_lv_adapter_lock(1);
        ui_gif_begin();
        assert(ui_gif_build(NULL, &tile, 0));
        assert(ui_gif_build(NULL, &tile, 1));
        ui_gif_resume();
        esp_lv_adapter_unlock();
        wait_picture(index);
        pump(20);
        assert(ui_gif_quiesce());
        assert(atomic_load(&allocations) == atomic_load(&frees));
    }
    puts("LVGL handoff, blocked-worker Page change, slide/idle/off/dim posters and Layout teardown: OK");
    return 0;
}
