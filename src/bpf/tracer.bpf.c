// Based on this example: https://github.com/libbpf/libbpf-rs/tree/master/examples/runqslower
#include <vmlinux.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
#include "tracer.h"

const volatile pid_t targ_pid = 0;

struct io_event _event = {0};

// This eBPF map is sent to userspace
struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 1 << 24);
} ringbuf SEC(".maps");

static __always_inline int submit_io_event(struct trace_event_raw_sys_enter *ctx) {
    struct io_event *e;
    u32 pid;

    pid = bpf_get_current_pid_tgid() >> 32;

    // not the target pid, don't do anything
    if (pid != targ_pid) {
      return 0;
    }

    e = bpf_ringbuf_reserve(&ringbuf, sizeof(struct io_event), 0);
    if (!e)
        return 0;

    e->timestamp = bpf_ktime_get_ns();
    e->syscall = ctx->id;

    // Submit the event to the ring buffer.
    bpf_ringbuf_submit(e, 0);
    return 0;
}

// Attach to the tracepoint for read syscalls.
SEC("tracepoint/syscalls/sys_enter_read")
int trace_read(struct trace_event_raw_sys_enter *ctx) {
    return submit_io_event(ctx);
}

// Attach to the tracepoint for write syscalls.
SEC("tracepoint/syscalls/sys_enter_write")
int trace_write(struct trace_event_raw_sys_enter *ctx) {
    return submit_io_event(ctx);
}

char LICENSE[] SEC("license") = "GPL";

