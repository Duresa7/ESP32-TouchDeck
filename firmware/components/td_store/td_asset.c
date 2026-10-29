#include "td_asset.h"
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include "esp_log.h"
#include "td_store.h"
#include "td_update_state.h"

#define UPLOAD_TMP TD_ASSET_DIR "/upload.tmp"
#define BIN_SUFFIX ".bin"
#define SHARD_PATH_MAX (sizeof(TD_ASSET_DIR) + 2)
#define ASSET_PATH_MAX (SHARD_PATH_MAX + TD_ASSET_ID_LEN + sizeof(BIN_SUFFIX))
#define SHARDS "0123456789abcdef"
#define HEAD_BYTES 12
#define READ_CHUNK 256

/* tdi1, per schema/icon-format.md: a 12-byte header and 64 by 64 RGB565 then A8. */
#define TDI1_BYTES 12300u

static const char *TAG = "asset";

static struct {
    bool open;
    int fd;
    char id[TD_ASSET_ID_LEN + 1];
    uint32_t bytes;
    uint32_t written;
    uint32_t cost;
    td_asset_format_t format;
    uint8_t head[HEAD_BYTES];
    td_sha256_t sha;
} s_up = { .fd = -1 };

bool td_asset_id_valid(const char *id)
{
    if (id == NULL || strlen(id) != TD_ASSET_ID_LEN) {
        return false;
    }
    for (size_t i = 0; i < TD_ASSET_ID_LEN; i++) {
        if (!((id[i] >= '0' && id[i] <= '9') || (id[i] >= 'a' && id[i] <= 'f'))) {
            return false;
        }
    }
    return true;
}

const char *td_asset_result_reason(td_asset_result_t result)
{
    switch (result) {
    case TD_ASSET_FULL:
        return "full";
    case TD_ASSET_HASH:
        return "hash";
    case TD_ASSET_FORMAT:
        return "format";
    case TD_ASSET_STATE:
        return "state";
    case TD_ASSET_INCOMPLETE:
        return "incomplete";
    case TD_ASSET_OK:
    case TD_ASSET_STORAGE:
    default:
        return "storage";
    }
}

td_asset_format_t td_asset_format_parse(const char *name)
{
    return name != NULL && strcmp(name, "tdi1") == 0 ? TD_ASSET_FMT_TDI1 : TD_ASSET_FMT_NONE;
}

const char *td_asset_format_name(td_asset_format_t format)
{
    return format == TD_ASSET_FMT_TDI1 ? "tdi1" : NULL;
}

static uint16_t le16(const uint8_t *p)
{
    return (uint16_t)(p[0] | (p[1] << 8));
}

static td_asset_format_t detect(const uint8_t head[HEAD_BYTES], uint32_t bytes)
{
    if (bytes == TDI1_BYTES && memcmp(head, "TDI1", 4) == 0 && le16(head + 4) == 64 && le16(head + 6) == 64
        && le16(head + 8) == 1 && le16(head + 10) == 0) {
        return TD_ASSET_FMT_TDI1;
    }
    return TD_ASSET_FMT_NONE;
}

static void shard_path(char out[SHARD_PATH_MAX], char shard)
{
    snprintf(out, SHARD_PATH_MAX, TD_ASSET_DIR "/%c", shard);
}

static void asset_path(char out[ASSET_PATH_MAX], const char *id)
{
    snprintf(out, ASSET_PATH_MAX, TD_ASSET_DIR "/%c/%s" BIN_SUFFIX, id[0], id);
}

static void hex(const uint8_t digest[TD_SHA256_BYTES], char out[TD_ASSET_ID_LEN + 1])
{
    for (size_t i = 0; i < TD_SHA256_BYTES; i++) {
        snprintf(out + 2 * i, 3, "%02x", digest[i]);
    }
}

uint32_t td_asset_cost(uint32_t bytes)
{
    return td_store_budget_cost(bytes);
}

static uint32_t upload_reservation(void)
{
    if (!s_up.open) {
        return 0;
    }
    return s_up.cost;
}

