#pragma once

/* `net pair`: opens a window that the next pair.request binds. NULL on success, or why it was refused. */
const char *td_netlink_pair_window(void);
