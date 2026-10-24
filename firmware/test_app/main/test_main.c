#include "unity.h"
#include "unity_test_runner.h"

void setUp(void)
{
}

void tearDown(void)
{
}

void app_main(void)
{
    UNITY_BEGIN();
    unity_run_all_tests();
    UNITY_END();
}
