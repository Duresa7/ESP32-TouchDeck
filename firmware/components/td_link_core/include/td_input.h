#pragma once
#include <stdbool.h>
#include <stdint.h>

typedef struct {
    char page[16];
    char tile[16];
    double value;
    uint32_t seq;
    uint32_t t;
    bool end;
} td_input_event_t;

typedef struct {
    td_input_event_t event;
    uint32_t seq;
    uint32_t sent_ms;
    bool sent;
    bool pending;
} td_input_mailbox_t;

/* The caller serializes mailbox access. Cancel preserves the boot sequence counter. */
bool td_input_put(td_input_mailbox_t *box, const char *page, const char *tile, double value, bool end, uint32_t now);
bool td_input_take(td_input_mailbox_t *box, uint32_t now, td_input_event_t *out);
void td_input_cancel(td_input_mailbox_t *box);
