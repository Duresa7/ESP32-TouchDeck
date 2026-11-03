#include <stdio.h>
#include <string.h>
#include "cJSON.h"
#include "unity.h"
#include "ui_now_playing.h"

/* schema/live/now-playing-vectors.json, embedded by the test app with a NUL after it. */
extern const char now_playing_vectors_start[] asm("_binary_now_playing_vectors_json_start");

static int num(const cJSON *object, const char *key)
{
    return cJSON_GetObjectItemCaseSensitive(object, key)->valueint;
}

TEST_CASE("now playing: every shared vector places the art, the text column and the parts, and hits them",
          "[ui_now_playing]")
{
    cJSON *root = cJSON_Parse(now_playing_vectors_start);
    TEST_ASSERT_NOT_NULL(root);
    const cJSON *k = cJSON_GetObjectItemCaseSensitive(root, "constants");
    TEST_ASSERT_EQUAL(UI_NP_PAD, num(k, "pad"));
    TEST_ASSERT_EQUAL(UI_NP_ART_MAX, num(k, "art_max"));
    TEST_ASSERT_EQUAL(UI_NP_ART_MIN, num(k, "art_min"));
    TEST_ASSERT_EQUAL(UI_NP_ROW_H, num(k, "row_h"));
    TEST_ASSERT_EQUAL(UI_NP_ROW_MIN_TILE_H, num(k, "row_min_tile_h"));
    TEST_ASSERT_EQUAL(UI_NP_ROW_MIN_TEXT_W, num(k, "row_min_text_w"));
    int cases = 0;
    const cJSON *c = NULL;
    cJSON_ArrayForEach(c, cJSON_GetObjectItemCaseSensitive(root, "cases"))
    {
        const char *name = cJSON_GetObjectItemCaseSensitive(c, "name")->valuestring;
        const ui_np_layout_t l = ui_np_layout((int16_t)num(c, "w"), (int16_t)num(c, "h"));
        const cJSON *art = cJSON_GetObjectItemCaseSensitive(c, "art");
        TEST_ASSERT_EQUAL_MESSAGE(!cJSON_IsNull(art), l.has_art, name);
        if (!cJSON_IsNull(art)) {
            TEST_ASSERT_EQUAL_INT16_MESSAGE(num(art, "x"), l.art.x, name);
            TEST_ASSERT_EQUAL_INT16_MESSAGE(num(art, "y"), l.art.y, name);
            TEST_ASSERT_EQUAL_INT16_MESSAGE(num(art, "size"), l.art.w, name);
            TEST_ASSERT_EQUAL_INT16_MESSAGE(num(art, "size"), l.art.h, name);
        }
        const cJSON *text = cJSON_GetObjectItemCaseSensitive(c, "text");
        TEST_ASSERT_EQUAL_INT16_MESSAGE(num(text, "x"), l.text_x, name);
        TEST_ASSERT_EQUAL_INT16_MESSAGE(num(text, "w"), l.text_w, name);
        TEST_ASSERT_EQUAL_MESSAGE(cJSON_IsTrue(cJSON_GetObjectItemCaseSensitive(c, "controls")), l.controls, name);
        const cJSON *parts = cJSON_GetObjectItemCaseSensitive(c, "parts");
        TEST_ASSERT_EQUAL_MESSAGE(cJSON_GetArraySize(parts), l.part_count, name);
        for (uint8_t i = 0; i < l.part_count; i++) {
            const cJSON *p = cJSON_GetArrayItem(parts, i);
            TEST_ASSERT_EQUAL_STRING_MESSAGE(cJSON_GetObjectItemCaseSensitive(p, "part")->valuestring,
                                             ui_part_name(l.parts[i]), name);
            TEST_ASSERT_EQUAL_INT16_MESSAGE(num(p, "x"), l.zones[i].x, name);
            TEST_ASSERT_EQUAL_INT16_MESSAGE(num(p, "y"), l.zones[i].y, name);
            TEST_ASSERT_EQUAL_INT16_MESSAGE(num(p, "w"), l.zones[i].w, name);
            TEST_ASSERT_EQUAL_INT16_MESSAGE(num(p, "h"), l.zones[i].h, name);
        }
        const cJSON *hit = NULL;
        cJSON_ArrayForEach(hit, cJSON_GetObjectItemCaseSensitive(c, "hits"))
        {
            char what[112];
            snprintf(what, sizeof what, "%s at %d,%d state %d", name, num(hit, "x"), num(hit, "y"), num(hit, "state"));
            const cJSON *part = cJSON_GetObjectItemCaseSensitive(hit, "part");
            const ui_part_t got =
                ui_np_hit(&l, (int16_t)num(hit, "x"), (int16_t)num(hit, "y"), ui_np_state(num(hit, "state")));
            if (cJSON_IsNull(part)) {
                TEST_ASSERT_NULL_MESSAGE(ui_part_name(got), what);
            } else {
                TEST_ASSERT_EQUAL_STRING_MESSAGE(part->valuestring, ui_part_name(got), what);
            }
        }
        cases++;
    }
    TEST_ASSERT_TRUE(cases >= 14);
    cJSON_Delete(root);
}

TEST_CASE("now playing: state 1 is paused, 2 playing, and anything else nothing playing", "[ui_now_playing]")
{
    TEST_ASSERT_EQUAL(UI_NP_IDLE, ui_np_state(0));
    TEST_ASSERT_EQUAL(UI_NP_PAUSED, ui_np_state(1));
    TEST_ASSERT_EQUAL(UI_NP_PLAYING, ui_np_state(2));
    TEST_ASSERT_EQUAL(UI_NP_IDLE, ui_np_state(1.5));
    TEST_ASSERT_EQUAL(UI_NP_IDLE, ui_np_state(-1));
    TEST_ASSERT_EQUAL(UI_NP_IDLE, ui_np_state(3));
}

TEST_CASE("now playing: parts have their wire names", "[ui_now_playing]")
{
    TEST_ASSERT_EQUAL_STRING("previous", ui_part_name(UI_PART_PREVIOUS));
    TEST_ASSERT_EQUAL_STRING("play_pause", ui_part_name(UI_PART_PLAY_PAUSE));
    TEST_ASSERT_EQUAL_STRING("next", ui_part_name(UI_PART_NEXT));
    TEST_ASSERT_EQUAL_STRING("mute", ui_part_name(UI_PART_MUTE));
    TEST_ASSERT_NULL(ui_part_name(UI_PART_NONE));
}
