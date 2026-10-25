#pragma once

#include "td_recovery.h"

/* The watcher's state through the function the touch component registered, or IDLE and zeros before it did. */
void td_recovery_get_info(td_recovery_info_t *out);
