#include "td_layout.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include "esp_heap_caps.h"

extern const char k_builtin_start[] asm("_binary_builtin_json_start");
extern const char k_builtin_end[] asm("_binary_builtin_json_end");

static td_layout_t *s_builtin;

static bool refuse(td_layout_error_t *err, const char *key, const char *message, const char *detail)
{
    snprintf(err->key, sizeof err->key, "%s", key);
    snprintf(err->message, sizeof err->message, message, detail);
    return false;
}

void td_layout_free(td_layout_t *layout)
{
    if (layout != NULL && !layout->builtin) {
        free(layout);
    }
}

/* EMBED_TXTFILES ends the file with a NUL that is not part of it. */
const char *td_layout_builtin_json(size_t *len)
{
    if (len != NULL) {
        *len = (size_t)(k_builtin_end - k_builtin_start) - 1;
    }
    return k_builtin_start;
}

/* app_main makes the first call, before any task that reads the layout starts. */
const td_layout_t *td_layout_builtin(void)
{
    if (s_builtin == NULL) {
        size_t len = 0;
        const char *json = td_layout_builtin_json(&len);
        td_layout_t *layout = NULL;
        if (td_layout_parse(json, len, &layout, NULL)) {
            layout->builtin = true;
            s_builtin = layout;
        }
    }
    return s_builtin;
}

bool td_layout_load_file(const char *path, td_layout_t **out, size_t *file_bytes, td_layout_error_t *err)
{
    td_layout_error_t scratch;
    if (err == NULL) {
        err = &scratch;
    }
    memset(err, 0, sizeof *err);
    *out = NULL;
    *file_bytes = 0;

    struct stat st;
    if (stat(path, &st) != 0) {
        return refuse(err, "file.missing", "no %s", path);
    }
    if (st.st_size > TD_LAYOUT_MAX_BYTES) {
        char size[16];
        snprintf(size, sizeof size, "%ld", (long)st.st_size);
        *file_bytes = (size_t)st.st_size;
        return refuse(err, "layout.too-large", "the file is %s bytes; maximum is 32768 bytes", size);
    }
    FILE *f = fopen(path, "rb");
    if (f == NULL) {
        return refuse(err, "file.read", "cannot open %s", path);
    }
    char *buf = heap_caps_malloc_prefer((size_t)st.st_size + 1, 2, MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT, MALLOC_CAP_8BIT);
    if (buf == NULL) {
        fclose(f);
        return refuse(err, "internal.no-memory", "no memory to read %s", path);
    }
    const size_t n = fread(buf, 1, (size_t)st.st_size, f);
    fclose(f);
    if (n != (size_t)st.st_size) {
        free(buf);
        return refuse(err, "file.read", "short read of %s", path);
    }
    *file_bytes = n;
    const bool ok = td_layout_parse(buf, n, out, err);
    free(buf);
    return ok;
}

void td_layout_path(const char *orientation, char *buf, size_t cap)
{
    snprintf(buf, cap, "/storage/layouts/%s.json", orientation);
}

td_layout_t *td_layout_replace(const td_layout_t **current, const td_layout_t *next)
{
    const td_layout_t *old = *current;
    *current = next;
    if (old == NULL || old == next || old->builtin) {
        return NULL;
    }
    return (td_layout_t *)old;
}

int td_layout_page_index(const td_layout_t *layout, const char *id)
{
    for (int i = 0; layout != NULL && id != NULL && i < layout->page_count; i++) {
        if (strcmp(layout->pages[i].id, id) == 0) {
            return i;
        }
    }
    return -1;
}

const ui_tile_t *td_layout_find_tile(const td_layout_t *layout, const char *id, uint8_t *page)
{
    for (uint8_t i = 0; layout != NULL && id != NULL && i < layout->page_count; i++) {
        const ui_tile_t *tile = ui_page_find_tile(&layout->pages[i], id);
        if (tile != NULL) {
            if (page != NULL) {
                *page = i;
            }
            return tile;
        }
    }
    return NULL;
}
