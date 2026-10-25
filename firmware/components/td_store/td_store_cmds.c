#include "td_store.h"
#include <stdlib.h>
#include <string.h>
#include "esp_heap_caps.h"
#include "esp_rom_crc.h"
#include "esp_timer.h"
#include "power.h"
#include "td_console.h"

/* Raw bytes in one @tds line. 240 base64 characters plus the prefix still fit the console's 255-character line. */
#define LINE_RAW 180
#define LINE_PREFIX "@tds "
/* The host waits for an ack after this many lines, so at most 6 * 246 bytes are in flight, under the console's
 * 2047-byte input queue. */
#define ACK_LINES 6
#define LS_MAX 64
#define LS_NAME_MAX 34
#define LS_REPLY_MAX 3072
#define WALK_DEPTH 4

_Static_assert(LINE_RAW % 3 == 0, "a line is whole base64 groups, so every full line has no padding");
_Static_assert(LINE_RAW <= TD_STORE_CHUNK_MAX, "one file chunk fills one line");

static void send(td_reply_t *r)
{
    td_reply_end(r);
    td_console_write_line(r->buf);
}

static void send_error(const char *cmd, const char *message)
{
    char buf[TD_REPLY_MAX];
    td_reply_error(buf, sizeof buf, cmd, message);
    td_console_write_line(buf);
}

static void send_op_error(const char *cmd, const char *op, const char *message)
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, cmd, false);
    td_reply_str(&r, "op", op);
    td_reply_str(&r, "error", message);
    send(&r);
}

/* Decimal digits only, so a sign or a space is refused, and no more than 32 bits. */
static bool parse_u32(const char *s, uint32_t *out)
{
    if (s[0] < '0' || s[0] > '9') {
        return false;
    }
    char *end = NULL;
    const unsigned long long v = strtoull(s, &end, 10);
    if (*end != '\0' || v > UINT32_MAX) {
        return false;
    }
    *out = (uint32_t)v;
    return true;
}

static bool ends_with_tmp(const char *path)
{
    const size_t len = strlen(path);
    return len >= 4 && strcmp(path + len - 4, ".tmp") == 0;
}

/* `store put` keeps the whole file in memory until its last line and only then swaps it in, so a reset in the middle of
 * a transfer leaves the volume as it was and no temporary file behind. */
static struct {
    bool active;
    char path[TD_STORE_PATH_MAX];
    uint32_t expected;
    uint32_t crc_expected;
    uint32_t got;
    uint32_t crc;
    uint32_t lines;
    uint8_t *data;
} s_put;

static void put_abort(void)
{
    free(s_put.data);
    memset(&s_put, 0, sizeof s_put);
}

static void put_reply(const char *phase, uint32_t crc, bool with_crc)
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "store", true);
    td_reply_str(&r, "op", "put");
    td_reply_str(&r, "phase", phase);
    td_reply_str(&r, "path", s_put.path);
    td_reply_int(&r, "bytes", s_put.expected);
    td_reply_int(&r, "got", s_put.got);
    if (with_crc) {
        td_reply_int(&r, "crc32", crc);
    } else {
        td_reply_int(&r, "line_bytes", LINE_RAW);
        td_reply_int(&r, "ack_lines", ACK_LINES);
    }
    send(&r);
}

