#include "gif_ui_host.h"
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static portMUX_TYPE lv_lock = portMUX_INITIALIZER_UNLOCKED;
static atomic_uint allocations, frees, notifications, worker_wakes, image_count;
static atomic_uint slow_locks, fail_slow_lock, fail_worker_allocations, fail_poster_allocations;
static _Atomic(size_t) poster_bytes;
static _Atomic(int64_t) clock_offset_us;
static atomic_bool hold_worker, worker_held;
static _Thread_local bool is_worker, holds_lv;
static _Thread_local unsigned critical_depth;
static lv_obj_t images[128];
static void (*timer_callback)(lv_timer_t *);
static power_info_t power_state;
static void (*worker_fn)(void *);
static td_layout_t *stored;
static unsigned first_image;
static uint8_t *frame_one;

static void raw_lock(portMUX_TYPE *lock)
{
#ifdef _WIN32
    AcquireSRWLockExclusive(lock);
#else
    pthread_mutex_lock(lock);
#endif
}
static void raw_unlock(portMUX_TYPE *lock)
{
#ifdef _WIN32
    ReleaseSRWLockExclusive(lock);
#else
    pthread_mutex_unlock(lock);
#endif
}
void host_lock(portMUX_TYPE *lock) { raw_lock(lock); critical_depth++; }
void host_unlock(portMUX_TYPE *lock) { critical_depth--; raw_unlock(lock); }
static void sleep_ms(unsigned ms)
{
#ifdef _WIN32
    Sleep(ms);
#else
    struct timespec duration = {ms / 1000, (long)(ms % 1000) * 1000000};
    nanosleep(&duration, NULL);
#endif
}
static int64_t real_us(void)
{
#ifdef _WIN32
    return (int64_t)GetTickCount64() * 1000;
#else
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (int64_t)now.tv_sec * 1000000 + now.tv_nsec / 1000;
#endif
}
int64_t esp_timer_get_time(void) { return real_us() + atomic_load(&clock_offset_us); }
static bool take(atomic_uint *count)
{
    unsigned n = atomic_load(count);
    while (n && !atomic_compare_exchange_weak(count, &n, n - 1)) {}
    return n != 0;
}
static bool refuse(size_t bytes)
{
    if (is_worker && take(&fail_worker_allocations)) return true;
    return bytes == atomic_load(&poster_bytes) && take(&fail_poster_allocations);
}
static void *counted(void *ptr)
{
    assert(ptr);
    atomic_fetch_add(&allocations, 1);
    return ptr;
}
void *heap_caps_malloc(size_t bytes, uint32_t caps)
{
    assert(!critical_depth && caps == (MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT));
    return refuse(bytes) ? NULL : counted(malloc(bytes));
}
void *heap_caps_calloc(size_t count, size_t bytes, uint32_t caps)
{
    assert(!critical_depth && caps == (MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT));
    return refuse(count * bytes) ? NULL : counted(calloc(count, bytes));
}
void heap_caps_free(void *ptr)
{
    if (!ptr) return;
    assert(!critical_depth);
    for (unsigned i = 0; i < atomic_load(&image_count); i++) assert(atomic_load(&images[i].pixels) != ptr);
    atomic_fetch_add(&frees, 1);
    free(ptr);
}
static void park(void)
{
    while (is_worker && atomic_load(&hold_worker)) {
        atomic_store(&worker_held, true);
        sleep_ms(1);
    }
    if (is_worker) atomic_store(&worker_held, false);
}
void vTaskDelay(TickType_t ticks)
{
    assert(!critical_depth);
    park();
    if (is_worker) atomic_fetch_add(&worker_wakes, 1);
    sleep_ms(ticks);
}
unsigned ulTaskNotifyTake(int clear, TickType_t timeout)
{
    (void)clear;
    assert(!critical_depth && is_worker);
    park();
    const int64_t start = esp_timer_get_time();
    while (!atomic_exchange(&notifications, 0)) {
        if (timeout != portMAX_DELAY && esp_timer_get_time() - start >= (int64_t)timeout * 1000) break;
        sleep_ms(1);
    }
    atomic_fetch_add(&worker_wakes, 1);
    park();
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
int esp_lv_adapter_lock(int timeout_ms)
{
    assert(!holds_lv && !is_worker);
    if (timeout_ms > 1 && atomic_fetch_add(&slow_locks, 1) + 1 == atomic_load(&fail_slow_lock)) return -1;
    raw_lock(&lv_lock);
    holds_lv = true;
    return ESP_OK;
}
void esp_lv_adapter_unlock(void)
{
    assert(holds_lv);
    holds_lv = false;
    raw_unlock(&lv_lock);
}
static void lvgl_only(void) { assert(holds_lv && !is_worker); }
lv_obj_t *lv_image_create(lv_obj_t *parent)
{
    (void)parent;
    lvgl_only();
    const unsigned index = atomic_fetch_add(&image_count, 1);
    assert(index < sizeof images / sizeof images[0]);
    return &images[index];
}
void lv_image_set_src(lv_obj_t *obj, const lv_image_dsc_t *src)
{
    lvgl_only();
    if (src) assert(src->header.w && src->header.h && src->data_size == (size_t)src->header.w * src->header.h * 3);
    atomic_store(&obj->pixels, src ? src->data : NULL);
}
void lv_image_cache_drop(const void *src) { (void)src; lvgl_only(); }
void lv_image_header_cache_drop(const void *src) { (void)src; lvgl_only(); }
void lv_obj_center(lv_obj_t *obj) { (void)obj; lvgl_only(); }
void lv_obj_set_clickable(lv_obj_t *obj, bool value) { (void)obj; (void)value; lvgl_only(); }
lv_timer_t *lv_timer_create(void (*cb)(lv_timer_t *), unsigned period, void *ctx)
{
    (void)period; (void)ctx;
    lvgl_only();
    timer_callback = cb;
    return (lv_timer_t *)&timer_callback;
}
int power_get_info(power_info_t *info, int64_t now) { (void)now; lvgl_only(); *info = power_state; return ESP_OK; }
bool td_asset_id_valid(const char *id) { return id && strlen(id) == 64; }

static void lvgl_lock(void) { assert(esp_lv_adapter_lock(1) == ESP_OK); }
static void pump(unsigned ms)
{
    for (unsigned n = 0; n < ms; n += 5) {
        lvgl_lock();
        if (timer_callback) timer_callback(NULL);
        esp_lv_adapter_unlock();
        sleep_ms(5);
    }
}
static const uint8_t *shown(unsigned image) { return atomic_load(&images[image].pixels); }
static unsigned live(void) { return atomic_load(&allocations) - atomic_load(&frees); }
static bool leaves(unsigned image, const uint8_t *picture, unsigned ms)
{
    for (const int64_t end = real_us() + (int64_t)ms * 1000; real_us() < end && shown(image) == picture;) pump(5);
    return shown(image) != picture;
}
static bool reaches(unsigned image, const uint8_t *picture, unsigned ms)
{
    for (const int64_t end = real_us() + (int64_t)ms * 1000; real_us() < end && shown(image) != picture;) pump(5);
    return shown(image) == picture;
}
static void stays(unsigned image, const uint8_t *picture, unsigned ms)
{
    for (const int64_t end = real_us() + (int64_t)ms * 1000; real_us() < end;) {
        assert(shown(image) == picture);
        pump(5);
    }
    assert(shown(image) == picture);
}
static unsigned settled_live(void)
{
    unsigned last = live();
    int64_t since = real_us();
    for (const int64_t end = real_us() + 3000000; real_us() < end && real_us() - since < 200000;) {
        pump(5);
        if (live() != last) { last = live(); since = real_us(); }
    }
    assert(real_us() - since >= 200000);
    return last;
}
static void quiet_window(void)
{
    unsigned last = atomic_load(&worker_wakes);
    int64_t since = real_us();
    for (const int64_t end = real_us() + 3000000; real_us() < end && real_us() - since < 150000;) {
        pump(5);
        if (atomic_load(&worker_wakes) != last) { last = atomic_load(&worker_wakes); since = real_us(); }
    }
    atomic_fetch_add(&clock_offset_us, 10 * 1000000);
    pump(300);
    assert(atomic_load(&worker_wakes) == last);
}
static void set_power(unsigned state, bool idle_face)
{
    lvgl_lock();
    power_state = (power_info_t){state, idle_face};
    esp_lv_adapter_unlock();
}
static void set_page(uint8_t page, bool sliding)
{
    lvgl_lock();
    ui_gif_page(page, sliding);
    esp_lv_adapter_unlock();
}
static void hold(void)
{
    atomic_store(&hold_worker, true);
    for (unsigned n = 0; n < 2000 && !atomic_load(&worker_held); n++) sleep_ms(1);
    assert(atomic_load(&worker_held));
}

td_layout_t *host_stored_layout(void) { return stored; }
void host_load_locked(const td_layout_t *layout)
{
    ui_gif_begin();
    first_image = atomic_load(&image_count);
    for (unsigned p = 0; p < layout->page_count; p++) {
        for (unsigned t = 0; t < layout->pages[p].tile_count; t++) {
            const ui_tile_t *tile = &layout->pages[p].tiles[t];
            if (tile->render == UI_RENDER_GIF) ui_gif_build(NULL, tile, (uint8_t)p);
        }
    }
    ui_gif_resume();
}
static unsigned load(td_layout_t *layout)
{
    stored = layout;
    atomic_store(&fail_slow_lock, 0);
    td_layout_error_t err = {0};
    assert(ui_layout_reload(false, &err));
    return first_image;
}

#define ASSET_ID "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
static ui_tile_t gif_tiles[3] = {{.render = UI_RENDER_GIF, .icon = "sha256:" ASSET_ID},
                                 {.render = UI_RENDER_GIF, .icon = "sha256:" ASSET_ID},
                                 {.render = UI_RENDER_GIF, .icon = "sha256:" ASSET_ID}};
static ui_page_t two_pages[2] = {{.tile_count = 1, .tiles = gif_tiles}, {.tile_count = 1, .tiles = gif_tiles}};
static td_layout_t layout_a = {.page_count = 2, .pages = two_pages};
static ui_page_t three_tiles[1] = {{.tile_count = 3, .tiles = gif_tiles}};
static td_layout_t layout_b = {.page_count = 1, .pages = three_tiles};
static ui_page_t reused[1] = {{.tile_count = 2, .tiles = gif_tiles}};
static td_layout_t layout_reused = {.page_count = 1, .pages = reused};
static ui_page_t no_tiles[1] = {{.tile_count = 0, .tiles = gif_tiles}};
static td_layout_t layout_empty = {.page_count = 1, .pages = no_tiles};

static void unload(void)
{
    load(&layout_empty);
    assert(live() == 0);
}
static bool keep_going(void *ctx) { (void)ctx; return true; }
static void read_fixture(void)
{
    FILE *file = fopen("assets/a/" ASSET_ID ".bin", "rb");
    assert(file);
    static uint8_t data[TD_GIF_MAX_BYTES];
    const size_t bytes = fread(data, 1, sizeof data, file);
    assert(bytes && fclose(file) == 0);
    td_gif_t *gif = NULL;
    assert(td_gif_open(data, (uint32_t)bytes, keep_going, NULL, &gif) == TD_GIF_OK);
    const td_gif_info_t *info = td_gif_info(gif);
    const size_t size = (size_t)info->width * info->height * 3;
    assert(info->frames >= 3);
    atomic_store(&poster_bytes, size);
    frame_one = malloc(size);
    uint8_t *other = malloc(size);
    assert(frame_one && other);
    assert(td_gif_render(gif, 1, frame_one, size, keep_going, NULL) == TD_GIF_OK);
    assert(td_gif_render(gif, 2, other, size, keep_going, NULL) == TD_GIF_OK);
    assert(memcmp(frame_one, other, size));
    free(other);
    td_gif_close(gif);
    assert(live() == 0);
}

static void scenario_playback(void)
{
    const unsigned a = load(&layout_a);
    const uint8_t *poster = shown(a), *second = shown(a + 1);
    assert(poster && second);
    assert(leaves(a, poster, 2000));
    hold();
    set_page(1, true);
    assert(shown(a) == poster);
    atomic_store(&hold_worker, false);
    stays(a + 1, second, 300);
    set_page(1, false);
    assert(leaves(a + 1, second, 2000));
    for (unsigned mode = 0; mode < 3; mode++) {
        if (mode == 0) set_power(0, true);
        if (mode == 1) set_power(POWER_OFF, false);
        if (mode == 2) ui_gif_set_dimmed(true);
        assert(reaches(a + 1, second, 500));
        stays(a + 1, second, 300);
        set_power(0, false);
        ui_gif_set_dimmed(false);
        assert(leaves(a + 1, second, 2000));
    }
    unload();
    assert(!shown(a) && !shown(a + 1));
    for (unsigned repeat = 0; repeat < 5; repeat++) {
        const unsigned index = load(&layout_a);
        assert(shown(index));
        pump(20);
        unload();
    }
}

static void scenario_sleep(void)
{
    const unsigned a = load(&layout_a);
    const uint8_t *second = shown(a + 1);
    set_page(1, false);
    assert(leaves(a + 1, second, 2000));
    set_page(1, true);
    assert(reaches(a + 1, second, 500));
    quiet_window();
    set_page(1, false);
    assert(leaves(a + 1, second, 2000));
    ui_gif_set_dimmed(true);
    assert(reaches(a + 1, second, 500));
    quiet_window();
    ui_gif_set_dimmed(false);
    assert(leaves(a + 1, second, 2000));
    set_power(POWER_OFF, false);
    assert(reaches(a + 1, second, 500));
    quiet_window();
    set_power(0, false);
    assert(leaves(a + 1, second, 2000));
    unload();
}

static void scenario_release(void)
{
    const unsigned a = load(&layout_a);
    const uint8_t *second = shown(a + 1);
    set_page(1, false);
    assert(leaves(a + 1, second, 2000));
    for (unsigned mode = 0; mode < 2; mode++) {
        set_power(mode ? 0 : POWER_OFF, mode == 1);
        assert(reaches(a + 1, second, 500));
        assert(settled_live() == 2);
        set_power(0, false);
        assert(leaves(a + 1, second, 2000));
        lvgl_lock();
        assert(!memcmp(shown(a + 1), frame_one, atomic_load(&poster_bytes)));
        esp_lv_adapter_unlock();
        assert(live() > 2);
    }
    unload();
}

static void scenario_memory(void)
{
    const unsigned a = load(&layout_a);
    const uint8_t *second = shown(a + 1);
    set_page(1, false);
    assert(leaves(a + 1, second, 2000));
    set_page(0, false);
    assert(leaves(a, shown(a), 2000));
    settled_live();
    hold();
    set_page(1, false);
    atomic_store(&fail_worker_allocations, 1);
    atomic_store(&hold_worker, false);
    assert(shown(a + 1) == second);
    assert(leaves(a + 1, second, 4000));
    assert(atomic_load(&fail_worker_allocations) == 0);
    unload();
}

static void scenario_reused(void)
{
    atomic_store(&fail_poster_allocations, 1);
    const unsigned before = atomic_load(&image_count);
    load(&layout_reused);
    assert(atomic_load(&fail_poster_allocations) == 0);
    assert(atomic_load(&image_count) == before + 1);
    assert(shown(before));
    ui_tile_t bad = {.render = UI_RENDER_GIF, .icon = "x"};
    lvgl_lock();
    assert(!ui_gif_build(NULL, &bad, 0));
    esp_lv_adapter_unlock();
    unload();
}

static void scenario_reload(void)
{
    const unsigned a = load(&layout_a);
    assert(leaves(a, shown(a), 2000));
    set_power(POWER_OFF, false);
    const unsigned resting = settled_live();
    set_power(0, false);
    bool refused = false, loaded = false;
    for (unsigned k = 1; k <= 4 && !loaded; k++) {
        stored = &layout_b;
        atomic_store(&slow_locks, 0);
        atomic_store(&fail_slow_lock, k);
        const unsigned before = atomic_load(&image_count);
        td_layout_error_t err = {0};
        loaded = ui_layout_reload(false, &err);
        atomic_store(&fail_slow_lock, 0);
        if (loaded) {
            assert(atomic_load(&image_count) == before + 3 && !shown(a));
            break;
        }
        refused = true;
        assert(!strcmp(err.key, "display.busy") && atomic_load(&image_count) == before);
        assert(shown(a));
        assert(leaves(a, shown(a), 2000));
        set_power(POWER_OFF, false);
        assert(settled_live() == resting);
        set_power(0, false);
    }
    assert(refused && loaded);
    unload();
}

int main(int argc, char **argv)
{
    assert(argc == 2);
    read_fixture();
    static const struct { const char *name; void (*run)(void); } scenarios[] = {
        {"playback", scenario_playback}, {"sleep", scenario_sleep}, {"release", scenario_release},
        {"memory", scenario_memory}, {"reused", scenario_reused}, {"reload", scenario_reload},
    };
    for (unsigned i = 0; i < sizeof scenarios / sizeof scenarios[0]; i++) {
        if (strcmp(argv[1], scenarios[i].name)) continue;
        scenarios[i].run();
        printf("%s: OK\n", argv[1]);
        return 0;
    }
    return 2;
}
