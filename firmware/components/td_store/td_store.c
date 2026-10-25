#include "td_store.h"
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdatomic.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include "esp_littlefs.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"

#define STORE_LABEL "storage"
#define TMP_SUFFIX ".tmp"
#define WRITE_CHUNK 4096
#define TMP_SCAN_DEPTH 3
#define CHILD_PATH_MAX 160
#define SETTINGS_FILE_MAX 512

static const char *TAG = "store";

static SemaphoreHandle_t s_lock;
static atomic_bool s_mounted;
static atomic_bool s_formatted;

/* Settings are read and written under the store lock, so one buffer serves both. */
static char s_settings_buf[SETTINGS_FILE_MAX];

/* The store is shared by the console, the link and boot. The lock is recursive because a list callback may walk a
 * directory below the one it was called for. */
static void lock(void)
{
    xSemaphoreTakeRecursive(s_lock, portMAX_DELAY);
}

static void unlock(void)
{
    xSemaphoreGiveRecursive(s_lock);
}

static bool has_tmp_suffix(const char *s)
{
    const size_t len = strlen(s);
    const size_t suffix = sizeof(TMP_SUFFIX) - 1;
    return len > suffix && strcmp(s + len - suffix, TMP_SUFFIX) == 0;
}

static esp_err_t errno_to_err(int err)
{
    switch (err) {
    case ENOENT:
        return ESP_ERR_NOT_FOUND;
    case ENOTDIR:
    case EISDIR:
        return ESP_ERR_INVALID_ARG;
    default:
        return ESP_FAIL;
    }
}

static bool join(char *out, size_t cap, const char *dir, const char *name)
{
    const int n = snprintf(out, cap, "%s/%s", dir, name);
    return n > 0 && (size_t)n < cap;
}

/* Restarts the scan after each removal, because littlefs may skip entries of a directory that changes under a walk. */
static unsigned remove_stale_tmp(const char *dir, int depth)
{
    unsigned removed = 0;
    for (bool again = true; again;) {
        again = false;
        DIR *d = opendir(dir);
        if (d == NULL) {
            break;
        }
        char victim[CHILD_PATH_MAX] = "";
        struct dirent *e;
        while (victim[0] == '\0' && (e = readdir(d)) != NULL) {
            char child[CHILD_PATH_MAX];
            if (!join(child, sizeof child, dir, e->d_name)) {
                continue;
            }
            if (e->d_type == DT_DIR) {
                if (depth > 0) {
                    removed += remove_stale_tmp(child, depth - 1);
                }
            } else if (has_tmp_suffix(e->d_name)) {
                strlcpy(victim, child, sizeof victim);
            }
        }
        closedir(d);
        if (victim[0] != '\0' && unlink(victim) == 0) {
            removed++;
            again = true;
        }
    }
    return removed;
}

esp_err_t td_store_mount(bool *formatted)
{
    if (s_lock != NULL) {
        return ESP_ERR_INVALID_STATE;
    }
    s_lock = xSemaphoreCreateRecursiveMutex();
    if (s_lock == NULL) {
        return ESP_ERR_NO_MEM;
    }

    const esp_vfs_littlefs_conf_t conf = {
        .base_path = TD_STORE_ROOT,
        .partition_label = STORE_LABEL,
        .format_if_mount_failed = false,
    };
    /* The library formats a volume that will not mount only when asked, and then says nothing to the caller, so the
     * format is a separate step here to tell a first boot from a later one. */
    bool did_format = false;
    esp_err_t err = esp_vfs_littlefs_register(&conf);
    if (err == ESP_FAIL) {
        ESP_LOGW(TAG, "volume does not mount, formatting it");
        err = esp_littlefs_format(STORE_LABEL);
        if (err == ESP_OK) {
            did_format = true;
            err = esp_vfs_littlefs_register(&conf);
        }
    }
    if (err != ESP_OK) {
        ESP_LOGE(TAG, "mount failed: %s", esp_err_to_name(err));
        return err == ESP_ERR_NOT_FOUND ? err : ESP_FAIL;
    }
    atomic_store(&s_mounted, true);
    atomic_store(&s_formatted, did_format);
    if (formatted != NULL) {
        *formatted = did_format;
    }

    if (did_format) {
        ESP_LOGI(TAG, "formatted");
    }
    const unsigned removed = remove_stale_tmp(TD_STORE_ROOT, TMP_SCAN_DEPTH);
    if (removed > 0) {
        ESP_LOGW(TAG, "removed %u stale temporary file(s)", removed);
    }
    td_store_info_t info;
    if (td_store_info(&info) == ESP_OK) {
        ESP_LOGI(TAG, "mounted, %u/%u bytes", (unsigned)info.used_bytes, (unsigned)info.total_bytes);
    } else {
        ESP_LOGW(TAG, "mounted, usage unknown");
    }
    return ESP_OK;
}

