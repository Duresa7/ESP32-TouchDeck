#include <string.h>
#include "ui_tile_action.h"

#define ARRAY_LEN(a) (sizeof(a) / sizeof((a)[0]))

static const ui_binding_t k_bindings[] = {
    { "f1", { .kind = UI_ACTION_KEY, .key = 58 } },
    { "f2", { .kind = UI_ACTION_KEY, .key = 59 } },
    { "f3", { .kind = UI_ACTION_KEY, .key = 60 } },
    { "f4", { .kind = UI_ACTION_KEY, .key = 61 } },
    { "f5", { .kind = UI_ACTION_KEY, .key = 62 } },
    { "f6", { .kind = UI_ACTION_KEY, .key = 63 } },
    { "f7", { .kind = UI_ACTION_KEY, .key = 64 } },
    { "f8", { .kind = UI_ACTION_KEY, .key = 65 } },
    { "f9", { .kind = UI_ACTION_KEY, .key = 66 } },
    { "f10", { .kind = UI_ACTION_KEY, .key = 67 } },
    { "f11", { .kind = UI_ACTION_KEY, .key = 68 } },
    { "f12", { .kind = UI_ACTION_KEY, .key = 69 } },
    { "shift", { .kind = UI_ACTION_MOD, .mod = KEY_MOD_SHIFT } },
    { "ctrl", { .kind = UI_ACTION_MOD, .mod = KEY_MOD_CTRL } },
    { "alt", { .kind = UI_ACTION_MOD, .mod = KEY_MOD_ALT } },
    { "win", { .kind = UI_ACTION_MOD, .mod = KEY_MOD_WIN } },
    { "prev", { .kind = UI_ACTION_CONSUMER, .consumer = UI_CONSUMER_PREV } },
    { "playpause", { .kind = UI_ACTION_CONSUMER, .consumer = UI_CONSUMER_PLAY_PAUSE } },
    { "next", { .kind = UI_ACTION_CONSUMER, .consumer = UI_CONSUMER_NEXT } },
    { "mute", { .kind = UI_ACTION_CONSUMER, .consumer = UI_CONSUMER_MUTE } },
    { "voldown", { .kind = UI_ACTION_CONSUMER, .consumer = UI_CONSUMER_VOLUME_DOWN } },
    { "volup", { .kind = UI_ACTION_CONSUMER, .consumer = UI_CONSUMER_VOLUME_UP } },
};

const ui_binding_t *ui_builtin_bindings(size_t *count)
{
    if (count != NULL) {
        *count = ARRAY_LEN(k_bindings);
    }
    return k_bindings;
}

ui_action_t ui_tile_action(const ui_tile_t *tile)
{
    if (tile == NULL) {
        return (ui_action_t){ .kind = UI_ACTION_NONE };
    }
    for (size_t i = 0; i < ARRAY_LEN(k_bindings); i++) {
        if (strcmp(k_bindings[i].tile_id, tile->id) == 0) {
            return k_bindings[i].action;
        }
    }
    return (ui_action_t){ .kind = UI_ACTION_NONE };
}

bool ui_tile_click(const ui_tile_t *tile, const ui_action_ops_t *ops, void *ctx)
{
    if (tile == NULL || ops == NULL) {
        return false;
    }
    const ui_action_t action = ui_tile_action(tile);
    switch (action.kind) {
    case UI_ACTION_KEY:
        if (ops->key == NULL) {
            return false;
        }
        ops->key(ctx, action.key);
        return true;
    case UI_ACTION_MOD:
        if (ops->mod == NULL) {
            return false;
        }
        ops->mod(ctx, action.mod);
        return true;
    case UI_ACTION_CONSUMER:
        if (ops->consumer == NULL) {
            return false;
        }
        ops->consumer(ctx, action.consumer);
        return true;
    case UI_ACTION_NONE:
        break;
    }
    return false;
}
