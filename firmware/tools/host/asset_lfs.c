#include "compat.h"
#include <errno.h>
#include <fcntl.h>
#include <unistd.h>
#include <assert.h>
#ifdef _MSC_VER
#pragma warning(push)
/* LittleFS isolates the lowest set bit using unsigned negation. */
#pragma warning(disable: 4146)
#endif
#include "lfs.h"
#ifdef _MSC_VER
#pragma warning(pop)
#endif

static unsigned char disk[128 * 4096];
static lfs_t fs;
static lfs_file_t files[8];
static bool opened[8], writable[8];
static int fail_sync, fail_close;
static int rd(const struct lfs_config *c, lfs_block_t b, lfs_off_t o, void *p, lfs_size_t n)
{ (void)c; memcpy(p, disk + b * 4096 + o, n); return 0; }
static int wr(const struct lfs_config *c, lfs_block_t b, lfs_off_t o, const void *p, lfs_size_t n)
{ (void)c; memcpy(disk + b * 4096 + o, p, n); return 0; }
static int er(const struct lfs_config *c, lfs_block_t b)
{ (void)c; memset(disk + b * 4096, 255, 4096); return 0; }
static int sy(const struct lfs_config *c) { (void)c; return 0; }
static const struct lfs_config config = {
    .read = rd, .prog = wr, .erase = er, .sync = sy,
    .read_size = 16, .prog_size = 16, .block_size = 4096, .block_count = 128,
    .cache_size = 512, .lookahead_size = 16, .block_cycles = 500
};
static int posix_result(int result)
{
    if (result >= 0) return result;
    errno = -result;
    return -1;
}
int host_handles(void)
{
    int n = 0;
    for (int i = 0; i < 8; i++) n += opened[i];
    return n;
}
void host_lfs_init(void)
{
    assert(host_handles() == 0);
    assert(lfs_format(&fs, &config) == 0);
    assert(lfs_mount(&fs, &config) == 0);
}
void host_lfs_done(void) { assert(host_handles() == 0); assert(lfs_unmount(&fs) == 0); }
void host_fail_sync(int error) { fail_sync = error; }
void host_fail_close(int error) { fail_close = error; }
int host_exists(const char *path)
{
    struct lfs_info info;
    return lfs_stat(&fs, path, &info) == 0;
}
static int lf_open(const char *path, int flags)
{
    for (int i = 0; i < 8; i++) {
        if (opened[i]) continue;
        int mode = flags & O_WRONLY ? LFS_O_WRONLY : LFS_O_RDONLY;
        if (flags & O_CREAT) mode |= LFS_O_CREAT;
        if (flags & O_TRUNC) mode |= LFS_O_TRUNC;
        int rc = lfs_file_open(&fs, &files[i], path, mode);
        if (rc < 0) return posix_result(rc);
        opened[i] = true;
        writable[i] = (flags & O_WRONLY) != 0;
        return i;
    }
    errno = EMFILE;
    return -1;
}
static int lf_close(int fd)
{
    assert(fd >= 0 && fd < 8 && opened[fd]);
    int rc = lfs_file_close(&fs, &files[fd]);
    opened[fd] = false;
    if (fail_close && writable[fd]) { errno = fail_close; fail_close = 0; return -1; }
    return posix_result(rc);
}
static int lf_sync(int fd)
{
    assert(opened[fd]);
    if (fail_sync) { errno = fail_sync; fail_sync = 0; return -1; }
    return posix_result(lfs_file_sync(&fs, &files[fd]));
}
static ssize_t lf_read(int fd, void *buf, size_t n)
{ return posix_result(lfs_file_read(&fs, &files[fd], buf, (lfs_size_t)n)); }
static ssize_t lf_write(int fd, const void *buf, size_t n)
{ return posix_result(lfs_file_write(&fs, &files[fd], buf, (lfs_size_t)n)); }
static int lf_stat(const char *path, struct stat *st)
{
    struct lfs_info info;
    int rc = lfs_stat(&fs, path, &info);
    if (rc < 0) return posix_result(rc);
    memset(st, 0, sizeof *st);
    st->st_size = info.size;
    st->st_mode = info.type == LFS_TYPE_REG ? S_IFREG : S_IFDIR;
    return 0;
}
typedef struct { int fd; int error; } lf_stream;
static lf_stream stream;
static lf_stream *lf_fopen(const char *path)
{
    int fd = lf_open(path, O_RDONLY);
    if (fd < 0) return NULL;
    stream = (lf_stream){fd, 0};
    return &stream;
}
static size_t lf_fread(void *buf, size_t size, size_t n, lf_stream *f)
{
    ssize_t rc = lf_read(f->fd, buf, size * n);
    if (rc < 0) { f->error = 1; return 0; }
    return (size_t)rc / size;
}
#undef open
#undef close
#undef fsync
#undef read
#undef write
#undef mkdir
#undef rmdir
#undef unlink
#define open(path, flags, ...) lf_open(path, flags)
#define close lf_close
#define fsync lf_sync
#define read lf_read
#define write lf_write
#define mkdir(path, mode) posix_result(lfs_mkdir(&fs, path))
#define rmdir(path) posix_result(lfs_remove(&fs, path))
#define unlink(path) posix_result(lfs_remove(&fs, path))
#define rename(from, to) posix_result(lfs_rename(&fs, from, to))
#define stat(path, st) lf_stat(path, st)
#define FILE lf_stream
#define fopen(path, mode) lf_fopen(path)
#define fread lf_fread
#define ferror(f) ((f)->error)
#define fclose(f) lf_close((f)->fd)
#include "../../components/td_store/td_asset.c"