bool td_store_mounted(void)
{
    return atomic_load(&s_mounted);
}

bool td_store_formatted(void)
{
    return atomic_load(&s_formatted);
}

void td_store_lock(void)
{
    lock();
}

void td_store_unlock(void)
{
    unlock();
}

esp_err_t td_store_info(td_store_info_t *out)
{
    if (!td_store_mounted()) {
        return ESP_ERR_INVALID_STATE;
    }
    size_t total = 0;
    size_t used = 0;
    lock();
    const esp_err_t err = esp_littlefs_info(STORE_LABEL, &total, &used);
    unlock();
    if (err != ESP_OK) {
        return err;
    }
    out->total_bytes = (uint32_t)total;
    out->used_bytes = (uint32_t)used;
    return ESP_OK;
}

esp_err_t td_store_stat(const char *path, td_store_stat_t *out)
{
    if (!td_store_path_valid(path, true)) {
        return ESP_ERR_INVALID_ARG;
    }
    if (!td_store_mounted()) {
        return ESP_ERR_INVALID_STATE;
    }
    struct stat st;
    lock();
    const int rc = stat(path, &st);
    const int saved = errno;
    unlock();
    if (rc != 0) {
        return errno_to_err(saved);
    }
    out->is_dir = S_ISDIR(st.st_mode);
    out->size = out->is_dir ? 0 : (uint32_t)st.st_size;
    return ESP_OK;
}

esp_err_t td_store_list(const char *dir, td_store_list_cb_t cb, void *ctx)
{
    if (cb == NULL || !td_store_path_valid(dir, true)) {
        return ESP_ERR_INVALID_ARG;
    }
    if (!td_store_mounted()) {
        return ESP_ERR_INVALID_STATE;
    }
    lock();
    DIR *d = opendir(dir);
    if (d == NULL) {
        const esp_err_t err = errno_to_err(errno);
        unlock();
        return err;
    }
    struct dirent *e;
    while ((e = readdir(d)) != NULL) {
        const bool is_dir = e->d_type == DT_DIR;
        uint32_t size = 0;
        char child[CHILD_PATH_MAX];
        struct stat st;
        if (!is_dir && join(child, sizeof child, dir, e->d_name) && stat(child, &st) == 0) {
            size = (uint32_t)st.st_size;
        }
        if (!cb(ctx, e->d_name, size, is_dir)) {
            break;
        }
    }
    closedir(d);
    unlock();
    return ESP_OK;
}

esp_err_t td_store_read(const char *path, void *buf, size_t max, size_t *out_len)
{
    if (buf == NULL || out_len == NULL || !td_store_path_valid(path, false)) {
        return ESP_ERR_INVALID_ARG;
    }
    if (!td_store_mounted()) {
        return ESP_ERR_INVALID_STATE;
    }
    esp_err_t err = ESP_OK;
    lock();
    struct stat st;
    int fd = -1;
    if (stat(path, &st) != 0) {
        err = errno_to_err(errno);
    } else if (S_ISDIR(st.st_mode)) {
        err = ESP_ERR_INVALID_ARG;
    } else if ((size_t)st.st_size > max) {
        *out_len = (size_t)st.st_size;
        err = ESP_ERR_INVALID_SIZE;
    } else if ((fd = open(path, O_RDONLY)) < 0) {
        err = errno_to_err(errno);
    } else {
        size_t got = 0;
        while (got < (size_t)st.st_size) {
            const ssize_t n = read(fd, (uint8_t *)buf + got, (size_t)st.st_size - got);
            if (n <= 0) {
                err = ESP_FAIL;
                break;
            }
            got += (size_t)n;
        }
        if (err == ESP_OK) {
            *out_len = got;
        }
    }
    if (fd >= 0) {
        close(fd);
    }
    unlock();
    return err;
}

