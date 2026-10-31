#pragma once
#ifdef _WIN32
#include <errno.h>
#define DT_DIR 4
struct dirent { unsigned char d_type; char d_name[256]; };
typedef struct { int unused; } DIR;
static inline DIR *opendir(const char *path) { (void)path; errno = ENOENT; return NULL; }
static inline struct dirent *readdir(DIR *d) { (void)d; return NULL; }
static inline int closedir(DIR *d) { (void)d; return 0; }
#else
#include_next <dirent.h>
#endif
