#include "td_device.h"
#include "unity.h"
#include <string.h>
TEST_CASE("Device names enforce UTF-8 byte and whitespace rules", "[td_device]")
{
    TEST_ASSERT_TRUE(td_device_name_valid("Open TouchDeck"));
    TEST_ASSERT_TRUE(td_device_name_valid("  Desk  "));
    TEST_ASSERT_FALSE(td_device_name_valid(""));
    TEST_ASSERT_FALSE(td_device_name_valid(" \xe2\x80\x83"));
    TEST_ASSERT_FALSE(td_device_name_valid("bad\nname"));
    TEST_ASSERT_FALSE(td_device_name_valid("\xc0\xaf"));
    TEST_ASSERT_FALSE(td_device_name_valid("\xed\xa0\x80"));
    TEST_ASSERT_FALSE(td_device_name_valid("\xf4\x90\x80\x80"));
    char name[50];
    memset(name, '"', 48); name[48] = 0;
    TEST_ASSERT_TRUE(td_device_name_valid(name));
    name[48] = 'x'; name[49] = 0;
    TEST_ASSERT_FALSE(td_device_name_valid(name));
}