esp_err_t td_store_read_chunks(const char *path, size_t chunk, td_store_chunk_cb_t cb, void *ctx)
{
    if (cb == NULL || chunk == 0 || chunk > TD_STORE_CHUNK_MAX || !td_store_path_valid(path, false)) {
        return ESP_ERR_INVALID_ARG;
    }
    if (!td_store_mounted()) {
        return ESP_ERR_INVALID_STATE;
    }
    uint8_t buf[TD_STORE_CHUNK_MAX];
    esp_err_t err = ESP_OK;
    lock();
    const int fd = open(path, O_RDONLY);
    if (fd < 0) {
        err = errno_to_err(errno);
    } else {
        for (;;) {
            const ssize_t n = read(fd, buf, chunk);
            if (n < 0) {
                err = ESP_FAIL;
                break;
            }
            if (n == 0) {
                break;
            }
            if (!cb(ctx, buf, (size_t)n)) {
                err = ESP_ERR_INVALID_RESPONSE;
                break;
            }
        }
        close(fd);
    }
    unlock();
    return err;
}

/* Creates every directory above path under the mount point. */
static bool make_parents(const char *path)
{
    char dir[TD_STORE_PATH_MAX];
    strlcpy(dir, path, sizeof dir);
    for (char *p = dir + sizeof(TD_STORE_ROOT); *p != '\0'; p++) {
        if (*p != '/') {
            continue;
        }
        *p = '\0';
        if (mkdir(dir, 0777) != 0 && errno != EEXIST) {
            return false;
        }
        *p = '/';
    }
    return true;
}

static bool write_all(int fd, const uint8_t *data, size_t len)
{
    size_t done = 0;
    while (done < len) {
        const size_t want = len - done < WRITE_CHUNK ? len - done : WRITE_CHUNK;
        const ssize_t n = write(fd, data + done, want);
        if (n <= 0) {
            return false;
        }
        done += (size_t)n;
    }
    return true;
}

esp_err_t td_store_write_atomic(const char *path, const void *data, size_t len)
{
    if ((data == NULL && len > 0) || !td_store_path_valid(path, false) || has_tmp_suffix(path)) {
        return ESP_ERR_INVALID_ARG;
    }
    if (!td_store_mounted()) {
        return ESP_ERR_INVALID_STATE;
    }
    char tmp[TD_STORE_PATH_MAX + sizeof(TMP_SUFFIX)];
    snprintf(tmp, sizeof tmp, "%s%s", path, TMP_SUFFIX);

    lock();
    bool ok = make_parents(path);
    int fd = ok ? open(tmp, O_WRONLY | O_CREAT | O_TRUNC, 0666) : -1;
    if (fd >= 0) {
        ok = write_all(fd, data, len) && fsync(fd) == 0;
        ok = close(fd) == 0 && ok;
    } else {
        ok = false;
    }
    ok = ok && rename(tmp, path) == 0;
    if (!ok) {
        ESP_LOGW(TAG, "write of %s failed, errno %d", path, errno);
        unlink(tmp);
    }
    unlock();
    return ok ? ESP_OK : ESP_FAIL;
}

esp_err_t td_store_remove(const char *path)
{
    if (!td_store_path_valid(path, false)) {
        return ESP_ERR_INVALID_ARG;
    }
    if (!td_store_mounted()) {
        return ESP_ERR_INVALID_STATE;
    }
    lock();
    struct stat st;
    int rc = stat(path, &st);
    if (rc == 0) {
        rc = S_ISDIR(st.st_mode) ? rmdir(path) : unlink(path);
    }
    const int saved = errno;
    unlock();
    if (rc == 0) {
        return ESP_OK;
    }
    return saved == ENOENT ? ESP_ERR_NOT_FOUND : ESP_FAIL;
}

esp_err_t td_store_settings_load(td_settings_t *out)
{
    td_settings_defaults(out);
    lock();
    size_t len = 0;
    esp_err_t err = td_store_read(TD_STORE_SETTINGS_PATH, s_settings_buf, sizeof s_settings_buf, &len);
    if (err == ESP_OK) {
        err = td_settings_parse(s_settings_buf, len, out) ? ESP_OK : ESP_ERR_INVALID_RESPONSE;
    } else if (err == ESP_ERR_INVALID_SIZE) {
        err = ESP_ERR_INVALID_RESPONSE;
    }
    unlock();
    return err;
}

esp_err_t td_store_settings_save(const td_settings_t *settings)
{
    lock();
    const size_t len = td_settings_format(settings, s_settings_buf, sizeof s_settings_buf);
    const esp_err_t err = len == 0 ? ESP_ERR_INVALID_ARG
                                   : td_store_write_atomic(TD_STORE_SETTINGS_PATH, s_settings_buf, len);
    unlock();
    return err;
}
