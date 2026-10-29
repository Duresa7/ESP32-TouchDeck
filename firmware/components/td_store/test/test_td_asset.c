#include "td_asset.h"
#include "td_store.h"
#include "td_update_state.h"
#include "unity.h"
#include "esp_timer.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>

#define ICON_BYTES 12300
#define CHUNK 1021

static void mount(void)
{
    if (!td_store_mounted()) {
        TEST_ASSERT_EQUAL(ESP_OK, td_store_mount(NULL));
    }
}

static uint8_t *icon(uint8_t seed)
{
    uint8_t *file = malloc(ICON_BYTES);
    TEST_ASSERT_NOT_NULL(file);
    const uint8_t head[12] = { 'T', 'D', 'I', '1', 64, 0, 64, 0, 1, 0, 0, 0 };
    memcpy(file, head, sizeof head);
    for (size_t i = sizeof head; i < ICON_BYTES; i++) {
        file[i] = (uint8_t)(i * 31 + seed);
    }
    return file;
}

static void id_of(const uint8_t *data, size_t len, char id[TD_ASSET_ID_LEN + 1])
{
    td_sha256_t sha;
    uint8_t digest[TD_SHA256_BYTES];
    td_sha256_init(&sha);
    td_sha256_update(&sha, data, len);
    td_sha256_final(&sha, digest);
    for (size_t i = 0; i < TD_SHA256_BYTES; i++) {
        snprintf(id + 2 * i, 3, "%02x", digest[i]);
    }
}

static td_asset_result_t put(const char *id, const uint8_t *data, size_t len)
{
    td_asset_result_t r = td_asset_upload_begin(id, len, TD_ASSET_FMT_TDI1);
    for (size_t at = 0; r == TD_ASSET_OK && at < len; at += CHUNK) {
        r = td_asset_upload_write(data + at, len - at < CHUNK ? len - at : CHUNK);
    }
    return r == TD_ASSET_OK ? td_asset_upload_finish() : r;
}

static bool listed(const char *id)
{
    td_asset_item_t items[TD_ASSET_PAGE];
    size_t count = 0;
    bool more = true;
    char after[TD_ASSET_ID_LEN + 1] = "";
    while (more) {
        TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_list(after[0] ? after : NULL, items, &count, &more));
        for (size_t i = 0; i < count; i++) {
            if (strcmp(items[i].id, id) == 0) {
                TEST_ASSERT_EQUAL_UINT32(ICON_BYTES, items[i].bytes);
                TEST_ASSERT_EQUAL(TD_ASSET_FMT_TDI1, items[i].format);
                return true;
            }
        }
        if (count > 0) {
            strlcpy(after, items[count - 1].id, sizeof after);
        }
    }
    return false;
}

static bool temp_exists(void)
{
    struct stat st;
    return stat(TD_ASSET_DIR "/upload.tmp", &st) == 0;
}

static bool shard_exists(const char *id)
{
    char shard[sizeof TD_ASSET_DIR + 2];
    snprintf(shard, sizeof shard, TD_ASSET_DIR "/%c", id[0]);
    struct stat st;
    return stat(shard, &st) == 0;
}

TEST_CASE("Asset put commits by hash, a duplicate is present, and delete removes it", "[td_asset]")
{
    mount();
    uint8_t *file = icon(1);
    char id[TD_ASSET_ID_LEN + 1];
    id_of(file, ICON_BYTES, id);
    bool deleted;
    td_asset_delete(id, &deleted);

    td_asset_space_t before;
    TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_space(&before));
    TEST_ASSERT_EQUAL(TD_ASSET_OK, put(id, file, ICON_BYTES));
    uint32_t bytes = 0;
    td_asset_format_t format = TD_ASSET_FMT_NONE;
    TEST_ASSERT_TRUE(td_asset_present(id, &bytes, &format));
    TEST_ASSERT_EQUAL_UINT32(ICON_BYTES, bytes);
    TEST_ASSERT_EQUAL(TD_ASSET_FMT_TDI1, format);
    TEST_ASSERT_TRUE(listed(id));
    TEST_ASSERT_FALSE(temp_exists());
    td_asset_space_t after;
    TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_space(&after));
    TEST_ASSERT_LESS_THAN_UINT32(before.free_bytes, after.free_bytes);

        TEST_ASSERT_TRUE(td_asset_present(id, NULL, NULL));

    TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_delete(id, &deleted));
    TEST_ASSERT_TRUE(deleted);
    TEST_ASSERT_FALSE(td_asset_present(id, NULL, NULL));
    TEST_ASSERT_FALSE(listed(id));
    TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_delete(id, &deleted));
    TEST_ASSERT_FALSE(deleted);
    free(file);
}