td_asset_result_t td_asset_space(td_asset_space_t *out)
{
    td_store_info_t info;
    if (td_store_budget(&info) != ESP_OK) {
        *out = (td_asset_space_t){ 0 };
        return TD_ASSET_STORAGE;
    }
    out->total_bytes = info.total_bytes;
    out->used_bytes = info.used_bytes;
    out->reserved_bytes = TD_ASSET_HEADROOM_BYTES + upload_reservation();
    const uint64_t taken = (uint64_t)out->used_bytes + out->reserved_bytes;
    out->free_bytes = taken < out->total_bytes ? (uint32_t)(out->total_bytes - taken) : 0;
    return TD_ASSET_OK;
}

static bool hash_file(const char *path, uint32_t size, uint8_t digest[TD_SHA256_BYTES], uint8_t head[HEAD_BYTES])
{
    const int fd = open(path, O_RDONLY);
    if (fd < 0) {
        return false;
    }
    uint8_t buf[READ_CHUNK];
    td_sha256_t sha;
    td_sha256_init(&sha);
    memset(head, 0, HEAD_BYTES);
    uint32_t done = 0;
    bool ok = true;
    while (done < size) {
        const ssize_t n = read(fd, buf, sizeof buf);
        if (n <= 0) {
            ok = false;
            break;
        }
        if (done < HEAD_BYTES) {
            const size_t take = (size_t)n < HEAD_BYTES - done ? (size_t)n : HEAD_BYTES - done;
            memcpy(head + done, buf, take);
        }
        td_sha256_update(&sha, buf, (size_t)n);
        done += (uint32_t)n;
    }
    close(fd);
    td_sha256_final(&sha, digest);
    return ok && done == size;
}

bool td_asset_present(const char *id, uint32_t *bytes, td_asset_format_t *format)
{
    if (!td_asset_id_valid(id) || !td_store_mounted()) {
        return false;
    }
    char path[ASSET_PATH_MAX];
    asset_path(path, id);
    uint8_t digest[TD_SHA256_BYTES];
    uint8_t head[HEAD_BYTES];
    char got[TD_ASSET_ID_LEN + 1];
    struct stat st;
    td_store_lock();
    bool ok = stat(path, &st) == 0 && S_ISREG(st.st_mode) && hash_file(path, (uint32_t)st.st_size, digest, head);
    td_store_unlock();
    if (!ok) {
        return false;
    }
    hex(digest, got);
    const td_asset_format_t detected = detect(head, (uint32_t)st.st_size);
    if (strcmp(got, id) != 0 || detected == TD_ASSET_FMT_NONE) {
        ESP_LOGW(TAG, "%.12s does not validate", id);
        return false;
    }
    if (bytes != NULL) {
        *bytes = (uint32_t)st.st_size;
    }
    if (format != NULL) {
        *format = detected;
    }
    return true;
}

static bool name_id(const char *name, char id[TD_ASSET_ID_LEN + 1])
{
    if (strlen(name) != TD_ASSET_ID_LEN + sizeof(BIN_SUFFIX) - 1 || strcmp(name + TD_ASSET_ID_LEN, BIN_SUFFIX) != 0) {
        return false;
    }
    memcpy(id, name, TD_ASSET_ID_LEN);
    id[TD_ASSET_ID_LEN] = '\0';
    return td_asset_id_valid(id);
}

typedef struct {
    const char *after;
    td_asset_item_t found[TD_ASSET_PAGE + 1];
    size_t count;
} list_walk_t;

static void list_take(list_walk_t *walk, const char *id)
{
    size_t at = walk->count;
    while (at > 0 && strcmp(walk->found[at - 1].id, id) > 0) {
        at--;
    }
    if (at > TD_ASSET_PAGE) {
        return;
    }
    const size_t keep = walk->count < TD_ASSET_PAGE + 1 ? walk->count : TD_ASSET_PAGE;
    memmove(&walk->found[at + 1], &walk->found[at], (keep - at) * sizeof walk->found[0]);
    strlcpy(walk->found[at].id, id, sizeof walk->found[at].id);
    walk->count = keep + 1;
}

static bool list_shard(list_walk_t *walk, char shard)
{
    char dir[SHARD_PATH_MAX];
    shard_path(dir, shard);
    DIR *d = opendir(dir);
    if (d == NULL) {
        return errno == ENOENT;
    }
    struct dirent *e;
    char id[TD_ASSET_ID_LEN + 1];
    while ((e = readdir(d)) != NULL) {
        if (e->d_type != DT_DIR && name_id(e->d_name, id) && id[0] == shard
            && (walk->after == NULL || strcmp(id, walk->after) > 0)) {
            list_take(walk, id);
        }
    }
    closedir(d);
    return true;
}