static void put_begin(const char *path, const char *bytes_arg, const char *crc_arg)
{
    put_abort();
    uint32_t bytes;
    uint32_t crc;
    if (!parse_u32(bytes_arg, &bytes) || !parse_u32(crc_arg, &crc) || bytes < 1 || bytes > TD_STORE_PUT_MAX) {
        send_op_error("store", "put", "usage: store put <path> <bytes 1-65536> <crc32 decimal>");
        return;
    }
    if (!td_store_path_valid(path, false) || ends_with_tmp(path)) {
        send_op_error("store", "put", "invalid path");
        return;
    }
    if (!td_store_mounted()) {
        send_op_error("store", "put", "store not mounted");
        return;
    }
    uint8_t *data = heap_caps_malloc(bytes, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
    if (data == NULL) {
        data = heap_caps_malloc(bytes, MALLOC_CAP_8BIT);
    }
    if (data == NULL) {
        send_op_error("store", "put", "no memory");
        return;
    }
    s_put.active = true;
    s_put.data = data;
    s_put.expected = bytes;
    s_put.crc_expected = crc;
    strlcpy(s_put.path, path, sizeof s_put.path);
    put_reply("ready", 0, false);
}

static void put_fail(const char *message)
{
    send_op_error("store", "put", message);
    put_abort();
}

static void put_finish(void)
{
    if (s_put.crc != s_put.crc_expected) {
        char buf[TD_REPLY_MAX];
        td_reply_t r;
        td_reply_begin(&r, buf, sizeof buf, "store", false);
        td_reply_str(&r, "op", "put");
        td_reply_str(&r, "error", "crc mismatch");
        td_reply_int(&r, "crc32", s_put.crc);
        td_reply_int(&r, "expected", s_put.crc_expected);
        send(&r);
        put_abort();
        return;
    }
    if (td_store_write_atomic(s_put.path, s_put.data, s_put.expected) != ESP_OK) {
        put_fail("write failed");
        return;
    }
    put_reply("done", s_put.crc, true);
    put_abort();
}

/* One line of a put: @tds <base64 of 180 bytes>, or fewer on the last line. */
static int cmd_tds(int argc, char **argv)
{
    if (!s_put.active) {
        send_op_error("store", "put", "no put in progress");
        return 0;
    }
    uint8_t raw[LINE_RAW];
    size_t n = 0;
    if (argc != 2 || !td_store_b64_decode(argv[1], strlen(argv[1]), raw, sizeof raw, &n)) {
        put_fail("bad line");
        return 0;
    }
    const uint32_t left = s_put.expected - s_put.got;
    if (n > left || (n != LINE_RAW && n != left)) {
        put_fail("bad line length");
        return 0;
    }
    memcpy(s_put.data + s_put.got, raw, n);
    s_put.crc = esp_rom_crc32_le(s_put.crc, raw, (uint32_t)n);
    s_put.got += (uint32_t)n;
    s_put.lines++;
    if (s_put.got == s_put.expected) {
        put_finish();
    } else if (s_put.lines % ACK_LINES == 0) {
        put_reply("ack", 0, false);
    }
    return 0;
}

typedef struct {
    uint32_t bytes;
    uint32_t crc;
    uint32_t lines;
} get_stream_t;

/* A line must not lose bytes, so it goes out through the console's own writer, as screenshot lines do. */
static bool get_sink(void *ctx, const uint8_t *data, size_t len)
{
    get_stream_t *g = ctx;
    char line[sizeof(LINE_PREFIX) - 1 + 4 * (LINE_RAW / 3) + 2];
    memcpy(line, LINE_PREFIX, sizeof(LINE_PREFIX) - 1);
    const size_t n = td_store_b64_encode(data, len, line + sizeof(LINE_PREFIX) - 1, sizeof line - (sizeof(LINE_PREFIX) - 1) - 1);
    if (n == 0) {
        return false;
    }
    const size_t total = sizeof(LINE_PREFIX) - 1 + n;
    line[total] = '\n';
    if (!td_console_write(line, total + 1)) {
        return false;
    }
    g->crc = esp_rom_crc32_le(g->crc, data, (uint32_t)len);
    g->bytes += (uint32_t)len;
    g->lines++;
    return true;
}

static void store_get(const char *path)
{
    td_store_lock();
    td_store_stat_t st;
    const esp_err_t stat_err = td_store_stat(path, &st);
    if (stat_err != ESP_OK || st.is_dir) {
        td_store_unlock();
        send_op_error("store", "get", stat_err == ESP_ERR_INVALID_ARG ? "invalid path"
                                      : stat_err == ESP_ERR_NOT_FOUND ? "not found"
                                      : stat_err != ESP_OK            ? "stat failed"
                                                                       : "not a file");
        return;
    }

    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "store", true);
    td_reply_str(&r, "op", "get");
    td_reply_str(&r, "phase", "start");
    td_reply_str(&r, "path", path);
    td_reply_int(&r, "bytes", st.size);
    td_reply_int(&r, "lines", (st.size + LINE_RAW - 1) / LINE_RAW);
    send(&r);

    get_stream_t g = {0};
    const esp_err_t err = td_store_read_chunks(path, LINE_RAW, get_sink, &g);
    td_store_unlock();

    td_reply_begin(&r, buf, sizeof buf, "store", err == ESP_OK);
    td_reply_str(&r, "op", "get");
    td_reply_str(&r, "phase", "end");
    td_reply_int(&r, "bytes", g.bytes);
    td_reply_int(&r, "crc32", g.crc);
    if (err != ESP_OK) {
        td_reply_str(&r, "error", "stream aborted");
    }
    send(&r);
}

typedef struct {
    char dir[TD_STORE_PATH_MAX];
    int depth;
    unsigned files;
    unsigned dirs;
} walk_t;