TEST_CASE("Asset put that does not hash to its id commits nothing", "[td_asset]")
{
    mount();
    uint8_t *file = icon(2);
    char id[TD_ASSET_ID_LEN + 1];
    id_of(file, ICON_BYTES, id);
    file[ICON_BYTES - 1] ^= 1;
    TEST_ASSERT_EQUAL(TD_ASSET_HASH, put(id, file, ICON_BYTES));
    TEST_ASSERT_FALSE(td_asset_present(id, NULL, NULL));
    TEST_ASSERT_FALSE(temp_exists());
    TEST_ASSERT_FALSE(shard_exists(id));
    TEST_ASSERT_NULL(td_asset_upload_id());
    free(file);
}

TEST_CASE("Asset put of bytes that are not a tdi1 file is refused as format", "[td_asset]")
{
    mount();
    uint8_t *file = icon(3);
    file[0] = 'X';
    char id[TD_ASSET_ID_LEN + 1];
    id_of(file, ICON_BYTES, id);
    TEST_ASSERT_EQUAL(TD_ASSET_FORMAT, put(id, file, ICON_BYTES));
    TEST_ASSERT_FALSE(td_asset_present(id, NULL, NULL));
    TEST_ASSERT_FALSE(temp_exists());
    free(file);
}

TEST_CASE("Asset put larger than the free space is refused as full before any write", "[td_asset]")
{
    mount();
    td_asset_space_t space;
    TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_space(&space));
    TEST_ASSERT_EQUAL_UINT32(space.total_bytes - space.used_bytes - space.reserved_bytes, space.free_bytes);
    const char *id = "0000000000000000000000000000000000000000000000000000000000000000";
    TEST_ASSERT_EQUAL(TD_ASSET_FULL, td_asset_upload_begin(id, space.free_bytes, TD_ASSET_FMT_TDI1));
    TEST_ASSERT_NULL(td_asset_upload_id());
    TEST_ASSERT_FALSE(temp_exists());
    td_asset_space_t again;
    TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_space(&again));
    TEST_ASSERT_EQUAL_UINT32(space.used_bytes, again.used_bytes);
}

TEST_CASE("An interrupted Asset upload leaves no partial Asset and reserves its space while open", "[td_asset]")
{
    mount();
    uint8_t *file = icon(4);
    char id[TD_ASSET_ID_LEN + 1];
    id_of(file, ICON_BYTES, id);
    td_asset_space_t idle;
    TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_space(&idle));
    TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_upload_begin(id, ICON_BYTES, TD_ASSET_FMT_TDI1));
    TEST_ASSERT_EQUAL(TD_ASSET_STATE, td_asset_upload_begin(id, ICON_BYTES, TD_ASSET_FMT_TDI1));
    td_asset_space_t open;
    TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_space(&open));
    TEST_ASSERT_EQUAL_UINT32(idle.reserved_bytes + td_asset_cost(ICON_BYTES), open.reserved_bytes);
    TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_upload_write(file, 2 * CHUNK));
    TEST_ASSERT_EQUAL(TD_ASSET_INCOMPLETE, td_asset_upload_finish());
    TEST_ASSERT_EQUAL_STRING(id, td_asset_upload_id());
    TEST_ASSERT_FALSE(td_asset_present(id, NULL, NULL));
    TEST_ASSERT_FALSE(listed(id));
    TEST_ASSERT_TRUE(temp_exists());
    td_asset_upload_abort();
    TEST_ASSERT_FALSE(temp_exists());
    TEST_ASSERT_FALSE(shard_exists(id));
    TEST_ASSERT_NULL(td_asset_upload_id());
    TEST_ASSERT_FALSE(td_asset_present(id, NULL, NULL));
    td_asset_space_t closed;
    TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_space(&closed));
    TEST_ASSERT_EQUAL_UINT32(idle.reserved_bytes, closed.reserved_bytes);
    free(file);
}

#define LISTED 24

