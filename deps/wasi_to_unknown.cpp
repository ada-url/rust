// Some shims for WASI symbols used by the WASI libc environment initializer,
// but not actually required by Ada. This allows to compile Ada Rust to
// wasm32-unknown-unknown with WASI SDK.

#include <stdint.h>

extern "C" {

int32_t __imported_wasi_snapshot_preview1_environ_get(int32_t, int32_t) {
  __builtin_unreachable();
}

int32_t __imported_wasi_snapshot_preview1_environ_sizes_get(int32_t, int32_t) {
  __builtin_unreachable();
}

int32_t __imported_wasi_snapshot_preview1_fd_close(int32_t) {
  __builtin_unreachable();
}

int32_t __imported_wasi_snapshot_preview1_fd_fdstat_get(int32_t, int32_t) {
  __builtin_unreachable();
}

int32_t __imported_wasi_snapshot_preview1_fd_read(int32_t,
                                                  int32_t,
                                                  int32_t,
                                                  int32_t) {
  __builtin_unreachable();
}

int32_t __imported_wasi_snapshot_preview1_fd_seek(int32_t,
                                                  int64_t,
                                                  int32_t,
                                                  int32_t) {
  __builtin_unreachable();
}

int32_t __imported_wasi_snapshot_preview1_fd_write(int32_t,
                                                   int32_t,
                                                   int32_t,
                                                   int32_t) {
  __builtin_unreachable();
}

int32_t __imported_wasi_snapshot_preview1_sched_yield() {
  return 0;
}

_Noreturn void __imported_wasi_snapshot_preview1_proc_exit(int32_t) {
  __builtin_unreachable();
}

} // extern "C"
