#pragma once
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
static inline size_t host_strlcpy(char *out, const char *in, size_t n)
{
    size_t len = strlen(in);
    if (n) {
        size_t take = len < n - 1 ? len : n - 1;
        memcpy(out, in, take);
        out[take] = 0;
    }
    return len;
}
#define strlcpy host_strlcpy
#ifdef _WIN32
#include <direct.h>
#include <fcntl.h>
#include <io.h>
typedef intptr_t ssize_t;
#define mkdir(path, mode) _mkdir(path)
#define rmdir _rmdir
#define unlink _unlink
#define fsync _commit
#define close _close
#define S_ISREG(mode) (((mode) & _S_IFMT) == _S_IFREG)
static inline int host_open(const char *path, int flags, int mode) { return _open(path, flags | _O_BINARY, mode); }
#define open(path, flags, ...) host_open(path, flags, 0666)
static inline ssize_t host_read(int fd, void *buf, size_t n) { return _read(fd, buf, (unsigned)n); }
static inline ssize_t host_write(int fd, const void *buf, size_t n) { return _write(fd, buf, (unsigned)n); }
#define read host_read
#define write host_write
#endif