static bool walk_entry(void *ctx, const char *name, uint32_t size, bool is_dir)
{
    (void)size;
    walk_t *w = ctx;
    if (!is_dir) {
        w->files++;
        return true;
    }
    w->dirs++;
    walk_t child = { .depth = w->depth - 1 };
    if (w->depth > 0 && snprintf(child.dir, sizeof child.dir, "%s/%s", w->dir, name) < (int)sizeof child.dir &&
        td_store_list(child.dir, walk_entry, &child) == ESP_OK) {
        w->files += child.files;
        w->dirs += child.dirs;
    }
    return true;
}

static void store_stat(const char *path)
{
    td_store_stat_t st;
    const esp_err_t err = td_store_stat(path, &st);
    if (err != ESP_OK) {
        send_op_error("store", "stat", err == ESP_ERR_INVALID_ARG ? "invalid path" : "not found");
        return;
    }
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "store", true);
    td_reply_str(&r, "op", "stat");
    td_reply_str(&r, "path", path);
    td_reply_bool(&r, "dir", st.is_dir);
    if (!st.is_dir) {
        td_reply_int(&r, "size", st.size);
        send(&r);
        return;
    }
    walk_t w = { .depth = WALK_DEPTH };
    strlcpy(w.dir, path, sizeof w.dir);
    td_store_list(path, walk_entry, &w);
    td_reply_int(&r, "files", w.files);
    td_reply_int(&r, "dirs", w.dirs);
    if (strcmp(path, TD_STORE_ROOT) == 0) {
        td_store_info_t info = {0};
        td_store_info(&info);
        td_reply_bool(&r, "formatted", td_store_formatted());
        td_reply_int(&r, "total", info.total_bytes);
        td_reply_int(&r, "used", info.used_bytes);
    }
    send(&r);
}

typedef struct {
    char (*names)[LS_NAME_MAX];
    unsigned stored;
    unsigned total;
} ls_t;

static bool ls_entry(void *ctx, const char *name, uint32_t size, bool is_dir)
{
    (void)size;
    ls_t *l = ctx;
    if (l->stored < LS_MAX) {
        snprintf(l->names[l->stored++], LS_NAME_MAX, "%s%s", name, is_dir ? "/" : "");
    }
    l->total++;
    return true;
}

static int compare_names(const void *a, const void *b)
{
    return strcmp(*(const char *const *)a, *(const char *const *)b);
}

static void store_ls(const char *dir)
{
    char (*names)[LS_NAME_MAX] = malloc(LS_MAX * LS_NAME_MAX);
    char *buf = malloc(LS_REPLY_MAX);
    if (names == NULL || buf == NULL) {
        free(names);
        free(buf);
        send_op_error("store", "ls", "no memory");
        return;
    }
    ls_t l = { .names = names };
    const esp_err_t err = td_store_list(dir, ls_entry, &l);
    if (err != ESP_OK) {
        send_op_error("store", "ls", err == ESP_ERR_INVALID_ARG ? "invalid path or not a directory" : "not found");
    } else {
        const char *sorted[LS_MAX];
        for (unsigned i = 0; i < l.stored; i++) {
            sorted[i] = names[i];
        }
        qsort(sorted, l.stored, sizeof sorted[0], compare_names);
        td_reply_t r;
        td_reply_begin(&r, buf, LS_REPLY_MAX, "store", true);
        td_reply_str(&r, "op", "ls");
        td_reply_str(&r, "dir", dir);
        td_reply_int(&r, "count", l.total);
        td_reply_bool(&r, "truncated", l.total > l.stored);
        td_reply_strs(&r, "names", sorted, l.stored);
        send(&r);
    }
    free(names);
    free(buf);
}

static void store_rm(const char *path)
{
    const esp_err_t err = td_store_remove(path);
    if (err != ESP_OK) {
        send_op_error("store", "rm", err == ESP_ERR_INVALID_ARG ? "invalid path"
                                     : err == ESP_ERR_NOT_FOUND  ? "not found"
                                                                  : "remove failed");
        return;
    }
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "store", true);
    td_reply_str(&r, "op", "rm");
    td_reply_str(&r, "path", path);
    send(&r);
}

#define STORE_USAGE "usage: store stat [path] | ls <dir> | rm <path> | put <path> <bytes> <crc32> | get <path>"