static td_asset_format_t read_item(td_asset_item_t *item)
{
    char path[ASSET_PATH_MAX];
    asset_path(path, item->id);
    struct stat st;
    uint8_t head[HEAD_BYTES] = { 0 };
    if (stat(path, &st) != 0 || !S_ISREG(st.st_mode)) {
        return TD_ASSET_FMT_NONE;
    }
    item->bytes = (uint32_t)st.st_size;
    const int fd = open(path, O_RDONLY);
    if (fd < 0) {
        return TD_ASSET_FMT_NONE;
    }
    const ssize_t n = read(fd, head, sizeof head);
    close(fd);
    return n == HEAD_BYTES ? detect(head, item->bytes) : TD_ASSET_FMT_NONE;
}

td_asset_result_t td_asset_list(const char *after, td_asset_item_t items[TD_ASSET_PAGE], size_t *count, bool *more)
{
    *count = 0;
    *more = false;
    if (!td_store_mounted()) {
        return TD_ASSET_STORAGE;
    }
    static list_walk_t walk;
    td_store_lock();
    walk = (list_walk_t){ .after = after };
    bool ok = true;
    for (const char *shard = after ? strchr(SHARDS, after[0]) : SHARDS; ok && shard && *shard; shard++) {
        ok = list_shard(&walk, *shard);
        if (walk.count > TD_ASSET_PAGE) {
            break;
        }
    }
    if (!ok) {
        td_store_unlock();
        return TD_ASSET_STORAGE;
    }
    for (size_t i = 0; i < walk.count && *count < TD_ASSET_PAGE; i++) {
        walk.found[i].format = read_item(&walk.found[i]);
        if (walk.found[i].format != TD_ASSET_FMT_NONE) {
            items[(*count)++] = walk.found[i];
        }
    }
    *more = walk.count > TD_ASSET_PAGE;
    td_store_unlock();
    return TD_ASSET_OK;
}

td_asset_result_t td_asset_delete(const char *id, bool *deleted)
{
    *deleted = false;
    if (!td_asset_id_valid(id) || !td_store_mounted()) {
        return TD_ASSET_STORAGE;
    }
    char path[ASSET_PATH_MAX];
    asset_path(path, id);
    char shard[SHARD_PATH_MAX];
    shard_path(shard, id[0]);
    td_store_lock();
    struct stat previous;
    bool existed = stat(path, &previous) == 0;
    const int rc = unlink(path);
    if (rc == 0 && existed) td_store_budget_adjust(-(int64_t)td_asset_cost((uint32_t)previous.st_size));
    const int saved = errno;
    if (rmdir(shard) == 0) td_store_budget_adjust(-8192);
    td_store_unlock();
    if (rc == 0) {
        *deleted = true;
        return TD_ASSET_OK;
    }
    return saved == ENOENT ? TD_ASSET_OK : TD_ASSET_STORAGE;
}

const char *td_asset_upload_id(void)
{
    return s_up.open ? s_up.id : NULL;
}

void td_asset_upload_abort(void)
{
    if (!s_up.open) {
        return;
    }
    td_store_lock();
    if (s_up.fd >= 0) {
        close(s_up.fd);
    }
    unlink(UPLOAD_TMP);
    td_store_unlock();
    s_up.fd = -1;
    s_up.open = false;
}

