#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include "key_engine.h"
#include "ui_pages.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum {
    UI_ACTION_NONE = 0,
    UI_ACTION_KEY,
    UI_ACTION_MOD,
} ui_action_kind_t;

/* What a tile sends. NONE is 0, so a tile with no binding sends nothing. */
typedef struct {
    ui_action_kind_t kind;
    uint8_t key;     /* KEY: a keyboard usage, 1 to 255 */
    key_mod_t mod;   /* MOD */
} ui_action_t;

typedef struct {
    const char *tile_id;
    ui_action_t action;
} ui_binding_t;

/* The built-in bindings, one per tile that sends something. The table lives for good. */
const ui_binding_t *ui_builtin_bindings(size_t *count);

/* What a tile sends, from its id. UI_ACTION_NONE for a tile with no binding. */
ui_action_t ui_tile_action(const ui_tile_t *tile);

typedef struct {
    void (*key)(void *ctx, uint8_t usage);
    void (*mod)(void *ctx, key_mod_t mod);
} ui_action_ops_t;

/*
 * Runs the tile's action through ops. True when it called one. Does nothing for a NULL tile or ops, an unbound
 * tile, or an action whose op is NULL. The caller decides that the press was a tap.
 */
bool ui_tile_click(const ui_tile_t *tile, const ui_action_ops_t *ops, void *ctx);

#ifdef __cplusplus
}
#endif
