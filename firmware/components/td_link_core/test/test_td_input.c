#include "td_input.h"
#include "unity.h"
#include <math.h>

TEST_CASE("input coalesces moves and prioritizes end", "[td_link]")
{
    td_input_mailbox_t box = {0};
    td_input_event_t event;
    TEST_ASSERT_TRUE(td_input_put(&box, "p", "t", 0.1, false, 0));
    TEST_ASSERT_TRUE(td_input_take(&box, 0, &event));
    TEST_ASSERT_TRUE(td_input_put(&box, "p", "t", 0.2, false, 10));
    TEST_ASSERT_TRUE(td_input_put(&box, "p", "t", 0.3, false, 20));
    TEST_ASSERT_FALSE(td_input_take(&box, 49, &event));
    TEST_ASSERT_TRUE(td_input_take(&box, 50, &event));
    TEST_ASSERT_EQUAL_UINT32(3, event.seq);
    TEST_ASSERT_DOUBLE_WITHIN(0.00001, 0.3, event.value);
    TEST_ASSERT_TRUE(td_input_put(&box, "p", "t", 0.4, true, 51));
    TEST_ASSERT_TRUE(td_input_take(&box, 51, &event));
    TEST_ASSERT_TRUE(event.end);
    TEST_ASSERT_TRUE(td_input_put(&box, "p", "t", 0.5, false, 52));
    td_input_cancel(&box);
    TEST_ASSERT_FALSE(td_input_take(&box, 100, &event));
    TEST_ASSERT_FALSE(td_input_put(&box, "p", "t", NAN, false, 101));
    TEST_ASSERT_FALSE(td_input_put(&box, "p", "t", 1.1, false, 101));
}