td_asset_result_t td_asset_upload_begin(const char *id, uint32_t bytes, td_asset_format_t format)
{
    if (s_up.open) {
        return TD_ASSET_STATE;
    }
    if (!td_asset_id_valid(id) || !td_store_mounted()) {
        return TD_ASSET_STORAGE;
    }
    td_asset_space_t space;
    td_store_lock();
    if (td_asset_space(&space) != TD_ASSET_OK) {
        td_store_unlock();
        return TD_ASSET_STORAGE;
    }
    if (td_asset_cost(bytes) > space.free_bytes) {
        td_store_unlock();
        return TD_ASSET_FULL;
    }
    int made = mkdir(TD_ASSET_DIR, 0777);
    if (made == 0) td_store_budget_adjust(8192);
    if (made != 0 && errno != EEXIST) {
        td_store_unlock();
        return TD_ASSET_STORAGE;
    }
    const int fd = open(UPLOAD_TMP, O_WRONLY | O_CREAT | O_TRUNC, 0666);
    td_store_unlock();
    if (fd < 0) {
        return errno == ENOSPC ? TD_ASSET_FULL : TD_ASSET_STORAGE;
    }
    s_up = (typeof(s_up)){ .open = true, .fd = fd, .bytes = bytes, .cost = td_asset_cost(bytes), .format = format };
    strlcpy(s_up.id, id, sizeof s_up.id);
    td_sha256_init(&s_up.sha);
    return TD_ASSET_OK;
}

td_asset_result_t td_asset_upload_write(const uint8_t *data, size_t len)
{
    if (!s_up.open || len > s_up.bytes - s_up.written) {
        return TD_ASSET_STATE;
    }
    if (s_up.written < HEAD_BYTES) {
        const size_t take = len < HEAD_BYTES - s_up.written ? len : HEAD_BYTES - s_up.written;
        memcpy(s_up.head + s_up.written, data, take);
    }
    td_store_lock();
    size_t done = 0;
    int saved = 0;
    while (done < len) {
        const ssize_t n = write(s_up.fd, data + done, len - done);
        if (n <= 0) {
            saved = n < 0 ? errno : EIO;
            break;
        }
        done += (size_t)n;
    }
    td_store_unlock();
    if (done < len) {
        ESP_LOGW(TAG, "upload write failed, errno %d", saved);
        td_asset_upload_abort();
        return saved == ENOSPC ? TD_ASSET_FULL : TD_ASSET_STORAGE;
    }
    td_sha256_update(&s_up.sha, data, len);
    s_up.written += (uint32_t)len;
    return TD_ASSET_OK;
}

td_asset_result_t td_asset_upload_finish(void)
{
    if (!s_up.open) {
        return TD_ASSET_STATE;
    }
    if (s_up.written != s_up.bytes) {
        return TD_ASSET_INCOMPLETE;
    }
    uint8_t digest[TD_SHA256_BYTES];
    char got[TD_ASSET_ID_LEN + 1];
    td_sha256_final(&s_up.sha, digest);
    hex(digest, got);
    td_asset_result_t result = TD_ASSET_OK;
    if (strcmp(got, s_up.id) != 0) {
        result = TD_ASSET_HASH;
    } else if (detect(s_up.head, s_up.bytes) != s_up.format) {
        result = TD_ASSET_FORMAT;
    }
    if (result != TD_ASSET_OK) {
        td_asset_upload_abort();
        return result;
    }
    char path[ASSET_PATH_MAX];
    char shard[SHARD_PATH_MAX];
    asset_path(path, s_up.id);
    shard_path(shard, s_up.id[0]);
    td_store_lock();
    struct stat previous;
    int64_t previous_cost = stat(path, &previous) == 0 ? td_asset_cost((uint32_t)previous.st_size) : 0;
    bool ok = fsync(s_up.fd) == 0;
    int saved = ok ? 0 : errno;
    ok = close(s_up.fd) == 0 && ok;
    s_up.fd = -1;
    int made = ok ? mkdir(shard, 0777) : -1;
    if (made == 0) td_store_budget_adjust(8192);
    if (ok && made != 0 && errno != EEXIST) {
        saved = errno;
        ok = false;
    }
    if (ok && rename(UPLOAD_TMP, path) != 0) {
        saved = errno;
        ok = false;
    }
    if (ok) td_store_budget_adjust((int64_t)td_asset_cost(s_up.bytes) - previous_cost);
    if (!ok) {
        unlink(UPLOAD_TMP);
        if (rmdir(shard) == 0) td_store_budget_adjust(-8192);
    }
    td_store_unlock();
    s_up.open = false;
    if (!ok) {
        ESP_LOGW(TAG, "commit of %.12s failed, errno %d", s_up.id, saved);
        return saved == ENOSPC ? TD_ASSET_FULL : TD_ASSET_STORAGE;
    }
    return TD_ASSET_OK;
}
