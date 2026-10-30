#include <stdio.h>
#include <string.h>
#include "cJSON.h"
#include "unity.h"
#include "ui_graph.h"

/* schema/live/graph-vectors.json, embedded by the test app with a NUL after it. The UI's Vitest suite checks
 * app/ui/src/lib/deck/graph.ts against the same file. */
extern const char graph_vectors_start[] asm("_binary_graph_vectors_json_start");

TEST_CASE("graph: the ceiling is 1, 2 or 5 times a power of ten", "[ui_graph]")
{
    TEST_ASSERT_EQUAL_DOUBLE(1.0, ui_graph_ceiling(1.0));
    TEST_ASSERT_EQUAL_DOUBLE(2.0, ui_graph_ceiling(1.01));
    TEST_ASSERT_EQUAL_DOUBLE(50.0, ui_graph_ceiling(37.0));
    TEST_ASSERT_EQUAL_DOUBLE(100.0, ui_graph_ceiling(99.9));
    TEST_ASSERT_EQUAL_DOUBLE(0.5, ui_graph_ceiling(0.37));
}

TEST_CASE("graph: every shared vector places its points as the UI preview does", "[ui_graph]")
{
    cJSON *root = cJSON_Parse(graph_vectors_start);
    TEST_ASSERT_NOT_NULL(root);
    const cJSON *cases = cJSON_GetObjectItemCaseSensitive(root, "cases");
    TEST_ASSERT_TRUE(cJSON_GetArraySize(cases) >= 10);
    const cJSON *c = NULL;
    cJSON_ArrayForEach(c, cases)
    {
        const char *name = cJSON_GetObjectItemCaseSensitive(c, "name")->valuestring;
        const cJSON *list = cJSON_GetObjectItemCaseSensitive(c, "samples");
        double samples[64];
        uint8_t count = 0;
        const cJSON *v = NULL;
        cJSON_ArrayForEach(v, list)
        {
            samples[count++] = v->valuedouble;
        }
        ui_graph_point_t points[64];
        const uint8_t n = ui_graph_points(samples, count, (uint8_t)cJSON_GetObjectItemCaseSensitive(c, "cap")->valueint,
                                          (int16_t)cJSON_GetObjectItemCaseSensitive(c, "w")->valueint,
                                          (int16_t)cJSON_GetObjectItemCaseSensitive(c, "h")->valueint, points);
        const cJSON *expected = cJSON_GetObjectItemCaseSensitive(c, "points");
        TEST_ASSERT_EQUAL_INT_MESSAGE(cJSON_GetArraySize(expected), n, name);
        for (uint8_t i = 0; i < n; i++) {
            const cJSON *p = cJSON_GetArrayItem(expected, i);
            char message[96];
            snprintf(message, sizeof message, "%s point %u", name, i);
            TEST_ASSERT_EQUAL_INT16_MESSAGE(cJSON_GetArrayItem(p, 0)->valueint, points[i].x, message);
            TEST_ASSERT_EQUAL_INT16_MESSAGE(cJSON_GetArrayItem(p, 1)->valueint, points[i].y, message);
        }
    }
    cJSON_Delete(root);
}
