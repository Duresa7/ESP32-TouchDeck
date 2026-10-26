#include "td_net.h"
#include <string.h>
#include "unity.h"

TEST_CASE("wifi credentials enforce byte limits and allow open networks", "[td_net]")
{
    TEST_ASSERT_TRUE(td_net_credentials_valid("deck", ""));
    TEST_ASSERT_TRUE(td_net_credentials_valid("deck", "12345678"));
    TEST_ASSERT_FALSE(td_net_credentials_valid("", "12345678"));
    TEST_ASSERT_FALSE(td_net_credentials_valid("deck", "1234567"));
    TEST_ASSERT_FALSE(td_net_credentials_valid(NULL, "12345678"));
    TEST_ASSERT_FALSE(td_net_credentials_valid("deck", NULL));
    char ssid[34], pass[65];
    memset(ssid, 's', 33);
    memset(pass, 'p', 64);
    ssid[32] = '\0';
    pass[63] = '\0';
    TEST_ASSERT_TRUE(td_net_credentials_valid(ssid, pass));
    ssid[32] = 's'; ssid[33] = '\0';
    TEST_ASSERT_FALSE(td_net_credentials_valid(ssid, pass));
    ssid[32] = '\0'; pass[63] = 'p'; pass[64] = '\0';
    TEST_ASSERT_FALSE(td_net_credentials_valid(ssid, pass));
}

TEST_CASE("wifi reconnect backoff reaches thirty seconds and stays there", "[td_net]")
{
    TEST_ASSERT_EQUAL_UINT(1, td_net_retry_seconds(0));
    TEST_ASSERT_EQUAL_UINT(2, td_net_retry_seconds(1));
    TEST_ASSERT_EQUAL_UINT(4, td_net_retry_seconds(2));
    TEST_ASSERT_EQUAL_UINT(8, td_net_retry_seconds(3));
    TEST_ASSERT_EQUAL_UINT(30, td_net_retry_seconds(4));
    TEST_ASSERT_EQUAL_UINT(30, td_net_retry_seconds(100000));
}