TEST_CASE("Asset listing pages four at a time in id order across shards, and delete drops empty shards", "[td_asset]")
{
    mount();
    static char ids[LISTED][TD_ASSET_ID_LEN + 1];
    bool shard_used[16] = { false };
    for (uint8_t i = 0; i < LISTED; i++) {
        uint8_t *file = icon(10 + i);
        id_of(file, ICON_BYTES, ids[i]);
        TEST_ASSERT_EQUAL(TD_ASSET_OK, put(ids[i], file, ICON_BYTES));
        shard_used[strchr("0123456789abcdef", ids[i][0]) - "0123456789abcdef"] = true;
        free(file);
    }
    size_t shards = 0;
    for (int i = 0; i < 16; i++) {
        shards += shard_used[i];
    }
    TEST_ASSERT_GREATER_THAN(4, shards);
    td_asset_item_t items[TD_ASSET_PAGE];
    size_t count;
    bool more;
    char last[TD_ASSET_ID_LEN + 1] = "";
    size_t seen = 0;
    do {
        TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_list(last[0] ? last : NULL, items, &count, &more));
        TEST_ASSERT_LESS_OR_EQUAL(TD_ASSET_PAGE, count);
        TEST_ASSERT_TRUE(!more || count == TD_ASSET_PAGE);
        for (size_t i = 0; i < count; i++) {
            TEST_ASSERT_LESS_THAN(0, strcmp(last, items[i].id));
            TEST_ASSERT_EQUAL_UINT32(ICON_BYTES, items[i].bytes);
            strlcpy(last, items[i].id, sizeof last);
            for (size_t k = 0; k < LISTED; k++) {
                seen += strcmp(ids[k], items[i].id) == 0;
            }
        }
    } while (more);
    TEST_ASSERT_EQUAL(LISTED, seen);
    for (int i = 0; i < LISTED; i++) {
        bool deleted;
        TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_delete(ids[i], &deleted));
        TEST_ASSERT_TRUE(deleted);
        bool shard_left = false;
        for (int k = i + 1; k < LISTED; k++) {
            shard_left |= ids[k][0] == ids[i][0];
        }
        TEST_ASSERT_EQUAL(shard_left, shard_exists(ids[i]));
    }
}

TEST_CASE("Asset admission cost stays bounded as the store grows", "[td_asset]")
{
    mount();
    char ids[64][TD_ASSET_ID_LEN + 1];
    int64_t empty = 0, populated = 0;
    int64_t empty_put = 0, populated_put = 0;
    for (unsigned count = 0; count <= 64; count++) {
        if (count == 0 || count == 64) {
            int64_t start = esp_timer_get_time();
            for (unsigned n = 0; n < 100; n++) {
                td_asset_space_t space;
                TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_space(&space));
            }
            if (count == 0) empty = esp_timer_get_time() - start;
            else populated = esp_timer_get_time() - start;
            uint8_t *probe = icon(200);
            char probe_id[TD_ASSET_ID_LEN + 1];
            id_of(probe, ICON_BYTES, probe_id);
            free(probe);
            start = esp_timer_get_time();
            for (unsigned n = 0; n < 5; n++) {
                TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_upload_begin(probe_id, ICON_BYTES, TD_ASSET_FMT_TDI1));
                td_asset_upload_abort();
            }
            if (count == 0) empty_put = esp_timer_get_time() - start;
            else populated_put = esp_timer_get_time() - start;
        }
        if (count < 64) {
            uint8_t *file = icon(100 + count);
            id_of(file, ICON_BYTES, ids[count]);
            TEST_ASSERT_EQUAL(TD_ASSET_OK, put(ids[count], file, ICON_BYTES));
            free(file);
        }
    }
    for (unsigned n = 0; n < 64; n++) {
        bool deleted;
        TEST_ASSERT_EQUAL(TD_ASSET_OK, td_asset_delete(ids[n], &deleted));
    }
    printf("Asset space checks (100): empty=%lld us, 64 Assets=%lld us\n", (long long)empty, (long long)populated);
    TEST_ASSERT_TRUE(populated < empty * 2 + 1000);
    printf("Asset put admission (5): empty=%lld us, 64 Assets=%lld us\n", (long long)empty_put, (long long)populated_put);
    TEST_ASSERT_TRUE(populated_put < empty_put * 2 + 25000);
}
