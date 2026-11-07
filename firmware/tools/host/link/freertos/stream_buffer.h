#pragma once

#include "freertos/FreeRTOS.h"

typedef struct host_stream *StreamBufferHandle_t;

StreamBufferHandle_t xStreamBufferCreate(size_t size, size_t trigger);
size_t xStreamBufferSend(StreamBufferHandle_t stream, const void *data, size_t len, TickType_t wait);
size_t xStreamBufferReceive(StreamBufferHandle_t stream, void *data, size_t len, TickType_t wait);
size_t xStreamBufferSpacesAvailable(StreamBufferHandle_t stream);
void vStreamBufferDelete(StreamBufferHandle_t stream);