static int cmd_store(int argc, char **argv)
{
    const char *sub = argc > 1 ? argv[1] : "";
    const bool stat = strcmp(sub, "stat") == 0 && (argc == 2 || argc == 3);
    const bool ls = strcmp(sub, "ls") == 0 && argc == 3;
    const bool rm = strcmp(sub, "rm") == 0 && argc == 3;
    const bool put = strcmp(sub, "put") == 0 && argc == 5;
    const bool get = strcmp(sub, "get") == 0 && argc == 3;
    if (!stat && !ls && !rm && !put && !get) {
        send_error("store", STORE_USAGE);
        return 0;
    }
    if (put) {
        put_begin(argv[2], argv[3], argv[4]);
        return 0;
    }
    if (!td_store_mounted()) {
        send_op_error("store", sub, "store not mounted");
    } else if (stat) {
        store_stat(argc == 3 ? argv[2] : TD_STORE_ROOT);
    } else if (ls) {
        store_ls(argv[2]);
    } else if (rm) {
        store_rm(argv[2]);
    } else {
        store_get(argv[2]);
    }
    return 0;
}

static void settings_reply(const char *op, const td_settings_t *s, bool stored)
{
    char buf[TD_REPLY_MAX];
    td_reply_t r;
    td_reply_begin(&r, buf, sizeof buf, "settings", true);
    td_reply_str(&r, "op", op);
    td_reply_int(&r, "idle_timeout_s", s->idle_timeout_s);
    td_reply_str(&r, "orientation", td_settings_orientation_name(s->orientation));
    td_reply_bool(&r, "stored", stored);
    send(&r);
}

static void settings_get(void)
{
    td_settings_t s;
    const esp_err_t err = td_store_settings_load(&s);
    if (err == ESP_ERR_INVALID_STATE) {
        send_error("settings", "store not mounted");
        return;
    }
    settings_reply("get", &s, err != ESP_ERR_NOT_FOUND);
}

static void settings_set(const char *field, const char *value)
{
    const bool idle = strcmp(field, "idle_timeout_s") == 0;
    const bool orientation = strcmp(field, "orientation") == 0;
    td_settings_t s;
    if (!td_store_mounted()) {
        send_error("settings", "store not mounted");
        return;
    }
    td_store_lock();
    td_store_settings_load(&s);
    if (idle) {
        uint32_t seconds;
        if (!parse_u32(value, &seconds)) {
            td_store_unlock();
            send_error("settings", "usage: settings set idle_timeout_s <seconds 1-86400>");
            return;
        }
        if (seconds < TD_SETTINGS_IDLE_MIN_S || seconds > TD_SETTINGS_IDLE_MAX_S) {
            td_store_unlock();
            send_error("settings", "out of range");
            return;
        }
        s.idle_timeout_s = seconds;
    } else if (orientation && !td_settings_orientation_parse(value, &s.orientation)) {
        td_store_unlock();
        send_error("settings", "usage: settings set orientation landscape|portrait");
        return;
    }
    const esp_err_t err = td_store_settings_save(&s);
    td_store_unlock();
    if (err != ESP_OK) {
        send_error("settings", "write failed");
        return;
    }
    if (idle) {
        /* The gate may not be running, which a store that outlives power does not care about. */
        power_set_idle_timeout_s(s.idle_timeout_s, esp_timer_get_time());
    }
    settings_reply("set", &s, true);
}

#define SETTINGS_USAGE "usage: settings get | set idle_timeout_s <seconds 1-86400> | set orientation landscape|portrait"

static int cmd_settings(int argc, char **argv)
{
    const char *sub = argc > 1 ? argv[1] : "";
    if (strcmp(sub, "get") == 0 && argc == 2) {
        settings_get();
    } else if (strcmp(sub, "set") == 0 && argc == 4 &&
               (strcmp(argv[2], "idle_timeout_s") == 0 || strcmp(argv[2], "orientation") == 0)) {
        settings_set(argv[2], argv[3]);
    } else {
        send_error("settings", SETTINGS_USAGE);
    }
    return 0;
}

const esp_console_cmd_t *td_store_console_commands(size_t *count)
{
    static const esp_console_cmd_t cmds[] = {
        { .command = "store", .help = "inspect, read and write the files on the storage partition",
          .hint = "stat [path] | ls <dir> | rm <path> | put <path> <bytes> <crc32> | get <path>", .func = cmd_store },
        { .command = "settings", .help = "read or change the settings that persist across resets",
          .hint = "get | set idle_timeout_s <seconds> | set orientation landscape|portrait", .func = cmd_settings },
        { .command = "@tds", .help = "one base64 line of a store put, sent by the host after it",
          .hint = "<base64>", .func = cmd_tds },
    };
    *count = sizeof cmds / sizeof cmds[0];
    return cmds;
}
