#include "td_v2_json.h"
#include <stdint.h>
#include <string.h>

/* Strict lexical rules shared with the Layout protocol: validate before cJSON loses number spelling or NUL. */
static bool utf8_valid(const uint8_t *p, size_t n)
{
    size_t i = 0;
    while (i < n) {
        const uint8_t c = p[i];
        if (c < 0x80) {
            i++;
            continue;
        }
        size_t need = 0;
        uint8_t lo = 0x80;
        uint8_t hi = 0xbf;
        if (c >= 0xc2 && c <= 0xdf) {
            need = 1;
        } else if (c >= 0xe0 && c <= 0xef) {
            need = 2;
            lo = c == 0xe0 ? 0xa0 : 0x80;
            hi = c == 0xed ? 0x9f : 0xbf;
        } else if (c >= 0xf0 && c <= 0xf4) {
            need = 3;
            lo = c == 0xf0 ? 0x90 : 0x80;
            hi = c == 0xf4 ? 0x8f : 0xbf;
        } else {
            return false;
        }
        if (n - i - 1 < need || p[i + 1] < lo || p[i + 1] > hi) {
            return false;
        }
        for (size_t k = 2; k <= need; k++) {
            if (p[i + k] < 0x80 || p[i + k] > 0xbf) {
                return false;
            }
        }
        i += need + 1;
    }
    return true;
}

/* The grammar of JSON.parse, which is stricter than cJSON's: only four whitespace bytes, no trailing text, no raw
 * control bytes in strings, and exact number and literal forms. A rule that JSON.parse lets through goes in `soft`,
 * and only the first one is kept. */
typedef struct {
    const uint8_t *p;
    const uint8_t *start;
    const uint8_t *end;
    int depth;
    bool too_deep;
    bool has_soft;
    const uint8_t *message;
    size_t message_len;
    bool live;
    bool in_value;
} scan_t;

static void soft(scan_t *s, const char *key, const char *message)
{
    (void)key;
    (void)message;
    s->has_soft = true;
}

static void skip_ws(scan_t *s)
{
    while (s->p < s->end && (*s->p == ' ' || *s->p == '\t' || *s->p == '\n' || *s->p == '\r')) {
        s->p++;
    }
}

static int hex4(const uint8_t *p)
{
    int v = 0;
    for (int i = 0; i < 4; i++) {
        const uint8_t c = p[i];
        int d = -1;
        if (c >= '0' && c <= '9') {
            d = c - '0';
        } else if (c >= 'a' && c <= 'f') {
            d = c - 'a' + 10;
        } else if (c >= 'A' && c <= 'F') {
            d = c - 'A' + 10;
        }
        if (d < 0) {
            return -1;
        }
        v = v * 16 + d;
    }
    return v;
}

static bool scan_string(scan_t *s)
{
    s->p++;
    while (s->p < s->end) {
        uint8_t c = *s->p++;
        if (c == '"') {
            return true;
        }
        if (c < 0x20) {
            return false;
        }
        if (c == 127) s->has_soft = true;
        if (c != '\\') {
            continue;
        }
        if (s->p >= s->end) {
            return false;
        }
        c = *s->p++;
        if (c == 'b' || c == 'f' || c == 'n' || c == 'r' || c == 't') s->has_soft = true;
        if (c == '"' || c == '\\' || c == '/' || c == 'b' || c == 'f' || c == 'n' || c == 'r' || c == 't') {
            continue;
        }
        if (c != 'u' || s->end - s->p < 4) {
            return false;
        }
        const int cp = hex4(s->p);
        if (cp < 0) {
            return false;
        }
        s->p += 4;
        if (cp < 32 || cp == 127) {
            soft(s, "string.control-character", "a string holds U+0000");
        } else if (cp >= 0xd800 && cp <= 0xdbff) {
            if (s->end - s->p >= 6 && s->p[0] == '\\' && s->p[1] == 'u') {
                const int low = hex4(s->p + 2);
                if (low >= 0xdc00 && low <= 0xdfff) {
                    s->p += 6;
                    continue;
                }
            }
            soft(s, "string.malformed-unicode", "a string holds a lone surrogate");
        } else if (cp >= 0xdc00 && cp <= 0xdfff) {
            soft(s, "string.malformed-unicode", "a string holds a lone surrogate");
        }
    }
    return false;
}

static bool is_digit(const scan_t *s)
{
    return s->p < s->end && *s->p >= '0' && *s->p <= '9';
}

static bool scan_digits(scan_t *s)
{
    if (!is_digit(s)) {
        return false;
    }
    while (is_digit(s)) {
        s->p++;
    }
    return true;
}

/* An integer is 0, or an optional minus and a nonzero digit and more digits. A fraction, an exponent or -0 is valid
 * JSON but breaks json.integer-form. */
