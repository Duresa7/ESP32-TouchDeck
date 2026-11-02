#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include "compat.h"
#include "td_layout.h"
#ifdef _WIN32
#define fileno _fileno
#endif

static int handles;
static size_t parsed_bytes;

static FILE *replaced_fopen(const char *path, const char *mode)
{
    (void)path;
    FILE *file = fopen("opened.bin", mode);
    if (file != NULL) handles++;
    return file;
}

static int tracked_fclose(FILE *file)
{
    handles--;
    return fclose(file);
}

static int replaced_stat(const char *path, struct stat *st)
{
    (void)path;
    return stat("checked.bin", st);
}

bool td_layout_parse(const char *json, size_t len, td_layout_t **out, td_layout_error_t *err)
{
    (void)json;
    (void)err;
    parsed_bytes = len;
    *out = calloc(1, sizeof **out);
    return *out != NULL;
}

const ui_tile_t *ui_page_find_tile(const ui_page_t *page, const char *id)
{
    (void)page;
    (void)id;
    return NULL;
}

#define asm(label)
const char k_builtin_start[] = "{}";
const char k_builtin_end[] = "";
#define fopen replaced_fopen
#define fclose tracked_fclose
#define stat(path, st) replaced_stat(path, st)
#include "layout.c"
#include "icon.c"
#undef fopen
#undef fclose
#undef stat

static void fixture(const char *path, size_t bytes, bool icon)
{
    FILE *file = fopen(path, "wb");
    assert(file != NULL);
    const unsigned char header[12] = { 'T', 'D', 'I', '1', 64, 0, 64, 0, 1, 0, 0, 0 };
    for (size_t i = 0; i < bytes; i++) {
        assert(fputc(icon && i < sizeof header ? header[i] : 0, file) != EOF);
    }
    assert(fclose(file) == 0);
}

static void layout_test(void)
{
    fixture("checked.bin", 2, false);
    fixture("opened.bin", TD_LAYOUT_MAX_BYTES + 1, false);
    td_layout_t *layout = NULL;
    td_layout_error_t err;
    size_t bytes = 0;
    bool loaded = td_layout_load_file("replaced.json", &layout, &bytes, &err);
    if (loaded || strcmp(err.key, "layout.too-large") != 0 || parsed_bytes != 0) {
        fputs("Layout accepted an oversized replacement using stale path metadata\n", stderr);
        exit(1);
    }
    assert(bytes == TD_LAYOUT_MAX_BYTES + 1 && handles == 0);
    fixture("opened.bin", 12, false);
    assert(td_layout_load_file("replaced.json", &layout, &bytes, &err));
    assert(bytes == 12 && parsed_bytes == 12 && handles == 0);
    td_layout_free(layout);
    assert(remove("opened.bin") == 0);
    assert(!td_layout_load_file("missing.json", &layout, &bytes, &err));
    assert(layout == NULL && handles == 0);
}

static void icon_test(void)
{
    fixture("checked.bin", ICON_FILE, true);
    fixture("opened.bin", ICON_FILE + 1, true);
    const ui_tile_t tile = { .icon = "file:0123456789abcdef" };
    const ui_page_t page = { .tiles = &tile, .tile_count = 1 };
    const td_layout_t layout = { .pages = &page, .page_count = 1 };
    ui_icon_counts_t counts = ui_icon_prepare(&layout, NULL);
    if (counts.loaded != 0 || counts.missing != 1) {
        fputs("Icon accepted a replacement with trailing bytes using stale path metadata\n", stderr);
        exit(1);
    }
    assert(handles == 0 && ui_icon_image(tile.icon) == NULL);
    fixture("opened.bin", ICON_FILE, true);
    counts = ui_icon_prepare(&layout, NULL);
    assert(counts.loaded == 1 && counts.missing == 0 && handles == 0);
    const lv_image_dsc_t *image = ui_icon_image(tile.icon);
    assert(image != NULL && image->data_size == ICON_PIXELS);
}

int main(int argc, char **argv)
{
    (void)replaced_stat;
    assert(argc == 2);
    if (strcmp(argv[1], "layout") == 0) layout_test();
    else icon_test();
    return 0;
}
