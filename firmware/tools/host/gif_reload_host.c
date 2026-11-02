#include "gif_ui_host.h"
#include <stdio.h>

#define LOCK_TIMEOUT_MS 3000
#define ESP_LOGW(tag, ...) ((void)(tag))
static const char *LAYOUT_TAG = "layout";
typedef int SemaphoreHandle_t;
typedef int ui_icon_counts_t;
typedef struct { char name[32]; char error[96]; } ui_layout_info_t;
static struct {
    SemaphoreHandle_t reload_lock;
    char orientation[16];
    td_layout_t *layout;
    ui_layout_info_t info;
} s;

static void make_locks(void) {}
static void xSemaphoreTake(SemaphoreHandle_t lock, unsigned timeout) { (void)lock; (void)timeout; }
static void xSemaphoreGive(SemaphoreHandle_t lock) { (void)lock; }
static bool td_recovery_mode(void) { return false; }
const td_layout_t *td_layout_builtin(void) { return NULL; }
void td_layout_path(const char *orientation, char *buf, size_t cap) { snprintf(buf, cap, "%s.json", orientation); }
bool td_layout_load_file(const char *path, td_layout_t **out, size_t *file_bytes, td_layout_error_t *err)
{
    (void)path; (void)err;
    *out = host_stored_layout();
    *file_bytes = 1;
    return true;
}
void td_layout_free(td_layout_t *layout) { (void)layout; }
static void layout_lock(void) {}
static void layout_unlock(void) {}
static void describe_error(char *buf, size_t cap, const td_layout_error_t *err) { snprintf(buf, cap, "%s", err->key); }
static ui_icon_counts_t ui_icon_prepare(const td_layout_t *layout, const td_layout_t *keep)
{
    (void)layout; (void)keep; return 0;
}
static void fill_info(ui_layout_info_t *info, const td_layout_t *layout, bool from_storage, size_t bytes,
                      ui_icon_counts_t icons)
{
    (void)layout; (void)from_storage; (void)bytes; (void)icons;
    snprintf(info->name, sizeof info->name, "stored");
}
static void load_locked(const td_layout_t *layout, const ui_layout_info_t *info, int64_t started_us)
{
    (void)started_us;
    s.layout = (td_layout_t *)layout;
    s.info = *info;
    host_load_locked(layout);
}
#include "ui_layout_reload.c"
