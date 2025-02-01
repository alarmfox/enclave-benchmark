// Based on this example: https://github.com/libbpf/libbpf-rs/tree/master/examples/runqslower
#include <vmlinux.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
#include "tracer.h"

const volatile pid_t targ_pid = 0;

struct exec_event _event = {0};

// This EBPF map is sent to userspace
struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 256*1024);
} ringbuf SEC(".maps");

struct execve_params {
  __u64 unused;
  __u64 unused2;
  char* filename;
};

// Attach to the tracepoint "syscalls:sys_enter".
// This tracepoint is invoked on every system call entry.
SEC("tp/syscalls/sys_enter_execve")
int trace_exec(struct execve_params *ctx) {

    u32 pid = bpf_get_current_pid_tgid() >> 32;

    // not the target pid, don't do anything
    if (pid != targ_pid) {
      return 0;
    }

    struct exec_event* event = bpf_ringbuf_reserve(&ringbuf, sizeof(struct exec_event), 0);

    if (!event) {
      bpf_printk("bpf_ringbuf_reserve failed\n");
      return 1;
    }
    
    event->timestamp = bpf_ktime_get_ns();
    
    bpf_probe_read_user_str(event->filename, sizeof(event->filename), ctx->filename);
    // emit the value to user space
    bpf_ringbuf_submit(event, 0);
    
    return 0;
}

char LICENSE[] SEC("license") = "GPL";

