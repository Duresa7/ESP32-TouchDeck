# Perf tools builds

`tools.defaults` turns on `CONFIG_TD_PERF_TOOLS`, the test-only `perf` commands that measure frame rate, frame
times, latency, memory and CPU on the deck. It also turns on FreeRTOS run-time stats on `esp_timer`. The tools wrap two
esp_lcd calls at link time and never format a storage volume that will not mount, so they stay out of `sdkconfig` and
`sdkconfig.defaults`. Off, the firmware compiles to the same code as without them.

Build from `firmware/` into a build directory of its own with its own `sdkconfig` there. Defaults files do not override
values already saved in an `sdkconfig`, so never reuse one from another build. `-B` takes an absolute path.

```sh
idf.py -B /abs/path/build-perf -D "SDKCONFIG=/abs/path/build-perf/sdkconfig" \
       -D "SDKCONFIG_DEFAULTS=sdkconfig.defaults;perf/tools.defaults" build
```

Flash it with the same `-B` and `-D` options and `-p <port> flash`. To compare builds that change other options, list
a defaults file of your own after `tools.defaults` and name the build in `CONFIG_TD_PERF_VARIANT`, which `perf info`
reports.

The commands and what each figure counts are in "Perf tools (test builds only)" in
[docs/firmware.md](../../docs/firmware.md).
