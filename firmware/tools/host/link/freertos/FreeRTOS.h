#pragma once

/* Single-threaded host stand-ins for the FreeRTOS calls td_link.c makes. link_carrier_test.c implements them. */

#include <assert.h>
#include <stddef.h>
#include <stdint.h>

typedef uint32_t TickType_t;
typedef int BaseType_t;
typedef unsigned UBaseType_t;
typedef int portMUX_TYPE;

#define portMUX_INITIALIZER_UNLOCKED 0
#define portENTER_CRITICAL(p) ((void)(p))
#define portEXIT_CRITICAL(p) ((void)(p))
#define portMAX_DELAY UINT32_MAX
#define pdMS_TO_TICKS(ms) ((TickType_t)(ms))
#define pdFALSE 0
#define pdTRUE 1
#define pdFAIL 0
#define pdPASS 1
#define configASSERT(x) assert(x)

#ifdef _MSC_VER
#define __ATOMIC_RELAXED 0
#define __atomic_add_fetch(p, v, order) (*(p) += (v))
#define __atomic_load_n(p, order) (*(p))
#endif