static bool scan_number(scan_t *s)
{
    const uint8_t *start = s->p;
    bool integer = true;
    if (*s->p == '-') {
        s->p++;
    }
    if (s->p < s->end && *s->p == '0') {
        s->p++;
    } else if (!scan_digits(s)) {
        return false;
    }
    if (s->p < s->end && *s->p == '.') {
        integer = false;
        s->p++;
        if (!scan_digits(s)) {
            return false;
        }
    }
    if (s->p < s->end && (*s->p == 'e' || *s->p == 'E')) {
        integer = false;
        s->p++;
        if (s->p < s->end && (*s->p == '+' || *s->p == '-')) {
            s->p++;
        }
        if (!scan_digits(s)) {
            return false;
        }
    }
    if (s->in_value) {
        return true;
    }
    if (!integer || (s->p - start == 2 && start[0] == '-' && start[1] == '0')) {
        const uint8_t *after = s->p;
        s->p = start;
        soft(s, "json.integer-form", "a number is not written as a JSON integer");
        s->p = after;
    }
    return true;
}

static bool scan_literal(scan_t *s, const char *word)
{
    const size_t n = strlen(word);
    if ((size_t)(s->end - s->p) < n || memcmp(s->p, word, n) != 0) {
        return false;
    }
    s->p += n;
    return true;
}

static bool scan_value(scan_t *s);

static bool scan_container(scan_t *s, uint8_t close)
{
    if (++s->depth > 16) {
        s->too_deep = true;
        return false;
    }
    s->p++;
    skip_ws(s);
    if (s->p < s->end && *s->p == close) {
        s->p++;
        s->depth--;
        return true;
    }
    for (;;) {
        bool message_key = false;
        bool value_key = false;
        if (close == '}') {
            skip_ws(s);
            const uint8_t *key = s->p;
            if (s->p >= s->end || *s->p != '"' || !scan_string(s)) {
                return false;
            }
            message_key = s->depth == 1 && ((s->p - key == 3 && memcmp(key, "\"m\"", 3) == 0) ||
                (s->p - key == 8 && memcmp(key, "\"\\u006d\"", 8) == 0));
            value_key = s->live && s->depth == 1 && s->p - key == 7 && memcmp(key, "\"value\"", 7) == 0;
            skip_ws(s);
            if (s->p >= s->end || *s->p != ':') {
                return false;
            }
            s->p++;
        }
        skip_ws(s);
        const uint8_t *value = s->p;
        s->in_value = s->in_value || value_key;
        const bool scanned = scan_value(s);
        s->in_value = s->in_value && !value_key;
        if (!scanned) {
            return false;
        }
        if (message_key && value < s->end && *value == '"' && !s->message) {
            s->message = value;
            s->message_len = (size_t)(s->p - value);
        }
        skip_ws(s);
        if (s->p >= s->end) {
            return false;
        }
        if (*s->p == ',') {
            s->p++;
            continue;
        }
        if (*s->p != close) {
            return false;
        }
        s->p++;
        s->depth--;
        return true;
    }
}

static bool scan_value(scan_t *s)
{
    skip_ws(s);
    if (s->p >= s->end) {
        return false;
    }
    switch (*s->p) {
    case '{':
        return scan_container(s, '}');
    case '[':
        return scan_container(s, ']');
    case '"':
        return scan_string(s);
    case 't':
        return scan_literal(s, "true");
    case 'f':
        return scan_literal(s, "false");
    case 'n':
        return scan_literal(s, "null");
    default:
        if (*s->p == '-' || (*s->p >= '0' && *s->p <= '9')) {
            return scan_number(s);
        }
        return false;
    }
}

bool td_v2_json_valid(const char *json, size_t len)
{
    return td_v2_json_valid_live(json, len, false);
}

bool td_v2_json_valid_live(const char *json, size_t len, bool live)
{
    if (!utf8_valid((const uint8_t *)json, len)) return false;
    scan_t s = { .p = (const uint8_t *)json, .start = (const uint8_t *)json, .end = (const uint8_t *)json + len,
                 .live = live };
    if (!scan_value(&s)) return false;
    skip_ws(&s);
    return s.p == s.end && !s.has_soft;
}

/* Recover only a syntactically identifiable top-level message when cJSON cannot
 * represent a forbidden Unicode scalar or rejects excessive nesting. */
bool td_v2_json_message(const char *json, size_t len, const char **message, size_t *message_len)
{
    scan_t s = { .p = (const uint8_t *)json, .start = (const uint8_t *)json, .end = (const uint8_t *)json + len };
    bool parsed = scan_value(&s);
    skip_ws(&s);
    if ((!parsed || s.p != s.end) && !s.too_deep) return false;
    *message = (const char *)s.message;
    *message_len = s.message_len;
    return s.message != NULL;
}
